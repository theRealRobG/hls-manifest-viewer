//! Platform amendment overlays for the Apple HLS Authoring Spec.

use std::collections::HashSet;

/// Platform profile selected in the Author UI. General rules always apply;
/// amendments replace or add rules for the selected platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthorProfile {
    #[default]
    None,
    Ios,
    Tvos,
    Macos,
    VisionOs,
    AirPlay2,
}

impl AuthorProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Ios => "iOS",
            Self::Tvos => "tvOS",
            Self::Macos => "macOS",
            Self::VisionOs => "visionOS",
            Self::AirPlay2 => "AirPlay2",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "iOS" => Self::Ios,
            "tvOS" => Self::Tvos,
            "macOS" => Self::Macos,
            "visionOS" => Self::VisionOs,
            "AirPlay2" => Self::AirPlay2,
            _ => Self::None,
        }
    }

    #[allow(dead_code)]
    pub const ALL: &[AuthorProfile] = &[
        Self::None,
        Self::Ios,
        Self::Tvos,
        Self::Macos,
        Self::VisionOs,
        Self::AirPlay2,
    ];
}

/// Runtime policy derived from the selected profile + stream characteristics.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AuthorPolicy {
    pub profile: AuthorProfile,
    /// When true (visionOS + all-stereo spatial), listed General `*` rules are N/A.
    pub exempt_general_compat: bool,
    /// §1.20 HDR ≤30 fps: Warn by default; MUST on tvOS.
    pub hdr_30fps_must: bool,
    /// §1.23 overlapping ladder: Warn by default; MUST on AirPlay2.
    pub overlapping_ladder_must: bool,
    /// §6.16 SDR I-frame required: Warn/`*` by default; MUST on tvOS.
    pub sdr_iframe_must: bool,
    /// §8.12 live window: 15 min general; 120 min on tvOS.
    pub live_window_min_s: f64,
    /// Subtitles MUST be WebVTT on AirPlay2 (§5.2).
    pub webvtt_only: bool,
    /// tvOS: no audio-only variants (§9.20+).
    pub forbid_audio_only_variants: bool,
    /// iOS: require ≤192 kbps variant (§9.21).
    pub require_192k_variant: bool,
    /// AirPlay2: full ladders per codec/fps (§9.23–9.24).
    pub require_full_codec_fps_ladders: bool,
    /// AirPlay2: I-frame codecs must match video (§6.13).
    pub iframe_codec_match_must: bool,
    /// macOS H.264 High@L5.0; iOS L6.0; tvOS L5.1 (Phase B uses these).
    pub h264_max_level: Option<&'static str>,
    /// visionOS DV profile amendments apply.
    pub vision_dv_amendments: bool,
}

impl AuthorPolicy {
    pub fn for_profile(profile: AuthorProfile, all_stereo_spatial: bool) -> Self {
        let exempt = profile == AuthorProfile::VisionOs && all_stereo_spatial;
        Self {
            profile,
            exempt_general_compat: exempt,
            hdr_30fps_must: profile == AuthorProfile::Tvos,
            overlapping_ladder_must: profile == AuthorProfile::AirPlay2,
            sdr_iframe_must: profile == AuthorProfile::Tvos,
            live_window_min_s: if profile == AuthorProfile::Tvos {
                120.0 * 60.0
            } else {
                15.0 * 60.0
            },
            webvtt_only: profile == AuthorProfile::AirPlay2,
            forbid_audio_only_variants: profile == AuthorProfile::Tvos,
            require_192k_variant: profile == AuthorProfile::Ios,
            require_full_codec_fps_ladders: profile == AuthorProfile::AirPlay2,
            iframe_codec_match_must: profile == AuthorProfile::AirPlay2,
            h264_max_level: match profile {
                AuthorProfile::Macos => Some("5.0"),
                AuthorProfile::Ios => Some("6.0"),
                AuthorProfile::Tvos => Some("5.1"),
                _ => None,
            },
            vision_dv_amendments: profile == AuthorProfile::VisionOs,
        }
    }

    /// General `*` rules that visionOS exempts when all variants are stereo spatial.
    pub fn vision_exempt_rules() -> HashSet<&'static str> {
        [
            "1.3a", "1.6a", "1.9b", "1.12", "1.24", "2.3", "2.6", "6.14", "6.16",
        ]
        .into_iter()
        .collect()
    }

    pub fn is_exempt(&self, rule: &str) -> bool {
        self.exempt_general_compat && Self::vision_exempt_rules().contains(rule)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tvos_upgrades_hdr_and_window() {
        let p = AuthorPolicy::for_profile(AuthorProfile::Tvos, false);
        assert!(p.hdr_30fps_must);
        assert!(p.sdr_iframe_must);
        assert!((p.live_window_min_s - 7200.0).abs() < f64::EPSILON);
        assert!(p.forbid_audio_only_variants);
    }

    #[test]
    fn visionos_exempts_when_all_stereo() {
        let p = AuthorPolicy::for_profile(AuthorProfile::VisionOs, true);
        assert!(p.exempt_general_compat);
        assert!(p.is_exempt("1.12"));
        assert!(!p.is_exempt("1.19"));
    }

    #[test]
    fn airplay_webvtt_and_ladder() {
        let p = AuthorPolicy::for_profile(AuthorProfile::AirPlay2, false);
        assert!(p.webvtt_only);
        assert!(p.overlapping_ladder_must);
        assert!(p.require_full_codec_fps_ladders);
    }
}
