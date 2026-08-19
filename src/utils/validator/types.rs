//! Validator data model types, shared by the Validate and Author sections.

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

/// How sure a check is of the evidence behind a finding, which is a different question
/// from how serious the finding would be.
///
/// A rule that read a playlist knows what it saw. A rule that measured a handful of
/// sampled segments, or read NAL syntax out of a payload it could not scope to one track,
/// does not — and reporting both as an `ERROR` made the second look as firm as the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Confidence {
    /// Read directly from the playlists, the init segments or the HTTP responses.
    #[default]
    Measured,
    /// Measured, but from a bounded sample of the media rather than all of it.
    Sampled,
    /// Inferred from a heuristic that can be wrong about conforming content.
    Heuristic,
}

impl Confidence {
    /// The most severe a finding at this confidence may be. `Error` is reserved for
    /// evidence read straight from the stream, so a MUST that was only inferred is
    /// reported as a warning rather than failing the run.
    pub fn cap(self, severity: Severity) -> Severity {
        match self {
            Self::Measured => severity,
            Self::Sampled | Self::Heuristic => severity.min(Severity::Warn),
        }
    }

    /// Short label for findings whose evidence is worth qualifying in the UI. `Measured`
    /// is the norm, so it is not worth the pixels.
    pub fn note(self) -> Option<&'static str> {
        match self {
            Self::Measured => None,
            Self::Sampled => Some("sampled evidence"),
            Self::Heuristic => Some("heuristic"),
        }
    }
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::Measured => write!(f, "MEASURED"),
            Confidence::Sampled => write!(f, "SAMPLED"),
            Confidence::Heuristic => write!(f, "HEURISTIC"),
        }
    }
}

/// Which check produced a finding.
///
/// Findings used to be routed to their check group in the UI by searching their message
/// for keywords, so a rule that happened to name `TARGETDURATION` was filed under whichever
/// keyword matched first and rules matching nothing at all were dropped. The producing check
/// names itself here instead, and the grouping is a lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CheckId {
    /// No check claimed this finding. Reserved for [`Issue`] defaults and for the Author
    /// section, which groups its own findings by citation; the Validate UI collects these
    /// into a catch-all group rather than dropping them.
    #[default]
    Unassigned,
    ExtM3uHeader,
    SingletonTags,
    TargetDurationCompliance,
    MediaSequenceTags,
    DiscontinuitySequence,
    PlaylistTypeEndlist,
    EncryptionConsistency,
    DeltaUpdates,
    BandwidthRequired,
    StreamInfConsistency,
    MediaGroupMembership,
    RenditionGroupReferences,
    LivePlaylistWindow,
    PdtCoverage,
    PdtAlignment,
    TargetDurationConsistency,
    CumulativeDrift,
    DurationDrift,
    SegmentCount,
    VersionCompatibility,
    LlHls,
    Interstitials,
    SegmentStructure,
    PlaylistFetch,
    /// §4.4.6.3 — an I-frame Variant Stream and the playlist it points at.
    IFramePlaylists,
    /// §6.2.4 — the Date Ranges of one rendition against the others that carry them.
    DateRangeConsistency,
    /// §4.4.2.3 — EXT-X-DEFINE declarations and whether their values can be resolved.
    VariableDefinitions,
}

impl CheckId {
    /// Every check that can name itself, so the group table can be verified to cover them.
    /// The verification is the only caller, which is why this is not compiled into the app.
    #[cfg(test)]
    pub const ALL: &'static [CheckId] = &[
        CheckId::ExtM3uHeader,
        CheckId::SingletonTags,
        CheckId::TargetDurationCompliance,
        CheckId::MediaSequenceTags,
        CheckId::DiscontinuitySequence,
        CheckId::PlaylistTypeEndlist,
        CheckId::EncryptionConsistency,
        CheckId::DeltaUpdates,
        CheckId::BandwidthRequired,
        CheckId::StreamInfConsistency,
        CheckId::MediaGroupMembership,
        CheckId::RenditionGroupReferences,
        CheckId::LivePlaylistWindow,
        CheckId::PdtCoverage,
        CheckId::PdtAlignment,
        CheckId::TargetDurationConsistency,
        CheckId::CumulativeDrift,
        CheckId::DurationDrift,
        CheckId::SegmentCount,
        CheckId::VersionCompatibility,
        CheckId::LlHls,
        CheckId::Interstitials,
        CheckId::SegmentStructure,
        CheckId::PlaylistFetch,
        CheckId::IFramePlaylists,
        CheckId::DateRangeConsistency,
        CheckId::VariableDefinitions,
    ];
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    /// Which check produced this finding, used to group it in the report.
    pub check_id: CheckId,
    /// How the evidence for this finding was obtained. Findings default to
    /// [`Confidence::Measured`], which is what every RFC 8216bis check reads.
    pub confidence: Confidence,
    pub segment_index: i32,
    pub rendition_a: Option<String>,
    pub rendition_b: Option<String>,
    pub uri_a: Option<String>,
    pub uri_b: Option<String>,
    pub message: String,
    pub uri_note: Option<String>,
    /// How many findings this one stands for. A run of segments that all break the same rule
    /// is reported once, and the report renders the range rather than one row per segment.
    pub count: usize,
    /// First and last segment index of the run, valid when `count` is greater than one.
    pub seg_first: i32,
    pub seg_last: i32,
}

impl Default for Issue {
    fn default() -> Self {
        Self {
            severity: Severity::Warn,
            check_id: CheckId::Unassigned,
            confidence: Confidence::Measured,
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
        Self { severity: Severity::Error, message, ..Default::default() }
    }

    pub fn warn(message: String) -> Self {
        Self { severity: Severity::Warn, message, ..Default::default() }
    }

    /// No Validate check reports at Info, and Author builds its own findings through
    /// `author_issue`, so this exists for the tests that need an Info finding to group.
    #[cfg(test)]
    pub fn info(message: String) -> Self {
        Self { severity: Severity::Info, message, ..Default::default() }
    }

    /// Create an issue with common fields, defaulting consolidation fields
    pub fn new(severity: Severity, segment_index: i32, message: String) -> Self {
        Self { severity, segment_index, message, ..Default::default() }
    }

    /// Name the check that produced this finding, so the report can group it.
    pub fn for_check(mut self, check_id: CheckId) -> Self {
        self.check_id = check_id;
        self
    }

    /// Name the rendition this finding is about.
    pub fn in_rendition(mut self, name: impl Into<String>) -> Self {
        self.rendition_a = Some(name.into());
        self
    }

    /// Name the two renditions a rule compared against each other, which is what lets the
    /// report offer to open both of them in the manifest viewer.
    pub fn between_renditions(mut self, a: impl Into<String>, b: impl Into<String>) -> Self {
        self.rendition_a = Some(a.into());
        self.rendition_b = Some(b.into());
        self
    }

    /// Record what the finding was measured on: the playlist, segment or part the reader
    /// would open to see it for themselves. The report shows it under the message.
    pub fn at_uri(mut self, uri: impl Into<String>) -> Self {
        self.uri_a = Some(uri.into());
        self
    }

    /// Record both sides of a comparison between two segments.
    pub fn between_uris(mut self, a: impl Into<String>, b: impl Into<String>) -> Self {
        self.uri_a = Some(a.into());
        self.uri_b = Some(b.into());
        self
    }

    /// The segment or Media Sequence Number this finding is about. Findings that are about a
    /// playlist as a whole leave this at its default, which the report renders as "Global".
    pub fn at_segment(mut self, index: i32) -> Self {
        self.segment_index = index;
        self
    }

    /// An aside for the report to show under the message, such as the other renditions that
    /// repeat the same finding.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.uri_note = Some(note.into());
        self
    }

    /// Record how the evidence for this finding was obtained, keeping the severity the
    /// rule chose. Rules that have already reasoned about their own sampling use this;
    /// rules that have not should let the severity be capped for them.
    pub fn with_confidence(mut self, confidence: Confidence) -> Self {
        self.confidence = confidence;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub uri: String,
    pub duration: f64,
    /// The EXTINF title: free text that no rule reads, kept so the parser is not silently
    /// discarding part of a tag it otherwise records in full. Author's rules construct
    /// `Segment` too, so this stays where both sections can see it.
    #[allow(dead_code)]
    pub title: Option<String>,
    pub pdt: Option<f64>,
    pub discontinuity: bool,
    pub byterange: Option<String>,
    /// Whether the segment falls inside an ad break. Nothing sets this to `true`: ad breaks are
    /// collected from the Date Ranges rather than marked onto the segments, so no rule may read
    /// it as if it meant anything. Author's rules construct `Segment` with it as well.
    #[allow(dead_code)]
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
    /// How many EXT-X-PROGRAM-DATE-TIME tags the playlist actually carries.
    ///
    /// [`Segment::pdt`] is extrapolated forward from the last tag, so every segment of a
    /// playlist that carries one tag has a PDT; only this count says whether the playlist
    /// declares the tag at all, which is what §6.2.4 and §4.4.5.1 ask about.
    pub program_date_time_tags: usize,
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
    /// EXT-X-DATERANGE tags in playlist order, including the later tags that augment an
    /// earlier one and the IN tags that pair with an interstitial. Every rule that reads Date
    /// Ranges reads these rather than re-scanning [`Self::raw_content`] for the tag.
    pub date_ranges: Vec<DateRange>,
    /// Findings raised while parsing this playlist, e.g. a segment URI whose EXTINF is
    /// missing or unparseable. The parser cannot represent such a segment, so it records
    /// why here instead of dropping the line without a trace.
    pub parse_issues: Vec<Issue>,
}

/// One EXT-X-DATERANGE tag, as the parser read it.
///
/// The attributes are kept as the pairs they were written as rather than as named fields:
/// §4.4.5.1 lets a Date Range carry any client-defined `X-` attribute, and §6.2.4 compares the
/// whole set across renditions, so there is no fixed shape to promote them to.
#[derive(Debug, Clone)]
pub struct DateRange {
    /// The ID attribute. Absent when the tag omits it, which is a finding of its own rather
    /// than a reason for the parser to drop the tag.
    pub id: Option<String>,
    /// Every attribute/value pair of this tag, with the names upper-cased as
    /// [`crate::utils::validator::parser::parse_attributes`] reads them.
    pub attributes: HashMap<String, String>,
    /// The attribute text as written. Two Date Ranges that both omit their ID cannot be told
    /// apart by anything else.
    pub raw_attributes: String,
}

impl DateRange {
    /// Whether this Date Range declares itself an HLS Interstitial (§D.2). The paired IN tag
    /// usually omits CLASS, so this is true of the OUT tag only.
    pub fn is_interstitial(&self) -> bool {
        self.attributes.get("CLASS")
            .is_some_and(|class| class.contains("com.apple.hls.interstitial"))
    }
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
            program_date_time_tags: 0,
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
            date_ranges: Vec::new(),
            parse_issues: Vec::new(),
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
    /// VIDEO attribute of EXT-X-STREAM-INF: the GROUP-ID of an alternative video group.
    pub video_group: Option<String>,
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
    // LL-HLS latency fields
    pub hold_back: Option<f64>,
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
}

/// Playlist Delta Update report entry
#[derive(Debug, Clone)]
pub struct DeltaReport {
    pub name: String,
    pub media_type: String,
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
    /// "PASS", "FAIL", "WARN", "INFO", or "N/A".
    ///
    /// "N/A" says the run held nothing for this check to read — no Multivariant Playlist, no
    /// Low-Latency tags, no Delta Update. Such a check did not pass, it did not run, and
    /// showing it as PASS credits the stream with a rule it was never measured against.
    pub status: String,
    pub issues: Vec<Issue>,
}

/// What one validation run had to work with, so a check with nothing to read can be told
/// apart from a check that read the stream and found nothing wrong.
///
/// Each row of the report table declares which of these it needs; the alternative was to
/// recognise the untested checks by name in the UI, which silently stops working as soon as
/// a check is renamed or a second one becomes conditional.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunInputs {
    /// The stream was reached through a Multivariant Playlist.
    pub has_master: bool,
    /// The Multivariant Playlist declares an I-frame Variant Stream.
    pub has_iframe_variants: bool,
    /// Media Playlists that were fetched and parsed.
    pub playlists: usize,
    /// VIDEO Media Playlists that are not I-frame only: what the drift checks compare.
    pub video_playlists: usize,
    /// Those of them that have ENDLIST, which is all the segment-count check compares.
    pub vod_video_playlists: usize,
    /// A playlist carries an interstitial EXT-X-DATERANGE.
    pub has_interstitials: bool,
    /// A playlist carries EXT-X-PART, EXT-X-PART-INF or EXT-X-SERVER-CONTROL.
    pub has_low_latency_tags: bool,
    /// A Playlist Delta Update was requested from at least one rendition.
    pub delta_probed: bool,
    /// A playlist declares EXT-X-KEY.
    pub has_encryption: bool,
    /// A playlist carries EXT-X-DATERANGE tags.
    pub has_dateranges: bool,
    /// A playlist carries EXT-X-PROGRAM-DATE-TIME tags.
    pub has_pdt_tags: bool,
    /// A playlist declares a variable with EXT-X-DEFINE.
    pub has_defines: bool,
}

impl RunInputs {
    /// Everything available, for tests that only care about how findings are grouped. A run
    /// reads its inputs off the playlists it fetched, with [`Self::from_run`].
    #[cfg(test)]
    pub fn everything() -> Self {
        Self {
            has_master: true,
            has_iframe_variants: true,
            playlists: 2,
            video_playlists: 2,
            vod_video_playlists: 2,
            has_interstitials: true,
            has_low_latency_tags: true,
            delta_probed: true,
            has_encryption: true,
            has_dateranges: true,
            has_pdt_tags: true,
            has_defines: true,
        }
    }

    /// Read off the playlists a run fetched.
    pub fn from_run(
        master: Option<&MasterPlaylist>,
        playlists: &[MediaPlaylist],
        delta_probed: bool,
    ) -> Self {
        let video: Vec<&MediaPlaylist> = playlists.iter()
            .filter(|pl| pl.media_type == "VIDEO" && !pl.is_iframe)
            .collect();
        Self {
            has_master: master.is_some(),
            has_iframe_variants: master.is_some_and(|m| m.variants.iter().any(|v| v.is_iframe)),
            playlists: playlists.len(),
            video_playlists: video.len(),
            vod_video_playlists: video.iter().filter(|pl| pl.has_endlist).count(),
            has_interstitials: playlists.iter()
                .any(|pl| pl.date_ranges.iter().any(DateRange::is_interstitial)),
            has_low_latency_tags: playlists.iter().any(|pl| {
                !pl.parts.is_empty() || pl.part_target.is_some() || pl.server_control.is_some()
            }),
            delta_probed,
            has_encryption: playlists.iter().any(|pl| !pl.encryption_methods.is_empty()),
            has_dateranges: playlists.iter().any(|pl| !pl.date_ranges.is_empty()),
            has_pdt_tags: playlists.iter().any(|pl| pl.program_date_time_tags > 0),
            has_defines: master.is_some_and(|m| m.raw_content.contains("#EXT-X-DEFINE:"))
                || playlists.iter().any(|pl| pl.raw_content.contains("#EXT-X-DEFINE:")),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_are_measured_unless_a_check_says_otherwise() {
        assert_eq!(Issue::error("boom".to_string()).confidence, Confidence::Measured);
        assert_eq!(Issue::default().confidence, Confidence::Measured);
        assert_eq!(
            Issue::warn("hmm".to_string())
                .with_confidence(Confidence::Heuristic)
                .confidence,
            Confidence::Heuristic
        );
    }

    #[test]
    fn only_measured_evidence_can_carry_an_error() {
        assert_eq!(Confidence::Measured.cap(Severity::Error), Severity::Error);
        assert_eq!(Confidence::Sampled.cap(Severity::Error), Severity::Warn);
        assert_eq!(Confidence::Heuristic.cap(Severity::Error), Severity::Warn);
        // Capping is a ceiling, so it never promotes a lesser finding.
        assert_eq!(Confidence::Heuristic.cap(Severity::Info), Severity::Info);
    }

    #[test]
    fn the_builders_fill_in_what_a_finding_names() {
        let issue = Issue::error("drift".to_string())
            .for_check(CheckId::DurationDrift)
            .between_renditions("v-hi", "v-lo")
            .between_uris("hi/s12.m4s", "lo/s12.m4s")
            .at_segment(12)
            .with_note("seen in 3 renditions");
        assert_eq!(issue.severity, Severity::Error);
        assert_eq!(issue.check_id, CheckId::DurationDrift);
        assert_eq!(issue.rendition_a.as_deref(), Some("v-hi"));
        assert_eq!(issue.rendition_b.as_deref(), Some("v-lo"));
        assert_eq!(issue.uri_a.as_deref(), Some("hi/s12.m4s"));
        assert_eq!(issue.uri_b.as_deref(), Some("lo/s12.m4s"));
        assert_eq!(issue.segment_index, 12);
        assert_eq!(issue.uri_note.as_deref(), Some("seen in 3 renditions"));

        // What a finding does not name keeps the defaults the report reads as "not said".
        let plain = Issue::warn("nothing else to say".to_string()).in_rendition("v").at_uri("v.m3u8");
        assert_eq!(plain.rendition_a.as_deref(), Some("v"));
        assert_eq!(plain.uri_a.as_deref(), Some("v.m3u8"));
        assert_eq!((plain.rendition_b, plain.uri_b, plain.uri_note), (None, None, None));
        assert_eq!((plain.segment_index, plain.count), (-1, 1));
    }

    #[test]
    fn measured_findings_need_no_qualifier() {
        assert_eq!(Confidence::Measured.note(), None);
        assert!(Confidence::Sampled.note().is_some());
        assert!(Confidence::Heuristic.note().is_some());
    }
}
