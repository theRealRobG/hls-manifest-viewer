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

/// Best-effort scan of a media segment for Author Phase C flags.
pub fn scan_segment_bytes(data: &[u8]) -> SegmentScan {
    let mut scan = SegmentScan::default();
    if data.len() >= 4 && data[0] == 0x47 {
        scan.looks_like_ts = true;
        // Look for IDR NAL hint in PES (0x00 0x00 0x01 0x65 / 0x25 etc.) — very rough
        for w in data.windows(4) {
            if w[0] == 0 && w[1] == 0 && w[2] == 1 {
                let nal = w[3] & 0x1f;
                if nal == 5 {
                    scan.has_idr_nal_hint = true;
                    break;
                }
            }
            if w[0] == 0 && w[1] == 0 && w[2] == 0 && w.get(3) == Some(&1) {
                // Annex-B start; check next byte if present
            }
        }
        // Also search for 00 00 00 01 65
        for w in data.windows(5) {
            if w[0] == 0 && w[1] == 0 && w[2] == 0 && w[3] == 1 {
                let nal = w[4] & 0x1f;
                if nal == 5 {
                    scan.has_idr_nal_hint = true;
                    break;
                }
            }
        }
    }

    // fMP4: look for moof/ftyp/mdat fourccs
    for w in data.windows(8) {
        let typ = &w[4..8];
        if typ == b"moof" || typ == b"ftyp" || typ == b"mdat" {
            scan.looks_like_fmp4 = true;
        }
        if typ == b"moof" {
            scan.has_moof = true;
        }
        if typ == b"tfdt" {
            scan.has_tfdt = true;
        }
        if typ == b"senc" {
            scan.has_senc = true;
        }
        if typ == b"saiz" {
            scan.has_saiz = true;
        }
        if typ == b"saio" {
            scan.has_saio = true;
        }
    }
    scan
}

#[derive(Debug, Clone, Default)]
pub struct SegmentScan {
    pub looks_like_ts: bool,
    pub looks_like_fmp4: bool,
    pub has_moof: bool,
    pub has_idr_nal_hint: bool,
    pub has_tfdt: bool,
    pub has_senc: bool,
    pub has_saiz: bool,
    pub has_saio: bool,
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
