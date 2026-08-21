//! Inspect Timing: what the browser can honestly say about how long an HLS stream's
//! requests took.
//!
//! Everything here is deliberately conservative. A browser cannot see a connection it did
//! not open, cannot separate a preflight from the request it precedes, and cannot report a
//! true time to first byte unless the response carried `Timing-Allow-Origin`. Where a
//! figure is unavailable this module returns `None` rather than a zero, and where two
//! figures measure different things (the browser's own total versus what the app
//! observed) it keeps both and names which one is authoritative.
//!
//! The module holds the types, the arithmetic and the orchestration. Playlist parsing
//! stays with Inspect and is handed in as [`PlaylistParsers`], so the timing code never
//! grows a second HLS parser of its own.

use crate::utils::author::stride_indices;
use crate::utils::network::{
    MeasuredFetch, MeasuredFetchError, RequestRange, fetch_bytes_measured, fetch_text_measured,
};
use std::collections::HashMap;
use web_sys::AbortSignal;

/// The live edge: reload planning, `_HLS_msn` / `_HLS_part`, and the observation lag.
mod live;

// ── Options and limits ───────────────────────────────────────────────────────

pub const DEFAULT_PLAYLIST_REQUESTS: usize = 4;
pub const DEFAULT_MEDIA_PAIRS: usize = 4;
pub const MIN_PLAYLIST_REQUESTS: usize = 1;
pub const MAX_PLAYLIST_REQUESTS: usize = 20;
pub const MIN_MEDIA_PAIRS: usize = 1;
pub const MAX_MEDIA_PAIRS: usize = 10;

/// Bytes a Timing run may download before it stops starting new pairs. Checked after each
/// pair against what actually arrived, so a stream whose segments are far larger than its
/// BANDWIDTH suggests still cannot run away with the user's connection.
pub const BYTE_BUDGET: u64 = 150 * 1024 * 1024;

/// How much Timing to do. Both counts are clamped, so a pasted or spun value cannot ask
/// for a thousand requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingOptions {
    /// Repeats of the selected media playlist. The multivariant playlist is fetched once
    /// on top of these.
    pub playlist_requests: usize,
    /// Video+audio pairs. Each pair is two concurrent requests, or one for a muxed
    /// rendition.
    pub media_pairs: usize,
    /// Whether the playlist rows are wanted at all.
    pub playlist_timing: bool,
    /// Whether media bytes may be downloaded from a **finished** asset: `EXT-X-ENDLIST`
    /// present, segments strided across the whole duration. This flag never waits at a
    /// live edge, because striding a sliding window would report percentages of an asset
    /// that is still being written.
    pub media_timing: bool,
    /// Whether the **live edge** may be watched: blocking reloads or polling, then the
    /// item that appeared. Costs waits rather than a stride, so it is asked for separately.
    pub live_timing: bool,
}

impl Default for TimingOptions {
    fn default() -> Self {
        Self {
            playlist_requests: DEFAULT_PLAYLIST_REQUESTS,
            media_pairs: DEFAULT_MEDIA_PAIRS,
            playlist_timing: true,
            media_timing: false,
            live_timing: false,
        }
    }
}

impl TimingOptions {
    /// The options as they will actually be run.
    pub fn clamped(self) -> Self {
        Self {
            playlist_requests: clamp_count(
                self.playlist_requests,
                MIN_PLAYLIST_REQUESTS,
                MAX_PLAYLIST_REQUESTS,
            ),
            media_pairs: clamp_count(self.media_pairs, MIN_MEDIA_PAIRS, MAX_MEDIA_PAIRS),
            ..self
        }
    }

    /// Whether either media check was asked for, so the run has a media phase to enter at
    /// all. Which of the two applies cannot be known until a playlist has said whether it
    /// carries `EXT-X-ENDLIST`; that is [`media_phase_kind`]'s job.
    pub fn wants_media_samples(self) -> bool {
        self.media_timing || self.live_timing
    }
}

/// What the media phase of a run does, for one playlist kind and one pair of checks.
///
/// The two media checks measure different things and neither substitutes for the other, so
/// a check that cannot apply to the playlist in hand declines out loud instead of quietly
/// running the other one. `run_timing` matches on this, so the table below is the branch
/// the run takes rather than a description of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPhaseKind {
    /// Neither media check was ticked: the run ends after the playlist rows.
    Skip,
    /// Stride a finished asset. `live_declined` is the both-boxes case, where the live edge
    /// was also asked for and there is no edge to watch.
    Vod { live_declined: bool },
    /// Watch the live edge. `vod_declined` is the both-boxes case, where a VOD stride was
    /// also asked for and this playlist is still being written.
    Live { vod_declined: bool },
    /// VOD sampling on a playlist with no `EXT-X-ENDLIST`. No media bytes are taken.
    DeclineVodOnLive,
    /// Live edge timing on a playlist that has `EXT-X-ENDLIST`. No media bytes are taken.
    DeclineLiveOnVod,
}

/// Which media phase one playlist and one pair of checks add up to.
pub fn media_phase_kind(is_live: bool, media_timing: bool, live_timing: bool) -> MediaPhaseKind {
    match (is_live, media_timing, live_timing) {
        (_, false, false) => MediaPhaseKind::Skip,
        (true, vod_declined, true) => MediaPhaseKind::Live { vod_declined },
        (true, true, false) => MediaPhaseKind::DeclineVodOnLive,
        (false, true, live_declined) => MediaPhaseKind::Vod { live_declined },
        (false, false, true) => MediaPhaseKind::DeclineLiveOnVod,
    }
}

pub fn clamp_count(value: usize, min: usize, max: usize) -> usize {
    value.clamp(min, max)
}

// ── Clock ────────────────────────────────────────────────────────────────────

/// There is no monotonic clock, so there is nothing to measure with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockUnavailable;

/// The clock every app-observed figure is taken from.
///
/// Deliberately not the `unwrap_or(0.0)` helper Validate uses for its own bookkeeping: a
/// zero there is a harmless default, whereas here it would be published as a duration.
pub fn now_ms() -> Result<f64, ClockUnavailable> {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .ok_or(ClockUnavailable)
}

// ── Measurement primitives ───────────────────────────────────────────────────

/// A duration that is only a duration if it is finite and not negative. A clock that
/// appears to run backwards has measured nothing.
fn duration_ms(value: f64) -> Option<f64> {
    (value.is_finite() && value >= 0.0).then_some(value)
}

/// A duration that additionally has to be positive to mean anything, which is the case
/// whenever both of its operands are zero unless `Timing-Allow-Origin` was sent.
fn positive_duration_ms(value: f64) -> Option<f64> {
    (value.is_finite() && value > 0.0).then_some(value)
}

/// `performance.now()` timestamps taken around one fetch, all from the same clock.
///
/// These are app-observed: the gaps include JS scheduling, promise resolution and — for a
/// byte body — the copy out of the `Uint8Array` into WASM memory. None of them is a
/// network measurement, and `header_ms` in particular is not a time to first byte.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppClockSpan {
    pub request_ms: f64,
    pub headers_ms: f64,
    pub body_end_ms: f64,
}

impl AppClockSpan {
    /// Time from issuing the request to the response headers resolving, as the app saw it.
    pub fn header_ms(&self) -> Option<f64> {
        duration_ms(self.headers_ms - self.request_ms)
    }

    /// Time spent receiving the body after the headers resolved.
    pub fn body_ms(&self) -> Option<f64> {
        duration_ms(self.body_end_ms - self.headers_ms)
    }

    /// The whole fetch as the app saw it.
    pub fn total_ms(&self) -> Option<f64> {
        duration_ms(self.body_end_ms - self.request_ms)
    }
}

/// One Resource Timing entry reduced to the fields Timing uses, in the same milliseconds
/// `performance.now()` reports.
///
/// `response_end - start_time` is available to any origin. Everything from
/// `request_start` through the connect fields is zero unless the response carried
/// `Timing-Allow-Origin`, which is why each accessor can return `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceTimingSample {
    pub start_time: f64,
    pub request_start: f64,
    pub response_start: f64,
    pub response_end: f64,
    pub connect_start: f64,
    pub connect_end: f64,
    pub secure_connection_start: f64,
    pub domain_lookup_start: f64,
    pub domain_lookup_end: f64,
    pub encoded_body_size: f64,
    pub transfer_size: f64,
    pub next_hop_protocol: String,
}

impl ResourceTimingSample {
    /// The authoritative total for this request: what the browser measured, without the
    /// app's scheduling or the WASM copy in it. Available with no `Timing-Allow-Origin`.
    pub fn total_ms(&self) -> Option<f64> {
        positive_duration_ms(self.response_end - self.start_time)
    }

    /// True time to first byte. Needs `Timing-Allow-Origin`: without it both operands are
    /// zero and their difference measures nothing.
    pub fn true_ttfb_ms(&self) -> Option<f64> {
        if self.request_start <= 0.0 || self.response_start <= 0.0 {
            return None;
        }
        positive_duration_ms(self.response_start - self.request_start)
    }

    /// Whether this request opened a connection or reused one.
    ///
    /// The connect fields are zero without `Timing-Allow-Origin`, and a reused connection
    /// reports them equal. Neither case may be presented as proof of a cold start.
    pub fn connection_state(&self) -> ConnectionState {
        if self.connect_start <= 0.0 || self.connect_end <= 0.0 {
            return ConnectionState::Unknown;
        }
        if self.connect_end > self.connect_start {
            ConnectionState::NewConnection
        } else {
            ConnectionState::ReusedConnection
        }
    }

    /// The encoded body size, which is the divisor any throughput figure would have to
    /// name. Zero means the browser did not expose it.
    pub fn encoded_body_bytes(&self) -> Option<u64> {
        (self.encoded_body_size.is_finite() && self.encoded_body_size > 0.0)
            .then_some(self.encoded_body_size as u64)
    }
}

/// What the connect timings, if any, say about the connection this request used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// A connection was opened for this request.
    NewConnection,
    /// An existing connection was reused.
    ReusedConnection,
    /// The browser did not expose the connect timings, so neither can be shown.
    Unknown,
}

impl ConnectionState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NewConnection => "Cold (connection opened)",
            Self::ReusedConnection => "Warm (connection reused)",
            Self::Unknown => "Unknown (no Timing-Allow-Origin)",
        }
    }
}

/// Which clock a total came from. Resource Timing wins whenever it matched an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotalSource {
    ResourceTiming,
    AppObserved,
}

impl TotalSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ResourceTiming => "Resource Timing",
            Self::AppObserved => "app-observed",
        }
    }
}

/// The median of the values present.
///
/// A median, strictly: the middle value, or the midpoint of the two middle values. Four
/// samples do not support a mean or a standard deviation and none is offered.
pub fn median(values: &[f64]) -> Option<f64> {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    Some(if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    })
}

/// A download time as a percentage of the media time it delivers.
///
/// `None` when the denominator is missing or not positive, so an absent EXTINF becomes an
/// empty cell rather than a 0% claim.
pub fn percent_of_duration(download_ms: f64, media_duration_s: f64) -> Option<f64> {
    if media_duration_s <= 0.0 || !media_duration_s.is_finite() {
        return None;
    }
    let download_ms = duration_ms(download_ms)?;
    Some(download_ms / (media_duration_s * 1000.0) * 100.0)
}

// ── Playlist vocabulary ──────────────────────────────────────────────────────

/// One EXT-INF segment as the Timing phase reads it.
///
/// `map_uri` is the EXT-X-MAP in scope for this segment. A GAP segment carries no media
/// and is never fetched.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SegmentEntry {
    pub uri: String,
    pub duration_s: f64,
    pub byterange: Option<RequestRange>,
    pub gap: bool,
    pub msn: Option<u64>,
    /// EXT-X-PROGRAM-DATE-TIME as the playlist wrote it, only for segments that carry the
    /// tag. An authoring clock, never a latency.
    pub program_date_time: Option<String>,
    pub parts: Vec<PartEntry>,
    pub map_uri: Option<String>,
    pub map_byterange: Option<RequestRange>,
}

/// One EXT-X-PART. Live is out of scope, but a VOD playlist may still carry parts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartEntry {
    pub uri: String,
    pub duration_s: f64,
    pub byterange: Option<RequestRange>,
    pub independent: bool,
    pub gap: bool,
}

/// What the Timing phase needs from a multivariant playlist.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MasterSummary {
    pub variants: Vec<VariantSummary>,
    pub audio: Vec<AudioSummary>,
    pub definitions: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VariantSummary {
    pub uri: String,
    /// How the results table names this rendition.
    pub label: String,
    pub bandwidth: Option<u64>,
    pub is_iframe: bool,
    pub audio_group: Option<String>,
    /// True when CODECS names an audio codec, so this variant's segments are muxed and no
    /// separate audio request belongs in the pair.
    pub has_audio_codec: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioSummary {
    pub name: String,
    pub group_id: String,
    /// `None` for a rendition carried inside the video segments.
    pub uri: Option<String>,
    pub is_default: bool,
}

/// What the Timing phase needs from a media playlist.
///
/// The EXT-X-SERVER-CONTROL fields are what decide how a live edge can be watched at all:
/// with `CAN-BLOCK-RELOAD=YES` the origin holds a reload until it has something to
/// publish, and without it there is nothing to do but poll and accept that a URI may have
/// appeared up to one interval before this tab saw it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaSummary {
    pub target_duration_s: Option<f64>,
    pub part_target_s: Option<f64>,
    pub is_live: bool,
    pub can_block_reload: bool,
    /// EXT-X-SERVER-CONTROL HOLD-BACK, in seconds.
    pub hold_back_s: Option<f64>,
    /// EXT-X-SERVER-CONTROL PART-HOLD-BACK, in seconds.
    pub part_hold_back_s: Option<f64>,
    /// The playlist's segments, and — for a live playlist read through Inspect's parser —
    /// the parent segment at the edge whose parts are published but whose URI is not.
    /// That entry has an empty `uri` and is only fetchable through its parts.
    pub segments: Vec<SegmentEntry>,
}

/// Inspect's media-playlist reader. Named because a live run has to carry it into each
/// reload it makes, and a function pointer is the whole of what it needs.
pub type MediaParser = fn(&str, &str, &HashMap<String, String>) -> MediaSummary;

/// Inspect's own playlist parsers, handed in so this module does not grow a second HLS
/// reader that could disagree with the one the report is built from.
#[derive(Clone, Copy)]
pub struct PlaylistParsers {
    pub master: fn(&str, &str) -> MasterSummary,
    pub media: MediaParser,
}

// ── Rendition and sample planning ────────────────────────────────────────────

/// The mid-BANDWIDTH regular variant: a rung a player might actually settle on, rather
/// than the cheapest or the most expensive one.
///
/// I-frame-only variants are excluded — they are trick-play copies with no continuous
/// video and no audio. With an even number of rungs the lower middle is taken, so the
/// choice is reproducible rather than rounding upward into the expensive half.
pub fn mid_bandwidth_variant(variants: &[VariantSummary]) -> Option<&VariantSummary> {
    let mut ladder: Vec<&VariantSummary> = variants.iter().filter(|v| !v.is_iframe).collect();
    if ladder.is_empty() {
        return None;
    }
    ladder.sort_by_key(|v| (v.bandwidth.unwrap_or(0), v.uri.clone()));
    Some(ladder[(ladder.len() - 1) / 2])
}

/// The audio rendition a player would start with: the group's DEFAULT if it has one, and
/// otherwise the first rendition with a URI of its own.
///
/// A variant that names no AUDIO group has no rendition to pair with, so `None` for
/// `group_id` means no pairing rather than "any group": pairing a variant with a
/// rendition from a group it never referenced would time a combination no player makes.
///
/// Renditions with no URI are muxed into the video segments and are not fetched
/// separately.
pub fn default_audio_rendition<'a>(
    audio: &'a [AudioSummary],
    group_id: Option<&str>,
) -> Option<&'a AudioSummary> {
    let group_id = group_id?;
    let with_uri: Vec<&AudioSummary> = audio
        .iter()
        .filter(|a| a.uri.is_some() && a.group_id == group_id)
        .collect();
    with_uri
        .iter()
        .find(|a| a.is_default)
        .or_else(|| with_uri.first())
        .copied()
}

/// Which kind of request a row describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestClass {
    MultivariantPlaylist,
    MediaPlaylist,
    /// A media playlist re-requested at the live edge, which the origin may hold until it
    /// has something to publish. Kept apart from an ordinary repeat because its duration
    /// is a wait on an author rather than a download, and a median over both would
    /// describe neither.
    LivePlaylistReload,
    VideoSegment,
    AudioSegment,
    /// One request carrying both video and audio, so a pair is a single download. Only
    /// claimed where CODECS named an audio codec.
    MuxedSegment,
    /// A segment whose muxing was never established: there was no multivariant playlist
    /// to declare CODECS, so whether the bytes carry audio as well as video is unknown.
    MediaSegment,
}

impl RequestClass {
    pub fn label(&self) -> &'static str {
        match self {
            Self::MultivariantPlaylist => "Multivariant playlist",
            Self::MediaPlaylist => "Media playlist",
            Self::LivePlaylistReload => "Live playlist reload (held)",
            Self::VideoSegment => "Video segment",
            Self::AudioSegment => "Audio segment",
            Self::MuxedSegment => "Muxed segment",
            Self::MediaSegment => "Media segment (muxing unknown)",
        }
    }

    pub fn is_media(&self) -> bool {
        matches!(
            self,
            Self::VideoSegment | Self::AudioSegment | Self::MuxedSegment | Self::MediaSegment
        )
    }
}

/// What a sampled segment of the chosen rendition can honestly be called.
///
/// With no multivariant playlist there is no CODECS to read, so the segments are neither
/// proven muxed nor proven video-only and the class says as much. Where a variant was
/// chosen, CODECS decides: an audio codec means one request carries both halves, and its
/// absence means the audio lives in a rendition of its own.
pub fn media_request_class(chosen: Option<&VariantSummary>) -> RequestClass {
    match chosen {
        None => RequestClass::MediaSegment,
        Some(variant) if variant.has_audio_codec => RequestClass::MuxedSegment,
        Some(_) => RequestClass::VideoSegment,
    }
}

/// The class a media sample takes, and the AUDIO group a second half could come from.
///
/// The group is only offered for a variant whose CODECS proves its segments carry no
/// audio. A muxed variant needs no pair, and an unknown-muxing segment has no variant to
/// have declared a group in the first place.
pub fn sample_class_and_audio_group(
    chosen: Option<&VariantSummary>,
) -> (RequestClass, Option<&str>) {
    let class = media_request_class(chosen);
    let group = match class {
        RequestClass::VideoSegment => chosen.and_then(|v| v.audio_group.as_deref()),
        _ => None,
    };
    (class, group)
}

/// Which media duration a percentage was measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationSource {
    /// EXTINF: what this segment actually carries. The primary denominator.
    Extinf,
    /// EXT-X-PART DURATION.
    PartDuration,
    /// EXT-X-TARGETDURATION: an upper bound on any segment, not this one's length.
    TargetDuration,
    /// EXT-X-PART-INF PART-TARGET.
    PartTarget,
}

impl DurationSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Extinf => "EXTINF",
            Self::PartDuration => "PART DURATION",
            Self::TargetDuration => "TARGETDURATION",
            Self::PartTarget => "PART-TARGET",
        }
    }
}

/// One media request the plan intends to make.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaRequestSpec {
    pub class: RequestClass,
    pub rendition: String,
    pub uri: String,
    pub byterange: Option<RequestRange>,
    /// The media this request carries, and which tag said so. `EXTINF` (or a part's
    /// DURATION) is the primary denominator for the percentage columns.
    pub media_duration_s: Option<f64>,
    pub media_duration_source: Option<DurationSource>,
    /// The secondary denominator: TARGETDURATION, or PART-TARGET for a part.
    pub target_duration_s: Option<f64>,
    pub target_duration_source: Option<DurationSource>,
    pub msn: Option<u64>,
    pub program_date_time: Option<String>,
}

/// One sample: the video and audio a player would have had to have at the same moment.
///
/// Both halves are fetched concurrently, which is what a player does and therefore what
/// the numbers should describe. A muxed rendition has no audio half.
#[derive(Debug, Clone, PartialEq)]
pub struct SamplePair {
    /// 1-based, for the table's sample column.
    pub number: usize,
    pub video: Option<MediaRequestSpec>,
    pub audio: Option<MediaRequestSpec>,
    /// Why a half is missing, when it is.
    pub notes: Vec<String>,
}

/// Segments worth sampling: the ones the playlist says carry media that can be fetched.
///
/// A GAP is dropped here rather than skipped later, so it is never fetched and never
/// silently counted as a sample. An entry with no URI of its own is only samplable through
/// a part, which is the live edge — the orchestrator does not reach it, but the planning
/// rule is the same either way.
fn samplable_indices(segments: &[SegmentEntry]) -> Vec<usize> {
    segments
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.gap && fetchable_target(s).is_some())
        .map(|(i, _)| i)
        .collect()
}

/// The URI this entry can be fetched at: its own, or its first part that carries media.
fn fetchable_target(segment: &SegmentEntry) -> Option<&str> {
    if !segment.uri.is_empty() {
        return Some(&segment.uri);
    }
    segment
        .parts
        .iter()
        .find(|p| !p.gap && !p.uri.is_empty())
        .map(|p| p.uri.as_str())
}

/// Plan `pairs` samples strided across a VOD asset.
///
/// The video rendition drives the stride; the audio rendition is sampled at the same
/// position in its own segment list, which is the closest a playlist-level plan can get to
/// "the audio a player would have needed at that point". Where the audio list is shorter,
/// or the segment there is a GAP, the pair says so and runs video-only.
pub fn plan_pairs(
    video: &MediaRenditionPlan,
    audio: Option<&MediaRenditionPlan>,
    pairs: usize,
) -> Vec<SamplePair> {
    let candidates = samplable_indices(&video.summary.segments);
    if candidates.is_empty() || pairs == 0 {
        return Vec::new();
    }
    let audio_candidates = audio.map(|a| samplable_indices(&a.summary.segments));

    stride_indices(candidates.len(), pairs)
        .into_iter()
        .enumerate()
        .map(|(k, position)| {
            let mut notes = Vec::new();
            let video_spec = media_spec(video, candidates[position]);
            let audio_spec = match (audio, &audio_candidates) {
                (Some(audio), Some(list)) => match list.get(position) {
                    Some(&index) => media_spec(audio, index),
                    None => {
                        notes.push(format!(
                            "Audio rendition \"{}\" has {} samplable segment(s), so pair {} \
                             has no audio half.",
                            audio.rendition,
                            list.len(),
                            k + 1,
                        ));
                        None
                    }
                },
                _ => None,
            };
            SamplePair {
                number: k + 1,
                video: video_spec,
                audio: audio_spec,
                notes,
            }
        })
        .collect()
}

/// One rendition the plan can sample from: its segment list, and how the table names it.
///
/// The playlist URI and its variables are kept alongside because a live run has to
/// re-request that playlist to watch its edge, and has to read the reloads with the same
/// EXT-X-DEFINE substitutions the first read used.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaRenditionPlan {
    pub class: RequestClass,
    pub rendition: String,
    pub playlist_uri: String,
    pub definitions: HashMap<String, String>,
    pub summary: MediaSummary,
}

fn media_spec(plan: &MediaRenditionPlan, index: usize) -> Option<MediaRequestSpec> {
    let segment = plan.summary.segments.get(index)?;
    // A part is only sampled when the segment has no URI of its own to fetch, which is
    // the live edge — out of scope here — so the segment wins whenever it exists.
    let (uri, byterange, duration, duration_source, target, target_source) = if segment
        .uri
        .is_empty()
    {
        let part = segment.parts.iter().find(|p| !p.gap && !p.uri.is_empty())?;
        (
            part.uri.clone(),
            part.byterange,
            Some(part.duration_s),
            Some(DurationSource::PartDuration),
            plan.summary.part_target_s,
            Some(DurationSource::PartTarget),
        )
    } else {
        (
            segment.uri.clone(),
            segment.byterange,
            (segment.duration_s > 0.0).then_some(segment.duration_s),
            Some(DurationSource::Extinf),
            plan.summary.target_duration_s,
            Some(DurationSource::TargetDuration),
        )
    };
    Some(MediaRequestSpec {
        class: plan.class,
        rendition: plan.rendition.clone(),
        uri,
        byterange,
        media_duration_s: duration,
        media_duration_source: duration.and(duration_source),
        target_duration_s: target.filter(|t| *t > 0.0),
        target_duration_source: target_source,
        msn: segment.msn,
        program_date_time: segment.program_date_time.clone(),
    })
}

// ── Results ──────────────────────────────────────────────────────────────────

/// A completed measurement of one request.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub status: u16,
    /// Bytes that arrived in the app. On a compressed response this is more than the
    /// encoded length the server declared.
    pub bytes: u64,
    /// `Content-Length` as declared, which is an encoded length. Resource Timing's
    /// `encodedBodySize` is preferred where it exists; this is the fallback.
    pub declared_length: Option<u64>,
    /// Set only when the response came from somewhere other than the URL requested. The
    /// redirect's cost is inside the measured time and cannot be separated out.
    pub redirected_to: Option<String>,
    pub app: AppClockSpan,
    pub rt: Option<ResourceTimingSample>,
    /// True when a `Range` was requested and the server answered `200` with the whole
    /// resource instead of `206`.
    pub range_ignored: bool,
}

impl Measurement {
    /// The total this measurement is divided by, and where it came from.
    ///
    /// Resource Timing wins whenever an entry matched: it is the browser's own figure and
    /// carries none of the app's scheduling.
    pub fn authoritative_total_ms(&self) -> Option<(f64, TotalSource)> {
        if let Some(rt) = self.rt.as_ref().and_then(ResourceTimingSample::total_ms) {
            return Some((rt, TotalSource::ResourceTiming));
        }
        self.app
            .total_ms()
            .map(|total| (total, TotalSource::AppObserved))
    }
}

/// What became of one planned request.
#[derive(Debug, Clone, PartialEq)]
pub enum RowOutcome {
    Measured(Measurement),
    /// The request did not complete. A failure has no duration to show.
    Failed(String),
    /// The run was stopped before this request finished.
    Cancelled,
    /// The request was never made, and why.
    Skipped(String),
}

/// Classify a measured fetch that did not succeed.
///
/// The one job here is that an abort stays an abort. A cancelled run reporting a network
/// failure would invent an error the network never gave.
pub fn outcome_from_error(error: &MeasuredFetchError) -> RowOutcome {
    if error.is_cancelled() {
        RowOutcome::Cancelled
    } else {
        RowOutcome::Failed(error.to_string())
    }
}

/// One row of the results table.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingRow {
    pub class: RequestClass,
    pub rendition: Option<String>,
    /// "request 2 / 4", "pair 3 / 4" — whichever the row is one of.
    pub sample_label: String,
    pub uri: String,
    pub byterange: Option<RequestRange>,
    pub media_duration_s: Option<f64>,
    pub media_duration_source: Option<DurationSource>,
    pub target_duration_s: Option<f64>,
    pub target_duration_source: Option<DurationSource>,
    /// For a live media row: the time from this tab first seeing this URI in a playlist
    /// body to its bytes being in hand. `None` for VOD, for playlist rows, and wherever
    /// either end of the interval is missing — it is never a zero.
    pub observation_lag_ms: Option<f64>,
    pub outcome: RowOutcome,
}

impl TimingRow {
    pub fn measurement(&self) -> Option<&Measurement> {
        match &self.outcome {
            RowOutcome::Measured(m) => Some(m),
            _ => None,
        }
    }

    pub fn status(&self) -> Option<u16> {
        self.measurement().map(|m| m.status)
    }

    pub fn bytes(&self) -> Option<u64> {
        self.measurement().map(|m| m.bytes)
    }

    /// The bytes that crossed the wire, and what said so.
    ///
    /// `encodedBodySize` is the browser's own count and is preferred. `Content-Length` is
    /// only what the server declared. Either differs from the app's byte count on a
    /// compressed response, which is why a throughput figure has to name its divisor.
    pub fn wire_bytes(&self) -> Option<(u64, &'static str)> {
        let measurement = self.measurement()?;
        if let Some(encoded) = measurement
            .rt
            .as_ref()
            .and_then(ResourceTimingSample::encoded_body_bytes)
        {
            return Some((encoded, "encodedBodySize"));
        }
        measurement
            .declared_length
            .map(|length| (length, "Content-Length"))
    }

    /// Where the response came from, when a redirect took it somewhere else.
    pub fn redirected_to(&self) -> Option<&str> {
        self.measurement()?.redirected_to.as_deref()
    }

    /// App-observed time to the response headers. Not TTFB, and labelled as such wherever
    /// it is shown.
    pub fn app_header_ms(&self) -> Option<f64> {
        self.measurement()?.app.header_ms()
    }

    /// True time to first byte, which exists only when the response allowed it.
    pub fn true_ttfb_ms(&self) -> Option<f64> {
        self.measurement()?.rt.as_ref()?.true_ttfb_ms()
    }

    /// App-observed body/download time.
    pub fn app_body_ms(&self) -> Option<f64> {
        self.measurement()?.app.body_ms()
    }

    /// The browser's own total, which is authoritative when it exists.
    pub fn rt_total_ms(&self) -> Option<f64> {
        self.measurement()?.rt.as_ref()?.total_ms()
    }

    /// The app-observed total, kept beside the authoritative one rather than instead of it.
    pub fn app_total_ms(&self) -> Option<f64> {
        self.measurement()?.app.total_ms()
    }

    /// The total the percentage columns divide, and where it came from.
    pub fn authoritative_total_ms(&self) -> Option<(f64, TotalSource)> {
        self.measurement()?.authoritative_total_ms()
    }

    pub fn connection_state(&self) -> ConnectionState {
        match self.measurement().and_then(|m| m.rt.as_ref()) {
            Some(rt) => rt.connection_state(),
            None => ConnectionState::Unknown,
        }
    }

    /// Download time as a percentage of what this segment or part carries — the primary
    /// ratio, because EXTINF is this segment's own length.
    ///
    /// `None` when a range was ignored: the duration then covers the whole resource while
    /// the denominator covers only the slice asked for, so the ratio would compare two
    /// different things and read as a much slower download than the segment's own.
    pub fn percent_of_media_duration(&self) -> Option<f64> {
        if self.range_ignored() {
            return None;
        }
        let (total, _) = self.authoritative_total_ms()?;
        percent_of_duration(total, self.media_duration_s?)
    }

    /// The same ratio against TARGETDURATION (or PART-TARGET), which bounds any segment
    /// rather than describing this one. Secondary for that reason, and suppressed for an
    /// ignored range for the same reason as above.
    pub fn percent_of_target_duration(&self) -> Option<f64> {
        if self.range_ignored() {
            return None;
        }
        let (total, _) = self.authoritative_total_ms()?;
        percent_of_duration(total, self.target_duration_s?)
    }

    /// The observation lag as a percentage of the target this item is bounded by:
    /// PART-TARGET for a part, TARGETDURATION for a segment.
    ///
    /// Suppressed for an ignored range for the same reason the other ratios are: the lag
    /// then contains the download of the whole resource while the denominator bounds one
    /// slice of it.
    pub fn observation_lag_percent_of_target(&self) -> Option<f64> {
        if self.range_ignored() {
            return None;
        }
        percent_of_duration(self.observation_lag_ms?, self.target_duration_s?)
    }

    pub fn range_ignored(&self) -> bool {
        self.measurement().is_some_and(|m| m.range_ignored)
    }
}

/// A median over the raw rows of one class and rendition. Raw rows stay in the table; this
/// is a summary of them, not a replacement.
#[derive(Debug, Clone, PartialEq)]
pub struct MedianRow {
    pub class: RequestClass,
    pub rendition: Option<String>,
    /// How many rows were measured. Failed and cancelled rows contribute nothing.
    pub samples: usize,
    pub rt_total_median_ms: Option<f64>,
    pub app_total_median_ms: Option<f64>,
}

/// Medians grouped by request class and rendition, in the order the classes first appear.
///
/// Cold and warm samples are not averaged together into one number where the connect
/// timings tell them apart: the rows carry the connection state and the median is stated
/// to be over whatever mix the run got.
pub fn medians(rows: &[TimingRow]) -> Vec<MedianRow> {
    let mut order: Vec<(RequestClass, Option<String>)> = Vec::new();
    for row in rows {
        let key = (row.class, row.rendition.clone());
        if !order.contains(&key) {
            order.push(key);
        }
    }
    order
        .into_iter()
        .map(|(class, rendition)| {
            let group: Vec<&TimingRow> = rows
                .iter()
                .filter(|r| r.class == class && r.rendition == rendition)
                .collect();
            let rt: Vec<f64> = group.iter().filter_map(|r| r.rt_total_ms()).collect();
            let app: Vec<f64> = group.iter().filter_map(|r| r.app_total_ms()).collect();
            MedianRow {
                class,
                rendition,
                samples: group.iter().filter(|r| r.measurement().is_some()).count(),
                rt_total_median_ms: median(&rt),
                app_total_median_ms: median(&app),
            }
        })
        .collect()
}

/// A caveat the results must be read with. Named so that a run can be asked whether it hit
/// a particular one without matching on prose.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingNote {
    pub id: &'static str,
    pub text: String,
}

impl TimingNote {
    fn new(id: &'static str, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
        }
    }
}

pub const NOTE_BYTE_BUDGET: &str = "byte_budget_reached";
/// Live, but nothing could be sampled: no numbered media to wait beyond, or every wait
/// ended without a new URI appearing.
pub const NOTE_LIVE_NOT_MEASURED: &str = "live_lag_not_measured";
/// What the observation lag is and is not, pushed by every run that takes live samples.
pub const NOTE_LIVE_OBSERVATION_BOUND: &str = "live_observation_lag_bound";
/// The origin advertised CAN-BLOCK-RELOAD, so the reloads carried `_HLS_msn` / `_HLS_part`
/// and the playlist rows include the hold.
pub const NOTE_LIVE_BLOCKING_RELOAD: &str = "live_blocking_reload";
/// No CAN-BLOCK-RELOAD, so the edge was polled and an appearance can be seen late.
pub const NOTE_LIVE_POLLED: &str = "live_edge_polled";
/// A wait ran out of time with nothing new on the playlist.
pub const NOTE_LIVE_WAIT_TIMEOUT: &str = "live_wait_timed_out";
/// VOD media sampling was asked for, but the playlist carries no `EXT-X-ENDLIST`.
pub const NOTE_VOD_SAMPLE_ON_LIVE: &str = "vod_sampling_on_live_playlist";
/// Live edge timing was asked for, but the playlist carries `EXT-X-ENDLIST`.
pub const NOTE_LIVE_SAMPLE_ON_VOD: &str = "live_timing_on_vod_playlist";
pub const NOTE_CLOCK_UNAVAILABLE: &str = "clock_unavailable";
pub const NOTE_RANGE_IGNORED: &str = "range_ignored";
pub const NOTE_MUXING_UNKNOWN: &str = "muxing_not_established";
pub const NOTE_NO_AUDIO_RENDITION: &str = "no_audio_rendition_found";

/// The result of one Timing run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimingReport {
    pub rows: Vec<TimingRow>,
    pub notes: Vec<TimingNote>,
    /// True when the run was stopped, so a short table is not read as a complete one.
    pub cancelled: bool,
    /// Set when the run never started, with the reason.
    pub declined: Option<String>,
    /// Bytes actually downloaded, which is what the budget is checked against.
    pub total_bytes: u64,
}

impl TimingReport {
    fn push_note(&mut self, id: &'static str, text: impl Into<String>) {
        let note = TimingNote::new(id, text);
        if !self.notes.contains(&note) {
            self.notes.push(note);
        }
    }
}

/// The caveats every Timing run carries, whatever it measured.
///
/// These are not decoration. Each one names something a browser cannot see, and without
/// them the table reads as a network measurement rather than as what a page observed.
pub fn provenance_notes() -> Vec<&'static str> {
    vec![
        "The Resource Timing total (responseEnd − startTime) is the authoritative duration \
         where it is shown. It needs no Timing-Allow-Origin.",
        "App-observed times come from performance.now() around the fetch. They include JS \
         scheduling and, for media, the copy of the body into WASM memory, so the \
         app-observed header time is not a time to first byte.",
        "A true TTFB (responseStart − requestStart) is shown only when the response \
         carried Timing-Allow-Origin. Otherwise the column is empty rather than zero.",
        "Cold versus warm is proven only by the Resource Timing connect fields, which also \
         need Timing-Allow-Origin. The first Timing sample is not a cold start in any case: \
         Inspect has already fetched these playlists.",
        "Failed and cancelled requests carry no duration.",
        "No cache-busting parameters are added, so a CDN hit and a CDN miss are \
         indistinguishable here.",
        "A range request's preflight cost, if any, sits inside the measured time and \
         cannot be separated from it without Timing-Allow-Origin.",
        "Encrypted media is timed as opaque bytes. No keys are fetched and nothing is \
         decrypted, so no key latency is included.",
        "Content-Length is an encoded length. Any throughput derived from these rows has to \
         name its divisor; encodedBodySize is the one to prefer.",
        "Raw rows and a median only. Four samples do not support a mean or a standard \
         deviation.",
        "A few pairs of one rung are not a measurement of the whole ladder.",
        "Observation lag, on live rows, runs from this tab first seeing a URI in a playlist \
         body to that item's bytes being in hand. It is an upper bound on what the fetch \
         cost — it also contains this tab's own scheduling between reading the playlist and \
         issuing the request — and at the same time a floor on the interval from the item \
         being published to its bytes being in hand, because whatever passed before this tab \
         read the body is outside it.",
        "Observation lag is not the time from a segment being added to the playlist. No \
         browser can see that moment, and EXT-X-PROGRAM-DATE-TIME is an authoring clock, \
         not a latency. Where a blocking reload returned quickly the item was already \
         published before this tab asked for it, and the lag then measures this tab's \
         lateness rather than the origin's.",
        "A blocking reload's playlist row is the origin holding the request until it had \
         something to publish. That hold is not a media download and is not a time to \
         first byte for the part that followed it.",
    ]
}

// ── Progress ─────────────────────────────────────────────────────────────────

/// How far a run has got, for the progress line beside Stop.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingProgress {
    pub done: usize,
    pub total: usize,
    pub label: String,
}

// ── Orchestration ────────────────────────────────────────────────────────────

/// Everything a run needs that is neither an option nor a callback.
pub struct TimingRequest {
    /// The URL the user probed.
    pub url: String,
    pub options: TimingOptions,
    pub parsers: PlaylistParsers,
}

type PlaylistFetch = Result<(MeasuredFetch, String), MeasuredFetchError>;

/// Run the Timing phase.
///
/// The ordering is the point. Playlist repeats are sequential, so each one measures a
/// request instead of competing with its siblings for the connection. The two halves of a
/// media pair go out together, because that is what a player does. Pairs run one after
/// another, so pair 4 is comparable with pair 1.
pub async fn run_timing(
    request: TimingRequest,
    signal: &AbortSignal,
    on_progress: impl Fn(TimingProgress),
) -> TimingReport {
    let mut report = TimingReport::default();

    if now_ms().is_err() {
        report.declined = Some(
            "This browser exposes no performance.now(), so Timing declined to run rather \
             than report zeros."
                .into(),
        );
        report.push_note(
            NOTE_CLOCK_UNAVAILABLE,
            "Timing needs performance.now(). Nothing was measured.",
        );
        return report;
    }

    let options = request.options.clamped();
    // Media sampling has to read a playlist to find a segment, so those requests happen
    // whether or not playlist timing was asked for — but only one of each, and the rows
    // say what they are rather than posing as a repeat study.
    let playlist_requests = if options.playlist_timing {
        options.playlist_requests
    } else {
        1
    };
    if !options.playlist_timing {
        report.push_note(
            "playlist_rows_incidental",
            "Playlist timing was not selected. The playlist rows below are the single requests \
             the media sample plan needed, not a repeat measurement.",
        );
    }
    let mut planned = playlist_requests + 1;

    // ── The playlist the user gave us, once ──────────────────────────────────
    on_progress(TimingProgress {
        done: 0,
        total: planned,
        label: "Fetching the playlist".into(),
    });
    let first = fetch_text_measured(request.url.clone(), Some(signal)).await;
    let first_body = first.as_ref().ok().map(|(_, body)| body.clone());
    let probed_master = first_body
        .as_deref()
        .is_some_and(crate::utils::validator::is_master_playlist);

    let mut master: Option<MasterSummary> = None;
    let mut chosen: Option<VariantSummary> = None;
    let mut media_playlist_uri = request.url.clone();
    // A media-playlist URL has no multivariant request to time, and the row above already
    // measured the playlist the repeats will re-request — so it counts as repeat 1.
    let mut first_repeat = 1;
    let mut first_class = RequestClass::MediaPlaylist;
    let mut first_label = format!("request 1 / {playlist_requests}");

    if let Some(body) = &first_body {
        if probed_master {
            first_class = RequestClass::MultivariantPlaylist;
            first_label = "single request".into();
            let summary = (request.parsers.master)(&request.url, body);
            chosen = mid_bandwidth_variant(&summary.variants).cloned();
            match &chosen {
                Some(variant) => {
                    media_playlist_uri = variant.uri.clone();
                    report.push_note(
                        "rendition_choice",
                        format!(
                            "Media playlist timed: \"{}\"{} — the mid-BANDWIDTH regular variant \
                             of {} rung(s). A few samples of one rung say nothing about the rest \
                             of the ladder.",
                            variant.label,
                            variant
                                .bandwidth
                                .map(|b| format!(" at {b} bps"))
                                .unwrap_or_default(),
                            summary.variants.iter().filter(|v| !v.is_iframe).count(),
                        ),
                    );
                }
                None => report.push_note(
                    "no_regular_variant",
                    "The multivariant playlist declares no regular STREAM-INF variant, so there \
                     is no media playlist to time.",
                ),
            }
            master = Some(summary);
        } else {
            first_repeat = 2;
            report.push_note(
                "media_playlist_url",
                "The URL probed is a media playlist, so there is no multivariant request to time \
                 and no rendition ladder to choose from.",
            );
        }
    }

    let media_label = chosen.as_ref().map(|v| v.label.clone());
    report.total_bytes = report
        .total_bytes
        .saturating_add(measured_bytes(&first).unwrap_or(0));
    report.rows.push(playlist_row(
        first_class,
        if probed_master {
            None
        } else {
            media_label.clone()
        },
        first_label,
        request.url.clone(),
        &first,
    ));
    if run_stopped(&report, signal) {
        return finish(report, signal);
    }
    if first_body.is_none() {
        // Nothing was read, so there is no ladder to choose from and nothing to repeat.
        // Asking the same failing URL another N times would only lengthen the table.
        report.push_note(
            "first_request_failed",
            "The first request did not return a playlist, so nothing further was timed.",
        );
        return finish(report, signal);
    }

    // ── The selected media playlist, N times ─────────────────────────────────
    let mut video_summary: Option<MediaSummary> = None;
    if let (Some(body), false) = (&first_body, probed_master) {
        let definitions = HashMap::new();
        video_summary = Some((request.parsers.media)(&request.url, body, &definitions));
    }

    if probed_master && chosen.is_none() {
        // Nothing to repeat: the ladder gave us no media playlist.
        return finish(report, signal);
    }

    for repeat in first_repeat..=playlist_requests {
        on_progress(TimingProgress {
            done: report.rows.len(),
            total: planned,
            label: format!("Media playlist request {repeat} of {playlist_requests}"),
        });
        let result = fetch_text_measured(media_playlist_uri.clone(), Some(signal)).await;
        report.total_bytes = report
            .total_bytes
            .saturating_add(measured_bytes(&result).unwrap_or(0));
        // A live baseline has to be the newest body this run read. Diffing a later reload
        // against the first repeat would date a URI's appearance to a request that had
        // already carried it, which is the one thing a live measurement must not do.
        let stale_live = video_summary.as_ref().is_some_and(|s| s.is_live);
        if let Ok((_, body)) = &result
            && (video_summary.is_none() || stale_live)
        {
            let definitions = master
                .as_ref()
                .map(|m| m.definitions.clone())
                .unwrap_or_default();
            video_summary = Some((request.parsers.media)(
                &media_playlist_uri,
                body,
                &definitions,
            ));
        }
        report.rows.push(playlist_row(
            RequestClass::MediaPlaylist,
            media_label.clone(),
            format!("request {repeat} / {playlist_requests}"),
            media_playlist_uri.clone(),
            &result,
        ));
        if run_stopped(&report, signal) {
            return finish(report, signal);
        }
    }

    // ── Media samples ────────────────────────────────────────────────────────
    if !options.wants_media_samples() {
        return finish(report, signal);
    }
    let Some(video_summary) = video_summary else {
        report.push_note(
            "no_media_playlist",
            "No media playlist body was read, so no media samples could be planned.",
        );
        return finish(report, signal);
    };
    // Which media check applies is decided here, before anything is fetched for the sample
    // plan: a check that cannot describe this playlist declines without spending a request.
    let phase = media_phase_kind(
        video_summary.is_live,
        options.media_timing,
        options.live_timing,
    );
    match phase {
        MediaPhaseKind::Skip => return finish(report, signal),
        MediaPhaseKind::DeclineVodOnLive => {
            report.push_note(NOTE_VOD_SAMPLE_ON_LIVE, vod_on_live_note(false));
            return finish(report, signal);
        }
        MediaPhaseKind::DeclineLiveOnVod => {
            report.push_note(NOTE_LIVE_SAMPLE_ON_VOD, live_on_vod_note(false));
            return finish(report, signal);
        }
        // Both boxes ticked, one of which this playlist cannot answer. The samples below
        // are of the other kind, and the note says which measurement is missing.
        MediaPhaseKind::Vod { live_declined: true } => {
            report.push_note(NOTE_LIVE_SAMPLE_ON_VOD, live_on_vod_note(true));
        }
        MediaPhaseKind::Live { vod_declined: true } => {
            report.push_note(NOTE_VOD_SAMPLE_ON_LIVE, vod_on_live_note(true));
        }
        MediaPhaseKind::Vod { .. } | MediaPhaseKind::Live { .. } => {}
    }
    // A GAP carries no media, so it is dropped from the sample plan rather than fetched.
    // Said out loud, because a sample plan that quietly moved past a third of the asset
    // would be describing a different asset.
    let gaps = video_summary.segments.iter().filter(|s| s.gap).count();
    if gaps > 0 {
        report.push_note(
            "gap_segments_excluded",
            format!(
                "{gaps} of {} segment(s) are marked EXT-X-GAP and carry no media, so they were \
                 left out of the sample plan and never requested.",
                video_summary.segments.len(),
            ),
        );
    }

    // A variant whose CODECS names an audio codec carries its audio in the same segments,
    // so its pair is a single request. Where there is no multivariant playlist to say,
    // the muxing is simply unknown and the class says so rather than claiming muxed.
    let (media_class, audio_group) = sample_class_and_audio_group(chosen.as_ref());
    let definitions = master
        .as_ref()
        .map(|m| m.definitions.clone())
        .unwrap_or_default();
    let mut video_plan = MediaRenditionPlan {
        class: media_class,
        rendition: media_label.clone().unwrap_or_else(|| "media".into()),
        playlist_uri: media_playlist_uri.clone(),
        definitions: definitions.clone(),
        summary: video_summary,
    };

    // The DEFAULT audio rendition of the video variant's own group, fetched once so its
    // segment list can be sampled alongside the video's.
    let mut audio_plan: Option<MediaRenditionPlan> = None;
    if media_class == RequestClass::VideoSegment
        && let Some(master) = master.as_ref()
        && let Some(rendition) = default_audio_rendition(&master.audio, audio_group)
        && let Some(uri) = rendition.uri.clone()
    {
        planned += 1;
        on_progress(TimingProgress {
            done: report.rows.len(),
            total: planned,
            label: format!("Audio playlist for \"{}\"", rendition.name),
        });
        let result = fetch_text_measured(uri.clone(), Some(signal)).await;
        report.total_bytes = report
            .total_bytes
            .saturating_add(measured_bytes(&result).unwrap_or(0));
        if let Ok((_, body)) = &result {
            audio_plan = Some(MediaRenditionPlan {
                class: RequestClass::AudioSegment,
                rendition: rendition.name.clone(),
                playlist_uri: uri.clone(),
                definitions: master.definitions.clone(),
                summary: (request.parsers.media)(&uri, body, &master.definitions),
            });
        }
        report.rows.push(playlist_row(
            RequestClass::MediaPlaylist,
            Some(rendition.name.clone()),
            "single request".into(),
            uri,
            &result,
        ));
        if run_stopped(&report, signal) {
            return finish(report, signal);
        }
    }

    push_media_plan_notes(&mut report, media_class, audio_plan.is_some());

    // ── Live: watch the edge instead of striding a finished asset ────────────
    if matches!(phase, MediaPhaseKind::Live { .. }) {
        live::run_live_samples(
            &request,
            options,
            &mut report,
            &mut video_plan,
            audio_plan.as_mut(),
            signal,
            &on_progress,
        )
        .await;
        return finish(report, signal);
    }

    let pairs = plan_pairs(&video_plan, audio_plan.as_ref(), options.media_pairs);
    if pairs.is_empty() {
        report.push_note(
            "no_samplable_segments",
            "The media playlist lists no segment that carries media — every entry is a GAP or \
             has no URI — so no sample was downloaded.",
        );
        return finish(report, signal);
    }
    planned = report.rows.len() + pairs.iter().map(SamplePair::request_count).sum::<usize>();

    for pair in &pairs {
        for note in &pair.notes {
            report.push_note("pair_incomplete", note.clone());
        }
        on_progress(TimingProgress {
            done: report.rows.len(),
            total: planned,
            label: format!("Media pair {} of {}", pair.number, pairs.len()),
        });

        let sample_label = format!("pair {} / {}", pair.number, pairs.len());
        // Both halves at once: a player fetches them together, so timing them apart would
        // describe a request pattern no player makes.
        let (video_result, audio_result) = futures::future::join(
            measure_media(pair.video.as_ref(), signal),
            measure_media(pair.audio.as_ref(), signal),
        )
        .await;

        for (spec, result) in [
            (pair.video.as_ref(), video_result),
            (pair.audio.as_ref(), audio_result),
        ] {
            let (Some(spec), Some(result)) = (spec, result) else {
                continue;
            };
            let outcome = match result {
                Ok(measurement) => {
                    report.total_bytes = report.total_bytes.saturating_add(measurement.bytes);
                    if measurement.range_ignored {
                        report.push_note(
                            NOTE_RANGE_IGNORED,
                            format!(
                                "A BYTERANGE request for {} was answered with HTTP {} and the \
                                 whole resource rather than 206, so its bytes and duration cover \
                                 more than the sample asked for.",
                                spec.uri, measurement.status,
                            ),
                        );
                    }
                    RowOutcome::Measured(measurement)
                }
                Err(error) => outcome_from_error(&error),
            };
            report
                .rows
                .push(media_row(spec, sample_label.clone(), outcome, None));
        }

        if run_stopped(&report, signal) {
            return finish(report, signal);
        }
        // Post-hoc, against bytes that actually arrived. A BANDWIDTH × EXTINF figure is a
        // guess, and a guess is not a budget.
        if report.total_bytes > BYTE_BUDGET {
            report.push_note(
                NOTE_BYTE_BUDGET,
                format!(
                    "Stopped after pair {} of {}: {} bytes had already arrived, past the \
                     {BYTE_BUDGET} byte cap.",
                    pair.number,
                    pairs.len(),
                    report.total_bytes,
                ),
            );
            // The pairs that will not now run stay in the table as skipped rows, so the
            // cap is visible as a decision rather than as a table that stops early.
            let total = pairs.len();
            for remaining in pairs.iter().filter(|p| p.number > pair.number) {
                let label = format!("pair {} / {total}", remaining.number);
                for spec in [remaining.video.as_ref(), remaining.audio.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    report.rows.push(media_row(
                        spec,
                        label.clone(),
                        RowOutcome::Skipped("byte cap reached before this pair".into()),
                        None,
                    ));
                }
            }
            break;
        }
    }

    finish(report, signal)
}

impl SamplePair {
    /// How many requests this pair will make. A muxed rendition makes one.
    pub fn request_count(&self) -> usize {
        usize::from(self.video.is_some()) + usize::from(self.audio.is_some())
    }
}

/// What the shape of the media sample plan itself has to be read with.
///
/// Both caveats are about what the playlists did not say: whether the segments carry
/// audio at all, and — where they provably do not — whether an audio rendition was found
/// to pair them with. A pair that is video-only for either reason is not a measurement of
/// what a player would have downloaded.
fn push_media_plan_notes(report: &mut TimingReport, class: RequestClass, has_audio_plan: bool) {
    if class == RequestClass::MediaSegment {
        report.push_note(
            NOTE_MUXING_UNKNOWN,
            "No multivariant playlist was read, so there is no CODECS to say whether these \
             segments carry audio as well as video. They are timed as media segments of \
             unknown muxing, and no separate audio request was made.",
        );
    }
    if class == RequestClass::VideoSegment && !has_audio_plan {
        report.push_note(
            NOTE_NO_AUDIO_RENDITION,
            "The chosen variant's CODECS names no audio codec, but no DEFAULT audio rendition \
             with a URI of its own could be planned from its AUDIO group — either the group \
             names none, or that playlist could not be read. The pairs below are therefore \
             video-only and understate what a player would have had to download.",
        );
    }
}

/// Why a VOD stride cannot describe a playlist that is still being written.
///
/// `took_live_samples` is the both-boxes case, where the note is about the stride that did
/// not happen rather than about a phase that took no bytes at all.
fn vod_on_live_note(took_live_samples: bool) -> String {
    let tail = if took_live_samples {
        "The live edge was watched instead, so the media rows below are edge samples rather \
         than a stride across the asset."
    } else {
        "No media bytes were taken. Select \"Live edge timing\" to measure the edge instead. \
         The playlist rows below stand as measured."
    };
    format!(
        "\"Media sample timing\" samples a finished asset, and this playlist carries no \
         EXT-X-ENDLIST. Striding a sliding window would give percentages of an asset that is \
         still being written, and would time media that was published before the run started. \
         {tail}"
    )
}

/// Why there is no edge to watch on a playlist that has finished publishing.
fn live_on_vod_note(took_vod_samples: bool) -> String {
    let tail = if took_vod_samples {
        "The VOD samples below are a stride across the finished asset, and carry no \
         observation lag."
    } else {
        "No media bytes were taken. Select \"Media sample timing\" for samples of the finished \
         asset. The playlist rows below stand as measured."
    };
    format!(
        "\"Live edge timing\" waits for media to appear, and this playlist carries \
         EXT-X-ENDLIST: every segment it lists is already published, so there is no edge to \
         wait at and no observation lag to report. {tail}"
    )
}

/// Whether the run should stop: the signal fired, or the last row it produced says the
/// request it describes was aborted.
fn run_stopped(report: &TimingReport, signal: &AbortSignal) -> bool {
    was_cancelled(&report.rows, signal.aborted())
}

/// Whether a run ended short of what it planned.
///
/// The signal matters on its own: Stop pressed between two requests aborts the run
/// without any request having been aborted, so a table of nothing but measured rows can
/// still be an incomplete one.
fn was_cancelled(rows: &[TimingRow], aborted: bool) -> bool {
    aborted
        || rows
            .iter()
            .any(|r| matches!(r.outcome, RowOutcome::Cancelled))
}

fn finish(report: TimingReport, signal: &AbortSignal) -> TimingReport {
    finish_with(report, signal.aborted())
}

/// [`finish`] without the browser type, so the bookkeeping can be tested off-target.
fn finish_with(mut report: TimingReport, aborted: bool) -> TimingReport {
    report.cancelled = was_cancelled(&report.rows, aborted);
    if report.cancelled {
        report.push_note(
            "cancelled",
            "The run was stopped, so the rows below are what had completed by then rather than \
             a full set of samples.",
        );
    }
    report
}

fn measured_bytes(result: &PlaylistFetch) -> Option<u64> {
    result.as_ref().ok().map(|(m, _)| m.body_bytes)
}

fn playlist_row(
    class: RequestClass,
    rendition: Option<String>,
    sample_label: String,
    uri: String,
    result: &PlaylistFetch,
) -> TimingRow {
    let outcome = match result {
        Ok((measured, _)) => RowOutcome::Measured(Measurement {
            status: measured.status,
            bytes: measured.body_bytes,
            declared_length: measured.content_length,
            redirected_to: redirect_of(measured, &uri),
            app: measured.app,
            rt: measured.resource_timing.clone(),
            range_ignored: false,
        }),
        Err(e) => outcome_from_error(e),
    };
    TimingRow {
        class,
        rendition,
        sample_label,
        uri,
        byterange: None,
        media_duration_s: None,
        media_duration_source: None,
        target_duration_s: None,
        target_duration_source: None,
        observation_lag_ms: None,
        outcome,
    }
}

/// One row for a planned media request, whatever became of it.
///
/// `observation_lag_ms` is only ever `Some` on a live row, and only where both ends of the
/// interval were read from the same clock.
fn media_row(
    spec: &MediaRequestSpec,
    sample_label: String,
    outcome: RowOutcome,
    observation_lag_ms: Option<f64>,
) -> TimingRow {
    TimingRow {
        class: spec.class,
        rendition: Some(spec.rendition.clone()),
        sample_label,
        uri: spec.uri.clone(),
        byterange: spec.byterange,
        media_duration_s: spec.media_duration_s,
        media_duration_source: spec.media_duration_source,
        target_duration_s: spec.target_duration_s,
        target_duration_source: spec.target_duration_source,
        observation_lag_ms,
        outcome,
    }
}

async fn measure_media(
    spec: Option<&MediaRequestSpec>,
    signal: &AbortSignal,
) -> Option<Result<Measurement, MeasuredFetchError>> {
    let spec = spec?;
    Some(
        fetch_bytes_measured(spec.uri.clone(), spec.byterange, Some(signal))
            .await
            .map(|(measured, bytes)| measurement_from(&measured, bytes.len() as u64, spec)),
    )
}

/// What a measured media fetch amounts to for one planned request.
///
/// A `Range` answered with anything but `206` got the whole resource, and charging that to
/// one segment's sample would overstate both its bytes and its duration — so the row is
/// flagged rather than quietly used.
fn measurement_from(measured: &MeasuredFetch, bytes: u64, spec: &MediaRequestSpec) -> Measurement {
    Measurement {
        status: measured.status,
        bytes,
        declared_length: measured.content_length,
        redirected_to: redirect_of(measured, &spec.uri),
        app: measured.app,
        rt: measured.resource_timing.clone(),
        range_ignored: spec.byterange.is_some() && measured.status != 206,
    }
}

/// Where the response came from, when that is not where it was asked for.
fn redirect_of(measured: &MeasuredFetch, requested: &str) -> Option<String> {
    (!measured.final_url.is_empty() && measured.final_url != requested)
        .then(|| measured.final_url.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::network::FetchError;
    use pretty_assertions::assert_eq;

    fn app(request: f64, headers: f64, end: f64) -> AppClockSpan {
        AppClockSpan {
            request_ms: request,
            headers_ms: headers,
            body_end_ms: end,
        }
    }

    /// A Resource Timing entry as a cross-origin response with no `Timing-Allow-Origin`
    /// produces one: the total is there, everything else is zero.
    fn rt_without_tao(start: f64, end: f64) -> ResourceTimingSample {
        ResourceTimingSample {
            start_time: start,
            response_end: end,
            ..Default::default()
        }
    }

    fn measured_row(
        class: RequestClass,
        app: AppClockSpan,
        rt: Option<ResourceTimingSample>,
    ) -> TimingRow {
        TimingRow {
            class,
            rendition: Some("720p".into()),
            sample_label: "pair 1 / 4".into(),
            uri: "https://example.com/1.m4s".into(),
            byterange: None,
            media_duration_s: None,
            media_duration_source: None,
            target_duration_s: None,
            target_duration_source: None,
            observation_lag_ms: None,
            outcome: RowOutcome::Measured(Measurement {
                status: 200,
                bytes: 1_000,
                declared_length: None,
                redirected_to: None,
                app,
                rt,
                range_ignored: false,
            }),
        }
    }

    fn segment(uri: &str, duration_s: f64) -> SegmentEntry {
        SegmentEntry {
            uri: uri.into(),
            duration_s,
            ..Default::default()
        }
    }

    fn rendition_plan(class: RequestClass, name: &str, segments: Vec<SegmentEntry>) -> MediaRenditionPlan {
        MediaRenditionPlan {
            class,
            rendition: name.into(),
            playlist_uri: format!("https://example.com/hls/{name}.m3u8"),
            definitions: HashMap::new(),
            summary: MediaSummary {
                target_duration_s: Some(6.0),
                segments,
                ..Default::default()
            },
        }
    }

    // ── Timestamp arithmetic ─────────────────────────────────────────────────

    #[test]
    fn app_spans_are_differences_of_one_clock() {
        let span = app(100.0, 140.5, 260.5);
        assert_eq!(span.header_ms(), Some(40.5));
        assert_eq!(span.body_ms(), Some(120.0));
        assert_eq!(span.total_ms(), Some(160.5));
    }

    #[test]
    fn a_clock_that_ran_backwards_measured_nothing() {
        let span = app(500.0, 400.0, 600.0);
        assert_eq!(span.header_ms(), None);
        assert_eq!(span.body_ms(), Some(200.0));
    }

    #[test]
    fn a_zero_length_app_span_is_still_a_measurement() {
        // performance.now() has sub-millisecond resolution, so an identical pair of
        // readings is a very fast fetch rather than a missing one.
        assert_eq!(app(10.0, 10.0, 10.0).total_ms(), Some(0.0));
    }

    #[test]
    fn resource_timing_total_needs_no_timing_allow_origin() {
        let rt = rt_without_tao(1_000.0, 1_120.0);
        assert_eq!(rt.total_ms(), Some(120.0));
        assert_eq!(rt.true_ttfb_ms(), None);
        assert_eq!(rt.connection_state(), ConnectionState::Unknown);
        assert_eq!(rt.encoded_body_bytes(), None);
    }

    #[test]
    fn true_ttfb_only_exists_when_the_response_allowed_it() {
        let rt = ResourceTimingSample {
            start_time: 1_000.0,
            request_start: 1_010.0,
            response_start: 1_045.0,
            response_end: 1_120.0,
            ..Default::default()
        };
        assert_eq!(rt.true_ttfb_ms(), Some(35.0));
    }

    #[test]
    fn a_ttfb_of_zero_is_never_rendered_as_a_duration() {
        // Both operands are zero without Timing-Allow-Origin, and 0 − 0 is not a TTFB.
        let rt = ResourceTimingSample {
            start_time: 5.0,
            response_end: 15.0,
            request_start: 0.0,
            response_start: 0.0,
            ..Default::default()
        };
        assert_eq!(rt.true_ttfb_ms(), None);
        assert_eq!(rt.total_ms(), Some(10.0));
    }

    #[test]
    fn connection_state_is_only_claimed_from_the_connect_fields() {
        let opened = ResourceTimingSample {
            connect_start: 100.0,
            connect_end: 160.0,
            ..Default::default()
        };
        let reused = ResourceTimingSample {
            connect_start: 100.0,
            connect_end: 100.0,
            ..Default::default()
        };
        assert_eq!(opened.connection_state(), ConnectionState::NewConnection);
        assert_eq!(reused.connection_state(), ConnectionState::ReusedConnection);
        assert_eq!(
            ResourceTimingSample::default().connection_state(),
            ConnectionState::Unknown
        );
    }

    // ── Provenance ───────────────────────────────────────────────────────────

    #[test]
    fn resource_timing_is_authoritative_where_it_matched() {
        let row = measured_row(
            RequestClass::VideoSegment,
            app(0.0, 30.0, 200.0),
            Some(rt_without_tao(0.0, 150.0)),
        );
        assert_eq!(
            row.authoritative_total_ms(),
            Some((150.0, TotalSource::ResourceTiming))
        );
        // The app-observed total stays beside it rather than being replaced by it.
        assert_eq!(row.app_total_ms(), Some(200.0));
    }

    #[test]
    fn without_a_matched_entry_the_total_is_app_observed_and_says_so() {
        let row = measured_row(RequestClass::VideoSegment, app(0.0, 30.0, 200.0), None);
        assert_eq!(
            row.authoritative_total_ms(),
            Some((200.0, TotalSource::AppObserved))
        );
        assert_eq!(row.rt_total_ms(), None);
        assert_eq!(row.true_ttfb_ms(), None);
        assert_eq!(row.connection_state(), ConnectionState::Unknown);
    }

    #[test]
    fn a_failed_request_carries_no_duration() {
        let mut row = measured_row(RequestClass::VideoSegment, app(0.0, 30.0, 200.0), None);
        row.outcome = RowOutcome::Failed("NetworkError".into());
        assert_eq!(row.app_total_ms(), None);
        assert_eq!(row.app_header_ms(), None);
        assert_eq!(row.rt_total_ms(), None);
        assert_eq!(row.bytes(), None);
        assert_eq!(row.status(), None);
        assert_eq!(row.percent_of_media_duration(), None);
    }

    #[test]
    fn a_cancelled_request_carries_no_duration_either() {
        let mut row = measured_row(RequestClass::MediaPlaylist, app(0.0, 5.0, 9.0), None);
        row.outcome = RowOutcome::Cancelled;
        assert_eq!(row.app_total_ms(), None);
        assert_eq!(row.authoritative_total_ms(), None);
    }

    // ── Abort classification ─────────────────────────────────────────────────

    #[test]
    fn an_abort_stays_an_abort() {
        assert_eq!(
            outcome_from_error(&MeasuredFetchError::Cancelled),
            RowOutcome::Cancelled
        );
    }

    #[test]
    fn everything_that_is_not_an_abort_is_a_failure_with_a_reason() {
        let http = MeasuredFetchError::HttpStatus {
            status: 404,
            status_text: "Not Found".into(),
        };
        assert_eq!(
            outcome_from_error(&http),
            RowOutcome::Failed("HTTP 404 Not Found".into())
        );
        let failed = MeasuredFetchError::Failed(FetchError {
            error: "TypeError: Failed to fetch".into(),
            extra_info: None,
        });
        assert_eq!(
            outcome_from_error(&failed),
            RowOutcome::Failed("TypeError: Failed to fetch".into())
        );
        assert!(matches!(
            outcome_from_error(&MeasuredFetchError::ClockUnavailable),
            RowOutcome::Failed(_)
        ));
    }

    // ── Median ───────────────────────────────────────────────────────────────

    #[test]
    fn median_is_the_middle_value_or_the_midpoint_of_two() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[7.0]), Some(7.0));
        assert_eq!(median(&[9.0, 1.0, 5.0]), Some(5.0));
        assert_eq!(median(&[10.0, 20.0, 30.0, 40.0]), Some(25.0));
        assert_eq!(median(&[f64::NAN, 4.0, 6.0]), Some(5.0));
        assert_eq!(median(&[f64::NAN]), None);
    }

    #[test]
    fn medians_group_by_class_and_rendition_and_count_only_measured_rows() {
        let mut rows = vec![
            measured_row(
                RequestClass::MediaPlaylist,
                app(0.0, 10.0, 20.0),
                Some(rt_without_tao(0.0, 18.0)),
            ),
            measured_row(
                RequestClass::MediaPlaylist,
                app(0.0, 10.0, 40.0),
                Some(rt_without_tao(0.0, 30.0)),
            ),
            measured_row(RequestClass::VideoSegment, app(0.0, 10.0, 500.0), None),
        ];
        rows[2].outcome = RowOutcome::Cancelled;
        let summary = medians(&rows);
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].class, RequestClass::MediaPlaylist);
        assert_eq!(summary[0].samples, 2);
        assert_eq!(summary[0].rt_total_median_ms, Some(24.0));
        assert_eq!(summary[0].app_total_median_ms, Some(30.0));
        // A cancelled row contributes no sample and no median.
        assert_eq!(summary[1].samples, 0);
        assert_eq!(summary[1].rt_total_median_ms, None);
        assert_eq!(summary[1].app_total_median_ms, None);
    }

    #[test]
    fn a_held_live_reload_is_not_averaged_into_the_media_playlist_median() {
        // The hold is the origin waiting until it had something to publish. Counted with
        // the repeats it would swamp them, and the median would describe neither.
        let rows = vec![
            measured_row(
                RequestClass::MediaPlaylist,
                app(0.0, 10.0, 20.0),
                Some(rt_without_tao(0.0, 20.0)),
            ),
            measured_row(
                RequestClass::MediaPlaylist,
                app(0.0, 10.0, 30.0),
                Some(rt_without_tao(0.0, 30.0)),
            ),
            measured_row(
                RequestClass::LivePlaylistReload,
                app(0.0, 10.0, 4_000.0),
                Some(rt_without_tao(0.0, 4_000.0)),
            ),
        ];
        let summary = medians(&rows);
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].class, RequestClass::MediaPlaylist);
        assert_eq!(summary[0].samples, 2);
        assert_eq!(summary[0].rt_total_median_ms, Some(25.0));
        assert_eq!(summary[1].class, RequestClass::LivePlaylistReload);
        assert_eq!(summary[1].rt_total_median_ms, Some(4_000.0));
        // And a hold is not a download, so it is not counted as media anywhere.
        assert!(!RequestClass::LivePlaylistReload.is_media());
        assert_ne!(
            RequestClass::LivePlaylistReload.label(),
            RequestClass::MediaPlaylist.label()
        );
    }

    // ── Percentage ratios ────────────────────────────────────────────────────

    #[test]
    fn percentages_need_a_positive_denominator() {
        assert_eq!(percent_of_duration(300.0, 6.0), Some(5.0));
        assert_eq!(percent_of_duration(6_000.0, 6.0), Some(100.0));
        assert_eq!(percent_of_duration(300.0, 0.0), None);
        assert_eq!(percent_of_duration(300.0, -6.0), None);
        assert_eq!(percent_of_duration(-1.0, 6.0), None);
    }

    #[test]
    fn extinf_is_the_primary_ratio_and_targetduration_the_secondary() {
        let mut row = measured_row(
            RequestClass::VideoSegment,
            app(0.0, 30.0, 900.0),
            Some(rt_without_tao(0.0, 600.0)),
        );
        row.media_duration_s = Some(4.0);
        row.media_duration_source = Some(DurationSource::Extinf);
        row.target_duration_s = Some(6.0);
        row.target_duration_source = Some(DurationSource::TargetDuration);
        // 600 ms of a 4 s segment, and of a 6 s bound. Both divide the authoritative
        // total, not the app-observed one.
        assert_eq!(row.percent_of_media_duration(), Some(15.0));
        assert_eq!(row.percent_of_target_duration(), Some(10.0));
    }

    #[test]
    fn a_range_the_server_ignored_leaves_both_percentages_empty() {
        let mut row = measured_row(
            RequestClass::VideoSegment,
            app(0.0, 30.0, 900.0),
            Some(rt_without_tao(0.0, 600.0)),
        );
        row.byterange = Some(RequestRange::from_length_with_offset(1_000, 500));
        row.media_duration_s = Some(4.0);
        row.target_duration_s = Some(6.0);
        if let RowOutcome::Measured(m) = &mut row.outcome {
            m.range_ignored = true;
        }
        // The duration covers the whole resource and the denominators cover one slice of
        // it, so neither ratio compares like with like.
        assert!(row.range_ignored());
        assert_eq!(row.percent_of_media_duration(), None);
        assert_eq!(row.percent_of_target_duration(), None);
        // The bytes that arrived are still the bytes that arrived, and are not clamped.
        assert_eq!(row.bytes(), Some(1_000));
    }

    #[test]
    fn a_live_rows_observation_lag_is_read_against_the_target_that_bounds_it() {
        let mut row = measured_row(
            RequestClass::VideoSegment,
            app(0.0, 30.0, 400.0),
            Some(rt_without_tao(0.0, 300.0)),
        );
        // A part row: PART-TARGET is what bounds a part, so it is the denominator.
        row.media_duration_s = Some(1.0);
        row.media_duration_source = Some(DurationSource::PartDuration);
        row.target_duration_s = Some(1.0);
        row.target_duration_source = Some(DurationSource::PartTarget);
        row.observation_lag_ms = Some(350.0);
        assert_eq!(row.observation_lag_percent_of_target(), Some(35.0));
        // The download ratio stays the download's, and is not replaced by the lag.
        assert_eq!(row.percent_of_media_duration(), Some(30.0));
    }

    #[test]
    fn an_ignored_range_leaves_the_observation_lag_percentage_empty_too() {
        let mut row = measured_row(
            RequestClass::VideoSegment,
            app(0.0, 30.0, 400.0),
            Some(rt_without_tao(0.0, 300.0)),
        );
        row.byterange = Some(RequestRange::from_length_with_offset(1_000, 500));
        row.target_duration_s = Some(1.0);
        row.observation_lag_ms = Some(350.0);
        if let RowOutcome::Measured(m) = &mut row.outcome {
            m.range_ignored = true;
        }
        // The lag contains the download of the whole resource while PART-TARGET bounds one
        // slice of it, so the ratio would compare two different things.
        assert_eq!(row.observation_lag_percent_of_target(), None);
        // The lag itself is still what this tab waited, and is not thrown away.
        assert_eq!(row.observation_lag_ms, Some(350.0));
    }

    #[test]
    fn a_row_with_no_observation_lag_has_no_percentage_of_one() {
        let mut row = measured_row(RequestClass::VideoSegment, app(0.0, 30.0, 400.0), None);
        row.target_duration_s = Some(6.0);
        assert_eq!(row.observation_lag_ms, None);
        assert_eq!(row.observation_lag_percent_of_target(), None);
    }

    #[test]
    fn a_missing_extinf_leaves_the_percentage_empty_rather_than_zero() {
        let mut row = measured_row(RequestClass::VideoSegment, app(0.0, 30.0, 900.0), None);
        row.target_duration_s = Some(6.0);
        assert_eq!(row.percent_of_media_duration(), None);
        assert_eq!(row.percent_of_target_duration(), Some(15.0));
    }

    // ── Options ──────────────────────────────────────────────────────────────

    #[test]
    fn counts_are_clamped_to_what_the_run_will_actually_do() {
        let clamped = TimingOptions {
            playlist_requests: 500,
            media_pairs: 0,
            playlist_timing: true,
            media_timing: true,
            live_timing: true,
        }
        .clamped();
        assert_eq!(clamped.playlist_requests, MAX_PLAYLIST_REQUESTS);
        assert_eq!(clamped.media_pairs, MIN_MEDIA_PAIRS);
        // Clamping is about the counts. A flag the user ticked has to survive it.
        assert!(clamped.media_timing);
        assert!(clamped.live_timing);
        let defaults = TimingOptions::default();
        assert_eq!(defaults.playlist_requests, 4);
        assert_eq!(defaults.media_pairs, 4);
    }

    #[test]
    fn neither_kind_of_media_sampling_is_on_by_default() {
        // Both cost bytes or waits, so both are opt-in however the page is reached.
        let defaults = TimingOptions::default();
        assert!(!defaults.media_timing);
        assert!(!defaults.live_timing);
        assert!(!defaults.wants_media_samples());
        assert!(
            TimingOptions { media_timing: true, ..Default::default() }.wants_media_samples()
        );
        assert!(TimingOptions { live_timing: true, ..Default::default() }.wants_media_samples());
    }

    // ── Which media phase a run enters ───────────────────────────────────────

    #[test]
    fn a_vod_playlist_is_strided_only_when_vod_media_sampling_was_asked_for() {
        assert_eq!(
            media_phase_kind(false, true, false),
            MediaPhaseKind::Vod { live_declined: false },
        );
        // Both boxes on a finished asset: the stride runs and the edge is declined, so the
        // report says why there is no observation lag rather than leaving it unexplained.
        assert_eq!(
            media_phase_kind(false, true, true),
            MediaPhaseKind::Vod { live_declined: true },
        );
    }

    #[test]
    fn a_live_playlist_is_watched_only_when_live_edge_timing_was_asked_for() {
        assert_eq!(
            media_phase_kind(true, false, true),
            MediaPhaseKind::Live { vod_declined: false },
        );
        assert_eq!(
            media_phase_kind(true, true, true),
            MediaPhaseKind::Live { vod_declined: true },
        );
    }

    #[test]
    fn vod_media_sampling_on_a_live_playlist_declines_instead_of_watching_the_edge() {
        // The bug this pins: one checkbox for both kinds meant a VOD-labelled check held
        // `_HLS_msn` and downloaded live media.
        assert_eq!(
            media_phase_kind(true, true, false),
            MediaPhaseKind::DeclineVodOnLive,
        );
    }

    #[test]
    fn live_edge_timing_on_a_vod_playlist_declines_instead_of_striding_the_asset() {
        assert_eq!(
            media_phase_kind(false, false, true),
            MediaPhaseKind::DeclineLiveOnVod,
        );
    }

    #[test]
    fn neither_media_check_leaves_the_run_at_its_playlist_rows() {
        assert_eq!(media_phase_kind(false, false, false), MediaPhaseKind::Skip);
        assert_eq!(media_phase_kind(true, false, false), MediaPhaseKind::Skip);
    }

    #[test]
    fn a_declined_media_phase_explains_which_check_could_not_apply() {
        // The note is what the user reads instead of an empty media table, so it has to
        // name the check that was ticked and the tag that ruled it out.
        let declined = vod_on_live_note(false);
        assert!(declined.contains("Media sample timing"));
        assert!(declined.contains("EXT-X-ENDLIST"));
        assert!(declined.contains("No media bytes were taken"));
        // Where the edge was watched, no bytes claim would be false.
        assert!(!vod_on_live_note(true).contains("No media bytes were taken"));
        let live_declined = live_on_vod_note(false);
        assert!(live_declined.contains("Live edge timing"));
        assert!(live_declined.contains("EXT-X-ENDLIST"));
        assert!(live_declined.contains("No media bytes were taken"));
        assert!(!live_on_vod_note(true).contains("No media bytes were taken"));
    }

    // ── Rendition choice ─────────────────────────────────────────────────────

    fn variant(uri: &str, bandwidth: u64, is_iframe: bool, has_audio_codec: bool) -> VariantSummary {
        VariantSummary {
            uri: uri.into(),
            label: uri.into(),
            bandwidth: Some(bandwidth),
            is_iframe,
            audio_group: Some("aud".into()),
            has_audio_codec,
        }
    }

    #[test]
    fn the_mid_bandwidth_rung_is_chosen_and_trick_play_is_not() {
        let variants = vec![
            variant("low.m3u8", 500_000, false, false),
            variant("iframe.m3u8", 9_000_000, true, false),
            variant("high.m3u8", 5_000_000, false, false),
            variant("mid.m3u8", 2_000_000, false, false),
        ];
        let chosen = mid_bandwidth_variant(&variants).expect("a rung");
        assert_eq!(chosen.uri, "mid.m3u8");
        assert!(mid_bandwidth_variant(&[]).is_none());
        // With an even ladder the lower middle is taken, so the choice is reproducible.
        let even = vec![
            variant("a.m3u8", 1_000, false, false),
            variant("b.m3u8", 2_000, false, false),
            variant("c.m3u8", 3_000, false, false),
            variant("d.m3u8", 4_000, false, false),
        ];
        assert_eq!(mid_bandwidth_variant(&even).expect("a rung").uri, "b.m3u8");
    }

    #[test]
    fn only_trick_play_variants_leaves_nothing_to_time() {
        let variants = vec![variant("iframe.m3u8", 9_000_000, true, false)];
        assert!(mid_bandwidth_variant(&variants).is_none());
    }

    fn audio(name: &str, group: &str, uri: Option<&str>, is_default: bool) -> AudioSummary {
        AudioSummary {
            name: name.into(),
            group_id: group.into(),
            uri: uri.map(str::to_string),
            is_default,
        }
    }

    #[test]
    fn the_default_audio_rendition_of_the_variants_own_group_is_chosen() {
        let renditions = vec![
            audio("Commentary", "aud", Some("commentary.m3u8"), false),
            audio("English", "aud", Some("en.m3u8"), true),
            audio("English HE", "other", Some("en-he.m3u8"), true),
        ];
        let chosen = default_audio_rendition(&renditions, Some("aud")).expect("a rendition");
        assert_eq!(chosen.uri.as_deref(), Some("en.m3u8"));
    }

    #[test]
    fn a_group_with_no_default_falls_back_to_its_first_rendition_with_a_uri() {
        let renditions = vec![
            audio("Muxed", "aud", None, true),
            audio("English", "aud", Some("en.m3u8"), false),
        ];
        let chosen = default_audio_rendition(&renditions, Some("aud")).expect("a rendition");
        assert_eq!(chosen.uri.as_deref(), Some("en.m3u8"));
    }

    #[test]
    fn a_rendition_with_no_uri_is_muxed_and_never_fetched_on_its_own() {
        let renditions = vec![audio("Muxed", "aud", None, true)];
        assert!(default_audio_rendition(&renditions, Some("aud")).is_none());
    }

    #[test]
    fn a_variant_that_named_no_audio_group_is_paired_with_nothing() {
        // No group means no pairing: a rendition the variant never referenced would be a
        // combination no player would have played.
        let across_groups = vec![
            audio("English", "aud", Some("en.m3u8"), true),
            audio("English HE", "other", Some("en-he.m3u8"), true),
        ];
        assert!(default_audio_rendition(&across_groups, None).is_none());
        let one_group = vec![audio("English", "aud", Some("en.m3u8"), true)];
        assert!(default_audio_rendition(&one_group, None).is_none());
        assert!(default_audio_rendition(&[], None).is_none());
    }

    // ── Muxing classification ────────────────────────────────────────────────

    #[test]
    fn muxing_is_only_claimed_where_codecs_said_so() {
        let muxed = variant("muxed.m3u8", 2_000_000, false, true);
        let demuxed = variant("video.m3u8", 2_000_000, false, false);
        assert_eq!(
            media_request_class(Some(&muxed)),
            RequestClass::MuxedSegment
        );
        assert_eq!(
            media_request_class(Some(&demuxed)),
            RequestClass::VideoSegment
        );
        // A media-playlist URL has no CODECS to read, so the muxing is unknown rather
        // than assumed — and never MuxedSegment.
        assert_eq!(media_request_class(None), RequestClass::MediaSegment);
        assert_ne!(media_request_class(None), RequestClass::MuxedSegment);
    }

    #[test]
    fn an_audio_group_is_only_offered_for_a_provably_demuxed_variant() {
        let muxed = variant("muxed.m3u8", 2_000_000, false, true);
        let demuxed = variant("video.m3u8", 2_000_000, false, false);
        assert_eq!(
            sample_class_and_audio_group(Some(&demuxed)),
            (RequestClass::VideoSegment, Some("aud"))
        );
        assert_eq!(
            sample_class_and_audio_group(Some(&muxed)),
            (RequestClass::MuxedSegment, None)
        );
        assert_eq!(
            sample_class_and_audio_group(None),
            (RequestClass::MediaSegment, None)
        );
    }

    #[test]
    fn a_run_that_could_not_establish_muxing_says_so() {
        let mut report = TimingReport::default();
        push_media_plan_notes(&mut report, RequestClass::MediaSegment, false);
        assert!(report.notes.iter().any(|n| n.id == NOTE_MUXING_UNKNOWN));
        // Unknown muxing is not a missing audio rendition: there was no variant to have
        // declared a group at all.
        assert!(!report.notes.iter().any(|n| n.id == NOTE_NO_AUDIO_RENDITION));
    }

    #[test]
    fn a_demuxed_variant_that_found_no_audio_rendition_says_so() {
        let mut report = TimingReport::default();
        push_media_plan_notes(&mut report, RequestClass::VideoSegment, false);
        assert!(report.notes.iter().any(|n| n.id == NOTE_NO_AUDIO_RENDITION));
        assert!(!report.notes.iter().any(|n| n.id == NOTE_MUXING_UNKNOWN));

        // The pairs it plans are one request each, and the note is why.
        let video = rendition_plan(
            RequestClass::VideoSegment,
            "720p",
            (0..4).map(|i| segment(&format!("v{i}.m4s"), 4.0)).collect(),
        );
        let pairs = plan_pairs(&video, None, 4);
        assert!(pairs.iter().all(|p| p.request_count() == 1));
    }

    #[test]
    fn a_plan_with_both_halves_carries_neither_caveat() {
        let mut report = TimingReport::default();
        push_media_plan_notes(&mut report, RequestClass::VideoSegment, true);
        push_media_plan_notes(&mut report, RequestClass::MuxedSegment, false);
        assert!(report.notes.is_empty());
    }

    // ── Pair planning ────────────────────────────────────────────────────────

    #[test]
    fn pairs_stride_across_the_asset_rather_than_its_opening_seconds() {
        let video = rendition_plan(
            RequestClass::VideoSegment,
            "720p",
            (0..100).map(|i| segment(&format!("v{i}.m4s"), 4.0)).collect(),
        );
        let audio = rendition_plan(
            RequestClass::AudioSegment,
            "English",
            (0..100).map(|i| segment(&format!("a{i}.m4s"), 4.0)).collect(),
        );
        let pairs = plan_pairs(&video, Some(&audio), 4);
        assert_eq!(pairs.len(), 4);
        let uris: Vec<&str> = pairs
            .iter()
            .map(|p| p.video.as_ref().expect("video half").uri.as_str())
            .collect();
        assert_eq!(uris, vec!["v0.m4s", "v33.m4s", "v66.m4s", "v99.m4s"]);
        // The audio half is sampled at the same position in its own list.
        assert_eq!(
            pairs[1].audio.as_ref().expect("audio half").uri,
            "a33.m4s"
        );
        assert_eq!(pairs[0].number, 1);
        assert_eq!(pairs[0].request_count(), 2);
    }

    #[test]
    fn a_muxed_rendition_makes_one_request_per_pair() {
        let video = rendition_plan(
            RequestClass::MuxedSegment,
            "720p",
            (0..4).map(|i| segment(&format!("v{i}.ts"), 6.0)).collect(),
        );
        let pairs = plan_pairs(&video, None, 4);
        assert_eq!(pairs.len(), 4);
        assert!(pairs.iter().all(|p| p.audio.is_none()));
        assert!(pairs.iter().all(|p| p.request_count() == 1));
    }

    #[test]
    fn gap_segments_are_never_planned_as_samples() {
        let mut segments: Vec<SegmentEntry> =
            (0..4).map(|i| segment(&format!("v{i}.m4s"), 4.0)).collect();
        segments[1].gap = true;
        segments[2].gap = true;
        let video = rendition_plan(RequestClass::VideoSegment, "720p", segments);
        let pairs = plan_pairs(&video, None, 4);
        let uris: Vec<&str> = pairs
            .iter()
            .map(|p| p.video.as_ref().expect("video half").uri.as_str())
            .collect();
        assert_eq!(uris, vec!["v0.m4s", "v3.m4s"]);
    }

    #[test]
    fn a_playlist_of_nothing_but_gaps_plans_no_samples() {
        let mut segments: Vec<SegmentEntry> =
            (0..3).map(|i| segment(&format!("v{i}.m4s"), 4.0)).collect();
        for s in &mut segments {
            s.gap = true;
        }
        let video = rendition_plan(RequestClass::VideoSegment, "720p", segments);
        assert!(plan_pairs(&video, None, 4).is_empty());
    }

    #[test]
    fn a_short_audio_list_leaves_a_video_only_pair_that_says_why() {
        let video = rendition_plan(
            RequestClass::VideoSegment,
            "720p",
            (0..10).map(|i| segment(&format!("v{i}.m4s"), 4.0)).collect(),
        );
        let audio = rendition_plan(
            RequestClass::AudioSegment,
            "English",
            vec![segment("a0.m4s", 4.0)],
        );
        let pairs = plan_pairs(&video, Some(&audio), 4);
        assert_eq!(pairs.len(), 4);
        assert!(pairs[0].audio.is_some());
        assert!(pairs[1].audio.is_none());
        assert!(pairs[1].notes.iter().any(|n| n.contains("no audio half")));
    }

    #[test]
    fn a_sample_carries_the_denominators_its_playlist_declared() {
        let video = rendition_plan(
            RequestClass::VideoSegment,
            "720p",
            vec![segment("v0.m4s", 3.75)],
        );
        let pairs = plan_pairs(&video, None, 1);
        let spec = pairs[0].video.as_ref().expect("video half");
        assert_eq!(spec.media_duration_s, Some(3.75));
        assert_eq!(spec.media_duration_source, Some(DurationSource::Extinf));
        assert_eq!(spec.target_duration_s, Some(6.0));
        assert_eq!(
            spec.target_duration_source,
            Some(DurationSource::TargetDuration)
        );
    }

    #[test]
    fn a_byterange_sample_asks_for_the_range_the_playlist_named() {
        let mut entry = segment("v0.m4s", 4.0);
        entry.byterange = Some(RequestRange::from_length_with_offset(1_000, 500));
        let video = rendition_plan(RequestClass::VideoSegment, "720p", vec![entry]);
        let pairs = plan_pairs(&video, None, 1);
        let spec = pairs[0].video.as_ref().expect("video half");
        assert_eq!(
            spec.byterange,
            Some(RequestRange::from_length_with_offset(1_000, 500))
        );
    }

    #[test]
    fn a_part_is_only_sampled_where_the_segment_has_no_uri_of_its_own() {
        // Trailing parts belong to a segment that is still being appended, which is the
        // live edge. A VOD entry with a URI is fetched as a segment.
        let mut entry = SegmentEntry {
            duration_s: 4.0,
            parts: vec![PartEntry {
                uri: "v0.0.m4s".into(),
                duration_s: 0.5,
                independent: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let plan_from = |entry: SegmentEntry| {
            let mut plan = rendition_plan(RequestClass::VideoSegment, "720p", vec![entry]);
            plan.summary.part_target_s = Some(0.5);
            plan_pairs(&plan, None, 1)
        };
        let parts_only = plan_from(entry.clone());
        let spec = parts_only[0].video.as_ref().expect("part half");
        assert_eq!(spec.uri, "v0.0.m4s");
        assert_eq!(spec.media_duration_s, Some(0.5));
        assert_eq!(
            spec.media_duration_source,
            Some(DurationSource::PartDuration)
        );
        assert_eq!(spec.target_duration_s, Some(0.5));
        assert_eq!(spec.target_duration_source, Some(DurationSource::PartTarget));

        entry.uri = "v0.m4s".into();
        let with_segment = plan_from(entry);
        let spec = with_segment[0].video.as_ref().expect("segment half");
        assert_eq!(spec.uri, "v0.m4s");
        assert_eq!(spec.media_duration_source, Some(DurationSource::Extinf));
    }

    #[test]
    fn a_gap_part_is_not_sampled_either() {
        let entry = SegmentEntry {
            duration_s: 4.0,
            parts: vec![PartEntry {
                uri: "v0.0.m4s".into(),
                duration_s: 0.5,
                gap: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let plan = rendition_plan(RequestClass::VideoSegment, "720p", vec![entry]);
        // The entry has no URI of its own and its only part is a GAP, so there is nothing
        // to fetch and no sample to plan.
        assert!(plan_pairs(&plan, None, 1).is_empty());
    }

    // ── Report bookkeeping ───────────────────────────────────────────────────

    #[test]
    fn notes_are_addressable_by_name_not_by_prose() {
        let mut report = TimingReport::default();
        report.push_note(NOTE_BYTE_BUDGET, "Stopped after pair 2 of 4.");
        report.push_note(NOTE_BYTE_BUDGET, "Stopped after pair 2 of 4.");
        assert!(report.notes.iter().any(|n| n.id == NOTE_BYTE_BUDGET));
        assert!(!report.notes.iter().any(|n| n.id == NOTE_LIVE_NOT_MEASURED));
        // The same note twice is one note.
        assert_eq!(report.notes.len(), 1);
    }

    #[test]
    fn the_live_notes_say_which_live_story_a_run_told() {
        // "Not measured" used to be the only thing a live run could say. It is now one
        // outcome among several, and each has a name of its own so a run can be asked
        // which happened without matching on prose.
        let ids = [
            NOTE_LIVE_NOT_MEASURED,
            NOTE_LIVE_OBSERVATION_BOUND,
            NOTE_LIVE_BLOCKING_RELOAD,
            NOTE_LIVE_POLLED,
            NOTE_LIVE_WAIT_TIMEOUT,
        ];
        let mut unique: Vec<&str> = ids.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());

        // A run that measured the edge carries the bound, and does not claim it measured
        // nothing.
        let mut report = TimingReport::default();
        report.push_note(NOTE_LIVE_OBSERVATION_BOUND, "an upper bound, not an add time");
        assert!(
            report
                .notes
                .iter()
                .any(|n| n.id == NOTE_LIVE_OBSERVATION_BOUND)
        );
        assert!(!report.notes.iter().any(|n| n.id == NOTE_LIVE_NOT_MEASURED));
    }

    #[test]
    fn the_provenance_list_says_what_the_observation_lag_is_not() {
        let notes = provenance_notes();
        assert!(notes.iter().any(|n| n.contains("upper bound")));
        // Both directions, because the figure bounds the fetch from above and the
        // publication-to-bytes interval from below, and either alone reads as a claim.
        assert!(notes.iter().any(|n| n.contains("a floor on the interval")));
        assert!(
            notes
                .iter()
                .any(|n| n.contains("EXT-X-PROGRAM-DATE-TIME is an authoring clock"))
        );
        assert!(
            notes
                .iter()
                .any(|n| n.contains("blocking reload's playlist row"))
        );
    }

    #[test]
    fn a_measured_row_reports_a_range_the_server_ignored() {
        let mut row = measured_row(RequestClass::VideoSegment, app(0.0, 10.0, 20.0), None);
        assert!(!row.range_ignored());
        row.outcome = RowOutcome::Measured(Measurement {
            status: 200,
            bytes: 5_000_000,
            declared_length: Some(4_800_000),
            redirected_to: Some("https://cdn.example.com/1.m4s".into()),
            app: app(0.0, 10.0, 20.0),
            rt: None,
            range_ignored: true,
        });
        assert!(row.range_ignored());
        assert_eq!(row.status(), Some(200));
        // With no Resource Timing entry the declared length is all there is, and it is
        // labelled as a declaration rather than as a measurement.
        assert_eq!(row.wire_bytes(), Some((4_800_000, "Content-Length")));
        assert_eq!(
            row.redirected_to(),
            Some("https://cdn.example.com/1.m4s")
        );
    }

    #[test]
    fn the_browsers_own_encoded_size_beats_the_servers_declaration() {
        let mut row = measured_row(
            RequestClass::MediaPlaylist,
            app(0.0, 5.0, 9.0),
            Some(ResourceTimingSample {
                start_time: 0.0,
                response_end: 9.0,
                encoded_body_size: 900.0,
                ..Default::default()
            }),
        );
        if let RowOutcome::Measured(m) = &mut row.outcome {
            m.declared_length = Some(1_234);
        }
        assert_eq!(row.wire_bytes(), Some((900, "encodedBodySize")));
    }

    #[test]
    fn media_classes_are_the_ones_that_download_bytes() {
        assert!(RequestClass::VideoSegment.is_media());
        assert!(RequestClass::AudioSegment.is_media());
        assert!(RequestClass::MuxedSegment.is_media());
        // Unknown muxing still downloaded media bytes, so it counts as a media request.
        assert!(RequestClass::MediaSegment.is_media());
        assert!(!RequestClass::MediaPlaylist.is_media());
        assert!(!RequestClass::MultivariantPlaylist.is_media());
    }

    #[test]
    fn a_class_of_unknown_muxing_names_itself_as_unknown() {
        assert_eq!(
            RequestClass::MediaSegment.label(),
            "Media segment (muxing unknown)"
        );
    }

    // ── Cancellation ─────────────────────────────────────────────────────────

    #[test]
    fn a_signal_that_fired_between_requests_still_cancelled_the_run() {
        // Stop pressed after the last row completed aborts nothing, so no row says it was
        // cancelled — but the run is still short of what it planned.
        let rows = vec![
            measured_row(RequestClass::MediaPlaylist, app(0.0, 5.0, 9.0), None),
            measured_row(RequestClass::MediaPlaylist, app(0.0, 5.0, 11.0), None),
        ];
        assert!(was_cancelled(&rows, true));
        assert!(!was_cancelled(&rows, false));
        assert!(!was_cancelled(&[], false));
    }

    #[test]
    fn an_aborted_row_cancels_the_run_whatever_the_signal_says() {
        let mut rows = vec![measured_row(
            RequestClass::VideoSegment,
            app(0.0, 5.0, 9.0),
            None,
        )];
        rows[0].outcome = RowOutcome::Cancelled;
        assert!(was_cancelled(&rows, false));
    }

    #[test]
    fn a_cancelled_run_says_so_and_carries_the_note() {
        let report = finish_with(
            TimingReport {
                rows: vec![measured_row(
                    RequestClass::MediaPlaylist,
                    app(0.0, 5.0, 9.0),
                    None,
                )],
                ..Default::default()
            },
            true,
        );
        assert!(report.cancelled);
        assert!(report.notes.iter().any(|n| n.id == "cancelled"));

        let complete = finish_with(
            TimingReport {
                rows: vec![measured_row(
                    RequestClass::MediaPlaylist,
                    app(0.0, 5.0, 9.0),
                    None,
                )],
                ..Default::default()
            },
            false,
        );
        assert!(!complete.cancelled);
        assert!(complete.notes.is_empty());
    }
}

