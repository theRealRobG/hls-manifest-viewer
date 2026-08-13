//! Entry point for the Author section: fetch a stream, probe media, run Authoring Spec rules.

use std::collections::HashSet;

use crate::utils::mp4_probe::{probe_init_segment, scan_segment_bytes};
use crate::utils::network::{fetch_array_buffer, FetchError, RequestRange};
use crate::utils::validator::types::{CheckGroup, Issue, MediaPlaylist, Severity};
use crate::utils::validator::{absolute_fetch_uri, collect_stream, now_ms};

use super::context::{AuthoringContext, InitProbeEntry, SegmentSample, ValidateAuthorOptions};
use super::profile::AuthorProfile;
use super::run_authoring_checks;

/// How many segments per playlist are sampled when deep checks are enabled.
const MAX_SEGMENT_SAMPLES_PER_PLAYLIST: usize = 3;

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
    pub elapsed_ms: u64,
}

/// Fetch `url`, probe its media and run the Apple HLS Authoring Specification rules.
pub async fn run_author_report(url: &str, options: AuthorOptions) -> Result<AuthorReport, FetchError> {
    let start = now_ms();
    let stream = collect_stream(url).await?;
    let playlists = stream.playlists;

    let author_opts = ValidateAuthorOptions {
        profile: options.profile,
        deep_checks: options.deep_checks,
    };
    let (init_probes, segment_samples) =
        collect_media_samples(&playlists, options.deep_checks).await;
    let ctx = AuthoringContext::new(
        stream.master.as_ref(),
        &playlists,
        &author_opts,
        &init_probes,
        &segment_samples,
    );

    let mut issues = stream.fetch_issues;
    issues.extend(run_authoring_checks(&ctx));

    let total_errors = issues.iter().filter(|i| i.severity == Severity::Error).count();
    let total_warnings = issues.iter().filter(|i| i.severity == Severity::Warn).count();
    let total_info = issues.iter().filter(|i| i.severity == Severity::Info).count();

    Ok(AuthorReport {
        url: url.to_string(),
        profile: options.profile.as_str().to_string(),
        deep_checks: options.deep_checks,
        result: if total_errors == 0 { "PASS".to_string() } else { "FAIL".to_string() },
        check_groups: categorize_author_issues(&issues),
        issues,
        probe_notes: ctx.probe_notes.clone(),
        total_errors,
        total_warnings,
        total_info,
        playlist_count: playlists.len(),
        init_probe_count: init_probes.len(),
        segment_sample_count: segment_samples.len(),
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

/// Fetch unique init segments and (optionally) a few media segments.
async fn collect_media_samples(
    playlists: &[MediaPlaylist],
    deep: bool,
) -> (Vec<InitProbeEntry>, Vec<SegmentSample>) {
    let mut init_jobs: Vec<(String, Option<String>, Option<RequestRange>)> = Vec::new();
    let mut seen_init: HashSet<String> = HashSet::new();
    for pl in playlists {
        let Some(map_uri) = pl.segments.iter().find_map(|s| s.map_uri.clone()) else {
            continue;
        };
        let br = pl.map_byterange.clone();
        let key = format!("{}|{}", map_uri, br.as_deref().unwrap_or(""));
        if !seen_init.insert(key) {
            continue;
        }
        let range = br.as_deref().and_then(parse_byterange);
        init_jobs.push((map_uri, br, range));
    }

    let init_fetches = futures::future::join_all(init_jobs.iter().map(|(uri, _, range)| {
        let uri = uri.clone();
        let range = *range;
        async move { fetch_array_buffer(uri, range).await }
    }))
    .await;

    let mut init_probes = Vec::new();
    for ((uri, br, _), result) in init_jobs.iter().zip(init_fetches) {
        if let Ok(resp) = result {
            init_probes.push(InitProbeEntry {
                uri: uri.clone(),
                byterange: br.clone(),
                probe: probe_init_segment(&resp.response_body),
            });
        }
    }

    let mut segment_samples = Vec::new();
    if deep {
        let mut ranged_jobs: Vec<(String, usize, String, f64, Option<RequestRange>)> = Vec::new();
        for pl in playlists {
            for (idx, seg) in pl.segments.iter().take(MAX_SEGMENT_SAMPLES_PER_PLAYLIST).enumerate() {
                let uri = if seg.uri.contains("://") {
                    seg.uri.clone()
                } else {
                    absolute_fetch_uri(&pl.url, &seg.uri, &pl.definitions)
                };
                let range = seg.byterange.as_deref().and_then(parse_byterange);
                ranged_jobs.push((pl.name.clone(), idx, uri, seg.duration, range));
            }
        }
        let fetches = futures::future::join_all(ranged_jobs.iter().map(|(_, _, uri, _, range)| {
            let uri = uri.clone();
            let range = *range;
            async move { fetch_array_buffer(uri, range).await }
        }))
        .await;
        for (job, result) in ranged_jobs.iter().zip(fetches) {
            if let Ok(resp) = result {
                let scan = scan_segment_bytes(&resp.response_body);
                segment_samples.push(SegmentSample {
                    playlist_name: job.0.clone(),
                    segment_index: job.1,
                    uri: job.2.clone(),
                    extinf_s: job.3,
                    bytes: resp.response_body.len(),
                    looks_like_ts: scan.looks_like_ts,
                    looks_like_fmp4: scan.looks_like_fmp4,
                    has_idr_nal_hint: scan.has_idr_nal_hint,
                    has_tfdt: scan.has_tfdt,
                    has_senc: scan.has_senc,
                    has_saiz: scan.has_saiz,
                    has_saio: scan.has_saio,
                });
            }
        }
    }

    (init_probes, segment_samples)
}

struct AuthorCheckDef {
    name: &'static str,
    reference: &'static str,
    keywords: &'static [&'static str],
}

const AUTHOR_CHECK_DEFS: &[AuthorCheckDef] = &[
    AuthorCheckDef { name: "Video", reference: "Apple Authoring Spec §1", keywords: &["Apple Authoring Spec §1."] },
    AuthorCheckDef { name: "Audio", reference: "Apple Authoring Spec §2", keywords: &["Apple Authoring Spec §2."] },
    AuthorCheckDef { name: "Ads", reference: "Apple Authoring Spec §3", keywords: &["Apple Authoring Spec §3."] },
    AuthorCheckDef { name: "Accessibility / Subtitles", reference: "Apple Authoring Spec §4–5", keywords: &["Apple Authoring Spec §4.", "Apple Authoring Spec §5."] },
    AuthorCheckDef { name: "Trick Play", reference: "Apple Authoring Spec §6", keywords: &["Apple Authoring Spec §6."] },
    AuthorCheckDef { name: "Segmentation", reference: "Apple Authoring Spec §7", keywords: &["Apple Authoring Spec §7."] },
    AuthorCheckDef { name: "Media Playlists", reference: "Apple Authoring Spec §8", keywords: &["Apple Authoring Spec §8."] },
    AuthorCheckDef { name: "Multivariant Playlist", reference: "Apple Authoring Spec §9", keywords: &["Apple Authoring Spec §9."] },
    AuthorCheckDef { name: "Delivery", reference: "Apple Authoring Spec §10", keywords: &["Apple Authoring Spec §10."] },
    AuthorCheckDef { name: "Privacy", reference: "Apple Authoring Spec §11", keywords: &["Apple Authoring Spec §11."] },
    AuthorCheckDef { name: "Security", reference: "Apple Authoring Spec §12", keywords: &["Apple Authoring Spec §12."] },
    AuthorCheckDef { name: "Content Protection", reference: "Apple Authoring Spec §13", keywords: &["Apple Authoring Spec §13.", "Apple Authoring Spec §1.41"] },
    AuthorCheckDef { name: "Low-Latency HLS", reference: "Apple Authoring Spec §14", keywords: &["Apple Authoring Spec §14."] },
    AuthorCheckDef { name: "SharePlay / Spatial", reference: "Apple Authoring Spec §15–16", keywords: &["Apple Authoring Spec §15.", "Apple Authoring Spec §16."] },
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
