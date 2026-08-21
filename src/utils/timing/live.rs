//! The live edge: what a browser can honestly say about how long after an item appeared on
//! a playlist it had that item's bytes.
//!
//! The measurement this module makes is deliberately narrow. A page cannot see the moment
//! an origin appended a segment, so nothing here claims to. What it can see is the first
//! playlist body *it* read that carried a URI, and the moment that URI's bytes finished
//! arriving. The gap between the two is the observation lag: an upper bound on what the
//! fetch cost, because it also contains this tab's own scheduling, and never a statement
//! about when the segment was published.
//!
//! Two ways to watch an edge, and they are not equivalent. With `CAN-BLOCK-RELOAD=YES` the
//! origin holds a reload carrying `_HLS_msn` / `_HLS_part` until it has the requested item,
//! so the body arrives close to publication. Without it there is nothing to do but poll,
//! and a polled appearance can be up to one interval late. Both say so in the notes.

use super::{
    BYTE_BUDGET, DurationSource, MediaParser, MediaRenditionPlan, MediaRequestSpec, MediaSummary,
    Measurement, NOTE_BYTE_BUDGET, NOTE_LIVE_BLOCKING_RELOAD, NOTE_LIVE_NOT_MEASURED,
    NOTE_LIVE_OBSERVATION_BOUND, NOTE_LIVE_POLLED, NOTE_LIVE_WAIT_TIMEOUT, NOTE_RANGE_IGNORED,
    PartEntry, RequestClass, RowOutcome, SegmentEntry, TimingOptions, TimingProgress, TimingReport,
    TimingRequest, TimingRow, duration_ms, measure_media, measured_bytes, media_row, now_ms,
    outcome_from_error, playlist_row, run_stopped,
};
use crate::utils::network::{MeasuredFetchError, RequestRange, fetch_text_measured};
use std::collections::HashSet;
use url::Url;
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortController, AbortSignal,
    js_sys::{Function, Promise},
};

// ── Delivery directives ──────────────────────────────────────────────────────

/// The two Delivery Directives a blocking reload uses (RFC 8216bis §6.2.5.2). These are
/// the only query parameters Timing ever adds to a URL, and only on a live reload: a
/// cache-buster anywhere would make a CDN hit and a CDN miss indistinguishable on purpose.
pub const HLS_MSN_PARAM: &str = "_HLS_msn";
pub const HLS_PART_PARAM: &str = "_HLS_part";

/// The playlist URL for a blocking reload that waits for `msn` (and `part`, if given).
///
/// Every other query parameter the playlist URI carried is kept exactly as it was written:
/// an origin that signs its playlist URLs would reject one whose parameters had been
/// dropped, and would reject one whose escaping had been normalised just as readily, so the
/// original bytes are moved rather than decoded and rebuilt. Any directive already on the
/// URL is replaced rather than repeated, because two `_HLS_msn` values are two different
/// requests as far as an origin is concerned.
pub fn blocking_reload_url(playlist_uri: &str, msn: u64, part: Option<u32>) -> String {
    let Ok(mut url) = Url::parse(playlist_uri) else {
        // Nothing here can build a query onto a URL it cannot read. The caller polls
        // instead, which is slower but is not a malformed request.
        return playlist_uri.to_string();
    };
    let mut query: String = url
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let name = pair.split('=').next().unwrap_or(pair);
            name != HLS_MSN_PARAM && name != HLS_PART_PARAM
        })
        .collect::<Vec<&str>>()
        .join("&");
    if !query.is_empty() {
        query.push('&');
    }
    query.push_str(&format!("{HLS_MSN_PARAM}={msn}"));
    if let Some(part) = part {
        query.push_str(&format!("&{HLS_PART_PARAM}={part}"));
    }
    url.set_query(Some(&query));
    url.to_string()
}

// ── Planning the wait ────────────────────────────────────────────────────────

/// What the next reload should ask the origin to wait for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveWait {
    /// `_HLS_msn`: the Media Sequence Number the response must contain.
    pub msn: u64,
    /// `_HLS_part`: the Part Index within that segment, which RFC 8216bis §3.2 numbers
    /// from zero. `None` waits for a complete segment instead.
    pub part: Option<u32>,
}

impl LiveWait {
    /// Whether this wait is for a Partial Segment, which decides both the hold-back the
    /// timeout comes from and the target duration a percentage is taken against.
    pub fn is_part(&self) -> bool {
        self.part.is_some()
    }
}

/// The Part Index of the last part of this parent segment that carries media.
///
/// A GAP part occupies a Part Index but has no bytes behind it, so a trailing GAP is not
/// the last thing a client could have fetched.
pub fn last_fetchable_part_index(segment: &SegmentEntry) -> Option<u32> {
    let index = segment
        .parts
        .iter()
        .rposition(|part| !part.gap && !part.uri.is_empty())?;
    u32::try_from(index).ok()
}

/// What to wait for beyond what this playlist already carries.
///
/// `EXT-X-PART-INF` is what says parts are the unit that appears; without it the next
/// thing to appear is a segment. A parent segment with no URI of its own is still being
/// appended, so the next part belongs to it — and one that has a URI will take no more
/// parts, so the wait moves to Part Index 0 of the segment after it. `None` when the
/// playlist has no numbered media to wait beyond.
pub fn next_live_wait(summary: &MediaSummary) -> Option<LiveWait> {
    let last = summary.segments.iter().rev().find(|s| s.msn.is_some())?;
    let msn = last.msn?;
    if summary.part_target_s.is_none() {
        return Some(LiveWait {
            msn: msn + 1,
            part: None,
        });
    }
    if last.uri.is_empty() && !last.parts.is_empty() {
        // The count of parts already listed, which is the next Part Index. A GAP part
        // still holds its index, so it is counted here even though it is never fetched:
        // asking for an index the playlist already carries would return at once and turn
        // the wait into a poll.
        let next_index = u32::try_from(last.parts.len()).ok()?;
        return Some(LiveWait {
            msn,
            part: Some(next_index),
        });
    }
    Some(LiveWait {
        msn: msn + 1,
        part: Some(0),
    })
}

/// One item that appeared at the live edge, ready to be turned into a request.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveEdgeItem {
    pub uri: String,
    pub byterange: Option<RequestRange>,
    pub media_duration_s: Option<f64>,
    pub media_duration_source: Option<DurationSource>,
    pub target_duration_s: Option<f64>,
    pub target_duration_source: Option<DurationSource>,
    pub msn: Option<u64>,
    pub program_date_time: Option<String>,
    /// True when this is a Partial Segment rather than a segment, which is what makes
    /// PART-TARGET the denominator its percentages use.
    pub is_part: bool,
}

impl LiveEdgeItem {
    /// The request this item becomes, once the caller says which rendition it belongs to.
    pub fn spec(&self, class: RequestClass, rendition: &str) -> MediaRequestSpec {
        MediaRequestSpec {
            class,
            rendition: rendition.to_string(),
            uri: self.uri.clone(),
            byterange: self.byterange,
            media_duration_s: self.media_duration_s,
            media_duration_source: self.media_duration_source,
            target_duration_s: self.target_duration_s,
            target_duration_source: self.target_duration_source,
            msn: self.msn,
            program_date_time: self.program_date_time.clone(),
        }
    }
}

/// What identifies one fetchable item: its URI and the range of it asked for.
///
/// The range is not decoration. A low-latency playlist commonly names every part of a
/// segment inside one file, so several parts share a URI and differ only in their
/// BYTERANGE — keyed on the URI alone, every part after the first would look like
/// something this tab had already seen.
type ItemKey<'a> = (&'a str, Option<RequestRange>);

/// Every fetchable item a playlist body carried, segments and parts alike.
fn known_items(summary: &MediaSummary) -> HashSet<ItemKey<'_>> {
    summary
        .segments
        .iter()
        .flat_map(|segment| {
            std::iter::once((segment.uri.as_str(), segment.byterange)).chain(
                segment
                    .parts
                    .iter()
                    .map(|part| (part.uri.as_str(), part.byterange)),
            )
        })
        .filter(|(uri, _)| !uri.is_empty())
        .collect()
}

fn part_item(summary: &MediaSummary, segment: &SegmentEntry, part: &PartEntry) -> LiveEdgeItem {
    LiveEdgeItem {
        uri: part.uri.clone(),
        byterange: part.byterange,
        media_duration_s: (part.duration_s > 0.0).then_some(part.duration_s),
        media_duration_source: Some(DurationSource::PartDuration),
        target_duration_s: summary.part_target_s.filter(|t| *t > 0.0),
        target_duration_source: Some(DurationSource::PartTarget),
        msn: segment.msn,
        program_date_time: segment.program_date_time.clone(),
        is_part: true,
    }
}

fn segment_item(summary: &MediaSummary, segment: &SegmentEntry) -> LiveEdgeItem {
    LiveEdgeItem {
        uri: segment.uri.clone(),
        byterange: segment.byterange,
        media_duration_s: (segment.duration_s > 0.0).then_some(segment.duration_s),
        media_duration_source: Some(DurationSource::Extinf),
        target_duration_s: summary.target_duration_s.filter(|t| *t > 0.0),
        target_duration_source: Some(DurationSource::TargetDuration),
        msn: segment.msn,
        program_date_time: segment.program_date_time.clone(),
        is_part: false,
    }
}

/// The fetchable media in `after` that `before` did not carry.
///
/// "Appeared" is the whole point: a sliding window drops old URIs, so a diff of the two
/// windows would also report everything that merely survived. Only URIs new to this tab
/// and at or beyond the newest parent segment the previous body carried are returned, and
/// a GAP is never one of them.
///
/// Where the playlist has parts, a parent segment whose parts were all listed before is
/// skipped even when its own URI is new: downloading the whole segment then would be
/// re-fetching media this tab could already have had part by part, which is the
/// VOD-shaped sampling a live measurement must not do.
pub fn new_edge_items(before: &MediaSummary, after: &MediaSummary) -> Vec<LiveEdgeItem> {
    let known = known_items(before);
    let floor = before.segments.iter().filter_map(|s| s.msn).max();
    let mut items = Vec::new();
    for segment in &after.segments {
        if let (Some(floor), Some(msn)) = (floor, segment.msn)
            && msn < floor
        {
            continue;
        }
        let new_parts: Vec<LiveEdgeItem> = segment
            .parts
            .iter()
            .filter(|part| !part.gap && !part.uri.is_empty())
            .filter(|part| !known.contains(&(part.uri.as_str(), part.byterange)))
            .map(|part| part_item(after, segment, part))
            .collect();
        if !new_parts.is_empty() {
            items.extend(new_parts);
            continue;
        }
        if after.part_target_s.is_some() && !segment.parts.is_empty() {
            continue;
        }
        if !segment.gap
            && !segment.uri.is_empty()
            && !known.contains(&(segment.uri.as_str(), segment.byterange))
        {
            items.push(segment_item(after, segment));
        }
    }
    items
}

// ── Bounds on the wait ───────────────────────────────────────────────────────

/// Three target durations, the interval after which RFC 8216bis §6.2.5.2 says a server
/// should give up on a blocking reload. Used where the playlist declares no hold-back and
/// no target duration at all.
const DEFAULT_LIVE_WAIT_MS: f64 = 18_000.0;
const MIN_LIVE_WAIT_MS: f64 = 1_000.0;
/// Whatever the playlist says, a wait the user cannot see ending is a hang. A minute is
/// already far past any conformant hold-back.
const MAX_LIVE_WAIT_MS: f64 = 60_000.0;
const MIN_POLL_INTERVAL_MS: f64 = 1_000.0;
const MAX_POLL_INTERVAL_MS: f64 = 10_000.0;
/// A ceiling on requests per wait, so a clock that stops or an origin that always answers
/// at once still cannot make the loop unbounded.
const MAX_WAIT_ATTEMPTS: usize = 24;

/// How long to wait for one appearance before giving up.
///
/// Three hold-backs is the distance from the edge the playlist itself says a client should
/// keep, so a wait longer than that has stopped measuring the edge. PART-HOLD-BACK is the
/// one to use when a part is what is being waited for; the segment hold-back is far too
/// long a ceiling for something that arrives every fraction of a second.
pub fn live_wait_timeout_ms(summary: &MediaSummary, waiting_for_part: bool) -> f64 {
    let seconds = waiting_for_part
        .then_some(summary.part_hold_back_s)
        .flatten()
        .or(summary.hold_back_s)
        .or(summary.target_duration_s)
        .filter(|s| s.is_finite() && *s > 0.0);
    let ms = seconds
        .map(|s| s * 3.0 * 1_000.0)
        .unwrap_or(DEFAULT_LIVE_WAIT_MS);
    ms.clamp(MIN_LIVE_WAIT_MS, MAX_LIVE_WAIT_MS)
}

/// How long to leave between polls of a playlist that cannot be blocked on.
///
/// One target duration is how often the playlist expects to change; a floor of a second
/// keeps a short PART-TARGET from turning the measurement into a request flood.
pub fn poll_interval_ms(summary: &MediaSummary, waiting_for_part: bool) -> f64 {
    let seconds = waiting_for_part
        .then_some(summary.part_target_s)
        .flatten()
        .or(summary.target_duration_s)
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0);
    (seconds * 1_000.0).clamp(MIN_POLL_INTERVAL_MS, MAX_POLL_INTERVAL_MS)
}

/// The observation lag for one measured media request.
///
/// Two parts, both from the same `performance.now()` clock: the gap between reading the
/// playlist and issuing the request — this tab's own scheduling, which is why the figure
/// is an upper bound on the fetch rather than a measurement of it — and the authoritative
/// download total, which is Resource Timing's where an entry matched. `None` if either end
/// is missing or the clock appears to have run backwards, because a negative lag has
/// measured nothing.
pub fn observation_lag_ms(playlist_seen_ms: f64, measurement: &Measurement) -> Option<f64> {
    let dispatch = duration_ms(measurement.app.request_ms - playlist_seen_ms)?;
    let (download, _) = measurement.authoritative_total_ms()?;
    duration_ms(dispatch + download)
}

// ── Waiting, in a browser ────────────────────────────────────────────────────

/// A child abort scope for one blocking reload: its own timeout, and the run's Stop.
///
/// A held reload has no natural end — the origin answers when it has something to publish,
/// or not at all — so a Timing run that cannot cap it would hang the page. The parent
/// signal is listened to rather than replaced, so Stop and a new probe still cancel the
/// held request as they cancel everything else.
struct WaitScope {
    controller: AbortController,
    parent: AbortSignal,
    timeout: i32,
    /// Held only to keep the callback alive for as long as the timeout can fire.
    _on_timeout: Closure<dyn FnMut()>,
    on_parent_abort: Closure<dyn FnMut()>,
}

impl WaitScope {
    /// `None` when this browser gives us no way to build the scope, in which case the
    /// caller must not block: an uncapped held request is a hang.
    fn new(parent: &AbortSignal, timeout_ms: f64) -> Option<Self> {
        let window = web_sys::window()?;
        let controller = AbortController::new().ok()?;
        let on_timeout = {
            let controller = controller.clone();
            Closure::<dyn FnMut()>::new(move || controller.abort())
        };
        let timeout = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                on_timeout.as_ref().unchecked_ref::<Function>(),
                timeout_ms as i32,
            )
            .ok()?;
        let on_parent_abort = {
            let controller = controller.clone();
            Closure::<dyn FnMut()>::new(move || controller.abort())
        };
        let scope = Self {
            controller,
            parent: parent.clone(),
            timeout,
            _on_timeout: on_timeout,
            on_parent_abort,
        };
        parent
            .add_event_listener_with_callback(
                "abort",
                scope.on_parent_abort.as_ref().unchecked_ref::<Function>(),
            )
            .ok()?;
        // Stop pressed before the wait began must not open a held request at all, and an
        // already-fired signal dispatches no further event to catch.
        if parent.aborted() {
            scope.controller.abort();
        }
        Some(scope)
    }

    fn signal(&self) -> AbortSignal {
        self.controller.signal()
    }
}

impl Drop for WaitScope {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            window.clear_timeout_with_handle(self.timeout);
        }
        let _ = self.parent.remove_event_listener_with_callback(
            "abort",
            self.on_parent_abort.as_ref().unchecked_ref::<Function>(),
        );
        // Nothing is waiting on this scope any more, and a reload still held open would
        // be a request whose body nobody reads.
        self.controller.abort();
    }
}

/// Wait `ms`, or return at once if this context has no window to time with.
async fn sleep_ms(ms: f64) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let promise = Promise::new(&mut |resolve, _reject| {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
    });
    let _ = JsFuture::from(promise).await;
}

/// Wait between polls, but not through a Stop.
///
/// Sitting out a whole poll interval after the user pressed Stop would leave the run
/// looking like it had ignored them, so the signal is checked as the interval passes.
async fn sleep_between_polls(ms: f64, signal: &AbortSignal) {
    const SLICE_MS: f64 = 250.0;
    let mut remaining = ms;
    while remaining > 0.0 && !signal.aborted() {
        let slice = remaining.min(SLICE_MS);
        sleep_ms(slice).await;
        remaining -= slice;
    }
}

/// How one wait for an appearance ended.
#[derive(Debug, Clone, PartialEq)]
enum WaitEnd {
    /// New fetchable media appeared on a body this tab read.
    Appeared,
    /// The wait ran out of time with nothing new on the playlist.
    TimedOut,
    /// A playlist request did not complete.
    Failed(String),
    /// The run was stopped.
    Cancelled,
    /// There is no numbered media to wait beyond.
    NothingToWaitFor,
}

/// What one wait produced: the requests it made, and what it found.
struct WaitResult {
    rows: Vec<TimingRow>,
    bytes: u64,
    /// When this tab first read a body carrying the new URIs. The anchor every observation
    /// lag is measured from, and `None` if the clock could not be read.
    seen_ms: Option<f64>,
    /// The newest body read, which becomes the baseline the next wait diffs against.
    summary: Option<MediaSummary>,
    items: Vec<LiveEdgeItem>,
    end: WaitEnd,
}

impl WaitResult {
    fn ended(end: WaitEnd) -> Self {
        Self {
            rows: Vec::new(),
            bytes: 0,
            seen_ms: None,
            summary: None,
            items: Vec::new(),
            end,
        }
    }
}

/// Which URL one reload asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReloadUrl {
    /// The playlist URI with the Delivery Directives on it, so the origin holds the
    /// request until it has the awaited item.
    Blocking,
    /// The playlist URI exactly as the manifest wrote it.
    Plain,
}

/// What one attempt of a wait was granted.
#[derive(Debug, Clone, PartialEq)]
struct ReloadAttempt {
    url: ReloadUrl,
    give_up_text: String,
}

/// Decide one attempt: how long this request may stay open, whether it may ask the origin
/// to hold it, and what to say if this run's own ceiling is what ended it.
///
/// `open_scope` is handed the duration and answers with whatever scope it managed to build,
/// so the figure quoted to the user cannot drift from the figure the timer was set to. The
/// two are easy to confuse: the ceiling bounds the whole wait, but a second attempt runs
/// under what is left of it, and a status naming the ceiling would say a request had been
/// open for a duration nothing here ever granted it.
///
/// A browser that hands back no scope gets the plain URI. Asking an origin to hold a
/// request nothing here could abandon is the one case that cannot be recovered from.
fn plan_reload_attempt<S>(
    remaining_ms: f64,
    timeout_ms: f64,
    can_block_reload: bool,
    open_scope: impl FnOnce(f64) -> Option<S>,
) -> (ReloadAttempt, Option<S>) {
    // A remainder too short to be worth opening a request for is rounded up rather than
    // spent on a reload that would be aborted before an origin could answer it.
    let scope_ms = remaining_ms.clamp(MIN_LIVE_WAIT_MS, timeout_ms.max(MIN_LIVE_WAIT_MS));
    let scope = open_scope(scope_ms);
    let url = if can_block_reload && scope.is_some() {
        ReloadUrl::Blocking
    } else {
        ReloadUrl::Plain
    };
    // The whole wait is named alongside it wherever the two differ, because a row that
    // ended well short of the ceiling otherwise reads like a request given up on early.
    let give_up_text = if scope_ms < timeout_ms {
        format!(
            "this request was still open for the last {scope_ms:.0} ms of a {timeout_ms:.0} ms \
             wait with nothing new on the playlist, so the wait was given up on"
        )
    } else {
        format!(
            "this request was still open after {scope_ms:.0} ms with nothing new on the \
             playlist, so the wait was given up on"
        )
    };
    (ReloadAttempt { url, give_up_text }, scope)
}

/// Reload one rendition's playlist until something new appears at its edge.
///
/// The reloads are in the table under a class of their own. On the blocking path the row's
/// duration is the origin holding the request until it had something to publish, which is
/// a wait on an author and not a download of anything, so it belongs in no median taken
/// over playlist fetches.
async fn wait_for_edge(
    plan: &MediaRenditionPlan,
    parse: MediaParser,
    sample_label: &str,
    signal: &AbortSignal,
) -> WaitResult {
    let Some(mut wait) = next_live_wait(&plan.summary) else {
        return WaitResult::ended(WaitEnd::NothingToWaitFor);
    };
    let timeout_ms = live_wait_timeout_ms(&plan.summary, wait.is_part());
    let deadline_ms = now_ms().ok().map(|start| start + timeout_ms);
    let mut out = WaitResult::ended(WaitEnd::TimedOut);

    for _ in 0..MAX_WAIT_ATTEMPTS {
        if signal.aborted() {
            out.end = WaitEnd::Cancelled;
            return out;
        }
        // Whatever is left of the wait, so a second attempt cannot start the ceiling over
        // and leave the whole wait taking twice as long as it said it would.
        let Some(remaining_ms) = remaining_wait_ms(deadline_ms, timeout_ms) else {
            out.end = WaitEnd::TimedOut;
            return out;
        };
        // Every reload gets a scope, held or polled: a request this run has no way to end
        // is a hung page whichever path opened it, and the parent signal only fires when
        // the user does.
        let (attempt, scope) = plan_reload_attempt(
            remaining_ms,
            timeout_ms,
            plan.summary.can_block_reload,
            |scope_ms| WaitScope::new(signal, scope_ms),
        );
        let url = match attempt.url {
            ReloadUrl::Blocking => blocking_reload_url(&plan.playlist_uri, wait.msn, wait.part),
            ReloadUrl::Plain => plan.playlist_uri.clone(),
        };
        let fetch_signal = scope
            .as_ref()
            .map(WaitScope::signal)
            .unwrap_or_else(|| signal.clone());

        let result = fetch_text_measured(url.clone(), Some(&fetch_signal)).await;
        let seen_ms = now_ms().ok();
        out.bytes = out.bytes.saturating_add(measured_bytes(&result).unwrap_or(0));
        let mut row = playlist_row(
            RequestClass::LivePlaylistReload,
            Some(plan.rendition.clone()),
            sample_label.to_string(),
            url,
            &result,
        );

        // How long to leave before the next attempt, which only a body that arrived can
        // say: every other arm below ends the wait.
        let interval_ms;
        match &result {
            // Our own ceiling firing is not the user stopping the run, and must not be
            // reported as one: a cancelled row would make the whole report cancelled.
            Err(error) if error.is_cancelled() && !signal.aborted() => {
                row.outcome = RowOutcome::Failed(attempt.give_up_text);
                out.rows.push(row);
                out.end = WaitEnd::TimedOut;
                return out;
            }
            Err(error) if error.is_cancelled() => {
                out.rows.push(row);
                out.end = WaitEnd::Cancelled;
                return out;
            }
            Err(error) => {
                out.rows.push(row);
                out.end = WaitEnd::Failed(error.to_string());
                return out;
            }
            Ok((_, body)) => {
                out.rows.push(row);
                let summary = parse(&plan.playlist_uri, body, &plan.definitions);
                // Against the body this wait began from, so the whole wait reports what
                // appeared during it rather than what the last attempt added.
                let items = new_edge_items(&plan.summary, &summary);
                // The directive can be satisfied without anything fetchable arriving — a
                // GAP at the awaited index does exactly that — so what to ask for next is
                // re-read from this body. Asking again for an index the playlist already
                // carries would be answered the same way until the ceiling ran out.
                let next = next_live_wait(&summary);
                let next_interval_ms = next.map(|next| poll_interval_ms(&summary, next.is_part()));
                out.summary = Some(summary);
                if !items.is_empty() {
                    out.seen_ms = seen_ms;
                    out.items = items;
                    out.end = WaitEnd::Appeared;
                    return out;
                }
                let (Some(next), Some(next_interval_ms)) = (next, next_interval_ms) else {
                    out.end = WaitEnd::NothingToWaitFor;
                    return out;
                };
                wait = next;
                interval_ms = next_interval_ms;
            }
        }

        let Some(remaining_ms) = remaining_wait_ms(deadline_ms, interval_ms) else {
            out.end = WaitEnd::TimedOut;
            return out;
        };
        // Also on the blocking path: an origin that advertises CAN-BLOCK-RELOAD and then
        // answers immediately with the same playlist would otherwise be asked again as
        // fast as the connection allows.
        sleep_between_polls(interval_ms.min(remaining_ms), signal).await;
    }
    out
}

/// What is left of the wait, or `None` once the ceiling has passed.
///
/// `fallback` stands in only where there was no clock to set a deadline from, which is
/// the one case with nothing better to go on than the interval itself.
fn remaining_wait_ms(deadline_ms: Option<f64>, fallback: f64) -> Option<f64> {
    let Some(deadline) = deadline_ms else {
        return Some(fallback);
    };
    let remaining = deadline - now_ms().ok()?;
    (remaining > 0.0).then_some(remaining)
}

/// One rendition's whole sample: what its wait found, and what became of downloading it.
struct RenditionSample {
    wait: WaitResult,
    spec: Option<MediaRequestSpec>,
    measurement: Option<Result<Measurement, MeasuredFetchError>>,
}

/// Wait at one rendition's edge and fetch what appeared there, without pausing in between.
///
/// The download has to start the moment *this* playlist named the URI. Holding it until
/// the other rendition's wait also ended would put that rendition's leftover wait inside
/// this one's observation lag — a figure that is already this tab's scheduling as much as
/// the network's, and would then be someone else's scheduling too.
async fn sample_rendition(
    plan: &MediaRenditionPlan,
    parse: MediaParser,
    sample_label: &str,
    signal: &AbortSignal,
) -> RenditionSample {
    let wait = wait_for_edge(plan, parse, sample_label, signal).await;
    // Only the first: the rest were in the same body, so their lag would be this tab's
    // queueing rather than anything the edge did.
    let spec = (wait.end == WaitEnd::Appeared)
        .then(|| wait.items.first())
        .flatten()
        .map(|item| item.spec(plan.class, &plan.rendition));
    let measurement = measure_media(spec.as_ref(), signal).await;
    RenditionSample {
        wait,
        spec,
        measurement,
    }
}

/// The audio half of a sample, where there is one.
async fn sample_audio_rendition(
    plan: Option<&MediaRenditionPlan>,
    parse: MediaParser,
    sample_label: &str,
    signal: &AbortSignal,
) -> Option<RenditionSample> {
    let plan = plan?;
    Some(sample_rendition(plan, parse, sample_label, signal).await)
}

// ── The live sample loop ─────────────────────────────────────────────────────

/// What a live run can say about its waits before it has made any: what the observation
/// lag on the rows below means, and how the edge was watched.
///
/// All three describe a wait, so a playlist with nothing numbered to wait beyond earns
/// none of them. That run makes no reload at all, and a report claiming `_HLS_msn` on
/// requests it never sent — or a lag on rows it never produced — is describing something
/// that did not happen.
fn live_run_notes(video: &MediaSummary) -> Vec<(&'static str, String)> {
    let Some(wait) = next_live_wait(video) else {
        return Vec::new();
    };
    let watched = if video.can_block_reload {
        (
            NOTE_LIVE_BLOCKING_RELOAD,
            format!(
                "EXT-X-SERVER-CONTROL declares CAN-BLOCK-RELOAD=YES, so each wait re-requested \
                 the media playlist with {HLS_MSN_PARAM}{part} — the only query parameters this \
                 run adds anywhere. Those reloads are the \"{reload}\" rows below, kept apart \
                 from the playlist repeats because their duration is the origin holding the \
                 request until it had something to publish, which is neither a media download \
                 nor a time to first byte for the item that followed. A reload that returned at \
                 once had nothing to hold, meaning the item was already published before this \
                 tab asked for it.",
                part = if wait.is_part() {
                    format!(" and {HLS_PART_PARAM}")
                } else {
                    String::new()
                },
                reload = RequestClass::LivePlaylistReload.label(),
            ),
        )
    } else {
        (
            NOTE_LIVE_POLLED,
            format!(
                "EXT-X-SERVER-CONTROL does not declare CAN-BLOCK-RELOAD=YES, so the edge was \
                 polled every {:.0} ms with no added query parameters. An item can therefore \
                 have been published up to one interval before this tab saw it, and the \
                 observation lag below is shorter than the wait a client that could block \
                 would have measured.",
                poll_interval_ms(video, wait.is_part()),
            ),
        )
    };
    vec![
        (
            NOTE_LIVE_OBSERVATION_BOUND,
            "The live rows below carry an observation lag: the time from this tab first reading \
             a playlist body that named the URI to that item's bytes being in hand. It is an \
             upper bound on what the fetch cost, because it also contains this tab's own \
             scheduling, and a floor on the interval from the item being published to its bytes \
             being in hand, because whatever passed before this tab read the body is outside it. \
             It is never the time from the item being added to the playlist — a browser cannot \
             see that moment, and EXT-X-PROGRAM-DATE-TIME is an authoring clock rather than a \
             latency."
                .to_string(),
        ),
        watched,
    ]
}

/// Take up to `options.media_pairs` live-edge samples.
///
/// One sample is one appearance: both renditions' edges are waited on together, because a
/// player watching a demuxed stream waits on both. Each half then downloads what appeared
/// as soon as its own playlist named it, so neither one's observation lag carries the
/// other's leftover wait. A video wait that ends any other way than an appearance stops the
/// sampling — an origin that has stopped publishing, or a wait this run gave up on, has
/// nothing further to say about its edge.
pub(super) async fn run_live_samples(
    request: &TimingRequest,
    options: TimingOptions,
    report: &mut TimingReport,
    video: &mut MediaRenditionPlan,
    mut audio: Option<&mut MediaRenditionPlan>,
    signal: &AbortSignal,
    on_progress: &impl Fn(TimingProgress),
) {
    for (id, text) in live_run_notes(&video.summary) {
        report.push_note(id, text);
    }

    let first_wait = next_live_wait(&video.summary);
    // A GAP part holds its Part Index without carrying any bytes, so the index the wait
    // asks for can be past the last part that could have been fetched. Said out loud,
    // because otherwise the directive in the table looks like it skipped a part.
    if let Some(wait) = first_wait
        && let Some(asked) = wait.part
        && let Some(have) = video
            .summary
            .segments
            .last()
            .and_then(last_fetchable_part_index)
        && asked > have + 1
    {
        report.push_note(
            "live_wait_past_gap_parts",
            format!(
                "The wait asks for Part Index {asked} of media sequence {}, past Part Index \
                 {have} — the last part listed that carries media. Asking for the index after \
                 that one would have returned a playlist this tab already had, because the \
                 parts in between are GAPs holding an index with nothing behind it.",
                wait.msn,
            ),
        );
    }

    let total = options.media_pairs;
    let mut measured = 0usize;
    let mut stopped_because: Option<String> = None;

    for sample in 1..=total {
        if run_stopped(report, signal) {
            return;
        }
        let remaining = total - sample + 1;
        on_progress(TimingProgress {
            done: report.rows.len(),
            // Live cannot know how many requests a wait will take, so the total is the
            // rows so far plus a floor for what is left rather than a promise.
            total: report.rows.len() + remaining * 2,
            label: format!("Waiting at the live edge, sample {sample} of {total}"),
        });

        let label = format!("live sample {sample} / {total}");
        let parse = request.parsers.media;
        let (mut video_sample, audio_sample) = futures::future::join(
            sample_rendition(video, parse, &label, signal),
            sample_audio_rendition(audio.as_deref(), parse, &label, signal),
        )
        .await;

        // The requests happened whatever they found, so their rows and bytes land first.
        report.rows.append(&mut video_sample.wait.rows);
        report.total_bytes = report.total_bytes.saturating_add(video_sample.wait.bytes);
        if let Some(summary) = video_sample.wait.summary.take() {
            video.summary = summary;
        }
        let mut audio_row = None;
        if let Some(mut result) = audio_sample {
            report.rows.append(&mut result.wait.rows);
            report.total_bytes = report.total_bytes.saturating_add(result.wait.bytes);
            if let (Some(summary), Some(plan)) = (result.wait.summary.take(), audio.as_deref_mut())
            {
                plan.summary = summary;
            }
            if result.wait.end == WaitEnd::Appeared {
                audio_row = result
                    .spec
                    .zip(result.measurement)
                    .map(|(spec, measurement)| (spec, measurement, result.wait.seen_ms));
            } else if let Some(plan) = audio.as_deref() {
                report.push_note(
                    "pair_incomplete",
                    format!(
                        "Audio rendition \"{}\" reached no new item for live sample {sample} \
                         ({}), so that sample is video-only and understates what a player \
                         would have had to download.",
                        plan.rendition,
                        wait_end_reason(&result.wait.end),
                    ),
                );
            }
        }

        if video_sample.wait.items.len() > 1 {
            report.push_note(
                "live_edge_multiple_items",
                format!(
                    "{} new items appeared on one reload of \"{}\". The first is the one \
                     measured: the others were in the same body, so their observation lag \
                     would be this tab's queueing rather than anything the edge did.",
                    video_sample.wait.items.len(),
                    video.rendition,
                ),
            );
        }

        // Each half downloaded whatever its own wait found, so a request this run made is
        // a row even where the other half's wait ended without an item to pair it with.
        let video_row = video_sample
            .spec
            .zip(video_sample.measurement)
            .map(|(spec, measurement)| (spec, measurement, video_sample.wait.seen_ms));
        for (spec, result, seen_ms) in video_row.into_iter().chain(audio_row) {
            record_media_row(report, &spec, result, seen_ms, &label);
            measured += 1;
        }

        if video_sample.wait.end != WaitEnd::Appeared {
            stopped_because = Some(wait_end_reason(&video_sample.wait.end).to_string());
            match &video_sample.wait.end {
                WaitEnd::Cancelled => return,
                WaitEnd::TimedOut => report.push_note(
                    NOTE_LIVE_WAIT_TIMEOUT,
                    format!(
                        "Live sample {sample} of {total} gave up waiting for the next item, so \
                         no further live samples were taken. The playlist rows for that wait are \
                         still real measurements of the requests it made.",
                    ),
                ),
                WaitEnd::Failed(reason) => report.push_note(
                    "live_wait_failed",
                    format!(
                        "The playlist reload for live sample {sample} of {total} did not \
                         complete ({reason}), so no further live samples were taken.",
                    ),
                ),
                // Nothing to wait beyond has nothing further to say than the note below,
                // which names the same reason for the run as a whole.
                WaitEnd::NothingToWaitFor | WaitEnd::Appeared => {}
            }
            break;
        }

        if run_stopped(report, signal) {
            return;
        }
        if report.total_bytes > BYTE_BUDGET {
            report.push_note(
                NOTE_BYTE_BUDGET,
                format!(
                    "Stopped after live sample {sample} of {total}: {} bytes had already \
                     arrived, past the {} byte cap.",
                    report.total_bytes,
                    BYTE_BUDGET,
                ),
            );
            break;
        }
    }

    if measured == 0 {
        report.push_note(
            NOTE_LIVE_NOT_MEASURED,
            format!(
                "No item was seen appearing at the live edge{}, so no live media was timed. \
                 The playlist rows above are still real measurements.",
                stopped_because
                    .map(|reason| format!(" ({reason})"))
                    .unwrap_or_default(),
            ),
        );
    }
}

fn wait_end_reason(end: &WaitEnd) -> &str {
    match end {
        WaitEnd::Appeared => "an item appeared",
        WaitEnd::TimedOut => "the wait was given up on",
        WaitEnd::Failed(reason) => reason,
        WaitEnd::Cancelled => "the run was stopped",
        WaitEnd::NothingToWaitFor => "the playlist numbers no media to wait beyond",
    }
}

/// Record one live media request, with the lag from seeing its URI to holding its bytes.
fn record_media_row(
    report: &mut TimingReport,
    spec: &MediaRequestSpec,
    result: Result<Measurement, MeasuredFetchError>,
    seen_ms: Option<f64>,
    sample_label: &str,
) {
    let (outcome, lag_ms) = match result {
        Ok(measurement) => {
            report.total_bytes = report.total_bytes.saturating_add(measurement.bytes);
            if measurement.range_ignored {
                report.push_note(
                    NOTE_RANGE_IGNORED,
                    format!(
                        "A BYTERANGE request for {} was answered with HTTP {} and the whole \
                         resource rather than 206, so its bytes, duration and observation lag \
                         cover more than the sample asked for.",
                        spec.uri, measurement.status,
                    ),
                );
            }
            let lag_ms = seen_ms.and_then(|seen| observation_lag_ms(seen, &measurement));
            (RowOutcome::Measured(measurement), lag_ms)
        }
        // A failure has no duration, so it has no lag either.
        Err(error) => (outcome_from_error(&error), None),
    };
    report
        .rows
        .push(media_row(spec, sample_label.to_string(), outcome, lag_ms));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::timing::{AppClockSpan, ResourceTimingSample};
    use pretty_assertions::assert_eq;

    fn part(uri: &str, duration_s: f64) -> PartEntry {
        PartEntry {
            uri: uri.into(),
            duration_s,
            independent: true,
            ..Default::default()
        }
    }

    fn segment(uri: &str, msn: u64, duration_s: f64) -> SegmentEntry {
        SegmentEntry {
            uri: uri.into(),
            duration_s,
            msn: Some(msn),
            ..Default::default()
        }
    }

    /// A parent segment the server is still appending: parts published, no URI yet.
    fn appending(msn: u64, parts: Vec<PartEntry>) -> SegmentEntry {
        SegmentEntry {
            uri: String::new(),
            msn: Some(msn),
            parts,
            ..Default::default()
        }
    }

    fn live_summary(segments: Vec<SegmentEntry>) -> MediaSummary {
        MediaSummary {
            target_duration_s: Some(4.0),
            is_live: true,
            can_block_reload: true,
            hold_back_s: Some(12.0),
            segments,
            ..Default::default()
        }
    }

    fn measurement(app: AppClockSpan, rt: Option<ResourceTimingSample>) -> Measurement {
        Measurement {
            status: 200,
            bytes: 1_000,
            declared_length: None,
            redirected_to: None,
            app,
            rt,
            range_ignored: false,
        }
    }

    // ── Delivery directives ──────────────────────────────────────────────────

    #[test]
    fn a_blocking_reload_keeps_the_query_the_playlist_uri_already_carried() {
        let url = blocking_reload_url(
            "https://example.com/hls/v/index.m3u8?token=abc&sid=7",
            42,
            Some(3),
        );
        assert_eq!(
            url,
            "https://example.com/hls/v/index.m3u8?token=abc&sid=7&_HLS_msn=42&_HLS_part=3"
        );
    }

    #[test]
    fn a_blocking_reload_replaces_directives_rather_than_repeating_them() {
        // Two _HLS_msn values are two different requests as far as an origin is concerned.
        let url = blocking_reload_url(
            "https://example.com/i.m3u8?_HLS_msn=10&keep=1&_HLS_part=4",
            11,
            Some(0),
        );
        assert_eq!(
            url,
            "https://example.com/i.m3u8?keep=1&_HLS_msn=11&_HLS_part=0"
        );
    }

    #[test]
    fn a_signed_query_is_moved_rather_than_re_escaped() {
        // Normalising the escaping of a signed parameter breaks the signature just as
        // surely as dropping it would.
        let url = blocking_reload_url(
            "https://example.com/i.m3u8?hdnts=exp%3D123~hmac%3Dab%2Fcd&x=a+b",
            5,
            None,
        );
        assert_eq!(
            url,
            "https://example.com/i.m3u8?hdnts=exp%3D123~hmac%3Dab%2Fcd&x=a+b&_HLS_msn=5"
        );
    }

    #[test]
    fn a_wait_for_a_whole_segment_sends_no_part_directive() {
        let url = blocking_reload_url("https://example.com/i.m3u8", 9, None);
        assert_eq!(url, "https://example.com/i.m3u8?_HLS_msn=9");
        assert!(!url.contains(HLS_PART_PARAM));
    }

    #[test]
    fn nothing_but_the_two_directives_is_ever_added() {
        let url = blocking_reload_url("https://example.com/i.m3u8", 9, Some(1));
        // No _HLS_skip, and no cache-buster: a CDN hit and a CDN miss stay
        // indistinguishable on purpose.
        assert!(!url.contains("_HLS_skip"));
        let added = url.split('?').nth(1).expect("a query");
        assert_eq!(added, "_HLS_msn=9&_HLS_part=1");
    }

    #[test]
    fn a_url_this_build_cannot_read_is_left_exactly_as_it_was() {
        assert_eq!(blocking_reload_url("not a url", 1, None), "not a url");
    }

    // ── What to wait for ─────────────────────────────────────────────────────

    #[test]
    fn a_playlist_of_complete_segments_waits_for_the_next_segment() {
        let summary = live_summary(vec![segment("a.m4s", 7, 4.0), segment("b.m4s", 8, 4.0)]);
        assert_eq!(
            next_live_wait(&summary),
            Some(LiveWait {
                msn: 9,
                part: None
            })
        );
        assert!(!next_live_wait(&summary).expect("a wait").is_part());
    }

    #[test]
    fn a_parent_segment_still_being_appended_waits_for_its_next_part() {
        // Part Indexes are numbered from zero (RFC 8216bis §3.2), so three listed parts
        // means the next one to appear is index 3 of the same segment.
        let mut summary = live_summary(vec![
            segment("a.m4s", 7, 4.0),
            appending(
                8,
                vec![part("b.0.m4s", 1.0), part("b.1.m4s", 1.0), part("b.2.m4s", 1.0)],
            ),
        ]);
        summary.part_target_s = Some(1.0);
        assert_eq!(
            next_live_wait(&summary),
            Some(LiveWait {
                msn: 8,
                part: Some(3)
            })
        );
    }

    #[test]
    fn a_complete_last_segment_waits_for_part_zero_of_the_next_one() {
        // A finished parent segment takes no more parts, and a Part Index is relative to
        // its own parent, so the wait moves on rather than asking past the end of this one.
        let mut summary = live_summary(vec![SegmentEntry {
            uri: "a.m4s".into(),
            duration_s: 4.0,
            msn: Some(7),
            parts: vec![part("a.0.m4s", 1.0), part("a.1.m4s", 1.0)],
            ..Default::default()
        }]);
        summary.part_target_s = Some(1.0);
        assert_eq!(
            next_live_wait(&summary),
            Some(LiveWait {
                msn: 8,
                part: Some(0)
            })
        );
    }

    #[test]
    fn a_playlist_with_no_part_inf_never_asks_for_a_part() {
        // Parts can only be waited for where EXT-X-PART-INF says they are published.
        let summary = live_summary(vec![appending(8, vec![part("b.0.m4s", 1.0)])]);
        assert_eq!(next_live_wait(&summary).and_then(|w| w.part), None);
    }

    #[test]
    fn a_playlist_that_numbers_no_media_has_nothing_to_wait_beyond() {
        assert_eq!(next_live_wait(&live_summary(Vec::new())), None);
        let unnumbered = live_summary(vec![SegmentEntry {
            uri: "a.m4s".into(),
            duration_s: 4.0,
            msn: None,
            ..Default::default()
        }]);
        assert_eq!(next_live_wait(&unnumbered), None);
    }

    #[test]
    fn a_trailing_gap_part_is_not_the_last_fetchable_part() {
        let mut parts = vec![part("b.0.m4s", 1.0), part("b.1.m4s", 1.0)];
        parts.push(PartEntry {
            uri: "b.2.m4s".into(),
            duration_s: 1.0,
            gap: true,
            ..Default::default()
        });
        let appending = appending(8, parts);
        assert_eq!(last_fetchable_part_index(&appending), Some(1));

        // The GAP still holds Part Index 2, so the next index to ask an origin for is 3:
        // asking for 2 would return a body this tab already has.
        let mut summary = live_summary(vec![appending]);
        summary.part_target_s = Some(1.0);
        assert_eq!(next_live_wait(&summary).and_then(|w| w.part), Some(3));
    }

    #[test]
    fn a_gap_at_the_awaited_index_moves_the_wait_on_rather_than_repeating_it() {
        let mut before = live_summary(vec![appending(8, vec![part("b.0.m4s", 1.0)])]);
        before.part_target_s = Some(1.0);
        let asked = next_live_wait(&before).expect("a wait");
        assert_eq!(
            asked,
            LiveWait {
                msn: 8,
                part: Some(1)
            }
        );

        // The origin answered the directive with a GAP: the index is filled, so nothing
        // fetchable appeared and the wait has to move on. Asking for Part Index 1 again
        // would be answered from a playlist this tab already holds, for as long as the
        // ceiling lasted.
        let mut after = before.clone();
        after.segments[0].parts.push(PartEntry {
            uri: "b.1.m4s".into(),
            duration_s: 1.0,
            gap: true,
            ..Default::default()
        });
        assert!(new_edge_items(&before, &after).is_empty());
        let next = next_live_wait(&after).expect("a wait");
        assert_ne!(next, asked);
        assert_eq!(
            next,
            LiveWait {
                msn: 8,
                part: Some(2)
            }
        );
    }

    #[test]
    fn a_parent_of_nothing_but_gap_parts_has_no_fetchable_part() {
        let gap = PartEntry {
            uri: "b.0.m4s".into(),
            duration_s: 1.0,
            gap: true,
            ..Default::default()
        };
        assert_eq!(last_fetchable_part_index(&appending(8, vec![gap])), None);
    }

    // ── What appeared ────────────────────────────────────────────────────────

    #[test]
    fn a_newly_appended_segment_is_the_only_thing_that_appeared() {
        let before = live_summary(vec![segment("a.m4s", 7, 4.0), segment("b.m4s", 8, 4.0)]);
        let after = live_summary(vec![
            segment("a.m4s", 7, 4.0),
            segment("b.m4s", 8, 4.0),
            segment("c.m4s", 9, 3.5),
        ]);
        let items = new_edge_items(&before, &after);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].uri, "c.m4s");
        assert_eq!(items[0].msn, Some(9));
        assert_eq!(items[0].media_duration_s, Some(3.5));
        assert_eq!(items[0].media_duration_source, Some(DurationSource::Extinf));
        assert_eq!(items[0].target_duration_s, Some(4.0));
        assert!(!items[0].is_part);
    }

    #[test]
    fn a_uri_that_slid_out_of_the_window_never_counts_as_an_appearance() {
        // A sliding window drops its oldest entry every time it grows. Only the new URI
        // appeared; the survivors did not, and the dropped one is not in `after` at all.
        let before = live_summary(vec![segment("a.m4s", 7, 4.0), segment("b.m4s", 8, 4.0)]);
        let after = live_summary(vec![segment("b.m4s", 8, 4.0), segment("c.m4s", 9, 4.0)]);
        let items = new_edge_items(&before, &after);
        let uris: Vec<&str> = items.iter().map(|item| item.uri.as_str()).collect();
        assert_eq!(uris, vec!["c.m4s"]);
    }

    #[test]
    fn nothing_appeared_when_the_reload_returned_the_same_window() {
        let before = live_summary(vec![segment("a.m4s", 7, 4.0)]);
        assert!(new_edge_items(&before, &before.clone()).is_empty());
    }

    #[test]
    fn a_newly_appended_part_is_what_appeared_at_a_low_latency_edge() {
        let mut before = live_summary(vec![appending(8, vec![part("b.0.m4s", 1.0)])]);
        before.part_target_s = Some(1.0);
        let mut after = live_summary(vec![appending(
            8,
            vec![part("b.0.m4s", 1.0), part("b.1.m4s", 1.0)],
        )]);
        after.part_target_s = Some(1.0);
        let items = new_edge_items(&before, &after);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].uri, "b.1.m4s");
        assert!(items[0].is_part);
        // A part's percentages are taken against PART-TARGET, not TARGETDURATION.
        assert_eq!(items[0].target_duration_s, Some(1.0));
        assert_eq!(
            items[0].target_duration_source,
            Some(DurationSource::PartTarget)
        );
        assert_eq!(
            items[0].media_duration_source,
            Some(DurationSource::PartDuration)
        );
    }

    #[test]
    fn a_gap_part_never_appears_because_it_carries_nothing_to_fetch() {
        let mut before = live_summary(vec![appending(8, vec![part("b.0.m4s", 1.0)])]);
        before.part_target_s = Some(1.0);
        let mut after = live_summary(vec![appending(
            8,
            vec![
                part("b.0.m4s", 1.0),
                PartEntry {
                    uri: "b.1.m4s".into(),
                    duration_s: 1.0,
                    gap: true,
                    ..Default::default()
                },
            ],
        )]);
        after.part_target_s = Some(1.0);
        assert!(new_edge_items(&before, &after).is_empty());
    }

    #[test]
    fn a_parent_segment_completing_is_not_a_new_download() {
        // The parts were all published while it was being appended, so fetching the whole
        // segment now would be re-downloading media this tab could already have had.
        let mut before = live_summary(vec![appending(
            8,
            vec![part("b.0.m4s", 1.0), part("b.1.m4s", 1.0)],
        )]);
        before.part_target_s = Some(1.0);
        let mut after = live_summary(vec![SegmentEntry {
            uri: "b.m4s".into(),
            duration_s: 2.0,
            msn: Some(8),
            parts: vec![part("b.0.m4s", 1.0), part("b.1.m4s", 1.0)],
            ..Default::default()
        }]);
        after.part_target_s = Some(1.0);
        assert!(new_edge_items(&before, &after).is_empty());
    }

    #[test]
    fn a_part_sharing_a_uri_at_a_new_byte_range_is_a_new_part() {
        // Packagers routinely put every part of a segment in one file. Keyed on the URI
        // alone, every part after the first would look like something already seen.
        let ranged = |offset: u64| PartEntry {
            uri: "seg267.m4s".into(),
            duration_s: 1.0,
            byterange: Some(RequestRange::from_length_with_offset(1_000, offset)),
            independent: true,
            gap: false,
        };
        let mut before = live_summary(vec![appending(8, vec![ranged(0)])]);
        before.part_target_s = Some(1.0);
        let mut after = live_summary(vec![appending(8, vec![ranged(0), ranged(1_000)])]);
        after.part_target_s = Some(1.0);
        let items = new_edge_items(&before, &after);
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].byterange,
            Some(RequestRange::from_length_with_offset(1_000, 1_000))
        );
    }

    #[test]
    fn an_unparted_segment_in_a_low_latency_playlist_is_still_an_appearance() {
        let mut before = live_summary(vec![segment("a.m4s", 7, 4.0)]);
        before.part_target_s = Some(1.0);
        let mut after = live_summary(vec![segment("a.m4s", 7, 4.0), segment("b.m4s", 8, 4.0)]);
        after.part_target_s = Some(1.0);
        let items = new_edge_items(&before, &after);
        assert_eq!(items.len(), 1);
        assert!(!items[0].is_part);
    }

    // ── Bounds ───────────────────────────────────────────────────────────────

    #[test]
    fn the_wait_for_a_part_is_bounded_by_part_hold_back() {
        let mut summary = live_summary(vec![segment("a.m4s", 7, 4.0)]);
        summary.part_target_s = Some(0.5);
        summary.part_hold_back_s = Some(1.5);
        // Three PART-HOLD-BACKs for a part, and three HOLD-BACKs for a segment: a part
        // that arrives every half second is not worth waiting a segment hold-back for.
        assert_eq!(live_wait_timeout_ms(&summary, true), 4_500.0);
        assert_eq!(live_wait_timeout_ms(&summary, false), 36_000.0);
    }

    #[test]
    fn a_playlist_with_no_hold_back_falls_back_to_three_target_durations() {
        let summary = MediaSummary {
            target_duration_s: Some(6.0),
            is_live: true,
            ..Default::default()
        };
        assert_eq!(live_wait_timeout_ms(&summary, true), 18_000.0);
        // And with nothing declared at all the ceiling is still finite and positive, so
        // the UI cannot be left waiting on a playlist that says nothing.
        let bare = MediaSummary {
            is_live: true,
            ..Default::default()
        };
        let bound = live_wait_timeout_ms(&bare, false);
        assert!(bound.is_finite() && bound > 0.0, "{bound}");
        // A playlist that asks for an hour of blocking still cannot have one.
        let absurd = MediaSummary {
            target_duration_s: Some(3_600.0),
            is_live: true,
            ..Default::default()
        };
        assert_eq!(live_wait_timeout_ms(&absurd, false), MAX_LIVE_WAIT_MS);
    }

    #[test]
    fn a_wait_with_no_clock_to_bound_it_still_gets_a_ceiling() {
        // No deadline means there was no clock to set one from. The alternative to a
        // fallback here is a reload nothing can give up on.
        assert_eq!(remaining_wait_ms(None, 4_000.0), Some(4_000.0));
    }

    #[test]
    fn the_poll_interval_is_never_shorter_than_a_second() {
        let mut summary = live_summary(vec![segment("a.m4s", 7, 4.0)]);
        summary.part_target_s = Some(0.2);
        assert_eq!(poll_interval_ms(&summary, true), MIN_POLL_INTERVAL_MS);
        assert_eq!(poll_interval_ms(&summary, false), 4_000.0);
    }

    // ── One attempt of a wait ────────────────────────────────────────────────

    #[test]
    fn the_first_attempt_of_a_wait_is_given_the_whole_ceiling() {
        let (attempt, granted) = plan_reload_attempt(36_000.0, 36_000.0, true, Some);
        assert_eq!(granted, Some(36_000.0));
        assert_eq!(
            attempt.give_up_text,
            "this request was still open after 36000 ms with nothing new on the playlist, so \
             the wait was given up on"
        );
    }

    #[test]
    fn a_later_attempt_reports_how_long_that_request_was_open_and_not_the_whole_wait() {
        // Six seconds left of a thirty-six second wait: the request the origin held ran
        // under the remainder, so a status quoting the ceiling overstates it six-fold.
        let (attempt, granted) = plan_reload_attempt(6_000.0, 36_000.0, true, Some);
        assert_eq!(granted, Some(6_000.0));
        assert_eq!(
            attempt.give_up_text,
            "this request was still open for the last 6000 ms of a 36000 ms wait with nothing \
             new on the playlist, so the wait was given up on"
        );
    }

    #[test]
    fn a_remainder_below_the_floor_is_rounded_up_and_the_status_quotes_what_was_granted() {
        // Not the 200 ms that were left, either: the scope was opened for a second, and
        // that is how long the request could have been held for.
        let (attempt, granted) = plan_reload_attempt(200.0, 36_000.0, true, Some);
        assert_eq!(granted, Some(MIN_LIVE_WAIT_MS));
        assert_eq!(
            attempt.give_up_text,
            "this request was still open for the last 1000 ms of a 36000 ms wait with nothing \
             new on the playlist, so the wait was given up on"
        );
    }

    #[test]
    fn directives_are_sent_only_where_the_held_request_could_be_given_up_on() {
        let (blocking, _) = plan_reload_attempt(9_000.0, 9_000.0, true, Some);
        assert_eq!(blocking.url, ReloadUrl::Blocking);

        // No scope means no way to end a held reload, and an uncapped hold is a hung page.
        let (uncapped, scope) = plan_reload_attempt(9_000.0, 9_000.0, true, |_| None::<f64>);
        assert_eq!(scope, None);
        assert_eq!(uncapped.url, ReloadUrl::Plain);

        // And an origin that never advertised CAN-BLOCK-RELOAD is polled, capped or not.
        let (polled, _) = plan_reload_attempt(9_000.0, 9_000.0, false, Some);
        assert_eq!(polled.url, ReloadUrl::Plain);
    }

    // ── What a run says about its waits ──────────────────────────────────────

    #[test]
    fn a_run_that_will_wait_says_what_the_lag_means_and_how_the_edge_was_watched() {
        let mut summary = live_summary(vec![segment("a.m4s", 7, 4.0)]);
        let ids = |summary: &MediaSummary| -> Vec<&'static str> {
            live_run_notes(summary)
                .into_iter()
                .map(|(id, _)| id)
                .collect()
        };
        assert_eq!(
            ids(&summary),
            vec![NOTE_LIVE_OBSERVATION_BOUND, NOTE_LIVE_BLOCKING_RELOAD]
        );

        summary.can_block_reload = false;
        assert_eq!(
            ids(&summary),
            vec![NOTE_LIVE_OBSERVATION_BOUND, NOTE_LIVE_POLLED]
        );
    }

    #[test]
    fn a_playlist_that_numbers_no_media_claims_no_wait_it_will_never_make() {
        // The wait ends before it makes a request, so there are no reloads carrying
        // _HLS_msn to describe and no rows below to put an observation lag on.
        let summary = live_summary(Vec::new());
        assert_eq!(next_live_wait(&summary), None);
        assert!(live_run_notes(&summary).is_empty());
    }

    // ── Observation lag ──────────────────────────────────────────────────────

    #[test]
    fn observation_lag_is_the_dispatch_gap_plus_the_authoritative_download() {
        // Seen at 1000, requested at 1020, and the browser's own total says 300 ms.
        let measured = measurement(
            AppClockSpan {
                request_ms: 1_020.0,
                headers_ms: 1_100.0,
                body_end_ms: 1_400.0,
            },
            Some(ResourceTimingSample {
                start_time: 1_020.0,
                response_end: 1_320.0,
                ..Default::default()
            }),
        );
        assert_eq!(observation_lag_ms(1_000.0, &measured), Some(320.0));
    }

    #[test]
    fn without_a_resource_timing_entry_the_lag_uses_the_app_observed_total() {
        let measured = measurement(
            AppClockSpan {
                request_ms: 1_020.0,
                headers_ms: 1_100.0,
                body_end_ms: 1_400.0,
            },
            None,
        );
        // 20 ms of this tab's own scheduling plus a 380 ms app-observed total, which is
        // why the figure is an upper bound rather than a measurement of the fetch.
        assert_eq!(observation_lag_ms(1_000.0, &measured), Some(400.0));
    }

    #[test]
    fn a_clock_that_ran_backwards_produced_no_observation_lag() {
        let measured = measurement(
            AppClockSpan {
                request_ms: 900.0,
                headers_ms: 950.0,
                body_end_ms: 1_000.0,
            },
            None,
        );
        // The playlist cannot have been read after the request it caused was issued.
        assert_eq!(observation_lag_ms(1_000.0, &measured), None);
    }

    #[test]
    fn a_measurement_with_no_total_has_no_lag_either() {
        let measured = measurement(
            AppClockSpan {
                request_ms: 1_020.0,
                headers_ms: 900.0,
                body_end_ms: 800.0,
            },
            None,
        );
        assert_eq!(observation_lag_ms(1_000.0, &measured), None);
    }

    // ── Planning works on a live summary ─────────────────────────────────────

    #[test]
    fn a_live_summary_is_something_the_planner_can_work_from() {
        // The orchestrator used to decline every live media sample. What replaced that
        // decision is these functions, so a live summary has to plan end to end.
        let mut before = live_summary(vec![
            segment("a.m4s", 7, 4.0),
            appending(8, vec![part("b.0.m4s", 1.0)]),
        ]);
        before.part_target_s = Some(1.0);
        before.part_hold_back_s = Some(3.0);
        assert!(before.is_live);

        let wait = next_live_wait(&before).expect("a wait");
        assert!(wait.is_part());
        let url = blocking_reload_url("https://example.com/v.m3u8", wait.msn, wait.part);
        assert_eq!(url, "https://example.com/v.m3u8?_HLS_msn=8&_HLS_part=1");
        assert_eq!(live_wait_timeout_ms(&before, wait.is_part()), 9_000.0);

        let mut after = before.clone();
        after.segments[1]
            .parts
            .push(part("https://example.com/b.1.m4s", 1.0));
        let items = new_edge_items(&before, &after);
        let spec = items[0].spec(RequestClass::VideoSegment, "720p");
        assert_eq!(spec.uri, "https://example.com/b.1.m4s");
        assert_eq!(spec.class, RequestClass::VideoSegment);
        assert_eq!(spec.target_duration_s, Some(1.0));
        assert_eq!(spec.msn, Some(8));
    }
}
