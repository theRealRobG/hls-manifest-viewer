//! Shared context passed into Author rule modules.

use crate::utils::mp4_probe::InitSegmentProbe;
use crate::utils::validator::types::{MasterPlaylist, MediaPlaylist, PlaylistHttpMeta};

use super::profile::{AuthorPolicy, AuthorProfile};

/// Options controlling Author validation.
#[derive(Debug, Clone)]
pub struct ValidateAuthorOptions {
    pub profile: AuthorProfile,
    /// When true, fetch a bounded sample of media segments for measured bitrate / bitstream checks.
    pub deep_checks: bool,
}

impl Default for ValidateAuthorOptions {
    fn default() -> Self {
        Self {
            profile: AuthorProfile::None,
            deep_checks: false,
        }
    }
}

/// Probe result keyed by init URI (+ optional byterange).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct InitProbeEntry {
    pub uri: String,
    pub byterange: Option<String>,
    pub probe: InitSegmentProbe,
}

/// Measured segment sample for Phase C deep checks.
#[derive(Debug, Clone)]
pub struct SegmentSample {
    pub playlist_name: String,
    pub segment_index: usize,
    pub uri: String,
    pub extinf_s: f64,
    pub bytes: usize,
    /// Parsed flags from the segment body (best-effort).
    pub looks_like_ts: bool,
    pub looks_like_fmp4: bool,
    pub has_idr_nal_hint: bool,
    pub has_tfdt: bool,
    pub has_senc: bool,
    pub has_saiz: bool,
    pub has_saio: bool,
}

/// Full input for `run_authoring_checks`.
#[derive(Debug, Clone)]
pub struct AuthoringContext<'a> {
    pub master: Option<&'a MasterPlaylist>,
    pub playlists: &'a [MediaPlaylist],
    pub master_http: Option<&'a PlaylistHttpMeta>,
    pub policy: AuthorPolicy,
    pub deep_checks: bool,
    pub init_probes: &'a [InitProbeEntry],
    pub segment_samples: &'a [SegmentSample],
    pub probe_notes: Vec<String>,
}

impl<'a> AuthoringContext<'a> {
    pub fn new(
        master: Option<&'a MasterPlaylist>,
        playlists: &'a [MediaPlaylist],
        options: &ValidateAuthorOptions,
        init_probes: &'a [InitProbeEntry],
        segment_samples: &'a [SegmentSample],
    ) -> Self {
        let all_stereo = master
            .map(|m| {
                let videos: Vec<_> = m.variants.iter().filter(|v| !v.is_iframe).collect();
                !videos.is_empty()
                    && videos.iter().all(|v| {
                        v.req_video_layout
                            .as_deref()
                            .is_some_and(|l| l.to_ascii_lowercase().contains("stereo"))
                    })
            })
            .unwrap_or(false);
        let mut notes = Vec::new();
        notes.push(
            "TLS cipher/certificate validation (Apple Authoring Spec §12.1–12.3) is not available in-browser."
                .to_string(),
        );
        if !options.deep_checks {
            notes.push(
                "Deep Author checks (segment sampling) are off — enable to measure bandwidth / IDR / continuity."
                    .to_string(),
            );
        }
        Self {
            master,
            playlists,
            master_http: master.map(|m| &m.http_meta),
            policy: AuthorPolicy::for_profile(options.profile, all_stereo),
            deep_checks: options.deep_checks,
            init_probes,
            segment_samples,
            probe_notes: notes,
        }
    }

    pub fn video_playlists(&self) -> impl Iterator<Item = &MediaPlaylist> {
        self.playlists
            .iter()
            .filter(|p| p.media_type == "VIDEO" && !p.is_iframe)
    }

    pub fn iframe_playlists(&self) -> impl Iterator<Item = &MediaPlaylist> {
        self.playlists.iter().filter(|p| p.is_iframe)
    }

    pub fn audio_playlists(&self) -> impl Iterator<Item = &MediaPlaylist> {
        self.playlists.iter().filter(|p| p.media_type == "AUDIO")
    }
}
