use std::io::BufReader;

use mp4_atom::{Atom, Ftyp, Header, Moof, ReadAtom, ReadFrom};
use url::Url;

use crate::utils::network::FetchArrayBufferResonse;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegmentType {
    WebVtt,
    Mp4,
    Unknown,
}

pub fn determine_segment_type(response: &FetchArrayBufferResonse) -> SegmentType {
    probe_content_type(&response.content_type)
        .or_else(|| probe_url(&response.url))
        .or_else(|| probe_data(&response.response_body))
        .unwrap_or(SegmentType::Unknown)
}

fn probe_content_type(content_type: &Option<String>) -> Option<SegmentType> {
    let Some(content_type) = content_type else {
        return None;
    };
    // https://developer.apple.com/documentation/http-live-streaming/hls-authoring-specification-for-apple-devices#Delivery
    match content_type.as_str() {
        "video/mp4" => Some(SegmentType::Mp4),
        "video/iso.segment" => Some(SegmentType::Mp4),
        "audio/mp4" => Some(SegmentType::Mp4),
        "application/mp4" => Some(SegmentType::Mp4), // IMSC1
        "text/vtt" => Some(SegmentType::WebVtt),
        "text/plain" => Some(SegmentType::WebVtt),
        _ => None,
    }
}

fn probe_url(url: &str) -> Option<SegmentType> {
    let Ok(url) = Url::parse(url) else {
        return None;
    };
    // https://developer.apple.com/documentation/http-live-streaming/hls-authoring-specification-for-apple-devices#Delivery
    url.path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|s| s.split('.').next_back())
        .and_then(|s| match s {
            "mp4" => Some(SegmentType::Mp4),
            "m4s" => Some(SegmentType::Mp4),
            "vtt" => Some(SegmentType::WebVtt),
            _ => None,
        })
}

fn probe_data(data: &[u8]) -> Option<SegmentType> {
    if probe_is_webvtt(data) {
        Some(SegmentType::WebVtt)
    } else if probe_is_mp4(data) {
        Some(SegmentType::Mp4)
    } else {
        None
    }
}

fn probe_is_webvtt(data: &[u8]) -> bool {
    // https://www.w3.org/TR/webvtt1/#file-structure
    //
    // A WebVTT file must consist of a WebVTT file body encoded as UTF-8 and labeled with the MIME
    // type text/vtt. [RFC3629]
    //
    // A WebVTT file body consists of the following components, in the following order:
    //   1. An optional U+FEFF BYTE ORDER MARK (BOM) character.
    //   2. The string "WEBVTT".
    //
    //
    // https://unicode.org/faq/utf_bom.html#bom4
    //
    // A BOM can be used as a signature no matter how the Unicode text is transformed: UTF-16,
    // UTF-8, or UTF-32. The exact bytes comprising the BOM will be whatever the Unicode character
    // U+FEFF is converted into by that transformation format. In that form, the BOM serves to
    // indicate both that it is a Unicode file, and which of the formats it is in. Examples:
    // | Bytes       | Encoding Form         |
    // | ----------- | --------------------- |
    // | 00 00 FE FF | UTF-32, big-endian    |
    // | FF FE 00 00 | UTF-32, little-endian |
    // | FE FF       | UTF-16, big-endian    |
    // | FF FE       | UTF-16, little-endian |
    // | EF BB BF    | UTF-8                 |
    const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
    let data = if data.starts_with(UTF8_BOM) {
        &data[UTF8_BOM.len()..]
    } else {
        data
    };
    data.starts_with(b"WEBVTT")
}

fn probe_is_mp4(data: &[u8]) -> bool {
    // https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-3.1.2
    //
    // Unlike regular MPEG-4 files that have a Movie Box ('moov') that contains sample tables and a
    // Media Data Box ('mdat') containing the corresponding samples, an MPEG-4 Fragment consists of
    // a Movie Fragment Box ('moof') containing a subset of the sample table and a Media Data Box
    // containing those samples.
    // [...]
    // The Media Initialization Section for an fMP4 Segment MUST contain a File Type Box ('ftyp')
    //
    // Therefore, we search for either an `ftyp` (for EXT-X-MAP) or a `moof` (for a Media Segment).
    let mut buf = BufReader::new(data);
    loop {
        let header = match Header::read_from(&mut buf) {
            Ok(header) => header,
            Err(_) => return false,
        };
        match header.kind {
            // Also parse the `ftyp` to give more confidence that it is valid.
            Ftyp::KIND => return Ftyp::read_atom(&header, &mut buf).is_ok(),
            // I don't parse the whole `moof` for 2 reasons:
            //   1. The `moof` can contain lots of data (e.g. in the `trun`)
            //   2. The mp4_atom lib fails when finding unexpected atoms and currently it has a bug
            //      where it does not expect `saio` or `saiz` to be within the `traf` (but they are
            //      allowed to be there and for DRM protected assets I am checking they are there).
            Moof::KIND => return true,
            _ => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn probe_url_mp4_file_extension_should_work() {
        let url = "https://example.com/file.mp4";
        assert_eq!(Some(SegmentType::Mp4), probe_url(url));
    }

    #[test]
    fn probe_url_m4s_file_extension_should_work() {
        let url = "https://example.com/file.m4s";
        assert_eq!(Some(SegmentType::Mp4), probe_url(url));
    }

    #[test]
    fn probe_url_vtt_file_extension_should_work() {
        let url = "https://example.com/file.vtt";
        assert_eq!(Some(SegmentType::WebVtt), probe_url(url));
    }

    #[test]
    fn probe_url_m3u8_file_extension_should_not_work() {
        let url = "https://example.com/file.m3u8";
        assert_eq!(None, probe_url(url));
    }

    #[test]
    fn probe_url_no_file_extension_should_not_work() {
        let url = "https://example.com/file";
        assert_eq!(None, probe_url(url));
    }
}
