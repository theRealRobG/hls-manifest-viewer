//! Entry point for the Author section: fetch a stream, probe media, run Authoring Spec rules.

use std::collections::{HashMap, HashSet};

use crate::utils::mp4_probe::{
    probe_init_segment, scan_segment_bytes, InitSegmentProbe, SegmentScanHints, VideoCodecHint,
};
use crate::utils::network::{
    fetch_array_buffer, fetch_text, FetchArrayBufferResonse, FetchError, RequestRange,
};
use crate::utils::validator::parser::parse_media_playlist;
use crate::utils::validator::types::{CheckGroup, Issue, MediaPlaylist, Severity};
use crate::utils::validator::{absolute_fetch_uri, apply_master_definitions, collect_stream, now_ms};

use super::context::{
    AuthoringContext, InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample,
};
use super::profile::AuthorProfile;
use super::run_authoring_checks;

/// How many segments per playlist are sampled when deep checks are enabled.
const MAX_DEEP_SEGMENT_SAMPLES_PER_PLAYLIST: usize = 3;
/// Extra samples for video playlists (better measured bitrate / IDR interval).
const MAX_DEEP_VIDEO_SEGMENT_SAMPLES: usize = 5;
/// Phase B light samples: one segment from each I-frame playlist (for §6.10 moof).
const MAX_IFRAME_LIGHT_SAMPLES: usize = 4;
/// Phase B: max subtitle playlists / VTT files to fetch for §5.3.
const MAX_SUBTITLE_PLAYLISTS: usize = 4;
const MAX_VTT_SAMPLES: usize = 4;

#[derive(Debug, Clone, Default)]
pub struct AuthorOptions {
    pub profile: AuthorProfile,
    pub deep_checks: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorReport {
    pub url: String,
    pub profile: String,
    pub deep_checks: bool,
    pub result: String,
    pub issues: Vec<Issue>,
    pub check_groups: Vec<CheckGroup>,
    pub probe_notes: Vec<String>,
    pub total_errors: usize,
    pub total_warnings: usize,
    pub total_info: usize,
    pub playlist_count: usize,
    pub init_probe_count: usize,
    pub segment_sample_count: usize,
    pub webvtt_sample_count: usize,
    pub elapsed_ms: u64,
}

/// Fetch `url`, probe its media and run the Apple HLS Authoring Specification rules.
pub async fn run_author_report(url: &str, options: AuthorOptions) -> Result<AuthorReport, FetchError> {
    let start = now_ms();
    let stream = collect_stream(url).await?;
    let mut playlists = stream.playlists;
    let mut fetch_issues = stream.fetch_issues;

    // Phase B: also fetch a few subtitle media playlists (Validate does not).
    if let Some(master) = stream.master.as_ref() {
        let (subs, sub_issues) = fetch_subtitle_playlists(url, master).await;
        fetch_issues.extend(sub_issues);
        playlists.extend(subs);
    }

    let author_opts = ValidateAuthorOptions {
        profile: options.profile,
        deep_checks: options.deep_checks,
    };
    let (init_probes, segment_samples, webvtt_samples, sample_notes) =
        collect_media_samples(&playlists, options.deep_checks).await;
    let ctx = AuthoringContext::new(
        stream.master.as_ref(),
        &playlists,
        &author_opts,
        &init_probes,
        &segment_samples,
        &webvtt_samples,
    );

    let mut issues = fetch_issues;
    issues.extend(run_authoring_checks(&ctx));
    let mut probe_notes = ctx.probe_notes.clone();
    probe_notes.extend(sample_notes);

    let total_errors = issues.iter().filter(|i| i.severity == Severity::Error).count();
    let total_warnings = issues.iter().filter(|i| i.severity == Severity::Warn).count();
    let total_info = issues.iter().filter(|i| i.severity == Severity::Info).count();

    Ok(AuthorReport {
        url: url.to_string(),
        profile: options.profile.as_str().to_string(),
        deep_checks: options.deep_checks,
        result: if total_errors == 0 {
            "PASS".to_string()
        } else {
            "FAIL".to_string()
        },
        check_groups: categorize_author_issues(&issues),
        issues,
        probe_notes,
        total_errors,
        total_warnings,
        total_info,
        playlist_count: playlists.len(),
        init_probe_count: init_probes.len(),
        segment_sample_count: segment_samples.len(),
        webvtt_sample_count: webvtt_samples.len(),
        elapsed_ms: (now_ms() - start) as u64,
    })
}

fn parse_byterange(br: &str) -> Option<RequestRange> {
    // HLS BYTERANGE is `n` or `n@o`
    let (length, offset) = if let Some((n, o)) = br.split_once('@') {
        (n.parse::<u64>().ok()?, o.parse::<u64>().ok()?)
    } else {
        (br.parse::<u64>().ok()?, 0)
    };
    Some(RequestRange::from_length_with_offset(length, offset))
}

/// Evenly spaced segment indices, so a deep sample strides across the whole asset
/// instead of measuring only its opening seconds.
pub(crate) fn stride_indices(segment_count: usize, max_samples: usize) -> Vec<usize> {
    if segment_count == 0 || max_samples == 0 {
        return Vec::new();
    }
    if segment_count <= max_samples {
        return (0..segment_count).collect();
    }
    let last = segment_count - 1;
    let steps = (max_samples - 1).max(1);
    (0..max_samples).map(|k| k * last / steps).collect()
}

/// Bytes to attribute to a possibly-ranged fetch, plus whether `Range` was ignored.
/// A server that ignores it answers 200 with the whole resource, which must not be
/// charged to one segment's measured bit rate.
fn ranged_body(resp: &FetchArrayBufferResonse, range: Option<RequestRange>) -> (&[u8], bool) {
    let body = resp.response_body.as_slice();
    let Some(range) = range else {
        return (body, false);
    };
    let wanted = (range.end + 1).saturating_sub(range.start) as usize;
    if resp.status == 206 || wanted == 0 || body.len() <= wanted {
        return (body, false);
    }
    let start = range.start as usize;
    let slice = body
        .get(start..start + wanted)
        .unwrap_or_else(|| &body[..wanted]);
    (slice, true)
}

/// Notes explaining anything that limited how media samples could be read: servers
/// that ignored `Range`, and fetches that failed outright.
#[derive(Default)]
struct SampleNotes {
    range_ignored: usize,
    first_range_uri: Option<String>,
    failures: Vec<String>,
    failure_count: usize,
}

/// Failed fetches are listed individually up to this many, then only counted.
const MAX_FETCH_FAILURE_NOTES: usize = 5;

impl SampleNotes {
    fn record_ignored(&mut self, uri: &str) {
        self.range_ignored += 1;
        if self.first_range_uri.is_none() {
            self.first_range_uri = Some(uri.to_string());
        }
    }

    /// A sample that could not be fetched. Probing is best-effort, so this only
    /// records why a check may lack evidence — it never fails the Author run.
    fn record_failure(&mut self, kind: &str, uri: &str, err: &FetchError) {
        self.failure_count += 1;
        if self.failures.len() < MAX_FETCH_FAILURE_NOTES {
            self.failures.push(format!(
                "{kind} probe fetch failed for '{uri}': {}",
                err.error
            ));
        }
    }

    fn into_notes(self) -> Vec<String> {
        let mut notes = Vec::new();
        if let Some(uri) = self.first_range_uri {
            notes.push(format!(
                "{} ranged request(s) answered without HTTP 206 (e.g. '{uri}') — the full resource was returned, so byte accounting was clamped to the requested range.",
                self.range_ignored
            ));
        }
        let hidden = self.failure_count - self.failures.len();
        notes.extend(self.failures);
        if hidden > 0 {
            notes.push(format!(
                "{hidden} further probe fetch failure(s) not listed — affected checks ran without that evidence."
            ));
        }
        notes
    }
}

/// One segment fetch for the Phase B light / Phase C deep samplers.
struct SegmentJob {
    playlist_name: String,
    segment_index: usize,
    uri: String,
    duration: f64,
    range: Option<RequestRange>,
    is_iframe_playlist: bool,
    hints: SegmentScanHints,
}

/// The playlist's first EXT-X-MAP as a fetchable URI plus the BYTERANGE from that
/// same tag. Variables are substituted here because MAP URIs are stored raw.
fn init_map_fetch_target(pl: &MediaPlaylist) -> Option<(String, Option<String>)> {
    let map = pl.init_maps.first()?;
    Some((
        absolute_fetch_uri(&pl.url, &map.uri, &pl.definitions),
        map.byterange.clone(),
    ))
}

/// The init segment probed for this playlist, if one was fetched.
fn probe_for_playlist<'a>(
    pl: &MediaPlaylist,
    init_probes: &'a [InitProbeEntry],
) -> Option<&'a InitSegmentProbe> {
    init_probes
        .iter()
        .find(|e| e.playlist_names.iter().any(|n| n == &pl.name))
        .map(|e| &e.probe)
}

/// Video codec for a playlist, from its CODECS attribute or the probed init fourCC.
fn codec_hint_for_playlist(pl: &MediaPlaylist, init_probes: &[InitProbeEntry]) -> VideoCodecHint {
    if let Some(codecs) = pl.codecs.as_deref() {
        for tok in codecs.split(',') {
            let hint = VideoCodecHint::from_codec_str(tok);
            if hint != VideoCodecHint::Unknown {
                return hint;
            }
        }
    }
    probe_for_playlist(pl, init_probes)
        .and_then(|p| p.video_sample_fourcc.as_deref())
        .map(VideoCodecHint::from_codec_str)
        .unwrap_or_default()
}

/// Codec plus the track IDs a segment scan needs to tell this playlist's media
/// fragments from the timed-metadata ones alongside them.
fn scan_hints_for_playlist(pl: &MediaPlaylist, init_probes: &[InitProbeEntry]) -> SegmentScanHints {
    let codec = codec_hint_for_playlist(pl, init_probes);
    match probe_for_playlist(pl, init_probes) {
        Some(probe) => probe.scan_hints(codec),
        None => SegmentScanHints::for_codec(codec),
    }
}

/// Fetch every job in parallel and turn each body into a [`SegmentSample`], noting
/// the ones that could not be fetched.
async fn fetch_segment_samples(
    jobs: &[SegmentJob],
    samples: &mut Vec<SegmentSample>,
    notes: &mut SampleNotes,
) {
    let fetches = futures::future::join_all(jobs.iter().map(|job| {
        let uri = job.uri.clone();
        let range = job.range;
        async move { fetch_array_buffer(uri, range).await }
    }))
    .await;
    for (job, result) in jobs.iter().zip(fetches) {
        match result {
            Ok(resp) => {
                let (bytes, range_ignored) = ranged_body(&resp, job.range);
                if range_ignored {
                    notes.record_ignored(&job.uri);
                }
                let scan = scan_segment_bytes(bytes, job.hints);
                samples.push(SegmentSample::from_scan(
                    job.playlist_name.clone(),
                    job.segment_index,
                    job.uri.clone(),
                    job.duration,
                    bytes.len(),
                    job.is_iframe_playlist,
                    &scan,
                ));
            }
            Err(e) => notes.record_failure("Media segment", &job.uri, &e),
        }
    }
}

async fn fetch_subtitle_playlists(
    master_url: &str,
    master: &crate::utils::validator::types::MasterPlaylist,
) -> (Vec<MediaPlaylist>, Vec<Issue>) {
    let mut issues = Vec::new();
    let mut jobs: Vec<(String, String, String)> = Vec::new(); // uri, name, group
    let mut seen = HashSet::new();
    for r in master
        .media_renditions
        .iter()
        .filter(|r| r.media_type == "SUBTITLES" && r.uri.is_some())
        .take(MAX_SUBTITLE_PLAYLISTS)
    {
        let Some(uri) = &r.uri else { continue };
        let fetch_uri = absolute_fetch_uri(master_url, uri, &master.definitions);
        if !seen.insert(fetch_uri.clone()) {
            continue;
        }
        jobs.push((fetch_uri, r.name.clone(), r.group_id.clone()));
    }

    let fetches = futures::future::join_all(jobs.iter().map(|(uri, _, _)| {
        let uri = uri.clone();
        async move { fetch_text(uri).await }
    }))
    .await;

    let mut playlists = Vec::new();
    for ((fetch_uri, name, group_id), result) in jobs.iter().zip(fetches) {
        match result {
            Ok(resp) => {
                let mut pl = MediaPlaylist::new(
                    format!("subtitles/{name} ({group_id})"),
                    fetch_uri.clone(),
                );
                pl.media_type = "SUBTITLES".to_string();
                pl.group_id = Some(group_id.clone());
                apply_master_definitions(
                    &resp.response_text,
                    &master.definitions,
                    &mut pl.definitions,
                );
                parse_media_playlist(fetch_uri, &resp.response_text, &mut pl);
                playlists.push(pl);
            }
            Err(e) => {
                issues.push(Issue::warn(format!(
                    "Could not fetch subtitle playlist '{fetch_uri}': {e}"
                )));
            }
        }
    }
    (playlists, issues)
}

/// Fetch unique init segments, Phase B light samples, and optional Phase C deep segment
/// samples, along with notes about anything that limits how the samples can be read.
async fn collect_media_samples(
    playlists: &[MediaPlaylist],
    deep: bool,
) -> (
    Vec<InitProbeEntry>,
    Vec<SegmentSample>,
    Vec<WebVttSample>,
    Vec<String>,
) {
    let mut notes = SampleNotes::default();

    // --- Init probes (Phase B core) ---
    struct InitJob {
        uri: String,
        byterange: Option<String>,
        range: Option<RequestRange>,
        playlist_names: Vec<String>,
        media_types: Vec<String>,
    }
    let mut init_by_key: HashMap<String, InitJob> = HashMap::new();
    for pl in playlists {
        let Some((uri, br)) = init_map_fetch_target(pl) else {
            continue;
        };
        let key = format!("{}|{}", uri, br.as_deref().unwrap_or(""));
        let entry = init_by_key.entry(key).or_insert_with(|| InitJob {
            uri,
            range: br.as_deref().and_then(parse_byterange),
            byterange: br,
            playlist_names: Vec::new(),
            media_types: Vec::new(),
        });
        if !entry.playlist_names.iter().any(|n| n == &pl.name) {
            entry.playlist_names.push(pl.name.clone());
        }
        if !entry.media_types.iter().any(|t| t == &pl.media_type) {
            entry.media_types.push(pl.media_type.clone());
        }
    }
    // Findings and probe notes name init segments, so the probe order has to come
    // from the keys rather than from however the map happens to iterate.
    let mut keyed_jobs: Vec<(String, InitJob)> = init_by_key.into_iter().collect();
    keyed_jobs.sort_by(|(a, _), (b, _)| a.cmp(b));
    let init_jobs: Vec<InitJob> = keyed_jobs.into_iter().map(|(_, job)| job).collect();

    let init_fetches = futures::future::join_all(init_jobs.iter().map(|job| {
        let uri = job.uri.clone();
        let range = job.range;
        async move { fetch_array_buffer(uri, range).await }
    }))
    .await;

    let mut init_probes = Vec::new();
    for (job, result) in init_jobs.iter().zip(init_fetches) {
        match result {
            Ok(resp) => {
                let (bytes, range_ignored) = ranged_body(&resp, job.range);
                if range_ignored {
                    notes.record_ignored(&job.uri);
                }
                init_probes.push(InitProbeEntry {
                    uri: job.uri.clone(),
                    byterange: job.byterange.clone(),
                    playlist_names: job.playlist_names.clone(),
                    media_types: job.media_types.clone(),
                    probe: probe_init_segment(bytes),
                });
            }
            Err(e) => notes.record_failure("Init segment", &job.uri, &e),
        }
    }

    // --- Segment samples ---
    let mut segment_samples = Vec::new();

    // Phase B light: one fMP4 I-frame segment per playlist (capped) for §6.10.
    {
        let mut ranged_jobs: Vec<SegmentJob> = Vec::new();
        for pl in playlists.iter().filter(|p| p.is_iframe).take(MAX_IFRAME_LIGHT_SAMPLES) {
            let Some((idx, seg)) = pl.segments.iter().enumerate().next() else {
                continue;
            };
            ranged_jobs.push(SegmentJob {
                playlist_name: pl.name.clone(),
                segment_index: idx,
                // Already variable-substituted and absolute — see parse_media_playlist.
                uri: seg.uri.clone(),
                duration: seg.duration,
                range: seg.byterange.as_deref().and_then(parse_byterange),
                is_iframe_playlist: true,
                hints: scan_hints_for_playlist(pl, &init_probes),
            });
        }
        fetch_segment_samples(&ranged_jobs, &mut segment_samples, &mut notes).await;
    }

    // Phase C deep samples (optional).
    if deep {
        let mut ranged_jobs: Vec<SegmentJob> = Vec::new();
        // Subtitle playlists are sampled as WebVTT text below, not as media segments.
        for pl in playlists.iter().filter(|p| p.media_type != "SUBTITLES") {
            let limit = if pl.media_type == "VIDEO" && !pl.is_iframe {
                MAX_DEEP_VIDEO_SEGMENT_SAMPLES
            } else {
                MAX_DEEP_SEGMENT_SAMPLES_PER_PLAYLIST
            };
            let hints = scan_hints_for_playlist(pl, &init_probes);
            for idx in stride_indices(pl.segments.len(), limit) {
                let seg = &pl.segments[idx];
                // Skip duplicates already sampled as iframe light probes.
                if pl.is_iframe && idx == 0 {
                    continue;
                }
                ranged_jobs.push(SegmentJob {
                    playlist_name: pl.name.clone(),
                    segment_index: idx,
                    uri: seg.uri.clone(),
                    duration: seg.duration,
                    range: seg.byterange.as_deref().and_then(parse_byterange),
                    is_iframe_playlist: pl.is_iframe,
                    hints,
                });
            }
        }
        fetch_segment_samples(&ranged_jobs, &mut segment_samples, &mut notes).await;
    }

    // --- WebVTT samples (§5.3) ---
    let mut webvtt_samples = Vec::new();
    {
        let mut jobs: Vec<(String, String)> = Vec::new();
        for pl in playlists
            .iter()
            .filter(|p| p.media_type == "SUBTITLES")
            .take(MAX_VTT_SAMPLES)
        {
            let Some(seg) = pl.segments.first() else {
                continue;
            };
            // Skip fMP4 subtitle segments (IMSC) — §5.3 is WebVTT text files.
            if seg.map_uri.is_some() {
                continue;
            }
            jobs.push((pl.name.clone(), seg.uri.clone()));
        }
        let fetches = futures::future::join_all(jobs.iter().map(|(_, uri)| {
            let uri = uri.clone();
            async move { fetch_text(uri).await }
        }))
        .await;
        for ((name, uri), result) in jobs.iter().zip(fetches) {
            match result {
                Ok(resp) => {
                    let body = resp.response_text;
                    let lower = body.to_ascii_lowercase();
                    webvtt_samples.push(WebVttSample {
                        playlist_name: name.clone(),
                        uri: uri.clone(),
                        has_webvtt_header: lower.contains("webvtt"),
                        has_x_timestamp_map: body.contains("X-TIMESTAMP-MAP"),
                    });
                }
                Err(e) => notes.record_failure("WebVTT", uri, &e),
            }
        }
    }

    (
        init_probes,
        segment_samples,
        webvtt_samples,
        notes.into_notes(),
    )
}

struct AuthorCheckDef {
    name: &'static str,
    reference: &'static str,
    keywords: &'static [&'static str],
}

const AUTHOR_CHECK_DEFS: &[AuthorCheckDef] = &[
    AuthorCheckDef {
        name: "Video",
        reference: "Apple Authoring Spec §1",
        keywords: &["Apple Authoring Spec §1."],
    },
    AuthorCheckDef {
        name: "Audio",
        reference: "Apple Authoring Spec §2",
        keywords: &["Apple Authoring Spec §2."],
    },
    AuthorCheckDef {
        name: "Ads",
        reference: "Apple Authoring Spec §3",
        keywords: &["Apple Authoring Spec §3."],
    },
    AuthorCheckDef {
        name: "Accessibility / Subtitles",
        reference: "Apple Authoring Spec §4–5",
        keywords: &["Apple Authoring Spec §4.", "Apple Authoring Spec §5."],
    },
    AuthorCheckDef {
        name: "Trick Play",
        reference: "Apple Authoring Spec §6",
        keywords: &["Apple Authoring Spec §6."],
    },
    AuthorCheckDef {
        name: "Segmentation",
        reference: "Apple Authoring Spec §7",
        keywords: &["Apple Authoring Spec §7."],
    },
    AuthorCheckDef {
        name: "Media Playlists",
        reference: "Apple Authoring Spec §8",
        keywords: &["Apple Authoring Spec §8."],
    },
    AuthorCheckDef {
        name: "Multivariant Playlist",
        reference: "Apple Authoring Spec §9",
        keywords: &["Apple Authoring Spec §9."],
    },
    AuthorCheckDef {
        name: "Delivery",
        reference: "Apple Authoring Spec §10",
        keywords: &["Apple Authoring Spec §10."],
    },
    AuthorCheckDef {
        name: "Privacy",
        reference: "Apple Authoring Spec §11",
        keywords: &["Apple Authoring Spec §11."],
    },
    AuthorCheckDef {
        name: "Security",
        reference: "Apple Authoring Spec §12",
        keywords: &["Apple Authoring Spec §12."],
    },
    AuthorCheckDef {
        name: "Content Protection",
        reference: "Apple Authoring Spec §13",
        keywords: &["Apple Authoring Spec §13.", "Apple Authoring Spec §1.41"],
    },
    AuthorCheckDef {
        name: "Low-Latency HLS",
        reference: "Apple Authoring Spec §14",
        keywords: &["Apple Authoring Spec §14."],
    },
    AuthorCheckDef {
        name: "SharePlay / Spatial",
        reference: "Apple Authoring Spec §15–16",
        keywords: &["Apple Authoring Spec §15.", "Apple Authoring Spec §16."],
    },
];

/// Index of the single group an issue belongs to: the definition whose *longest*
/// keyword matches. Longest wins so that `§1.41` lands in Content Protection rather
/// than in Video, whose `§1.` keyword is a prefix of it. Ties keep the earlier group.
fn group_index_for_issue(message: &str) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None; // (keyword length, group index)
    for (index, def) in AUTHOR_CHECK_DEFS.iter().enumerate() {
        let Some(len) = def
            .keywords
            .iter()
            .filter(|k| message.contains(*k))
            .map(|k| k.len())
            .max()
        else {
            continue;
        };
        if best.is_none_or(|(best_len, _)| len > best_len) {
            best = Some((len, index));
        }
    }
    best.map(|(_, index)| index)
}

/// Worst finding in a group. Info-only findings are not a warning, but they are not
/// a clean pass either.
fn group_status(issues: &[Issue]) -> &'static str {
    if issues.iter().any(|i| i.severity == Severity::Error) {
        "FAIL"
    } else if issues.iter().any(|i| i.severity == Severity::Warn) {
        "WARN"
    } else if issues.is_empty() {
        "PASS"
    } else {
        "INFO"
    }
}

/// Group Author issues by spec section, emitting a PASS row for sections with no findings.
fn categorize_author_issues(issues: &[Issue]) -> Vec<CheckGroup> {
    let mut matched: Vec<Vec<Issue>> = vec![Vec::new(); AUTHOR_CHECK_DEFS.len()];
    let mut uncategorized: Vec<Issue> = Vec::new();
    for issue in issues {
        match group_index_for_issue(&issue.message) {
            Some(index) => matched[index].push(issue.clone()),
            None => uncategorized.push(issue.clone()),
        }
    }

    let mut groups: Vec<CheckGroup> = AUTHOR_CHECK_DEFS
        .iter()
        .zip(matched)
        .map(|(def, issues)| CheckGroup {
            name: def.name.to_string(),
            section: "Author".to_string(),
            reference: def.reference.to_string(),
            status: group_status(&issues).to_string(),
            issues,
        })
        .collect();

    if !uncategorized.is_empty() {
        groups.push(CheckGroup {
            name: "Other".to_string(),
            section: "Author".to_string(),
            reference: "Apple HLS Authoring Specification".to_string(),
            status: group_status(&uncategorized).to_string(),
            issues: uncategorized,
        });
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::validator::types::InitMap;

    #[test]
    fn stride_indices_spread_across_playlist() {
        assert_eq!(stride_indices(0, 5), Vec::<usize>::new());
        assert_eq!(stride_indices(3, 5), vec![0, 1, 2]);
        assert_eq!(stride_indices(100, 5), vec![0, 24, 49, 74, 99]);
        assert_eq!(stride_indices(100, 1), vec![0]);
        // Indices must stay distinct and in range so no segment is fetched twice.
        let idx = stride_indices(6, 5);
        assert_eq!(idx.len(), 5);
        assert!(idx.windows(2).all(|w| w[0] < w[1]));
        assert!(idx.iter().all(|&i| i < 6));
    }

    fn response(len: usize, status: u16) -> FetchArrayBufferResonse {
        FetchArrayBufferResonse {
            response_body: vec![0u8; len],
            content_type: None,
            url: "https://example.com/s.m4s".to_string(),
            status,
        }
    }

    #[test]
    fn ranged_body_clamps_when_range_was_ignored() {
        let resp = response(5_000, 200);
        let range = RequestRange::from_length_with_offset(100, 10);
        let (bytes, ignored) = ranged_body(&resp, Some(range));
        assert!(ignored);
        assert_eq!(bytes.len(), 100);
    }

    #[test]
    fn ranged_body_trusts_partial_content() {
        let resp = response(100, 206);
        let range = RequestRange::from_length_with_offset(100, 10);
        let (bytes, ignored) = ranged_body(&resp, Some(range));
        assert!(!ignored);
        assert_eq!(bytes.len(), 100);
    }

    #[test]
    fn ranged_body_passes_through_unranged_fetch() {
        let resp = response(5_000, 200);
        let (bytes, ignored) = ranged_body(&resp, None);
        assert!(!ignored);
        assert_eq!(bytes.len(), 5_000);
    }

    fn playlist_with_maps(maps: Vec<InitMap>) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(
            "v1".to_string(),
            "https://ex.com/hls/v1/prog.m3u8".to_string(),
        );
        pl.init_maps = maps;
        pl
    }

    #[test]
    fn init_map_target_pairs_uri_with_its_own_byterange() {
        let pl = playlist_with_maps(vec![
            InitMap {
                uri: "init_a.mp4".to_string(),
                byterange: Some("800@0".to_string()),
            },
            // A later MAP must not lend its byterange to the first one.
            InitMap {
                uri: "init_b.mp4".to_string(),
                byterange: Some("1200@5000".to_string()),
            },
        ]);
        assert_eq!(
            init_map_fetch_target(&pl),
            Some((
                "https://ex.com/hls/v1/init_a.mp4".to_string(),
                Some("800@0".to_string())
            ))
        );
    }

    #[test]
    fn init_map_target_substitutes_define_variables() {
        let mut pl = playlist_with_maps(vec![InitMap {
            uri: "{$path}/init.mp4".to_string(),
            byterange: None,
        }]);
        pl.definitions
            .insert("path".to_string(), "trak0".to_string());
        assert_eq!(
            init_map_fetch_target(&pl),
            Some(("https://ex.com/hls/v1/trak0/init.mp4".to_string(), None))
        );
    }

    #[test]
    fn init_map_target_absent_without_map_tag() {
        assert_eq!(init_map_fetch_target(&playlist_with_maps(Vec::new())), None);
    }

    #[test]
    fn parse_byterange_reads_length_and_offset() {
        let with_offset = parse_byterange("1200@800").expect("n@o is a valid BYTERANGE");
        assert_eq!((with_offset.start, with_offset.end), (800, 1999));
        let length_only = parse_byterange("100").expect("n is a valid BYTERANGE");
        assert_eq!((length_only.start, length_only.end), (0, 99));
        assert!(parse_byterange("not-a-range").is_none());
    }

    fn group_name(message: &str) -> Option<&'static str> {
        group_index_for_issue(message).map(|i| AUTHOR_CHECK_DEFS[i].name)
    }

    #[test]
    fn longest_keyword_wins_so_section_1_41_is_content_protection() {
        assert_eq!(
            group_name("Apple Authoring Spec §1.41: SAMPLE-AES is required"),
            Some("Content Protection")
        );
        assert_eq!(
            group_name("Apple Authoring Spec §1.2: frame rate too high"),
            Some("Video")
        );
        assert_eq!(
            group_name("Apple Authoring Spec §13.2: KEYFORMAT missing"),
            Some("Content Protection")
        );
        assert_eq!(group_name("Playlist could not be fetched"), None);
    }

    #[test]
    fn info_only_group_is_not_labelled_warn() {
        let groups = categorize_author_issues(&[Issue::info("Stream uses an unusual layout".to_string())]);
        let other = groups
            .iter()
            .find(|g| g.name == "Other")
            .expect("uncategorized issue must land in Other");
        assert_eq!(other.status, "INFO");

        let warned = categorize_author_issues(&[Issue::warn("Stream looks unusual".to_string())]);
        assert_eq!(
            warned
                .iter()
                .find(|g| g.name == "Other")
                .map(|g| g.status.as_str()),
            Some("WARN")
        );
    }

    #[test]
    fn each_issue_lands_in_exactly_one_group() {
        let issues = vec![
            Issue::error("Apple Authoring Spec §1.41: encryption is missing".to_string()),
            Issue::warn("Apple Authoring Spec §1.2: check frame rate".to_string()),
        ];
        let groups = categorize_author_issues(&issues);
        let placed: usize = groups.iter().map(|g| g.issues.len()).sum();
        assert_eq!(placed, issues.len());
        let video = groups.iter().find(|g| g.name == "Video").expect("Video row");
        assert_eq!(video.status, "WARN");
        assert_eq!(video.issues.len(), 1);
        let protection = groups
            .iter()
            .find(|g| g.name == "Content Protection")
            .expect("Content Protection row");
        assert_eq!(protection.status, "FAIL");
        assert_eq!(protection.issues.len(), 1);
    }

    #[test]
    fn empty_group_still_passes() {
        let groups = categorize_author_issues(&[]);
        assert!(groups.iter().all(|g| g.status == "PASS"));
        assert!(groups.iter().all(|g| g.name != "Other"));
    }

    fn fetch_error(msg: &str) -> FetchError {
        FetchError {
            error: msg.to_string(),
            extra_info: None,
        }
    }

    #[test]
    fn failed_probe_fetches_are_named_in_notes() {
        let mut notes = SampleNotes::default();
        notes.record_failure(
            "Init segment",
            "https://ex.com/init.mp4",
            &fetch_error("Bad HTTP status code: 404 Not Found"),
        );
        let notes = notes.into_notes();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("https://ex.com/init.mp4"));
        assert!(notes[0].contains("404"));
    }

    #[test]
    fn probe_failure_notes_are_capped_but_counted() {
        let mut notes = SampleNotes::default();
        for i in 0..MAX_FETCH_FAILURE_NOTES + 3 {
            notes.record_failure(
                "Media segment",
                &format!("https://ex.com/{i}.m4s"),
                &fetch_error("NetworkError"),
            );
        }
        let notes = notes.into_notes();
        assert_eq!(notes.len(), MAX_FETCH_FAILURE_NOTES + 1);
        assert!(notes.last().expect("summary note").contains("3 further"));
    }
}
