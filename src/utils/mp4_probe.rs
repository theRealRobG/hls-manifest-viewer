//! Shared init-segment probe for Inspect and Author validation.

use std::io::Cursor;

use mp4_atom::{Header, ReadFrom};

use crate::utils::mp4_atom_properties::{AtomPropertyValue, get_properties};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InitSegmentProbe {
    pub major_brand: Option<String>,
    pub compatible_brands: Vec<String>,
    /// Raw stsd sample-entry fourCC (e.g. "avc1", "hvc1", "encv").
    pub video_sample_fourcc: Option<String>,
    pub audio_sample_fourcc: Option<String>,
    pub video_profile: Option<String>,
    pub video_level: Option<String>,
    /// HEVC general_tier_flag when available ("Main" / "High").
    pub video_tier: Option<String>,
    pub has_ludt: bool,
    pub has_senc: bool,
    pub has_saiz: bool,
    pub has_saio: bool,
    pub has_vexu: bool,
    /// `lhvC` layered-HEVC configuration, which is how MV-HEVC is signaled (§1.36).
    pub has_lhvc: bool,
    pub has_tenc: bool,
    /// True when a `tenc` was parsed inside a video track (`vide` handler).
    pub has_video_tenc: bool,
    /// CENC scheme_type from `schm` (e.g. "cenc", "cbcs").
    pub scheme_type: Option<String>,
    /// From `tenc` version ≥1 pattern encryption fields (last track seen).
    pub crypt_byte_block: Option<u8>,
    pub skip_byte_block: Option<u8>,
    /// Same fields, but only from the video track — the 1:9 `cbcs` pattern
    /// requirement applies to video, while audio is commonly encrypted without one.
    pub video_crypt_byte_block: Option<u8>,
    pub video_skip_byte_block: Option<u8>,
    /// HDR10 static metadata boxes / SEI containers often signaled near hvcC.
    pub has_mdcv: bool,
    pub has_clli: bool,
    /// True when an encrypted sample entry (`encv`/`enca`) was observed before `frma`.
    pub had_encrypted_sample_entry: bool,
    /// Media timescale from `mdhd` (prefer video track).
    pub timescale: Option<u32>,
    /// Movie timescale from `mvhd`.
    pub movie_timescale: Option<u32>,
}

fn prop_str(val: &AtomPropertyValue) -> String {
    match val {
        AtomPropertyValue::Basic(b) => String::from(b),
        AtomPropertyValue::Table(_) => String::new(),
    }
}

fn fourcc_str(header: &Header) -> String {
    header.kind.to_string()
}

const VIDEO_SAMPLE_ENTRIES: &[&str] = &[
    "avc1", "avc3", "hvc1", "hev1", "dvh1", "dvhe", "av01", "vp09", "vp08", "encv", "mjpg",
];
const AUDIO_SAMPLE_ENTRIES: &[&str] = &[
    "mp4a", "ac-3", "ec-3", "ac-4", "Opus", "enca", "apac", "fLaC",
];

/// Parse init-segment bytes (ftyp + moov). Stops on first decode error.
pub fn probe_init_segment(data: &[u8]) -> InitSegmentProbe {
    let mut info = InitSegmentProbe::default();
    let mut reader = Cursor::new(data.to_vec());
    let mut container_ends: Vec<u64> = Vec::new();
    let mut in_video = false;
    let mut in_audio = false;

    loop {
        while let Some(&end) = container_ends.last() {
            if reader.position() >= end {
                container_ends.pop();
            } else {
                break;
            }
        }
        if reader.position() as usize >= reader.get_ref().len() {
            break;
        }
        let Ok(header) = Header::read_from(&mut reader) else {
            break;
        };
        let kind = fourcc_str(&header);
        match kind.as_str() {
            "ludt" => info.has_ludt = true,
            "senc" => info.has_senc = true,
            "saiz" => info.has_saiz = true,
            "saio" => info.has_saio = true,
            "vexu" => info.has_vexu = true,
            "lhvC" => info.has_lhvc = true,
            "tenc" => info.has_tenc = true,
            "mdcv" => info.has_mdcv = true,
            "clli" => info.has_clli = true,
            _ => {}
        }

        let Ok(atom) = get_properties(&header, &mut reader) else {
            break;
        };
        if let Some(e) = atom.new_depth_until {
            container_ends.push(e);
        }

        let props = &atom.properties;
        let get = |key: &str| -> Option<String> {
            props
                .properties
                .iter()
                .find(|(k, _)| k.as_ref() == key)
                .map(|(_, v)| prop_str(v))
                .filter(|s| !s.is_empty())
        };

        match props.box_name {
            "FileTypeBox" => {
                info.major_brand = get("major_brand");
                if let Some(cb) = get("compatible_brands") {
                    info.compatible_brands = cb
                        .split(|c: char| c == ',' || c.is_whitespace())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
            "HandlerBox" => match get("handler").as_deref() {
                Some("vide") => {
                    in_video = true;
                    in_audio = false;
                }
                Some("soun") => {
                    in_audio = true;
                    in_video = false;
                }
                _ => {
                    in_video = false;
                    in_audio = false;
                }
            },
            "OriginalFormatBox" => {
                if let Some(fmt) = get("data_format") {
                    if in_video && info.video_sample_fourcc.as_deref() == Some("encv") {
                        info.video_sample_fourcc = Some(fmt);
                    } else if in_audio && info.audio_sample_fourcc.as_deref() == Some("enca") {
                        info.audio_sample_fourcc = Some(fmt);
                    }
                }
            }
            "MediaHeaderBox" => {
                if let Some(ts) = get("timescale").and_then(|s| s.parse().ok()) {
                    // Prefer video track timescale when in vide handler; otherwise first seen.
                    if in_video || info.timescale.is_none() {
                        info.timescale = Some(ts);
                    }
                }
            }
            "MovieHeaderBox" if info.movie_timescale.is_none() => {
                info.movie_timescale = get("timescale").and_then(|s| s.parse().ok());
            }
            "AVCConfigurationBox" if info.video_profile.is_none() => {
                info.video_profile = get("avc_profile_indication");
                info.video_level = get("avc_level_indication");
            }
            "HEVCConfigurationBox" if info.video_profile.is_none() => {
                info.video_profile = get("general_profile_idc").or_else(|| get("profile"));
                info.video_level = get("general_level_idc").or_else(|| get("level"));
                info.video_tier = get("general_tier_flag").or_else(|| get("tier"));
            }
            "TrackEncryptionBox" => {
                info.has_tenc = true;
                if let Some(c) = get("default_crypt_byte_block") {
                    info.crypt_byte_block = c.split_whitespace().next().and_then(|s| s.parse().ok());
                }
                if let Some(s) = get("default_skip_byte_block") {
                    info.skip_byte_block = s.split_whitespace().next().and_then(|x| x.parse().ok());
                }
                if in_video {
                    info.has_video_tenc = true;
                    info.video_crypt_byte_block = info.crypt_byte_block;
                    info.video_skip_byte_block = info.skip_byte_block;
                }
            }
            "SchemeTypeBox" if info.scheme_type.is_none() => {
                info.scheme_type = get("scheme_type");
            }
            _ => {}
        }

        // Capture raw sample-entry fourCC from the header kind
        if VIDEO_SAMPLE_ENTRIES.iter().any(|e| e.eq_ignore_ascii_case(&kind))
            && (in_video || info.video_sample_fourcc.is_none())
        {
            if kind.eq_ignore_ascii_case("encv") {
                info.had_encrypted_sample_entry = true;
            }
            info.video_sample_fourcc = Some(kind.clone());
        }
        if AUDIO_SAMPLE_ENTRIES.iter().any(|e| e.eq_ignore_ascii_case(&kind))
            && (in_audio || info.audio_sample_fourcc.is_none())
        {
            if kind.eq_ignore_ascii_case("enca") {
                info.had_encrypted_sample_entry = true;
            }
            info.audio_sample_fourcc = Some(kind.clone());
        }
    }

    // Byte scan fallback for boxes our property walker may skip.
    for pos in 4..data.len().saturating_sub(3) {
        let typ = &data[pos..pos + 4];
        if !matches!(typ, b"mdcv" | b"clli" | b"ludt" | b"vexu" | b"tenc")
            || !is_plausible_box_at(data, pos)
        {
            continue;
        }
        match typ {
            b"mdcv" => info.has_mdcv = true,
            b"clli" => info.has_clli = true,
            b"ludt" => info.has_ludt = true,
            b"vexu" => info.has_vexu = true,
            b"tenc" => info.has_tenc = true,
            _ => {}
        }
    }

    info
}

/// Whether the fourCC at `pos` is preceded by a box size that fits the buffer.
/// Without that check any ASCII run in a payload — a `vexu` inside a URL, or in
/// compressed sample data — would register as a box and set a probe flag.
fn is_plausible_box_at(data: &[u8], pos: usize) -> bool {
    if pos < 4 || pos + 4 > data.len() {
        return false;
    }
    let start = pos - 4;
    let size = u32::from_be_bytes([data[start], data[start + 1], data[start + 2], data[start + 3]]);
    match size {
        // Size 0 means the box runs to the end of the file.
        0 => true,
        // Size 1 moves the real size into a 64-bit largesize after the type.
        1 => {
            let Some(raw) = data.get(pos + 4..pos + 12) else {
                return false;
            };
            let mut large = [0u8; 8];
            large.copy_from_slice(raw);
            let large = u64::from_be_bytes(large);
            large >= 16 && start as u64 + large <= data.len() as u64
        }
        _ => size >= 8 && start as u64 + size as u64 <= data.len() as u64,
    }
}

impl InitSegmentProbe {
    /// Brands declared on this init (major + compatible).
    pub fn all_brands(&self) -> Vec<&str> {
        let mut out = Vec::new();
        if let Some(m) = &self.major_brand {
            out.push(m.as_str());
        }
        for b in &self.compatible_brands {
            out.push(b.as_str());
        }
        out
    }

    /// HLS requires fMP4 init brands compatible with `iso6` or higher / CMAF.
    pub fn has_iso6_compatible_brand(&self) -> bool {
        self.all_brands().iter().any(|b| {
            let b = b.to_ascii_lowercase();
            matches!(
                b.as_str(),
                "iso6" | "iso7" | "iso8" | "iso9" | "cmfc" | "cmfs" | "cfsd" | "msdh" | "msix"
            )
        })
    }

    pub fn looks_like_fmp4_init(&self) -> bool {
        self.major_brand.is_some()
            || self.video_sample_fourcc.is_some()
            || self.audio_sample_fourcc.is_some()
            || !self.compatible_brands.is_empty()
    }
}

/// Which video codec's NAL syntax should be trusted while scanning a segment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VideoCodecHint {
    /// Codec unknown — both H.264 and HEVC NAL types are checked, which can
    /// over-count (an H.264 SPS header reads as an HEVC IDR NAL type).
    #[default]
    Unknown,
    Avc,
    Hevc,
}

impl VideoCodecHint {
    /// Derive from a CODECS token or an fMP4 sample-entry fourCC.
    pub fn from_codec_str(s: &str) -> Self {
        let s = s.trim().to_ascii_lowercase();
        if s.starts_with("avc1") || s.starts_with("avc3") {
            Self::Avc
        } else if s.starts_with("hvc1")
            || s.starts_with("hev1")
            || s.starts_with("dvh1")
            || s.starts_with("dvhe")
        {
            Self::Hevc
        } else {
            Self::Unknown
        }
    }

    fn checks_avc(self) -> bool {
        matches!(self, Self::Avc | Self::Unknown)
    }

    fn checks_hevc(self) -> bool {
        matches!(self, Self::Hevc | Self::Unknown)
    }
}

/// Best-effort scan of a media segment for Author Phase B/C flags.
pub fn scan_segment_bytes(data: &[u8], codec: VideoCodecHint) -> SegmentScan {
    use std::collections::HashMap;

    let mut scan = SegmentScan::default();

    // MPEG-TS path
    if data.len() >= 188 && data[0] == 0x47 {
        scan.looks_like_ts = true;
        let mut last_cc: HashMap<u16, u8> = HashMap::new();
        let mut cc_ok = true;
        let mut offset = 0usize;
        while offset + 188 <= data.len() {
            if data[offset] != 0x47 {
                offset += 1;
                continue;
            }
            let pid = (((data[offset + 1] as u16) & 0x1f) << 8) | data[offset + 2] as u16;
            let adaptation = (data[offset + 3] >> 4) & 0x3;
            let cc = data[offset + 3] & 0x0f;
            // Continuity only increments when payload is present (adaptation != 0b10)
            if adaptation != 0b10 && pid != 0x1fff {
                if let Some(prev) = last_cc.get(&pid).copied() {
                    let expect = (prev + 1) & 0x0f;
                    if cc != expect {
                        cc_ok = false;
                    }
                }
                last_cc.insert(pid, cc);
            }
            offset += 188;
        }
        scan.ts_continuity_ok = Some(cc_ok);
        scan_nal_hints(data, &mut scan, codec);
        return scan;
    }

    // ISOBMFF: size-based box walk with recursion into containers that hold tfdt/mdat.
    walk_boxes(data, 0, data.len(), &mut scan, 0, codec);

    if scan.looks_like_fmp4 && !scan.has_idr_nal_hint {
        scan_nal_hints(data, &mut scan, codec);
    }

    scan
}

fn walk_boxes(
    data: &[u8],
    start: usize,
    end: usize,
    scan: &mut SegmentScan,
    depth: usize,
    codec: VideoCodecHint,
) {
    if depth > 12 {
        return;
    }
    let mut i = start;
    while i + 8 <= end {
        let mut size =
            u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let typ = &data[i + 4..i + 8];
        let mut header = 8usize;
        if size == 1 {
            // 64-bit largesize
            if i + 16 > end {
                break;
            }
            size = u64::from_be_bytes([
                data[i + 8],
                data[i + 9],
                data[i + 10],
                data[i + 11],
                data[i + 12],
                data[i + 13],
                data[i + 14],
                data[i + 15],
            ]) as usize;
            header = 16;
        } else if size == 0 {
            size = end.saturating_sub(i);
        }
        if size < header || i + size > end {
            break;
        }
        let body_start = i + header;
        let body_end = i + size;

        match typ {
            b"ftyp" | b"moov" | b"sidx" => scan.looks_like_fmp4 = true,
            b"moof" => {
                scan.looks_like_fmp4 = true;
                scan.has_moof = true;
                walk_boxes(data, body_start, body_end, scan, depth + 1, codec);
            }
            b"traf" | b"trak" | b"mdia" | b"minf" | b"stbl" => {
                walk_boxes(data, body_start, body_end, scan, depth + 1, codec);
            }
            b"mdat" => {
                scan.looks_like_fmp4 = true;
                scan_nal_hints(&data[body_start..body_end], scan, codec);
            }
            b"senc" => scan.has_senc = true,
            b"saiz" => scan.has_saiz = true,
            b"saio" => scan.has_saio = true,
            b"tfdt" => {
                scan.has_tfdt = true;
                scan.looks_like_fmp4 = true;
                if scan.tfdt_base_media_decode_time.is_none() && body_start + 8 <= body_end {
                    let version = data[body_start];
                    if version == 1 && body_start + 12 <= body_end {
                        scan.tfdt_base_media_decode_time = Some(u64::from_be_bytes([
                            data[body_start + 4],
                            data[body_start + 5],
                            data[body_start + 6],
                            data[body_start + 7],
                            data[body_start + 8],
                            data[body_start + 9],
                            data[body_start + 10],
                            data[body_start + 11],
                        ]));
                    } else if body_start + 8 <= body_end {
                        scan.tfdt_base_media_decode_time = Some(u32::from_be_bytes([
                            data[body_start + 4],
                            data[body_start + 5],
                            data[body_start + 6],
                            data[body_start + 7],
                        ]) as u64);
                    }
                }
            }
            _ => {}
        }

        i += size;
    }
}

/// Soft cap on bytes scanned per payload; only pathological segments are truncated,
/// in which case the IDR count under-reports (§1.13 estimates read as conservative).
const MAX_NAL_SCAN_BYTES: usize = 8 * 1024 * 1024;

fn scan_nal_hints(data: &[u8], scan: &mut SegmentScan, codec: VideoCodecHint) {
    let buf = &data[..data.len().min(MAX_NAL_SCAN_BYTES)];

    // Offsets of the NAL header byte, so the Annex-B and length-prefixed passes
    // agree on a position for the same NAL and can be deduped.
    let mut idr_offsets: Vec<usize> = Vec::new();

    // Annex-B start codes (MPEG-TS, and some fMP4 payloads).
    let mut i = 0usize;
    while i + 4 < buf.len() {
        let (sc_len, nal_off) = if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1 {
            (3usize, i + 3)
        } else if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 0 && buf[i + 3] == 1 {
            (4usize, i + 4)
        } else {
            i += 1;
            continue;
        };
        classify_nal(buf, nal_off, codec, &mut idr_offsets, scan);
        i += sc_len;
    }

    // Length-prefixed NALs (common in fMP4 mdat). The walk must stay aligned from
    // offset 0: a bad length means the payload is not length-prefixed, so we stop
    // instead of resynchronising, which would invent NAL headers in Annex-B/TS data.
    let mut off = 0usize;
    while off + 4 < buf.len() {
        let nalu_len =
            u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as usize;
        if nalu_len == 0 || nalu_len > buf.len() - (off + 4) {
            break;
        }
        classify_nal(buf, off + 4, codec, &mut idr_offsets, scan);
        off += 4 + nalu_len;
    }

    idr_offsets.sort_unstable();
    idr_offsets.dedup();
    if let Some(&first) = idr_offsets.first() {
        scan.idr_count += idr_offsets.len();
        if !scan.idr_at_start {
            scan.idr_at_start = first < 8 * 1024;
        }
    }

    if buf.windows(4).any(|w| w == b"asp ") {
        scan.has_asp_hint = true;
    }
}

/// Inspect the NAL header at `nal_off`, recording IDR offsets and CEA-608/708 SEI hints.
fn classify_nal(
    buf: &[u8],
    nal_off: usize,
    codec: VideoCodecHint,
    idr_offsets: &mut Vec<usize>,
    scan: &mut SegmentScan,
) {
    let Some(&b0) = buf.get(nal_off) else {
        return;
    };
    if codec.checks_avc() {
        match b0 & 0x1f {
            5 => {
                idr_offsets.push(nal_off);
                scan.has_idr_nal_hint = true;
            }
            6 if has_ga94_payload(buf, nal_off) => scan.has_cc_sei_hint = true,
            _ => {}
        }
    }
    if codec.checks_hevc() {
        match (b0 >> 1) & 0x3f {
            19 | 20 => {
                idr_offsets.push(nal_off);
                scan.has_idr_nal_hint = true;
            }
            39 if has_ga94_payload(buf, nal_off) => scan.has_cc_sei_hint = true,
            _ => {}
        }
    }
}

fn has_ga94_payload(buf: &[u8], nal_off: usize) -> bool {
    let end = (nal_off + 64).min(buf.len());
    buf[nal_off..end].windows(4).any(|w| w == b"GA94")
}

#[derive(Debug, Clone, Default)]
pub struct SegmentScan {
    pub looks_like_ts: bool,
    pub looks_like_fmp4: bool,
    pub has_moof: bool,
    pub has_idr_nal_hint: bool,
    /// IDR found near the start of the segment payload.
    pub idr_at_start: bool,
    pub idr_count: usize,
    pub has_tfdt: bool,
    pub tfdt_base_media_decode_time: Option<u64>,
    pub has_senc: bool,
    pub has_saiz: bool,
    pub has_saio: bool,
    /// `Some(false)` when a continuity counter discontinuity was observed in-sample.
    pub ts_continuity_ok: Option<bool>,
    /// Best-effort CEA-608/708 SEI / GA94 hint.
    pub has_cc_sei_hint: bool,
    /// Best-effort APAC ASP marker hint.
    pub has_asp_hint: bool,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_probe() {
        let p = probe_init_segment(&[]);
        assert!(p.major_brand.is_none());
    }

    #[test]
    fn ascii_fourcc_without_a_box_size_is_not_a_box() {
        // The four bytes ahead of "vexu" read as a ~1.7 GB size, so nothing here
        // looks like a box even though the fourCC is present verbatim.
        let p = probe_init_segment(b"free-text-with-vexu-inside-and-tenc-too");
        assert!(!p.has_vexu);
        assert!(!p.has_tenc);
    }

    #[test]
    fn fallback_scan_accepts_a_sized_box() {
        let mut data = vec![0u8; 16];
        data.extend_from_slice(&8u32.to_be_bytes());
        data.extend_from_slice(b"vexu");
        assert!(probe_init_segment(&data).has_vexu);
    }

    #[test]
    fn box_size_must_fit_the_buffer() {
        let mut data = 64u32.to_be_bytes().to_vec();
        data.extend_from_slice(b"vexu");
        // A 64-byte box in a 12-byte buffer is a truncated read at best.
        data.extend_from_slice(&[0u8; 4]);
        assert!(!is_plausible_box_at(&data, 4));
    }

    #[test]
    fn scan_ts_sync() {
        let mut data = vec![0u8; 188];
        data[0] = 0x47;
        let s = scan_segment_bytes(&data, VideoCodecHint::Unknown);
        assert!(s.looks_like_ts);
    }

    /// Length-prefixed NAL of `nal_header` padded to `payload_len` bytes.
    fn length_prefixed_nal(nal_header: u8, payload_len: usize) -> Vec<u8> {
        let mut out = (payload_len as u32).to_be_bytes().to_vec();
        out.push(nal_header);
        out.resize(4 + payload_len, 0);
        out
    }

    fn mdat(payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(b"mdat");
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn idr_count_accumulates_across_mdat_boxes() {
        let mut data = mdat(&length_prefixed_nal(0x65, 32));
        data.extend(mdat(&length_prefixed_nal(0x65, 32)));
        let s = scan_segment_bytes(&data, VideoCodecHint::Avc);
        assert!(s.has_idr_nal_hint);
        assert_eq!(s.idr_count, 2);
    }

    #[test]
    fn avc_sps_is_not_counted_as_idr() {
        // 0x27 is an H.264 SPS, but reads as HEVC NAL type 19 (IDR_W_RADL).
        let data = mdat(&length_prefixed_nal(0x27, 32));
        let avc = scan_segment_bytes(&data, VideoCodecHint::Avc);
        assert!(!avc.has_idr_nal_hint);
        assert_eq!(avc.idr_count, 0);

        let unknown = scan_segment_bytes(&data, VideoCodecHint::Unknown);
        assert!(unknown.has_idr_nal_hint);
    }

    #[test]
    fn annex_b_and_length_prefixed_idr_deduped() {
        // A 4-byte start code also parses as a length prefix of 1, so both passes
        // see the same NAL header and must count it once.
        let mut payload = vec![0u8, 0, 0, 1, 0x65];
        payload.resize(64, 0);
        let s = scan_segment_bytes(&mdat(&payload), VideoCodecHint::Avc);
        assert_eq!(s.idr_count, 1);
    }

    #[test]
    fn codec_hint_from_codec_str() {
        assert_eq!(
            VideoCodecHint::from_codec_str("avc1.640028"),
            VideoCodecHint::Avc
        );
        assert_eq!(
            VideoCodecHint::from_codec_str("hvc1.2.4.L153.B0"),
            VideoCodecHint::Hevc
        );
        assert_eq!(
            VideoCodecHint::from_codec_str("mp4a.40.2"),
            VideoCodecHint::Unknown
        );
    }
}
