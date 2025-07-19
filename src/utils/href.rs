use crate::utils::{
    network::RequestRange,
    query_codec::{encode_map, encode_segment},
};
use leptos::prelude::GetUntracked;
use leptos_router::hooks::use_query_map;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::borrow::Cow;
use url::Url;

pub const PLAYLIST_URL_QUERY_NAME: &str = "playlist_url";
pub const SUPPLEMENTAL_VIEW_QUERY_NAME: &str = "supplemental_view_context";

// https://url.spec.whatwg.org/#query-percent-encode-set
// The query percent-encode set is the C0 control percent-encode set and U+0020 SPACE, U+0022 ("),
// U+0023 (#), U+003C (<), and U+003E (>).
//
// Given that the values will be URLs contained within a query value, I also need to encode b'&' and
// b'=', as I don't want to inadvertently split the query value if the source URL has multiple query
// parameters.
const QUERY: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'&')
    .add(b'=');

pub fn media_playlist_href(relative_uri: &str) -> Option<String> {
    playlist_href(base_url()?, relative_uri)
}

pub fn segment_href(
    segment_uri: &str,
    media_sequence: u64,
    byterange: Option<RequestRange>,
) -> Option<String> {
    media_segment_href(
        base_url()?,
        segment_uri,
        media_sequence,
        byterange,
        SegmentType::Segment,
    )
}

pub fn map_href(
    segment_uri: &str,
    media_sequence: u64,
    byterange: Option<RequestRange>,
) -> Option<String> {
    media_segment_href(
        base_url()?,
        segment_uri,
        media_sequence,
        byterange,
        SegmentType::Map,
    )
}

// This function can't be run in tests because `use_query_map` must be run from within a Leptos
// `Router` context (tests crash otherwise). Therefore, the bulk of the logic is extracted to below
// so that it is testable.
fn base_url() -> Option<Url> {
    let base_url_query_parameter = use_query_map()
        .get_untracked()
        .get(PLAYLIST_URL_QUERY_NAME)?;
    Url::parse(&base_url_query_parameter).ok()
}

fn playlist_href(base_url: Url, relative_uri: &str) -> Option<String> {
    let absolute_url = base_url.join(relative_uri).ok()?;
    let query_encoded_url = utf8_percent_encode(absolute_url.as_str(), QUERY);
    Some(format!("?{PLAYLIST_URL_QUERY_NAME}={query_encoded_url}"))
}

fn media_segment_href(
    base_url: Url,
    segment_uri: &str,
    media_sequence: u64,
    byterange: Option<RequestRange>,
    segment_type: SegmentType,
) -> Option<String> {
    let absolute_segment_url = base_url.join(segment_uri).ok()?;
    let query_encoded_base_url = utf8_percent_encode(base_url.as_str(), QUERY);
    let query_encoded_segment_url = utf8_percent_encode(absolute_segment_url.as_str(), QUERY);
    let encoded_supplemental_context = match segment_type {
        SegmentType::Segment => encode_segment(
            &Cow::from(query_encoded_segment_url),
            media_sequence,
            byterange,
        ),
        SegmentType::Map => encode_map(
            &Cow::from(query_encoded_segment_url),
            media_sequence,
            byterange,
        ),
    };
    Some(format!(
        "?{}={}&{}={}",
        PLAYLIST_URL_QUERY_NAME,
        query_encoded_base_url,
        SUPPLEMENTAL_VIEW_QUERY_NAME,
        encoded_supplemental_context,
    ))
}

#[derive(Clone, Copy)]
enum SegmentType {
    Segment,
    Map,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_relative_uri() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8";
        test_media_and_segment_href(base_url, uri, "https://example.com/hls/hi/video-only.m3u8");
    }

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_absolute_uri() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "https://ads.server.com/ads/hls/hi/video-only.m3u8";
        test_media_and_segment_href(
            base_url,
            uri,
            "https://ads.server.com/ads/hls/hi/video-only.m3u8",
        );
    }

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_uri_stepping_out_of_base_path() {
        let base_url = Url::parse("https://ads.com/1234/main/mvp.m3u8").unwrap();
        let uri = "../media/3.m3u8";
        test_media_and_segment_href(base_url, uri, "https://ads.com/1234/media/3.m3u8");
    }

    #[test]
    fn resolve_href_should_escape_query_pairs() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8?token=1234&id=abcd";
        test_media_and_segment_href(
            base_url,
            uri,
            "https://example.com/hls/hi/video-only.m3u8?token%3D1234%26id%3Dabcd",
        );
    }

    #[test]
    fn resolve_href_should_escape_fragment() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8?token=1234#fragment";
        test_media_and_segment_href(
            base_url,
            uri,
            "https://example.com/hls/hi/video-only.m3u8?token%3D1234%23fragment",
        );
    }

    fn test_media_and_segment_href(base_url: Url, uri: &str, expected: &str) {
        assert_eq!(
            Some(format!("?playlist_url={expected}")),
            playlist_href(base_url.clone(), uri)
        );
        assert_eq!(
            Some(format!(
                "?playlist_url={}&supplemental_view_context={}",
                base_url.as_str(),
                format!("SEGMENT,100,-,{expected}")
            )),
            media_segment_href(base_url.clone(), uri, 100, None, SegmentType::Segment)
        );
        assert_eq!(
            Some(format!(
                "?playlist_url={}&supplemental_view_context={}",
                base_url.as_str(),
                format!("MAP,100,-,{expected}")
            )),
            media_segment_href(base_url, uri, 100, None, SegmentType::Map)
        );
    }
}
