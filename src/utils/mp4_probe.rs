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
    pub has_tenc: bool,
    /// CENC scheme_type from `schm` (e.g. "cenc", "cbcs").
    pub scheme_type: Option<String>,
    /// From `tenc` version ≥1 pattern encryption fields.
    pub crypt_byte_block: Option<u8>,
    pub skip_byte_block: Option<u8>,
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
                info.video_profile = get("profile");
                info.video_level = get("level");
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
    for w in data.windows(8) {
        let typ = &w[4..8];
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

/// Best-effort scan of a media segment for Author Phase B/C flags.
pub fn scan_segment_bytes(data: &[u8]) -> SegmentScan {
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
        scan_nal_hints(data, &mut scan);
        return scan;
    }

    // ISOBMFF: size-based box walk with recursion into containers that hold tfdt/mdat.
    walk_boxes(data, 0, data.len(), &mut scan, 0);

    if scan.looks_like_fmp4 && !scan.has_idr_nal_hint {
        scan_nal_hints(data, &mut scan);
    }

    scan
}

fn walk_boxes(data: &[u8], start: usize, end: usize, scan: &mut SegmentScan, depth: usize) {
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
                walk_boxes(data, body_start, body_end, scan, depth + 1);
            }
            b"traf" | b"trak" | b"mdia" | b"minf" | b"stbl" => {
                walk_boxes(data, body_start, body_end, scan, depth + 1);
            }
            b"mdat" => {
                scan.looks_like_fmp4 = true;
                scan_nal_hints(&data[body_start..body_end], scan);
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

fn scan_nal_hints(data: &[u8], scan: &mut SegmentScan) {
    let early_limit = data.len().min(256 * 1024);
    let early = &data[..early_limit];

    let mut idr_positions: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i + 4 < early.len() {
        let (sc_len, nal_off) = if early[i] == 0 && early[i + 1] == 0 && early[i + 2] == 1 {
            (3usize, i + 3)
        } else if i + 4 < early.len()
            && early[i] == 0
            && early[i + 1] == 0
            && early[i + 2] == 0
            && early[i + 3] == 1
        {
            (4usize, i + 4)
        } else {
            i += 1;
            continue;
        };
        if nal_off >= early.len() {
            break;
        }
        let b0 = early[nal_off];
        let h264_type = b0 & 0x1f;
        if h264_type == 5 {
            idr_positions.push(i);
            scan.has_idr_nal_hint = true;
        }
        if h264_type == 6 {
            let sei_end = (nal_off + 64).min(early.len());
            let sei = &early[nal_off..sei_end];
            if sei.windows(4).any(|w| w == b"GA94") {
                scan.has_cc_sei_hint = true;
            }
        }
        let hevc_type = (b0 >> 1) & 0x3f;
        if hevc_type == 19 || hevc_type == 20 {
            idr_positions.push(i);
            scan.has_idr_nal_hint = true;
        }
        i += sc_len;
    }

    // Length-prefixed NALs (common in fMP4 mdat)
    if !scan.has_idr_nal_hint {
        let mut off = 0usize;
        while off + 4 < early.len() {
            let nalu_len =
                u32::from_be_bytes([early[off], early[off + 1], early[off + 2], early[off + 3]])
                    as usize;
            if nalu_len == 0 || nalu_len > early.len().saturating_sub(off + 4) || nalu_len > 8_000_000
            {
                off += 1;
                continue;
            }
            let nal_off = off + 4;
            let b0 = early[nal_off];
            let h264_type = b0 & 0x1f;
            if h264_type == 5 {
                idr_positions.push(off);
                scan.has_idr_nal_hint = true;
            }
            if h264_type == 6 {
                let sei_end = (nal_off + 64).min(early.len());
                let sei = &early[nal_off..sei_end];
                if sei.windows(4).any(|w| w == b"GA94") {
                    scan.has_cc_sei_hint = true;
                }
            }
            let hevc_type = (b0 >> 1) & 0x3f;
            if hevc_type == 19 || hevc_type == 20 {
                idr_positions.push(off);
                scan.has_idr_nal_hint = true;
            }
            off += 4 + nalu_len;
        }
    }

    if !idr_positions.is_empty() {
        scan.idr_count = scan.idr_count.max(idr_positions.len());
        if !scan.idr_at_start {
            scan.idr_at_start = idr_positions.first().is_some_and(|&p| p < 8 * 1024);
        }
    }

    if early.windows(4).any(|w| w == b"asp ") {
        scan.has_asp_hint = true;
    }
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
    fn scan_ts_sync() {
        let mut data = vec![0u8; 188];
        data[0] = 0x47;
        let s = scan_segment_bytes(&data);
        assert!(s.looks_like_ts);
    }
}
