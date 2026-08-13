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
    /// Playlist names that reference this init (video/audio/iframe).
    pub playlist_names: Vec<String>,
    pub media_types: Vec<String>,
    pub probe: InitSegmentProbe,
}

/// Measured segment sample for Phase B light probes / Phase C deep checks.
#[derive(Debug, Clone)]
pub struct SegmentSample {
    pub playlist_name: String,
    pub segment_index: usize,
    pub uri: String,
    pub extinf_s: f64,
    pub bytes: usize,
    pub is_iframe_playlist: bool,
    /// Parsed flags from the segment body (best-effort).
    pub looks_like_ts: bool,
    pub looks_like_fmp4: bool,
    pub has_moof: bool,
    pub has_idr_nal_hint: bool,
    pub idr_at_start: bool,
    pub idr_count: usize,
    pub has_tfdt: bool,
    pub tfdt_base_media_decode_time: Option<u64>,
    pub has_senc: bool,
    pub has_saiz: bool,
    pub has_saio: bool,
    pub ts_continuity_ok: Option<bool>,
    pub has_cc_sei_hint: bool,
    pub has_asp_hint: bool,
}

impl SegmentSample {
    pub fn from_scan(
        playlist_name: String,
        segment_index: usize,
        uri: String,
        extinf_s: f64,
        bytes: usize,
        is_iframe_playlist: bool,
        scan: &crate::utils::mp4_probe::SegmentScan,
    ) -> Self {
        Self {
            playlist_name,
            segment_index,
            uri,
            extinf_s,
            bytes,
            is_iframe_playlist,
            looks_like_ts: scan.looks_like_ts,
            looks_like_fmp4: scan.looks_like_fmp4,
            has_moof: scan.has_moof,
            has_idr_nal_hint: scan.has_idr_nal_hint,
            idr_at_start: scan.idr_at_start,
            idr_count: scan.idr_count,
            has_tfdt: scan.has_tfdt,
            tfdt_base_media_decode_time: scan.tfdt_base_media_decode_time,
            has_senc: scan.has_senc,
            has_saiz: scan.has_saiz,
            has_saio: scan.has_saio,
            ts_continuity_ok: scan.ts_continuity_ok,
            has_cc_sei_hint: scan.has_cc_sei_hint,
            has_asp_hint: scan.has_asp_hint,
        }
    }
}

/// WebVTT cue file sample for §5.3.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WebVttSample {
    pub playlist_name: String,
    pub uri: String,
    pub has_webvtt_header: bool,
    pub has_x_timestamp_map: bool,
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
    pub webvtt_samples: &'a [WebVttSample],
    pub probe_notes: Vec<String>,
}

impl<'a> AuthoringContext<'a> {
    pub fn new(
        master: Option<&'a MasterPlaylist>,
        playlists: &'a [MediaPlaylist],
        options: &ValidateAuthorOptions,
        init_probes: &'a [InitProbeEntry],
        segment_samples: &'a [SegmentSample],
        webvtt_samples: &'a [WebVttSample],
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
        notes.push(format!(
            "Phase B/C probes: {} unique init(s), {} segment sample(s), {} WebVTT sample(s).",
            init_probes.len(),
            segment_samples.len(),
            webvtt_samples.len(),
        ));
        if !options.deep_checks {
            notes.push(
                "Deep Author checks are off — enable to measure bandwidth, IDR interval, tfdt/TS continuity, and CC SEI."
                    .to_string(),
            );
        } else {
            notes.push(
                "Deep Author checks are on — sampled media segments for measured bitrate and bitstream heuristics."
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
            webvtt_samples,
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

    pub fn probe_for_playlist(&self, name: &str) -> Option<&InitProbeEntry> {
        self.init_probes
            .iter()
            .find(|e| e.playlist_names.iter().any(|n| n == name))
    }
}
