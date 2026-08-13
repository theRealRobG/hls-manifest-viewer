//! Entry point for the Author section: fetch a stream, probe media, run Authoring Spec rules.

use std::collections::{HashMap, HashSet};

use crate::utils::mp4_probe::{probe_init_segment, scan_segment_bytes};
use crate::utils::network::{fetch_array_buffer, fetch_text, FetchError, RequestRange};
use crate::utils::validator::parser::parse_media_playlist;
use crate::utils::validator::types::{CheckGroup, Issue, MediaPlaylist, Severity};
use crate::utils::validator::{absolute_fetch_uri, collect_stream, now_ms};

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
    let (init_probes, segment_samples, webvtt_samples) =
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
        probe_notes: ctx.probe_notes.clone(),
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

/// Fetch unique init segments, Phase B light samples, and optional Phase C deep segment samples.
async fn collect_media_samples(
    playlists: &[MediaPlaylist],
    deep: bool,
) -> (Vec<InitProbeEntry>, Vec<SegmentSample>, Vec<WebVttSample>) {
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
        let Some(map_uri) = pl.segments.iter().find_map(|s| s.map_uri.clone()) else {
            continue;
        };
        let br = pl.map_byterange.clone();
        let key = format!("{}|{}", map_uri, br.as_deref().unwrap_or(""));
        let entry = init_by_key.entry(key).or_insert_with(|| InitJob {
            uri: map_uri,
            byterange: br.clone(),
            range: br.as_deref().and_then(parse_byterange),
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
    let init_jobs: Vec<InitJob> = init_by_key.into_values().collect();

    let init_fetches = futures::future::join_all(init_jobs.iter().map(|job| {
        let uri = job.uri.clone();
        let range = job.range;
        async move { fetch_array_buffer(uri, range).await }
    }))
    .await;

    let mut init_probes = Vec::new();
    for (job, result) in init_jobs.iter().zip(init_fetches) {
        if let Ok(resp) = result {
            init_probes.push(InitProbeEntry {
                uri: job.uri.clone(),
                byterange: job.byterange.clone(),
                playlist_names: job.playlist_names.clone(),
                media_types: job.media_types.clone(),
                probe: probe_init_segment(&resp.response_body),
            });
        }
    }

    // --- Segment samples ---
    let mut segment_samples = Vec::new();

    // Phase B light: one fMP4 I-frame segment per playlist (capped) for §6.10.
    {
        let mut ranged_jobs: Vec<(String, usize, String, f64, Option<RequestRange>, bool)> =
            Vec::new();
        for pl in playlists.iter().filter(|p| p.is_iframe).take(MAX_IFRAME_LIGHT_SAMPLES) {
            let Some((idx, seg)) = pl.segments.iter().enumerate().next() else {
                continue;
            };
            let uri = if seg.uri.contains("://") {
                seg.uri.clone()
            } else {
                absolute_fetch_uri(&pl.url, &seg.uri, &pl.definitions)
            };
            let range = seg.byterange.as_deref().and_then(parse_byterange);
            ranged_jobs.push((pl.name.clone(), idx, uri, seg.duration, range, true));
        }
        let fetches = futures::future::join_all(ranged_jobs.iter().map(|(_, _, uri, _, range, _)| {
            let uri = uri.clone();
            let range = *range;
            async move { fetch_array_buffer(uri, range).await }
        }))
        .await;
        for (job, result) in ranged_jobs.iter().zip(fetches) {
            if let Ok(resp) = result {
                let scan = scan_segment_bytes(&resp.response_body);
                segment_samples.push(SegmentSample::from_scan(
                    job.0.clone(),
                    job.1,
                    job.2.clone(),
                    job.3,
                    resp.response_body.len(),
                    job.5,
                    &scan,
                ));
            }
        }
    }

    // Phase C deep samples (optional).
    if deep {
        let mut ranged_jobs: Vec<(String, usize, String, f64, Option<RequestRange>, bool)> =
            Vec::new();
        for pl in playlists {
            let limit = if pl.media_type == "VIDEO" && !pl.is_iframe {
                MAX_DEEP_VIDEO_SEGMENT_SAMPLES
            } else {
                MAX_DEEP_SEGMENT_SAMPLES_PER_PLAYLIST
            };
            for (idx, seg) in pl.segments.iter().take(limit).enumerate() {
                // Skip duplicates already sampled as iframe light probes.
                if pl.is_iframe && idx == 0 {
                    continue;
                }
                let uri = if seg.uri.contains("://") {
                    seg.uri.clone()
                } else {
                    absolute_fetch_uri(&pl.url, &seg.uri, &pl.definitions)
                };
                let range = seg.byterange.as_deref().and_then(parse_byterange);
                ranged_jobs.push((
                    pl.name.clone(),
                    idx,
                    uri,
                    seg.duration,
                    range,
                    pl.is_iframe,
                ));
            }
        }
        let fetches = futures::future::join_all(ranged_jobs.iter().map(|(_, _, uri, _, range, _)| {
            let uri = uri.clone();
            let range = *range;
            async move { fetch_array_buffer(uri, range).await }
        }))
        .await;
        for (job, result) in ranged_jobs.iter().zip(fetches) {
            if let Ok(resp) = result {
                let scan = scan_segment_bytes(&resp.response_body);
                segment_samples.push(SegmentSample::from_scan(
                    job.0.clone(),
                    job.1,
                    job.2.clone(),
                    job.3,
                    resp.response_body.len(),
                    job.5,
                    &scan,
                ));
            }
        }
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
            let uri = if seg.uri.contains("://") {
                seg.uri.clone()
            } else {
                absolute_fetch_uri(&pl.url, &seg.uri, &pl.definitions)
            };
            jobs.push((pl.name.clone(), uri));
        }
        let fetches = futures::future::join_all(jobs.iter().map(|(_, uri)| {
            let uri = uri.clone();
            async move { fetch_text(uri).await }
        }))
        .await;
        for ((name, uri), result) in jobs.iter().zip(fetches) {
            if let Ok(resp) = result {
                let body = resp.response_text;
                let lower = body.to_ascii_lowercase();
                webvtt_samples.push(WebVttSample {
                    playlist_name: name.clone(),
                    uri: uri.clone(),
                    has_webvtt_header: lower.contains("webvtt"),
                    has_x_timestamp_map: body.contains("X-TIMESTAMP-MAP"),
                });
            }
        }
    }

    (init_probes, segment_samples, webvtt_samples)
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

/// Group Author issues by spec section, emitting a PASS row for sections with no findings.
fn categorize_author_issues(issues: &[Issue]) -> Vec<CheckGroup> {
    let mut groups: Vec<CheckGroup> = AUTHOR_CHECK_DEFS
        .iter()
        .map(|def| {
            let matched: Vec<Issue> = issues
                .iter()
                .filter(|i| def.keywords.iter().any(|k| i.message.contains(k)))
                .cloned()
                .collect();
            let status = if matched.iter().any(|i| i.severity == Severity::Error) {
                "FAIL"
            } else if matched.iter().any(|i| i.severity == Severity::Warn) {
                "WARN"
            } else {
                "PASS"
            };
            CheckGroup {
                name: def.name.to_string(),
                section: "Author".to_string(),
                reference: def.reference.to_string(),
                status: status.to_string(),
                issues: matched,
            }
        })
        .collect();

    let uncategorized: Vec<Issue> = issues
        .iter()
        .filter(|i| {
            !AUTHOR_CHECK_DEFS
                .iter()
                .any(|def| def.keywords.iter().any(|k| i.message.contains(k)))
        })
        .cloned()
        .collect();
    if !uncategorized.is_empty() {
        let status = if uncategorized.iter().any(|i| i.severity == Severity::Error) {
            "FAIL"
        } else {
            "WARN"
        };
        groups.push(CheckGroup {
            name: "Other".to_string(),
            section: "Author".to_string(),
            reference: "Apple HLS Authoring Specification".to_string(),
            status: status.to_string(),
            issues: uncategorized,
        });
    }

    groups
}
