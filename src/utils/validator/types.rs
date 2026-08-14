//! Validator data model types. Some fields are populated for UI/report
//! completeness and are not yet read by every check.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Severity {
    Info,
    #[default]
    Warn,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "INFO"),
            Severity::Warn => write!(f, "WARN"),
            Severity::Error => write!(f, "ERROR"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    pub segment_index: i32,
    pub rendition_a: Option<String>,
    pub rendition_b: Option<String>,
    pub uri_a: Option<String>,
    pub uri_b: Option<String>,
    pub message: String,
    pub uri_note: Option<String>,
    // Consolidation fields
    pub count: usize,
    pub seg_first: i32,
    pub seg_last: i32,
}

impl Default for Issue {
    fn default() -> Self {
        Self {
            severity: Severity::Warn,
            segment_index: -1,
            rendition_a: None,
            rendition_b: None,
            uri_a: None,
            uri_b: None,
            message: String::new(),
            uri_note: None,
            count: 1,
            seg_first: -1,
            seg_last: -1,
        }
    }
}

impl Issue {
    pub fn error(message: String) -> Self {
        Self {
            severity: Severity::Error,
            segment_index: -1,
            rendition_a: None,
            rendition_b: None,
            uri_a: None,
            uri_b: None,
            message,
            uri_note: None,
            count: 1,
            seg_first: -1,
            seg_last: -1,
        }
    }

    pub fn warn(message: String) -> Self {
        Self {
            severity: Severity::Warn,
            segment_index: -1,
            rendition_a: None,
            rendition_b: None,
            uri_a: None,
            uri_b: None,
            message,
            uri_note: None,
            count: 1,
            seg_first: -1,
            seg_last: -1,
        }
    }

    pub fn info(message: String) -> Self {
        Self {
            severity: Severity::Info,
            segment_index: -1,
            rendition_a: None,
            rendition_b: None,
            uri_a: None,
            uri_b: None,
            message,
            uri_note: None,
            count: 1,
            seg_first: -1,
            seg_last: -1,
        }
    }

    /// Create an issue with common fields, defaulting consolidation fields
    pub fn new(severity: Severity, segment_index: i32, message: String) -> Self {
        Self {
            severity,
            segment_index,
            rendition_a: None,
            rendition_b: None,
            uri_a: None,
            uri_b: None,
            message,
            uri_note: None,
            count: 1,
            seg_first: -1,
            seg_last: -1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub uri: String,
    pub duration: f64,
    pub title: Option<String>,
    pub pdt: Option<f64>,
    pub discontinuity: bool,
    pub byterange: Option<String>,
    pub is_ad: bool,
    pub map_uri: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MediaPlaylist {
    pub name: String,
    pub url: String,
    pub raw_content: String,
    pub segments: Vec<Segment>,
    pub target_duration: f64,
    pub media_sequence: u64,
    pub discontinuity_sequence: u64,
    pub has_endlist: bool,
    pub playlist_type: Option<String>,
    pub version: u32,
    pub encryption_methods: HashSet<String>,
    pub skipped_segments: u64,
    // LL-HLS fields
    pub server_control: Option<ServerControl>,
    pub part_target: Option<f64>,
    pub preload_hint_uri: Option<String>,
    pub preload_hint_type: Option<String>,
    pub parts: Vec<PartialSegment>,
    pub rendition_reports: Vec<RenditionReport>,
    /// Variable definitions from EXT-X-DEFINE tags in this playlist
    pub definitions: HashMap<String, String>,
    // Rendition info (from master playlist)
    pub media_type: String,  // "VIDEO" or "AUDIO"
    pub bandwidth: Option<u64>,
    pub average_bandwidth: Option<u64>,
    pub codecs: Option<String>,
    pub resolution: Option<String>,
    pub frame_rate: Option<f64>,
    pub audio_group: Option<String>,
    pub closed_captions: Option<String>,
    pub video_range: Option<String>,
    pub color_info: Option<String>,
    pub group_id: Option<String>,
    pub is_iframe: bool,
    pub independent_segments: bool,
    pub iframes_only: bool,
    /// KEYFORMAT values seen on EXT-X-KEY tags
    pub key_formats: HashSet<String>,
    /// HDCP-LEVEL from STREAM-INF (copied onto video playlists)
    pub hdcp_level: Option<String>,
    /// SCORE from STREAM-INF
    pub score: Option<f64>,
    /// REQ-VIDEO-LAYOUT from STREAM-INF
    pub req_video_layout: Option<String>,
    /// PATHWAY-ID from STREAM-INF / content steering
    pub pathway_id: Option<String>,
    /// HTTP response metadata from the playlist fetch
    pub http_meta: PlaylistHttpMeta,
    /// Distinct EXT-X-MAP tags in playlist order (for init probing).
    pub init_maps: Vec<InitMap>,
}

/// One EXT-X-MAP tag: the URI and BYTERANGE that were written together.
///
/// The URI is stored **raw** (as written in the playlist) because it may contain
/// `{$VAR}` references; callers must substitute EXT-X-DEFINE variables and resolve
/// against the playlist URL before fetching (see
/// [`crate::utils::validator::absolute_fetch_uri`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitMap {
    pub uri: String,
    pub byterange: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ServerControl {
    pub can_skip_until: Option<f64>,
    pub hold_back: Option<f64>,
    pub part_hold_back: Option<f64>,
    pub can_block_reload: bool,
    pub can_skip_dateranges: bool,
}

#[derive(Debug, Clone)]
pub struct PartialSegment {
    pub uri: String,
    pub duration: f64,
    pub independent: bool,
    pub gap: bool,
}

impl MediaPlaylist {
    pub fn new(name: String, url: String) -> Self {
        Self {
            name,
            url,
            raw_content: String::new(),
            segments: Vec::new(),
            target_duration: 0.0,
            media_sequence: 0,
            discontinuity_sequence: 0,
            has_endlist: false,
            playlist_type: None,
            version: 1,
            encryption_methods: HashSet::new(),
            definitions: HashMap::new(),
            skipped_segments: 0,
            server_control: None,
            part_target: None,
            preload_hint_uri: None,
            preload_hint_type: None,
            parts: Vec::new(),
            rendition_reports: Vec::new(),
            media_type: "VIDEO".to_string(),
            bandwidth: None,
            average_bandwidth: None,
            codecs: None,
            resolution: None,
            frame_rate: None,
            audio_group: None,
            closed_captions: None,
            video_range: None,
            color_info: None,
            group_id: None,
            is_iframe: false,
            independent_segments: false,
            iframes_only: false,
            key_formats: HashSet::new(),
            hdcp_level: None,
            score: None,
            req_video_layout: None,
            pathway_id: None,
            http_meta: PlaylistHttpMeta::default(),
            init_maps: Vec::new(),
        }
    }
}

/// HTTP response metadata captured when fetching a playlist.
#[derive(Debug, Clone, Default)]
pub struct PlaylistHttpMeta {
    pub request_url: String,
    pub final_url: String,
    pub content_encoding: Option<String>,
    /// `Content-Length` from the response, which counts the bytes as sent.
    pub content_length: Option<u64>,
    /// Bytes of the playlist text after the browser decoded any content coding. Equal
    /// lengths on both fields mean nothing was decoded, so nothing was compressed.
    pub body_bytes: Option<u64>,
    pub last_modified: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MasterRendition {
    /// Raw URI as written in the playlist (may contain `{$VAR}`). Callers must
    /// substitute DEFINE variables and resolve against the master URL before fetch.
    pub uri: String,
    pub bandwidth: Option<u64>,
    pub average_bandwidth: Option<u64>,
    pub codecs: Option<String>,
    pub resolution: Option<String>,
    pub frame_rate: Option<f64>,
    pub audio_group: Option<String>,
    pub subtitle_group: Option<String>,
    pub closed_captions: Option<String>,
    pub video_range: Option<String>,
    pub is_iframe: bool,
    pub score: Option<f64>,
    pub hdcp_level: Option<String>,
    pub pathway_id: Option<String>,
    pub req_video_layout: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MediaRendition {
    pub media_type: String,
    pub group_id: String,
    pub name: String,
    /// Raw URI as written in the playlist (may contain `{$VAR}`). Callers must
    /// substitute DEFINE variables and resolve against the master URL before fetch.
    pub uri: Option<String>,
    pub language: Option<String>,
    pub is_default: bool,
    pub autoselect: bool,
    pub channels: Option<String>,
    pub characteristics: Option<String>,
    pub forced: bool,
}

#[derive(Debug, Clone)]
pub struct MasterPlaylist {
    pub url: String,
    pub raw_content: String,
    pub version: u32,
    pub variants: Vec<MasterRendition>,
    pub media_renditions: Vec<MediaRendition>,
    /// Variable definitions from EXT-X-DEFINE tags (NAME+VALUE and QUERYPARAM)
    pub definitions: HashMap<String, String>,
    pub independent_segments: bool,
    /// HTTP response metadata from the master playlist fetch
    pub http_meta: PlaylistHttpMeta,
}

/// Rendition info for UI display (combines master + media playlist data)
#[derive(Debug, Clone)]
pub struct Rendition {
    pub name: String,
    pub media_type: String,  // "VIDEO" or "AUDIO"
    pub url: String,
    pub bandwidth: u64,
    pub average_bandwidth: Option<u64>,
    pub resolution: Option<String>,
    pub codecs: Option<String>,
    pub frame_rate: Option<f64>,
    pub closed_captions: Option<String>,
    pub color_info: Option<String>,
    pub group_id: Option<String>,
    pub segment_count: usize,
    pub target_duration: f64,
    pub media_sequence: u64,
    pub discontinuity_sequence: u64,
    // LL-HLS latency fields
    pub hold_back: Option<f64>,
    pub part_target: Option<f64>,
    pub part_hold_back: Option<f64>,
    pub has_parts: bool,
}

/// Interstitial entry parsed from EXT-X-DATERANGE
#[derive(Debug, Clone)]
pub struct Interstitial {
    pub rendition: String,
    pub id: String,
    pub start_date: String,
    pub asset_uri: Option<String>,
    pub asset_list: Option<String>,
    pub resume_offset: Option<f64>,
    pub playout_limit: Option<f64>,
    /// PLANNED-DURATION from the OUT tag — used as fallback when X-PLAYOUT-LIMIT is absent
    pub planned_duration_s: Option<f64>,
    pub snap: Option<String>,
    pub cue: Option<String>,
    pub timeline_style: Option<String>,
    pub errors: Vec<String>,
    /// Offset in seconds from the earliest interstitial (for timeline)
    pub start_offset_s: Option<f64>,
    /// Total content duration for the rendition (for timeline)
    pub content_duration_s: f64,
    /// URL of the primary media playlist this interstitial was found in (for manifest viewer link)
    pub rendition_url: String,
    /// Variable definitions from EXT-X-DEFINE in the source playlist (for URL substitution)
    pub definitions: HashMap<String, String>,
}

/// An SCTE-35 ad break extracted from EXT-X-DATERANGE tags
#[derive(Debug, Clone)]
pub struct AdBreak {
    /// DATERANGE ID (unique per break)
    pub id: String,
    /// ISO 8601 start time from START-DATE
    pub start_date: String,
    /// PLANNED-DURATION value in seconds
    pub planned_duration_s: Option<f64>,
    /// Computed from END-DATE − START-DATE (seconds); present only when the break is closed
    pub actual_duration_s: Option<f64>,
    /// "ad_break" | "frame_ad" | "program" | "other"
    pub break_type: String,
    /// Offset in seconds from first segment PDT in the current window (for timeline)
    pub start_offset_s: Option<f64>,
    /// URL of the media playlist this break was sourced from (for manifest viewer link)
    pub rendition_url: String,
}

/// EXT-X-RENDITION-REPORT entry from an LL-HLS media playlist
#[derive(Debug, Clone)]
pub struct RenditionReport {
    pub uri: String,
    pub last_msn: i64,
    pub last_part: i64,
}

/// Playlist Delta Update report entry
#[derive(Debug, Clone)]
pub struct DeltaReport {
    pub name: String,
    pub media_type: String,
    pub url: String,
    pub delta_url: String,
    pub can_skip_until: f64,
    pub hold_back: f64,
    pub can_block_reload: bool,
    pub full_segment_count: usize,
    pub delta_segment_count: usize,
    pub skipped_segments: usize,
    pub delta_error: Option<String>,
}

/// Grouped check result for the UI table
#[derive(Debug, Clone)]
pub struct CheckGroup {
    pub name: String,
    pub section: String,
    pub reference: String,
    pub status: String,  // "PASS", "FAIL", "WARN"
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub issues: Vec<Issue>,
    pub playlists: Vec<MediaPlaylist>,
    pub master: Option<MasterPlaylist>,
    pub total_errors: usize,
    pub total_warnings: usize,
    pub total_info: usize,
    // Enhanced fields for UI
    pub renditions: Vec<Rendition>,
    pub interstitials: Vec<Interstitial>,
    pub check_groups: Vec<CheckGroup>,
    pub delta_report: Vec<DeltaReport>,
    pub ad_breaks: Vec<AdBreak>,
    pub result: String,  // "PASS" or "FAIL"
    pub elapsed_ms: u64,
    pub tolerance_ms: f64,
    pub master_url: String,
    pub has_interstitials_data: bool,
    pub has_scte35_data: bool,
    /// Duration of the best video playlist window in seconds (PDT-based; for SCTE timeline)
    pub playlist_window_s: f64,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self {
            issues: Vec::new(),
            playlists: Vec::new(),
            master: None,
            total_errors: 0,
            total_warnings: 0,
            total_info: 0,
            renditions: Vec::new(),
            interstitials: Vec::new(),
            check_groups: Vec::new(),
            delta_report: Vec::new(),
            ad_breaks: Vec::new(),
            result: "PASS".to_string(),
            elapsed_ms: 0,
            tolerance_ms: 100.0,
            master_url: String::new(),
            has_interstitials_data: false,
            has_scte35_data: false,
            playlist_window_s: 0.0,
        }
    }

    pub fn finalize(&mut self) {
        self.total_errors = self.issues.iter().filter(|i| i.severity == Severity::Error).count();
        self.total_warnings = self.issues.iter().filter(|i| i.severity == Severity::Warn).count();
        self.total_info = self.issues.iter().filter(|i| i.severity == Severity::Info).count();
        self.result = if self.total_errors > 0 { "FAIL".to_string() } else { "PASS".to_string() };
    }
}
