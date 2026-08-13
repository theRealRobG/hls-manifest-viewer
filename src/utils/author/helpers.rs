//! Shared Author helpers: issue builders and codec classification.

use crate::utils::validator::types::{Issue, Severity};

pub fn author_issue(severity: Severity, section: &str, message: impl Into<String>) -> Issue {
    Issue::new(
        severity,
        -1,
        format!("Apple Authoring Spec §{section}: {}", message.into()),
    )
}

pub fn author_error(section: &str, message: impl Into<String>) -> Issue {
    author_issue(Severity::Error, section, message)
}

pub fn author_warn(section: &str, message: impl Into<String>) -> Issue {
    author_issue(Severity::Warn, section, message)
}

pub fn author_info(section: &str, message: impl Into<String>) -> Issue {
    author_issue(Severity::Info, section, message)
}

/// Split a CODECS attribute into tokens.
pub fn codec_tokens(codecs: &str) -> Vec<&str> {
    codecs
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn video_codec_family(token: &str) -> Option<&'static str> {
    let t = token.to_ascii_lowercase();
    if t.starts_with("avc1") || t.starts_with("avc3") {
        Some("avc")
    } else if t.starts_with("hvc1") || t.starts_with("hev1") {
        Some("hevc")
    } else if t.starts_with("dvh1") || t.starts_with("dvhe") {
        Some("dv")
    } else if t.starts_with("av01") {
        Some("av1")
    } else if t.starts_with("mjpg") || t.starts_with("jpeg") {
        Some("mjpg")
    } else {
        None
    }
}

pub fn prefers_parameter_sets_in_sample_entry(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    t.starts_with("avc3") || t.starts_with("hev1") || t.starts_with("dvhe")
}

pub fn is_aac_lc_family(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    // MPEG-4 AAC object types: 2 = AAC-LC, 5 = HE-AACv1 (SBR), 29 = HE-AACv2
    t.starts_with("mp4a.40.2")
        || t.starts_with("mp4a.40.5")
        || t.starts_with("mp4a.40.29")
        || t == "mp4a.40.2"
}

pub fn is_he_aac(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    t.starts_with("mp4a.40.5") || t.starts_with("mp4a.40.29")
}

pub fn is_ac3(token: &str) -> bool {
    token.eq_ignore_ascii_case("ac-3")
}

pub fn is_ec3(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    t == "ec-3" || t.starts_with("ec+3")
}

pub fn is_apac(token: &str) -> bool {
    token.to_ascii_lowercase().starts_with("apac")
}

pub fn parse_resolution(res: &str) -> Option<(u32, u32)> {
    let (w, h) = res.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

pub fn aspect_ratio(res: &str) -> Option<f64> {
    let (w, h) = parse_resolution(res)?;
    if h == 0 {
        return None;
    }
    Some(w as f64 / h as f64)
}

pub fn is_hdr_range(vr: Option<&str>) -> bool {
    matches!(
        vr.map(|s| s.to_ascii_uppercase()).as_deref(),
        Some("PQ") | Some("HLG")
    )
}

pub fn playlist_duration_s(pl: &crate::utils::validator::types::MediaPlaylist) -> f64 {
    pl.segments.iter().map(|s| s.duration).sum()
}

/// Approximate VOD frame-rate set from §1.18.
pub fn is_recommended_vod_framerate(fps: f64) -> bool {
    const ALLOWED: &[f64] = &[23.976, 24.0, 25.0, 29.97, 30.0, 50.0, 59.94, 60.0];
    ALLOWED.iter().any(|&a| (a - fps).abs() < 0.02)
}

pub fn av1_level_ok(token: &str) -> bool {
    // av01.P.LL.T... where LL is two-digit level*10 (e.g. 32 = 3.2). Max 6.2 → 62.
    let t = token.to_ascii_lowercase();
    if !t.starts_with("av01.") {
        return true;
    }
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() < 3 {
        return true;
    }
    let Ok(level) = parts[2].parse::<u32>() else {
        return true;
    };
    level <= 62
}
