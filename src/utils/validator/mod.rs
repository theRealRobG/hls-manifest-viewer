pub mod types;
pub mod parser;
pub mod checks;

use types::*;
use parser::*;
use crate::utils::href::replace_hls_variables;
use crate::utils::network::{fetch_text, FetchError};
use std::collections::HashMap;

/// Substitute EXT-X-DEFINE variables in `uri`, then resolve against `base`.
pub fn absolute_fetch_uri(base: &str, uri: &str, defs: &HashMap<String, String>) -> String {
    let substituted = replace_hls_variables(uri, defs);
    resolve_url(base, substituted.as_ref())
}

/// Determine if content is a master (multivariant) playlist
pub fn is_master_playlist(content: &str) -> bool {
    content.contains("#EXT-X-STREAM-INF:") || content.contains("#EXT-X-I-FRAME-STREAM-INF:")
}

/// Get current time in milliseconds (browser performance.now() or Date.now())
pub(crate) fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

fn http_meta_from_fetch(request_url: &str, resp: &crate::utils::network::FetchTextResponse) -> PlaylistHttpMeta {
    PlaylistHttpMeta {
        request_url: request_url.to_string(),
        final_url: resp.final_url.clone(),
        content_encoding: resp.content_encoding.clone(),
        content_length: resp.content_length,
        body_bytes: Some(resp.response_text.len() as u64),
        last_modified: resp.last_modified.clone(),
        date: resp.date.clone(),
    }
}

/// Master and media playlists fetched for one stream. Shared by the Validate
/// and Author sections so both parse the stream the same way.
pub struct CollectedStream {
    pub master: Option<MasterPlaylist>,
    pub playlists: Vec<MediaPlaylist>,
    /// Warnings raised while fetching renditions.
    pub fetch_issues: Vec<Issue>,
}

/// Fetch a master (and all of its media playlists) or a single media playlist.
pub async fn collect_stream(url: &str) -> Result<CollectedStream, FetchError> {
    let response = fetch_text(url.to_string()).await?;
    let content = &response.response_text;
    let mut fetch_issues = Vec::new();

    if !is_master_playlist(content) {
        let mut pl = MediaPlaylist::new("media".to_string(), url.to_string());
        pl.http_meta = http_meta_from_fetch(url, &response);
        parse_media_playlist(url, content, &mut pl);
        return Ok(CollectedStream {
            master: None,
            playlists: vec![pl],
            fetch_issues,
        });
    }

    let mut master = parse_master_playlist(url, content);
    master.http_meta = http_meta_from_fetch(url, &response);

    let mut playlists = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();

    // Collect variant info including closed_captions and video_range
    struct VariantInfo {
        uri: String,
        bandwidth: Option<u64>,
        average_bandwidth: Option<u64>,
        codecs: Option<String>,
        resolution: Option<String>,
        frame_rate: Option<f64>,
        audio_group: Option<String>,
        closed_captions: Option<String>,
        video_range: Option<String>,
        is_iframe: bool,
        score: Option<f64>,
        hdcp_level: Option<String>,
        pathway_id: Option<String>,
        req_video_layout: Option<String>,
    }
    let variant_infos: Vec<VariantInfo> = master.variants.iter().map(|v| VariantInfo {
        uri: v.uri.clone(),
        bandwidth: v.bandwidth,
        average_bandwidth: v.average_bandwidth,
        codecs: v.codecs.clone(),
        resolution: v.resolution.clone(),
        frame_rate: v.frame_rate,
        audio_group: v.audio_group.clone(),
        closed_captions: v.closed_captions.clone(),
        video_range: v.video_range.clone(),
        is_iframe: v.is_iframe,
        score: v.score,
        hdcp_level: v.hdcp_level.clone(),
        pathway_id: v.pathway_id.clone(),
        req_video_layout: v.req_video_layout.clone(),
    }).collect();

    // Build audio group → codec lookup from STREAM-INF CODECS strings
    // (each STREAM-INF CODECS is "videocodec,audiocodec"; take the audio portion)
    let audio_group_codec: std::collections::HashMap<String, String> = master.variants.iter()
        .filter_map(|v| {
            let grp = v.audio_group.as_ref()?;
            let codecs = v.codecs.as_ref()?;
            let audio_codec = codecs.split(',').nth(1).map(|c| c.trim().to_string())?;
            Some((grp.clone(), audio_codec))
        })
        .collect();

    // Carry channels along with each audio rendition for naming/display
    let audio_uris: Vec<(String, String, String, Option<String>)> = master.media_renditions.iter()
        .filter(|r| r.media_type == "AUDIO" && r.uri.is_some())
        .map(|r| (r.uri.clone().unwrap(), r.name.clone(), r.group_id.clone(), r.channels.clone()))
        .collect();

    // Fetch variant playlists in parallel (unique absolute URIs only).
    let mut variant_jobs: Vec<(usize, String)> = Vec::new();
    for (idx, vi) in variant_infos.iter().enumerate() {
        let fetch_uri = absolute_fetch_uri(url, &vi.uri, &master.definitions);
        if !seen_urls.insert(fetch_uri.clone()) {
            continue;
        }
        variant_jobs.push((idx, fetch_uri));
    }
    let variant_fetches = futures::future::join_all(
        variant_jobs.iter().map(|(_, fetch_uri)| {
            let fetch_uri = fetch_uri.clone();
            async move { fetch_text(fetch_uri).await }
        }),
    )
    .await;

    for ((idx, fetch_uri), result) in variant_jobs.iter().zip(variant_fetches) {
        let vi = &variant_infos[*idx];
        // Include bandwidth in name to disambiguate renditions that share a resolution
        // (e.g. two 1920×1080 tiers at different bitrates).
        let name = if let Some(r) = &vi.resolution {
            if let Some(b) = vi.bandwidth {
                format!("video/{} · {}k", r, b / 1000)
            } else {
                format!("video/{}", r)
            }
        } else if let Some(b) = vi.bandwidth {
            format!("video/{}k", b / 1000)
        } else {
            fetch_uri
                .split('/')
                .next_back()
                .unwrap_or("unknown")
                .to_string()
        };
        match result {
            Ok(resp) => {
                let mut pl = MediaPlaylist::new(name, fetch_uri.clone());
                pl.media_type = "VIDEO".to_string();
                pl.bandwidth = vi.bandwidth;
                pl.average_bandwidth = vi.average_bandwidth;
                pl.codecs = vi.codecs.clone();
                pl.resolution = vi.resolution.clone();
                pl.frame_rate = vi.frame_rate;
                pl.audio_group = vi.audio_group.clone();
                pl.closed_captions = vi.closed_captions.clone();
                pl.video_range = vi.video_range.clone();
                pl.color_info = derive_color_info(vi.video_range.as_deref(), vi.codecs.as_deref());
                pl.is_iframe = vi.is_iframe;
                pl.score = vi.score;
                pl.hdcp_level = vi.hdcp_level.clone();
                pl.pathway_id = vi.pathway_id.clone();
                pl.req_video_layout = vi.req_video_layout.clone();
                pl.http_meta = http_meta_from_fetch(fetch_uri, &resp);
                // Seed inherited variables before parsing so segment and MAP URIs that
                // reference them resolve; a local EXT-X-DEFINE still wins because the parser
                // overwrites the inherited fallback as it reads the tag.
                apply_master_definitions(&resp.response_text, &master.definitions, &mut pl.definitions);
                parse_media_playlist(fetch_uri, &resp.response_text, &mut pl);
                playlists.push(pl);
            }
            Err(e) => {
                fetch_issues.push(Issue::warn(format!(
                    "Could not fetch media playlist '{}': {}", fetch_uri, e
                )));
            }
        }
    }

    // Fetch audio renditions in parallel (unique absolute URIs only).
    let mut audio_jobs: Vec<(usize, String)> = Vec::new();
    for (idx, (uri, _, _, _)) in audio_uris.iter().enumerate() {
        let fetch_uri = absolute_fetch_uri(url, uri, &master.definitions);
        if !seen_urls.insert(fetch_uri.clone()) {
            continue;
        }
        audio_jobs.push((idx, fetch_uri));
    }
    let audio_fetches = futures::future::join_all(
        audio_jobs.iter().map(|(_, fetch_uri)| {
            let fetch_uri = fetch_uri.clone();
            async move { fetch_text(fetch_uri).await }
        }),
    )
    .await;

    for ((idx, fetch_uri), result) in audio_jobs.iter().zip(audio_fetches) {
        let (_, name, group_id, channels) = &audio_uris[*idx];
        // Build a unique, human-readable name.
        // "ENG (audio1) · 2ch" disambiguates entries that share the same NAME.
        let channel_suffix = channels.as_deref()
            .map(|c| format!(" · {}ch", c.split('/').next().unwrap_or(c)))
            .unwrap_or_default();
        let audio_name = format!("audio/{} ({}){}", name, group_id, channel_suffix);

        match result {
            Ok(resp) => {
                let mut pl = MediaPlaylist::new(audio_name, fetch_uri.clone());
                pl.media_type = "AUDIO".to_string();
                pl.group_id = Some(group_id.clone());
                // Derive audio codec from the STREAM-INF entry that references this group
                pl.codecs = audio_group_codec.get(group_id.as_str()).cloned();
                pl.http_meta = http_meta_from_fetch(fetch_uri, &resp);
                apply_master_definitions(&resp.response_text, &master.definitions, &mut pl.definitions);
                parse_media_playlist(fetch_uri, &resp.response_text, &mut pl);
                playlists.push(pl);
            }
            Err(e) => {
                fetch_issues.push(Issue::warn(format!(
                    "Could not fetch audio rendition '{}': {}", fetch_uri, e
                )));
            }
        }
    }

    Ok(CollectedStream {
        master: Some(master),
        playlists,
        fetch_issues,
    })
}

/// Main validation entry point with tolerance option
pub async fn validate_hls_with_options(url: &str, tolerance_ms: f64) -> Result<ValidationReport, FetchError> {
    let start = now_ms();
    let stream = collect_stream(url).await?;
    let mut report = ValidationReport::new();

    report.tolerance_ms = tolerance_ms;
    report.master_url = url.to_string();
    report.issues.extend(stream.fetch_issues);

    let playlists = stream.playlists;

    if let Some(master) = stream.master {
        report.issues.extend(checks::check_codecs_attribute(&master));
        report.issues.extend(checks::check_bandwidth_required(&master));
        report.issues.extend(checks::check_media_group_membership(&master));

        // Run checks and parse interstitials
        run_media_checks(&playlists, &mut report);

        // Parse HLS Interstitials from media playlists
        let has_interstitials = playlists.iter().any(|pl|
            pl.raw_content.contains("com.apple.hls.interstitial")
        );
        if has_interstitials {
            let (interstitial_issues, mut interstitials) = checks::check_interstitials(&playlists);
            report.issues.extend(interstitial_issues);
            compute_interstitial_offsets(&mut interstitials, &playlists);
            report.interstitials = interstitials;
            report.has_interstitials_data = true;
        }

        // Collect SCTE-35 ad breaks from EXT-X-DATERANGE tags
        let ad_breaks = collect_scte35_ad_breaks(&playlists);
        if !ad_breaks.is_empty() {
            report.has_scte35_data = true;
            report.ad_breaks = ad_breaks;
        }

        // Compute playlist window duration for the SCTE-35 timeline.
        // Use the best (highest-bandwidth) video playlist with PDT span or EXTINF sum.
        report.playlist_window_s = playlists.iter()
            .filter(|pl| pl.media_type == "VIDEO")
            .max_by_key(|pl| pl.bandwidth.unwrap_or(0))
            .map(pdt_span_or_extinf_sum)
            .unwrap_or(0.0);

        // Fetch delta updates for playlists with CAN-SKIP-UNTIL
        let (delta_issues, delta_reports) = check_playlist_delta_updates(&playlists).await;
        report.issues.extend(delta_issues);
        report.delta_report = delta_reports;

        // MSN monotonicity check: re-fetch live playlists and compare MSNs
        let msn_issues = check_media_sequence_monotonicity(&playlists).await;
        report.issues.extend(msn_issues);

        // Build renditions for UI
        report.renditions = build_renditions(&playlists);
        report.playlists = playlists;
        report.master = Some(master);
    } else {
        run_media_checks(&playlists, &mut report);
        report.renditions = build_renditions(&playlists);
        report.playlists = playlists;
    }

    report.finalize();
    report.check_groups = categorize_issues(&report.issues);
    report.elapsed_ms = (now_ms() - start) as u64;
    Ok(report)
}

/// Resolve EXT-X-DEFINE:IMPORT references in a media playlist against the parent (master)
/// definitions, inserting any matched values into `dest` that are not already present.
fn resolve_imports(
    content: &str,
    master_defs: &std::collections::HashMap<String, String>,
    dest: &mut std::collections::HashMap<String, String>,
) {
    for line in content.lines() {
        if let Some(attr_str) = line.trim().strip_prefix("#EXT-X-DEFINE:") {
            let attrs = parse_attributes(attr_str);
            if let Some(name) = attrs.get("IMPORT")
                && let Some(value) = master_defs.get(name.as_str()) {
                    dest.entry(name.clone()).or_insert_with(|| value.clone());
                }
        }
    }
}

/// Propagate all master playlist variable definitions into a media playlist's definitions map.
/// First, explicit IMPORT tags in the playlist content are resolved (only named imports are
/// pulled in).  Then, all remaining master definitions are inserted as fallbacks so that
/// `{$VAR}` references in DATERANGE/X-ASSET-LIST URLs resolve even when the media playlist
/// has no EXT-X-DEFINE:IMPORT lines (as is common with session-based ad-proxy streams).
pub(crate) fn apply_master_definitions(
    content: &str,
    master_defs: &std::collections::HashMap<String, String>,
    pl_defs: &mut std::collections::HashMap<String, String>,
) {
    resolve_imports(content, master_defs, pl_defs);
    for (k, v) in master_defs {
        pl_defs.entry(k.clone()).or_insert_with(|| v.clone());
    }
}

/// Run all media-level checks on a set of playlists
fn run_media_checks(playlists: &[MediaPlaylist], report: &mut ValidationReport) {
    let tolerance_ms = report.tolerance_ms;

    report.issues.extend(checks::check_extm3u_header(playlists));
    report.issues.extend(checks::check_target_duration_compliance(playlists));
    report.issues.extend(checks::check_pdt_coverage(playlists));
    report.issues.extend(checks::check_media_sequence_duplicate_tags(playlists));
    report.issues.extend(checks::check_version_compatibility(playlists));
    report.issues.extend(checks::check_live_playlist_min_segments(playlists));
    report.issues.extend(checks::check_targetduration_consistency(playlists));
    report.issues.extend(checks::check_playlist_type_endlist(playlists));
    report.issues.extend(checks::check_encryption_consistency(playlists));
    report.issues.extend(checks::check_discontinuity_sequence(playlists));
    report.issues.extend(checks::check_segment_count(playlists));
    report.issues.extend(checks::check_duration_drift(playlists, tolerance_ms));
    report.issues.extend(checks::check_pdt_alignment(playlists, tolerance_ms));
    report.issues.extend(checks::check_cumulative_drift(playlists, tolerance_ms));
    report.issues.extend(checks::check_ll_hls_compliance(playlists));
    report.issues.extend(checks::check_media_sequence_continuity(playlists));

    // Sort issues by severity (errors first)
    report.issues.sort_by_key(|a| std::cmp::Reverse(a.severity));
}

/// Build renditions list for UI display from parsed playlists
fn build_renditions(playlists: &[MediaPlaylist]) -> Vec<Rendition> {
    playlists.iter().map(|pl| {
        Rendition {
            name: pl.name.clone(),
            media_type: pl.media_type.clone(),
            url: pl.url.clone(),
            bandwidth: pl.bandwidth.unwrap_or(0),
            average_bandwidth: pl.average_bandwidth,
            resolution: pl.resolution.clone(),
            codecs: pl.codecs.clone(),
            frame_rate: pl.frame_rate,
            closed_captions: pl.closed_captions.clone(),
            color_info: pl.color_info.clone(),
            group_id: if pl.media_type == "AUDIO" {
                pl.group_id.clone()
            } else {
                pl.audio_group.clone()
            },
            segment_count: pl.segments.len(),
            target_duration: pl.target_duration,
            media_sequence: pl.media_sequence,
            discontinuity_sequence: pl.discontinuity_sequence,
            hold_back: pl.server_control.as_ref().and_then(|sc| sc.hold_back),
            part_target: pl.part_target,
            part_hold_back: pl.server_control.as_ref().and_then(|sc| sc.part_hold_back),
            has_parts: !pl.parts.is_empty(),
        }
    }).collect()
}

/// Derive human-readable color info from VIDEO-RANGE and CODECS
fn derive_color_info(video_range: Option<&str>, codecs: Option<&str>) -> Option<String> {
    let codecs_str = codecs.unwrap_or("");
    // Dolby Vision detection
    let is_dv = codecs_str.contains("dvh1") || codecs_str.contains("dvhe")
        || codecs_str.contains("dav1") || codecs_str.contains("dva1") || codecs_str.contains("dvav");
    if is_dv {
        let base = if codecs_str.contains("hvc1") || codecs_str.contains("hev1") {
            "Dolby Vision (HEVC)"
        } else if codecs_str.contains("av01") {
            "Dolby Vision (AV1)"
        } else {
            "Dolby Vision"
        };
        return Some(base.to_string());
    }
    match video_range {
        Some("PQ") => Some("HDR10".to_string()),
        Some("HLG") => Some("HLG".to_string()),
        Some("SDR") => Some("SDR".to_string()),
        _ => None,
    }
}

/// Check definition for categorization
struct CheckDef {
    name: &'static str,
    section: &'static str,
    reference: &'static str,
    keywords: &'static [&'static str],
}

const CHECK_DEFS: &[CheckDef] = &[
    // §4.4.1 Basic Tags
    CheckDef { name: "EXTM3U Header", section: "Basic Tags", reference: "rfc8216bis §4.4.1.1", keywords: &["§4.4.1.1", "#EXTM3U MUST"] },
    // §4.4.2 / §4.4.3 Singleton Tag Presence
    CheckDef { name: "Singleton Tags", section: "Structural", reference: "rfc8216bis §4.4.1.2/§4.4.2/§4.4.3", keywords: &["appears", "times in", "MUST appear at most once"] },
    // §4.4.3.1
    CheckDef { name: "Target Duration Compliance", section: "Structural", reference: "rfc8216bis §4.4.3.1", keywords: &["§4.4.3.1", "TARGETDURATION"] },
    // §4.4.3.2
    CheckDef { name: "Media Sequence Tags", section: "Structural", reference: "rfc8216bis §4.4.3.2", keywords: &["§4.4.3.2", "MEDIA-SEQUENCE", "MSN monotonicity"] },
    // §4.4.3.3
    CheckDef { name: "Discontinuity Sequence", section: "Alignment", reference: "rfc8216bis §4.4.3.3", keywords: &["Discontinuity"] },
    // §4.4.3.5
    CheckDef { name: "Playlist Type / ENDLIST", section: "Structural", reference: "rfc8216bis §4.4.3.5", keywords: &["§4.4.3.5", "PLAYLIST-TYPE:VOD"] },
    // §4.4.4.4
    CheckDef { name: "Encryption Consistency", section: "Security", reference: "rfc8216bis §4.4.4.4", keywords: &["§4.4.4.4", "encryption method"] },
    // §4.4.5.1 / §4.4.5.2
    CheckDef { name: "Playlist Delta Updates", section: "LL-HLS", reference: "rfc8216bis §4.4.5.2/§6.2.5.1", keywords: &["§6.2.5.1", "delta update", "Delta update"] },
    // §4.4.6.2
    CheckDef { name: "BANDWIDTH Required", section: "Multivariant", reference: "rfc8216bis §4.4.6.2", keywords: &["BANDWIDTH attribute, which is REQUIRED"] },
    CheckDef { name: "CODECS Consistency", section: "Multivariant", reference: "rfc8216bis §4.4.6.2", keywords: &["§4.4.5.2", "CODECS"] },
    // §4.4.6.1.1
    CheckDef { name: "Media Group Membership", section: "Multivariant", reference: "rfc8216bis §4.4.6.1.1", keywords: &["§4.4.6.1.1", "missing member"] },
    // §6.2.2
    CheckDef { name: "Live Playlist Window", section: "Live", reference: "rfc8216bis §6.2.2", keywords: &["§6.2.2", "Live playlist"] },
    // §6.2.4
    CheckDef { name: "PDT Coverage", section: "Timing", reference: "rfc8216bis §6.2.4", keywords: &["partial PDT coverage"] },
    CheckDef { name: "PDT Alignment", section: "Alignment", reference: "rfc8216bis §6.2.4", keywords: &["PDT misalignment"] },
    CheckDef { name: "Target Duration Consistency", section: "Alignment", reference: "rfc8216bis §6.2.4", keywords: &["TARGETDURATION values differ"] },
    CheckDef { name: "Cumulative Drift", section: "Alignment", reference: "rfc8216bis §6.2.4", keywords: &["Cumulative EXTINF drift"] },
    // §4.4.4.1 drift cross-rendition
    CheckDef { name: "EXTINF Duration Drift", section: "Alignment", reference: "rfc8216bis §4.4.4.1", keywords: &["EXTINF drift"] },
    CheckDef { name: "Segment Count", section: "Alignment", reference: "rfc8216bis §4.4.4.1", keywords: &["Segment count mismatch"] },
    // §8
    CheckDef { name: "Version Compatibility", section: "Version", reference: "rfc8216bis §8", keywords: &["rfc8216bis §8:", "RFC 8216bis §8:"] },
    // LL-HLS §4.4.3–4.4.5, §6.2.5.2
    CheckDef { name: "LL-HLS Compliance", section: "LL-HLS", reference: "rfc8216bis §4.4.3–4.4.5", keywords: &["LL-HLS", "rfc8216bis §4.4.3.8", "rfc8216bis §4.4.4.9", "rfc8216bis §4.4.5"] },
    // Appendix D
    CheckDef { name: "HLS Interstitials", section: "Interstitials", reference: "rfc8216bis Appendix D", keywords: &["Interstitial:"] },
];

/// Categorize issues into named check groups (matches Go categorizeIssues)
fn categorize_issues(issues: &[Issue]) -> Vec<CheckGroup> {
    let mut assigned = vec![false; issues.len()];
    let mut groups = Vec::new();

    for def in CHECK_DEFS {
        let mut matched = Vec::new();
        for (i, issue) in issues.iter().enumerate() {
            if assigned[i] {
                continue;
            }
            for kw in def.keywords {
                if issue.message.contains(kw) {
                    matched.push(issue.clone());
                    assigned[i] = true;
                    break;
                }
            }
        }
        let status = if matched.iter().any(|i| i.severity == Severity::Error) {
            "FAIL"
        } else if matched.iter().any(|i| i.severity == Severity::Warn) {
            "WARN"
        } else {
            "PASS"
        };
        groups.push(CheckGroup {
            name: def.name.to_string(),
            section: def.section.to_string(),
            reference: def.reference.to_string(),
            status: status.to_string(),
            issues: matched,
        });
    }
    groups
}

/// Compute start_offset_s and content_duration_s for interstitials.
/// start_offset_s = interstitial START-DATE minus the rendition's first-segment PDT.
/// content_duration_s = PDT span of the current window (accounts for delta-updated live
/// playlists where skipped segments are not in the segment list).
fn compute_interstitial_offsets(interstitials: &mut [Interstitial], playlists: &[MediaPlaylist]) {
    if interstitials.is_empty() {
        return;
    }
    // Build per-rendition lookup: (first_pdt, window_duration_s)
    let mut rendition_info: std::collections::HashMap<&str, (Option<f64>, f64)> =
        std::collections::HashMap::new();
    for pl in playlists {
        let first_pdt = pl.segments.first().and_then(|s| s.pdt);
        let window_dur = pdt_span_or_extinf_sum(pl);
        rendition_info.insert(pl.name.as_str(), (first_pdt, window_dur));
    }

    for it in interstitials.iter_mut() {
        if let Some(&(first_pdt, content_dur)) = rendition_info.get(it.rendition.as_str()) {
            it.content_duration_s = content_dur;
            if let Some(first_pdt_epoch) = first_pdt
                && let Some(it_epoch) = parser::parse_iso8601_to_epoch(&it.start_date) {
                    it.start_offset_s = Some(it_epoch - first_pdt_epoch);
                }
        }
    }
}

/// Best-effort content duration for a playlist window.
///
/// For live playlists with PDTs, compute (last_pdt + last_dur − first_pdt) which correctly
/// spans the entire DVR window even when skipped-segment counts are unreliable.
/// Falls back to EXTINF sum (+ skipped_segments * target_duration) when PDTs are absent.
fn pdt_span_or_extinf_sum(pl: &MediaPlaylist) -> f64 {
    if let (Some(first_seg), Some(last_seg)) = (pl.segments.first(), pl.segments.last())
        && let (Some(first_pdt), Some(last_pdt)) = (first_seg.pdt, last_seg.pdt) {
            return (last_pdt - first_pdt) + last_seg.duration;
        }
    // No PDTs — fall back to EXTINF sum, padding for any skipped segments
    let extinf: f64 = pl.segments.iter().map(|s| s.duration).sum();
    let skip_estimate = pl.skipped_segments as f64 * pl.target_duration.max(1.0);
    extinf + skip_estimate
}

/// Collect SCTE-35 ad breaks from EXT-X-DATERANGE tags across all media playlists.
/// Results are deduplicated by ID (same break appears in every rendition).
///
/// # Interstitial deduplication
/// Apple HLS Interstitials embed SCTE-35 signalling inside EXT-X-DATERANGE tags that carry
/// `CLASS="com.apple.hls.interstitial"` on the SCTE35-OUT tag.  The matching SCTE35-IN tag
/// for the same ID typically **omits** the CLASS attribute, so a naïve single-pass filter
/// that only checks CLASS would let the IN half through, causing the same break to appear in
/// both the Interstitials section and the SCTE-35 section.
///
/// To prevent this we do two passes:
///   1. Collect every DATERANGE ID whose CLASS contains "com.apple.hls.interstitial".
///   2. Skip any DATERANGE tag whose ID is in that set (regardless of CLASS on the IN tag).
pub fn collect_scte35_ad_breaks(playlists: &[MediaPlaylist]) -> Vec<AdBreak> {
    use std::collections::{HashMap, HashSet};

    // ── Pass 1: gather IDs that belong to HLS Interstitials across ALL playlists ──
    let mut interstitial_ids: HashSet<String> = HashSet::new();
    for pl in playlists {
        if pl.media_type != "VIDEO" {
            continue;
        }
        for line in pl.raw_content.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("#EXT-X-DATERANGE:") else {
                continue;
            };
            let attrs = parser::parse_attributes(rest);
            if attrs.get("CLASS").is_some_and(|c| c.contains("com.apple.hls.interstitial"))
                && let Some(id) = attrs.get("ID") {
                    interstitial_ids.insert(id.clone());
                }
        }
    }

    // ── Pass 2: collect genuine SCTE-35 breaks, skipping all interstitial IDs ──

    // id → (break, first_pdt_of_source_playlist)
    let mut map: HashMap<String, (AdBreak, Option<f64>)> = HashMap::new();

    for pl in playlists {
        // Only scan video renditions to avoid duplication noise from audio tracks
        if pl.media_type != "VIDEO" {
            continue;
        }
        let first_pdt = pl.segments.first().and_then(|s| s.pdt);

        for line in pl.raw_content.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("#EXT-X-DATERANGE:") else {
                continue;
            };
            let attrs = parser::parse_attributes(rest);

            // Must carry at least one SCTE35 payload attribute
            let has_scte35 = attrs.contains_key("SCTE35-OUT") || attrs.contains_key("SCTE35-IN");
            if !has_scte35 {
                continue;
            }

            let id = attrs.get("ID").cloned().unwrap_or_default();

            // Skip any ID associated with HLS Interstitials (catches both OUT and IN halves)
            if interstitial_ids.contains(&id) {
                continue;
            }

            let start_date = attrs.get("START-DATE").cloned().unwrap_or_default();
            let planned = attrs.get("PLANNED-DURATION").and_then(|v| v.parse::<f64>().ok());
            let end_date_epoch = attrs.get("END-DATE")
                .and_then(|v| parser::parse_iso8601_to_epoch(v));
            let start_epoch = parser::parse_iso8601_to_epoch(&start_date);
            let actual = start_epoch.zip(end_date_epoch).map(|(s, e)| e - s);

            // Classify by segmentation type byte encoded in the ID prefix
            // NBCU pattern: "0x{typebyte}-{net}-{upid}"
            let break_type = classify_scte35_id(&id);

            // Compute offset from the first segment PDT (so break can be placed on timeline)
            let start_offset_s = start_epoch.zip(first_pdt).map(|(se, fp)| se - fp);

            let entry = map.entry(id.clone()).or_insert_with(|| {
                (AdBreak {
                    id: id.clone(),
                    start_date: start_date.clone(),
                    planned_duration_s: planned,
                    actual_duration_s: actual,
                    break_type,
                    start_offset_s,
                    rendition_url: pl.url.clone(),
                }, first_pdt)
            });

            // Update: prefer actual duration once the matching SCTE35-IN arrives
            if actual.is_some() && entry.0.actual_duration_s.is_none() {
                entry.0.actual_duration_s = actual;
            }
            if planned.is_some() && entry.0.planned_duration_s.is_none() {
                entry.0.planned_duration_s = planned;
            }
        }
    }

    // ── Pass 3: EXT-X-SCTE35 per-segment fallback ────────────────────────────────────────────────
    // Used when no genuine SCTE-35 DATERANGE breaks were found (Pass 2 was empty).
    // In streams where the server suppresses DATERANGE tags (e.g. via `_NBCU_interstitial=v1_no_slot`)
    // the per-segment EXT-X-SCTE35 tags still carry break metadata.
    //
    // IMPORTANT: EXT-X-SCTE35 per-segment tags are also emitted for interstitial breaks.
    // We track whether each tag is inside an "interstitial context" (i.e. the surrounding
    // DATERANGE has CLASS=com.apple.hls.interstitial) and skip those — they are already shown
    // in the Interstitials section and must not be duplicated here.
    if map.is_empty() {
        for pl in playlists {
            if pl.media_type != "VIDEO" {
                continue;
            }
            let first_pdt = pl.segments.first().and_then(|s| s.pdt);

            let mut last_pdt_str: Option<String> = None;
            // true while we are within the span of an interstitial DATERANGE
            let mut in_interstitial_ctx = false;
            // attrs of the CUE-OUT=YES tag we are waiting to pair with a segment URI
            let mut pending: Option<std::collections::HashMap<String, String>> = None;

            for line in pl.raw_content.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
                    last_pdt_str = Some(rest.to_string());
                } else if let Some(rest) = line.strip_prefix("#EXT-X-DATERANGE:") {
                    // Update context: is this DATERANGE an interstitial?
                    let attrs = parser::parse_attributes(rest);
                    let is_interstitial = attrs.get("CLASS")
                        .is_some_and(|c| c.contains("com.apple.hls.interstitial"));
                    if is_interstitial {
                        in_interstitial_ctx = true;
                    } else {
                        // Non-interstitial DATERANGE: check if same ID as an interstitial IN tag
                        let id = attrs.get("ID").cloned().unwrap_or_default();
                        in_interstitial_ctx = interstitial_ids.contains(&id);
                    }
                    // Cancel any pending non-interstitial CUE-OUT=YES when context changes
                    if in_interstitial_ctx { pending = None; }
                } else if let Some(rest) = line.strip_prefix("#EXT-X-SCTE35:") {
                    if in_interstitial_ctx {
                        // This tag belongs to an interstitial break — skip it entirely.
                        continue;
                    }
                    let attrs = parser::parse_attributes(rest);
                    // Only the first segment of a break carries CUE-OUT=YES
                    if attrs.get("CUE-OUT").map(|v| v.as_str()) == Some("YES") {
                        pending = Some(attrs);
                    }
                } else if !line.is_empty() && !line.starts_with('#') {
                    // Segment URI line — pair with the pending CUE-OUT=YES tag
                    if let Some(attrs) = pending.take() {
                        let type_attr = attrs.get("TYPE").cloned().unwrap_or_default();
                        let id_attr = attrs.get("ID").cloned().unwrap_or_default();
                        // Synthetic ID: "scte35-TYPE-ID" (e.g. "scte35-0x30-1")
                        let id = format!("scte35-{}-{}", type_attr, id_attr);

                        if !interstitial_ids.contains(&id) && !map.contains_key(&id) {
                            let planned = attrs.get("DURATION")
                                .and_then(|v| v.parse::<f64>().ok());
                            let pdt_str = last_pdt_str.clone().unwrap_or_default();
                            let start_epoch = parser::parse_iso8601_to_epoch(&pdt_str);
                            let start_offset_s = start_epoch.zip(first_pdt).map(|(se, fp)| se - fp);
                            let break_type = classify_scte35_id(&format!("{}-dummy", type_attr));
                            map.insert(id.clone(), (AdBreak {
                                id: id.clone(),
                                start_date: pdt_str,
                                planned_duration_s: planned,
                                actual_duration_s: None,
                                break_type,
                                start_offset_s,
                                rendition_url: pl.url.clone(),
                            }, first_pdt));
                        }
                    }
                }
            }
        }
    }

    let mut breaks: Vec<AdBreak> = map.into_values().map(|(b, _)| b).collect();
    // Sort chronologically
    breaks.sort_by(|a, b| a.start_date.cmp(&b.start_date));
    breaks
}

/// Classify a SCTE-35 DATERANGE ID into a human-readable break type.
/// Uses the hex type-byte prefix that NBCU (and many other operators) encode in the ID.
fn classify_scte35_id(id: &str) -> String {
    // Pattern: "0x{hex_byte}-..." — extract the first hex byte
    let lower = id.to_lowercase();
    if let Some(rest) = lower.strip_prefix("0x") {
        let hex_part = rest.split('-').next().unwrap_or("");
        if let Ok(type_byte) = u8::from_str_radix(hex_part, 16) {
            return match type_byte {
                // SCTE 35 Table 22 — Segmentation Type ID
                0x10..=0x17 => "program".to_string(),  // Program Start/End/Early termination
                0x20..=0x27 => "chapter".to_string(),  // Chapter Start/End
                0x30..=0x37 => "ad_break".to_string(), // Provider/Distributor Ad Start/End
                0x38..=0x3f => "frame_ad".to_string(), // Provider/Distributor Placement Opportunity
                0x40..=0x4f => "breakaway".to_string(), // Unscheduled Event
                0x50..=0x5f => "network".to_string(),  // Network Start/End
                _ => "other".to_string(),
            };
        }
    }
    "other".to_string()
}



/// rfc8216bis §4.4.3.2 — MSN monotonicity: re-fetch live playlists and verify the
/// EXT-X-MEDIA-SEQUENCE value does not decrease between fetches.
async fn check_media_sequence_monotonicity(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();
    for pl in playlists {
        if pl.has_endlist { continue; }           // VOD / EVENT-ended — immutable
        if !seen_urls.insert(pl.url.clone()) { continue; }

        match fetch_text(pl.url.clone()).await {
            Ok(resp) => {
                let new_msn: Option<u64> = resp.response_text.lines()
                    .find(|l| l.trim().starts_with("#EXT-X-MEDIA-SEQUENCE:"))
                    .and_then(|l| l.split_once(':').and_then(|(_, v)| v.trim().parse().ok()));
                if let Some(new_msn) = new_msn
                    && new_msn < pl.media_sequence {
                        issues.push(Issue {
                            severity: Severity::Error,
                            segment_index: -1,
                            rendition_a: Some(pl.name.clone()),
                            rendition_b: None,
                            uri_a: Some(pl.url.clone()),
                            uri_b: None,
                            message: format!(
                                "rfc8216bis §4.4.3.2 — MSN monotonicity: EXT-X-MEDIA-SEQUENCE \
                                 regressed for '{}': first fetch MSN={}, reload MSN={} \
                                 (regressed by {}). A server MUST NOT decrease the Media \
                                 Sequence Number — clients will discard cached segments.",
                                pl.name, pl.media_sequence, new_msn,
                                pl.media_sequence - new_msn
                            ),
                            uri_note: Some(format!(
                                "First fetch MSN={} → reload MSN={} (delta={} regression).",
                                pl.media_sequence, new_msn, pl.media_sequence - new_msn
                            )),
                            count: 1, seg_first: -1, seg_last: -1,
                        });
                    }
            }
            Err(e) => {
                issues.push(Issue::warn(format!(
                    "rfc8216bis §4.4.3.2 — MSN monotonicity: Could not re-fetch '{}' \
                     for monotonicity check: {}",
                    pl.name, e
                )));
            }
        }
    }
    issues
}

/// Fetch and validate Playlist Delta Updates for playlists that advertise CAN-SKIP-UNTIL
async fn check_playlist_delta_updates(playlists: &[MediaPlaylist]) -> (Vec<Issue>, Vec<DeltaReport>) {
    let mut issues = Vec::new();
    let mut reports = Vec::new();

    for pl in playlists {
        let sc = match &pl.server_control {
            Some(sc) => sc,
            None => continue,
        };
        let can_skip = match sc.can_skip_until {
            Some(v) if v > 0.0 => v,
            _ => continue,
        };

        // Build delta URL by appending _HLS_skip=YES
        let sep = if pl.url.contains('?') { "&" } else { "?" };
        let delta_url = format!("{}{}_HLS_skip=YES", pl.url, sep);

        let hold_back = sc.hold_back.unwrap_or(0.0);
        let can_block_reload = sc.can_block_reload;

        match fetch_text(delta_url.clone()).await {
            Ok(delta_response) => {
                let delta_content = &delta_response.response_text;
                // Parse the delta playlist
                let mut delta_pl = MediaPlaylist::new(
                    format!("{} (delta)", pl.name),
                    delta_url.clone(),
                );
                parse_media_playlist(&delta_url, delta_content, &mut delta_pl);

                let skipped = delta_pl.skipped_segments as usize;
                let delta_seg_count = delta_pl.segments.len();

                // Validate delta response content per rfc8216bis §6.2.5.1
                // 1. EXT-X-SKIP MUST be present
                if !delta_content.contains("#EXT-X-SKIP:") {
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: Some(delta_url.clone()),
                        uri_b: None,
                        message: format!(
                            "rfc8216bis §6.2.5.1: Delta update response for '{}' does not \
                             contain EXT-X-SKIP. The server MUST include EXT-X-SKIP when \
                             responding to _HLS_skip=YES.",
                            pl.name
                        ),
                        uri_note: None,
                        count: 1, seg_first: -1, seg_last: -1,
                    });
                } else if skipped == 0 {
                    // 2. SKIPPED-SEGMENTS MUST be > 0
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: Some(delta_url.clone()),
                        uri_b: None,
                        message: format!(
                            "rfc8216bis §6.2.5.1: Delta update for '{}' has \
                             EXT-X-SKIP SKIPPED-SEGMENTS=0. At least one segment \
                             must be skipped in a valid delta response.",
                            pl.name
                        ),
                        uri_note: None,
                        count: 1, seg_first: -1, seg_last: -1,
                    });
                }
                // 3. EXT-X-MEDIA-SEQUENCE MUST still be present in delta response
                if !delta_content.contains("#EXT-X-MEDIA-SEQUENCE:") {
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: Some(delta_url.clone()),
                        uri_b: None,
                        message: format!(
                            "rfc8216bis §6.2.5.1: Delta update response for '{}' is missing \
                             EXT-X-MEDIA-SEQUENCE. All tags not skipped MUST remain in the \
                             delta playlist.",
                            pl.name
                        ),
                        uri_note: None,
                        count: 1, seg_first: -1, seg_last: -1,
                    });
                }
                // 4. VERSION MUST be >= 9 when EXT-X-SKIP is used
                if delta_pl.version < 9 {
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: Some(delta_url.clone()),
                        uri_b: None,
                        message: format!(
                            "rfc8216bis §8: Delta update response for '{}' declares \
                             EXT-X-VERSION:{} but EXT-X-SKIP requires VERSION >= 9.",
                            pl.name, delta_pl.version
                        ),
                        uri_note: None,
                        count: 1, seg_first: -1, seg_last: -1,
                    });
                }

                reports.push(DeltaReport {
                    name: pl.name.clone(),
                    media_type: pl.media_type.clone(),
                    url: pl.url.clone(),
                    delta_url,
                    can_skip_until: can_skip,
                    hold_back,
                    can_block_reload,
                    full_segment_count: pl.segments.len(),
                    delta_segment_count: delta_seg_count,
                    skipped_segments: skipped,
                    delta_error: None,
                });
            }
            Err(e) => {
                let err_msg = format!("Failed to fetch delta playlist: {}", e);
                reports.push(DeltaReport {
                    name: pl.name.clone(),
                    media_type: pl.media_type.clone(),
                    url: pl.url.clone(),
                    delta_url: delta_url.clone(),
                    can_skip_until: can_skip,
                    hold_back,
                    can_block_reload,
                    full_segment_count: pl.segments.len(),
                    delta_segment_count: 0,
                    skipped_segments: 0,
                    delta_error: Some(err_msg.clone()),
                });
                issues.push(Issue {
                    severity: Severity::Warn,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: Some(delta_url),
                    uri_b: None,
                    message: format!(
                        "rfc8216bis §6.2.5.1: delta update request for '{}' failed: {}",
                        pl.name, err_msg
                    ),
                    uri_note: None,
                    count: 1,
                    seg_first: -1,
                    seg_last: -1,
                });
            }
        }
    }

    (issues, reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(duration: f64, pdt: Option<f64>) -> types::Segment {
        types::Segment {
            uri: "seg.mp4".to_string(),
            duration,
            title: None,
            pdt,
            discontinuity: false,
            byterange: None,
            is_ad: false,
            map_uri: None,
        }
    }

    // ── is_master_playlist ────────────────────────────────────────────────────

    #[test]
    fn is_master_detects_stream_inf() {
        assert!(is_master_playlist("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=3000000\nvideo.m3u8\n"));
    }

    #[test]
    fn is_master_detects_iframe_stream_inf() {
        assert!(is_master_playlist("#EXTM3U\n#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=1000000,URI=\"iframe.m3u8\"\n"));
    }

    #[test]
    fn is_master_false_for_media_playlist() {
        assert!(!is_master_playlist("#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:4.0,\nseg0.mp4\n"));
    }

    // ── derive_color_info ─────────────────────────────────────────────────────

    #[test]
    fn derive_color_info_hdr10_from_pq() {
        assert_eq!(derive_color_info(Some("PQ"), None), Some("HDR10".to_string()));
    }

    #[test]
    fn derive_color_info_hlg() {
        assert_eq!(derive_color_info(Some("HLG"), None), Some("HLG".to_string()));
    }

    #[test]
    fn derive_color_info_sdr() {
        assert_eq!(derive_color_info(Some("SDR"), None), Some("SDR".to_string()));
    }

    #[test]
    fn derive_color_info_dolby_vision_hevc() {
        let result = derive_color_info(None, Some("dvh1.08.07,hvc1.2.4.L153.B0"));
        assert_eq!(result, Some("Dolby Vision (HEVC)".to_string()));
    }

    #[test]
    fn derive_color_info_dolby_vision_av1() {
        let result = derive_color_info(None, Some("av01.0.08M.10,dav1.0.09M.10"));
        assert_eq!(result, Some("Dolby Vision (AV1)".to_string()));
    }

    #[test]
    fn derive_color_info_no_range_no_dv_is_none() {
        assert_eq!(derive_color_info(None, Some("avc1.64001f,mp4a.40.2")), None);
    }

    // ── pdt_span_or_extinf_sum ────────────────────────────────────────────────

    #[test]
    fn pdt_span_uses_pdt_when_available() {
        let mut pl = MediaPlaylist::new("v".to_string(), "https://cdn.example.com/v.m3u8".to_string());
        // 3 segments with explicit PDTs
        let base = 1_700_000_000.0_f64;
        pl.segments = vec![
            seg(4.0, Some(base)),
            seg(4.0, Some(base + 4.0)),
            seg(4.0, Some(base + 8.0)),
        ];
        // span = (last_pdt - first_pdt) + last_dur = (base+8 - base) + 4 = 12
        let span = pdt_span_or_extinf_sum(&pl);
        assert!((span - 12.0).abs() < 0.001, "expected 12.0 got {span}");
    }

    #[test]
    fn pdt_span_falls_back_to_extinf_sum_without_pdt() {
        let mut pl = MediaPlaylist::new("v".to_string(), "https://cdn.example.com/v.m3u8".to_string());
        pl.segments = vec![seg(4.0, None), seg(6.0, None), seg(4.0, None)];
        let sum = pdt_span_or_extinf_sum(&pl);
        assert!((sum - 14.0).abs() < 0.001, "expected 14.0 got {sum}");
    }

    #[test]
    fn pdt_span_includes_skipped_segment_estimate() {
        let mut pl = MediaPlaylist::new("v".to_string(), "https://cdn.example.com/v.m3u8".to_string());
        pl.target_duration = 4.0;
        pl.skipped_segments = 3;
        pl.segments = vec![seg(4.0, None), seg(4.0, None)];
        // sum = 8.0 + 3*4.0 = 20.0
        let sum = pdt_span_or_extinf_sum(&pl);
        assert!((sum - 20.0).abs() < 0.001, "expected 20.0 got {sum}");
    }

    // ── classify_scte35_id ────────────────────────────────────────────────────

    #[test]
    fn scte35_classify_ad_break() {
        assert_eq!(classify_scte35_id("0x30-1-12345678"), "ad_break");
        assert_eq!(classify_scte35_id("0x34-2-99999999"), "ad_break");
    }

    #[test]
    fn scte35_classify_program() {
        assert_eq!(classify_scte35_id("0x10-1-12345678"), "program");
    }

    #[test]
    fn scte35_classify_chapter() {
        assert_eq!(classify_scte35_id("0x20-1-12345678"), "chapter");
    }

    #[test]
    fn scte35_classify_frame_ad() {
        assert_eq!(classify_scte35_id("0x38-1-12345678"), "frame_ad");
    }

    #[test]
    fn scte35_classify_network() {
        assert_eq!(classify_scte35_id("0x50-1-12345678"), "network");
    }

    #[test]
    fn scte35_classify_breakaway() {
        assert_eq!(classify_scte35_id("0x40-1-12345678"), "breakaway");
    }

    #[test]
    fn scte35_classify_unknown_is_other() {
        assert_eq!(classify_scte35_id("unknown-id"), "other");
        assert_eq!(classify_scte35_id(""), "other");
    }

    // ── apply_master_definitions ──────────────────────────────────────────────

    #[test]
    fn absolute_fetch_uri_with_absolute_define_value() {
        let defs = HashMap::from([(
            "HOST".to_string(),
            "https://cdn.example/hls/".to_string(),
        )]);
        assert_eq!(
            absolute_fetch_uri(
                "https://origin.example/master.m3u8",
                "{$HOST}v.m3u8",
                &defs
            ),
            "https://cdn.example/hls/v.m3u8"
        );
    }

    #[test]
    fn absolute_fetch_uri_joins_relative_after_substitution() {
        let defs = HashMap::from([("PATH".to_string(), "a1".to_string())]);
        assert_eq!(
            absolute_fetch_uri(
                "https://ex.com/hls/master.m3u8",
                "{$PATH}/prog.m3u8",
                &defs
            ),
            "https://ex.com/hls/a1/prog.m3u8"
        );
    }

    #[test]
    fn apply_master_definitions_copies_all_defs_as_fallback() {
        let mut master_defs = std::collections::HashMap::new();
        master_defs.insert("VAR_A".to_string(), "valueA".to_string());
        master_defs.insert("VAR_B".to_string(), "valueB".to_string());

        let mut pl_defs = std::collections::HashMap::new();
        // No IMPORT lines in content; apply_master_definitions should copy everything
        apply_master_definitions("", &master_defs, &mut pl_defs);

        assert_eq!(pl_defs.get("VAR_A"), Some(&"valueA".to_string()));
        assert_eq!(pl_defs.get("VAR_B"), Some(&"valueB".to_string()));
    }

    #[test]
    fn apply_master_definitions_does_not_overwrite_existing_pl_def() {
        let mut master_defs = std::collections::HashMap::new();
        master_defs.insert("VAR_A".to_string(), "from_master".to_string());

        let mut pl_defs = std::collections::HashMap::new();
        pl_defs.insert("VAR_A".to_string(), "from_pl".to_string());

        apply_master_definitions("", &master_defs, &mut pl_defs);

        // Playlist's own definition must win
        assert_eq!(pl_defs.get("VAR_A"), Some(&"from_pl".to_string()));
    }

    #[test]
    fn resolve_imports_picks_up_explicit_import_tag() {
        let mut master_defs = std::collections::HashMap::new();
        master_defs.insert("TOKEN".to_string(), "abc123".to_string());
        master_defs.insert("OTHER".to_string(), "ignored".to_string());

        let content = "#EXT-X-DEFINE:IMPORT=\"TOKEN\"\n";
        let mut pl_defs = std::collections::HashMap::new();
        resolve_imports(content, &master_defs, &mut pl_defs);

        // Only "TOKEN" should be imported (explicit IMPORT)
        assert_eq!(pl_defs.get("TOKEN"), Some(&"abc123".to_string()));
        assert!(!pl_defs.contains_key("OTHER"), "OTHER was not imported");
    }
}
