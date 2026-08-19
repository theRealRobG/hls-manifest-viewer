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

/// The audio codec named by a CODECS attribute, if one of its values names one.
///
/// §4.4.6.2 gives CODECS as a comma-separated list of formats present in the Variant Stream
/// and says nothing about their order, so the audio format is found by looking at what each
/// value names. Taking the second value assumed video-then-audio, which turned
/// `CODECS="mp4a.40.2,avc1.64001f"` into an audio group labelled with an AVC codec and left
/// audio-only Variant Streams — which have one value — with no audio codec at all.
pub fn audio_codec_of(codecs: &str) -> Option<String> {
    /// Registered sample entry prefixes, from the "HLS Authoring" codec strings in common use.
    /// Matching is by prefix and case-insensitive, which covers both `flac` and `fLaC` and the
    /// several DTS and MPEG-H variants without listing each one.
    const AUDIO: [&str; 13] = [
        "mp4a", "ac-3", "ec-3", "ac-4", "alac", "flac", "opus", "vorbis", "dts", "mha", "mhm",
        "iamf", "apac",
    ];
    const VIDEO: [&str; 13] = [
        "avc1", "avc3", "hvc1", "hev1", "dvh1", "dvhe", "dav1", "vp08", "vp8", "vp09", "vp9",
        "av01", "mp4v",
    ];
    const TEXT: [&str; 4] = ["wvtt", "stpp", "c608", "c708"];

    fn looks_like(token: &str, prefixes: &[&str]) -> bool {
        let token = token.to_ascii_lowercase();
        prefixes.iter().any(|p| token.starts_with(p))
    }

    let tokens: Vec<&str> = codecs.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    tokens.iter()
        .find(|t| looks_like(t, &AUDIO))
        // An unrecognised format may still be audio — new codecs are registered all the time —
        // so anything that is not video or a text track is taken as the audio value.
        .or_else(|| tokens.iter().find(|t| !looks_like(t, &VIDEO) && !looks_like(t, &TEXT)))
        .map(|t| t.to_string())
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

    // Build audio group → codec lookup from the CODECS attribute of the STREAM-INF tags that
    // name the group.
    let audio_group_codec: std::collections::HashMap<String, String> = master.variants.iter()
        .filter_map(|v| {
            let grp = v.audio_group.as_ref()?;
            let audio_codec = audio_codec_of(v.codecs.as_ref()?)?;
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
                fetch_issues.push(
                    Issue::warn(format!(
                        "rfc8216bis §6.2: Could not fetch media playlist '{}': {}. \
                         No check could be run against this rendition.",
                        fetch_uri, e
                    ))
                    .for_check(CheckId::PlaylistFetch),
                );
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
                fetch_issues.push(
                    Issue::warn(format!(
                        "rfc8216bis §6.2: Could not fetch audio rendition '{}': {}. \
                         No check could be run against this rendition.",
                        fetch_uri, e
                    ))
                    .for_check(CheckId::PlaylistFetch),
                );
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

    run_fetched_stream_checks(stream.master.as_ref(), &playlists, &mut report);

    // Every rendition's Delta Update is requested in one round rather than one at a time.
    let (delta_issues, delta_reports) = check_playlist_delta_updates(&playlists).await;
    let delta_probed = !delta_reports.is_empty();
    report.issues.extend(delta_issues);
    report.delta_report = delta_reports;

    report.renditions = build_renditions(&playlists);
    let inputs = RunInputs::from_run(stream.master.as_ref(), &playlists, delta_probed);
    report.playlists = playlists;
    report.master = stream.master;

    report.finalize();
    report.check_groups = categorize_issues(&report.issues, &inputs);
    report.elapsed_ms = (now_ms() - start) as u64;
    Ok(report)
}

/// Everything that can be checked from playlists already in hand, whether or not the stream
/// was reached through a multivariant playlist.
///
/// Only the five checks that read the multivariant playlist itself are conditional on there
/// being one. Interstitials, SCTE-35 and the media-playlist checks used to sit inside that
/// same condition, so a media-playlist URL was never checked for any of them.
fn run_fetched_stream_checks(
    master: Option<&MasterPlaylist>,
    playlists: &[MediaPlaylist],
    report: &mut ValidationReport,
) {
    if let Some(master) = master {
        report.issues.extend(checks::check_master_structure(master));
        report.issues.extend(checks::check_stream_inf_consistency(master));
        report.issues.extend(checks::check_bandwidth_required(master));
        report.issues.extend(checks::check_media_group_membership(master));
        report.issues.extend(checks::check_rendition_group_references(master));
    }

    // Needs the multivariant playlist to know which playlists are I-frame Variant Streams,
    // and the playlists themselves to see whether they say so.
    if master.is_some() {
        report.issues.extend(checks::check_iframe_playlists(playlists));
    }

    run_media_checks(playlists, report);

    // Parse HLS Interstitials from media playlists
    let has_interstitials = playlists.iter()
        .any(|pl| pl.date_ranges.iter().any(DateRange::is_interstitial));
    if has_interstitials {
        let (interstitial_issues, mut interstitials) = checks::check_interstitials(playlists);
        report.issues.extend(interstitial_issues);
        compute_interstitial_offsets(&mut interstitials, playlists);
        report.interstitials = interstitials;
        report.has_interstitials_data = true;
    }

    // Collect SCTE-35 ad breaks from EXT-X-DATERANGE tags
    let ad_breaks = collect_scte35_ad_breaks(playlists);
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

    // Segments the parser could not build carry their own findings (missing EXTINF, and
    // similar), which would otherwise be invisible because no later check ever sees them.
    for pl in playlists {
        report.issues.extend(pl.parse_issues.iter().cloned());
    }
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
    report.issues.extend(checks::check_daterange_consistency(playlists));
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
            hold_back: pl.server_control.as_ref().and_then(|sc| sc.hold_back),
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

/// How a check is presented in the report table.
struct CheckDef {
    id: CheckId,
    name: &'static str,
    section: &'static str,
    reference: &'static str,
    /// Whether the run held what this check reads. A check with nothing to read is reported
    /// N/A: it did not pass, it did not run, and a green tick would credit the stream with a
    /// rule it was never measured against.
    applies: fn(&RunInputs) -> bool,
}

/// Reads something in every run, so it is only ever PASS, WARN or FAIL.
const ALWAYS: fn(&RunInputs) -> bool = |_| true;

/// The report table, in display order. Every [`CheckId`] appears exactly once, which
/// `check_defs_cover_every_check_id` enforces.
const CHECK_DEFS: &[CheckDef] = &[
    // §4.4.1 Basic Tags
    CheckDef { id: CheckId::ExtM3uHeader, name: "EXTM3U Header", section: "Basic Tags", reference: "rfc8216bis §4.4.1.1", applies: ALWAYS },
    // §4.4.1.2 / §4.4.2 / §4.4.3 Singleton Tag Presence
    CheckDef { id: CheckId::SingletonTags, name: "Singleton Tags", section: "Structural", reference: "rfc8216bis §4.4.1.2/§4.4.2/§4.4.3", applies: ALWAYS },
    // §4.4.3.1
    CheckDef { id: CheckId::TargetDurationCompliance, name: "Target Duration Compliance", section: "Structural", reference: "rfc8216bis §4.4.3.1", applies: ALWAYS },
    // §4.4.3.2
    CheckDef { id: CheckId::MediaSequenceTags, name: "Media Sequence Tags", section: "Structural", reference: "rfc8216bis §4.4.3.2", applies: ALWAYS },
    // §4.4.3.3 / §6.2.4
    CheckDef { id: CheckId::DiscontinuitySequence, name: "Discontinuity Sequence", section: "Alignment", reference: "rfc8216bis §4.4.3.3/§6.2.4", applies: |r| r.playlists > 1 },
    // §4.4.3.5
    CheckDef { id: CheckId::PlaylistTypeEndlist, name: "Playlist Type / ENDLIST", section: "Structural", reference: "rfc8216bis §4.4.3.5", applies: ALWAYS },
    // §4.4.4.1
    CheckDef { id: CheckId::SegmentStructure, name: "Segment Structure", section: "Structural", reference: "rfc8216bis §4.4.4.1", applies: ALWAYS },
    // §4.4.4.4
    CheckDef { id: CheckId::EncryptionConsistency, name: "Encryption Consistency", section: "Security", reference: "rfc8216bis §4.4.4.4", applies: |r| r.has_encryption },
    // §4.4.5.2 / §6.2.5.1
    CheckDef { id: CheckId::DeltaUpdates, name: "Playlist Delta Updates", section: "LL-HLS", reference: "rfc8216bis §4.4.5.2/§6.2.5.1", applies: |r| r.delta_probed },
    // §4.4.6.2
    CheckDef { id: CheckId::BandwidthRequired, name: "BANDWIDTH Required", section: "Multivariant", reference: "rfc8216bis §4.4.6.2", applies: |r| r.has_master },
    CheckDef { id: CheckId::StreamInfConsistency, name: "STREAM-INF Consistency", section: "Multivariant", reference: "rfc8216bis §4.4.6.2/§6.2.4", applies: |r| r.has_master },
    CheckDef { id: CheckId::RenditionGroupReferences, name: "Rendition Group References", section: "Multivariant", reference: "rfc8216bis §4.4.6.2", applies: |r| r.has_master },
    // §4.4.6.1.1
    CheckDef { id: CheckId::MediaGroupMembership, name: "Media Group Membership", section: "Multivariant", reference: "rfc8216bis §4.4.6.1.1", applies: |r| r.has_master },
    // §4.4.6.3
    CheckDef { id: CheckId::IFramePlaylists, name: "I-Frame Playlists", section: "Multivariant", reference: "rfc8216bis §4.4.6.3", applies: |r| r.has_iframe_variants },
    // §6.2.2
    CheckDef { id: CheckId::LivePlaylistWindow, name: "Live Playlist Window", section: "Live", reference: "rfc8216bis §6.2.2", applies: ALWAYS },
    // §6.2.4
    CheckDef { id: CheckId::PdtCoverage, name: "PDT Coverage", section: "Timing", reference: "rfc8216bis §6.2.4/§4.4.5.1", applies: |r| r.has_pdt_tags || r.has_dateranges },
    CheckDef { id: CheckId::PdtAlignment, name: "PDT Alignment", section: "Alignment", reference: "rfc8216bis §6.2.4", applies: |r| r.has_pdt_tags && r.video_playlists > 1 },
    CheckDef { id: CheckId::TargetDurationConsistency, name: "Target Duration Consistency", section: "Alignment", reference: "rfc8216bis §6.2.4", applies: |r| r.playlists > 1 },
    CheckDef { id: CheckId::CumulativeDrift, name: "Cumulative Drift", section: "Alignment", reference: "rfc8216bis §6.2.4", applies: |r| r.video_playlists > 1 },
    CheckDef { id: CheckId::DateRangeConsistency, name: "Date Range Consistency", section: "Alignment", reference: "rfc8216bis §6.2.4", applies: |r| r.has_dateranges && r.playlists > 1 },
    // §4.4.4.1 drift cross-rendition
    CheckDef { id: CheckId::DurationDrift, name: "EXTINF Duration Drift", section: "Alignment", reference: "rfc8216bis §4.4.4.1", applies: |r| r.video_playlists > 1 },
    CheckDef { id: CheckId::SegmentCount, name: "Segment Count", section: "Alignment", reference: "rfc8216bis §4.4.4.1", applies: |r| r.vod_video_playlists > 1 },
    // §4.4.2.3
    CheckDef { id: CheckId::VariableDefinitions, name: "Variable Definitions", section: "Structural", reference: "rfc8216bis §4.4.2.3", applies: |r| r.has_defines },
    // §8
    CheckDef { id: CheckId::VersionCompatibility, name: "Version Compatibility", section: "Version", reference: "rfc8216bis §8", applies: ALWAYS },
    // LL-HLS §4.4.3–4.4.5, §6.2.5.2
    CheckDef { id: CheckId::LlHls, name: "LL-HLS Compliance", section: "LL-HLS", reference: "rfc8216bis §4.4.3–4.4.5", applies: |r| r.has_low_latency_tags },
    // Appendix D
    CheckDef { id: CheckId::Interstitials, name: "HLS Interstitials", section: "Interstitials", reference: "rfc8216bis Appendix D", applies: |r| r.has_interstitials },
    // Delivery
    CheckDef { id: CheckId::PlaylistFetch, name: "Playlist Retrieval", section: "Delivery", reference: "rfc8216bis §6.2", applies: ALWAYS },
];

/// Group of last resort, so a finding whose check has no row in the table is still shown.
const UNGROUPED_NAME: &str = "Other Findings";

/// [`CheckGroup::status`] for a check the run held nothing for.
pub const NOT_APPLICABLE: &str = "N/A";

/// Group issues by the check that produced them.
///
/// Grouping used to search each message for a check's keywords, which meant one check could
/// claim another's findings — "TARGETDURATION" matched the target-duration row before the
/// consistency row could see it — and a finding matching no keyword at all never reached the
/// UI. Each finding now names its own check and this is a lookup, with anything unclaimed
/// collected into [`UNGROUPED_NAME`] rather than dropped.
/// `inputs` decides which rows are reported N/A: a check whose subject the run never held
/// cannot have passed. A check that did have something to read but produced no finding is
/// still PASS, even where `inputs` says the subject was absent, because a finding is proof
/// that the check ran.
fn categorize_issues(issues: &[Issue], inputs: &RunInputs) -> Vec<CheckGroup> {
    fn status_of(issues: &[Issue]) -> String {
        if issues.iter().any(|i| i.severity == Severity::Error) {
            "FAIL".to_string()
        } else if issues.iter().any(|i| i.severity == Severity::Warn) {
            "WARN".to_string()
        } else {
            "PASS".to_string()
        }
    }

    let mut groups: Vec<CheckGroup> = CHECK_DEFS.iter().map(|def| {
        let matched: Vec<Issue> = issues.iter()
            .filter(|i| i.check_id == def.id)
            .cloned()
            .collect();
        let status = if matched.is_empty() && !(def.applies)(inputs) {
            NOT_APPLICABLE.to_string()
        } else {
            status_of(&matched)
        };
        CheckGroup {
            name: def.name.to_string(),
            section: def.section.to_string(),
            reference: def.reference.to_string(),
            status,
            issues: matched,
        }
    }).collect();

    let known: std::collections::HashSet<CheckId> = CHECK_DEFS.iter().map(|d| d.id).collect();
    let ungrouped: Vec<Issue> = issues.iter()
        .filter(|i| !known.contains(&i.check_id))
        .cloned()
        .collect();
    if !ungrouped.is_empty() {
        groups.push(CheckGroup {
            name: UNGROUPED_NAME.to_string(),
            section: "Other".to_string(),
            reference: "rfc8216bis".to_string(),
            status: status_of(&ungrouped),
            issues: ungrouped,
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
        for range in &pl.date_ranges {
            if range.is_interstitial()
                && let Some(id) = range.id.as_ref() {
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

        for range in &pl.date_ranges {
            let attrs = &range.attributes;

            // Must carry at least one SCTE35 payload attribute
            let has_scte35 = attrs.contains_key("SCTE35-OUT") || attrs.contains_key("SCTE35-IN");
            if !has_scte35 {
                continue;
            }

            let id = range.id.clone().unwrap_or_default();

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
    //
    // Unlike the passes above, this one reads the playlist text rather than the parsed Date
    // Ranges: what it needs is the *order* of the EXT-X-SCTE35 tags, the Date Ranges and the
    // segment URIs between them, which is what carries the break from one line to the next.
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
                        // Interstitial breaks are excluded by `in_interstitial_ctx` above, not
                        // by this ID: no Date Range can declare one that starts with "scte35-".
                        let id = format!("scte35-{}-{}", type_attr, id_attr);

                        if !map.contains_key(&id) {
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



// rfc8216bis §6.2.2 requires that a Media Playlist's Media Sequence Number never decrease.
// The check that used to live here re-fetched every live playlist and compared the reloaded
// EXT-X-MEDIA-SEQUENCE with the first one. It is deliberately gone, and no replacement is
// attempted, because neither of its outcomes said anything true about the stream:
//
//   * Its pass was vacuous. The reload happens milliseconds after the first fetch, over the
//     same HTTP cache, so the expected answer is byte-identical to the response already in
//     hand. Nothing was measured, yet the report showed a green tick against a MUST.
//   * Its failure was unsound. A presentation served from more than one CDN edge can answer
//     two requests from two generations of the playlist, and the older one carries the lower
//     Media Sequence Number. That is a stale edge, not a server decreasing its MSN, and
//     reporting it as an Error against a MUST is a false accusation on a conforming stream.
//
// Making it sound needs a second observation the tool cannot honestly obtain: a re-fetch that
// provably bypasses every cache between here and the origin, taken far enough apart in time
// to expect the playlist to have advanced. A cache-busting query parameter is not that — it
// changes the resource identity, so signed-URL streams answer 403 and CDNs are free to serve
// a different generation — and `fetch` cache modes only reach the browser's own cache.
// Monotonicity is therefore left to a tool that can watch a stream over time.


/// The CAN-SKIP-UNTIL boundary to probe for `pl`, or `None` when a Playlist Delta Update
/// does not apply to it.
///
/// A playlist that has ENDLIST is final: it will never gain a segment, so there is nothing
/// for a client to skip past and no reason for a server to answer `_HLS_skip=YES` with
/// EXT-X-SKIP.
fn delta_update_can_skip_until(pl: &MediaPlaylist) -> Option<f64> {
    if pl.has_endlist {
        return None;
    }
    pl.server_control.as_ref()?.can_skip_until.filter(|v| *v > 0.0)
}

/// What a Playlist Delta Update response must contain (rfc8216bis §6.2.5.1, §8).
///
/// SKIPPED-SEGMENTS is deliberately not checked for being greater than zero: the server skips
/// the segments older than CAN-SKIP-UNTIL, and a window that has just started or has been
/// trimmed can have none to skip, which is a conforming answer.
fn delta_response_issues(
    name: &str,
    delta_url: &str,
    delta_content: &str,
    delta_pl: &MediaPlaylist,
) -> Vec<Issue> {
    let mut issues = Vec::new();

    if !delta_content.contains("#EXT-X-SKIP:") {
        issues.push(Issue::error(format!(
            "rfc8216bis §6.2.5.1: Delta update response for '{}' does not contain \
             EXT-X-SKIP. The server MUST include EXT-X-SKIP when responding to \
             _HLS_skip=YES.",
            name
        )).for_check(CheckId::DeltaUpdates).in_rendition(name).at_uri(delta_url));
    }
    if !delta_content.contains("#EXT-X-MEDIA-SEQUENCE:") {
        issues.push(Issue::error(format!(
            "rfc8216bis §6.2.5.1: Delta update response for '{}' is missing \
             EXT-X-MEDIA-SEQUENCE. All tags not skipped MUST remain in the delta playlist.",
            name
        )).for_check(CheckId::DeltaUpdates).in_rendition(name).at_uri(delta_url));
    }
    if delta_pl.version < 9 {
        issues.push(Issue::error(format!(
            "rfc8216bis §8: Delta update response for '{}' declares EXT-X-VERSION:{} \
             but EXT-X-SKIP requires VERSION >= 9.",
            name, delta_pl.version
        )).for_check(CheckId::VersionCompatibility).in_rendition(name).at_uri(delta_url));
    }

    issues
}

/// Fetch and validate Playlist Delta Updates for playlists that advertise CAN-SKIP-UNTIL.
///
/// Only live playlists are probed. A Playlist Delta Update exists so a client can reload a
/// playlist that is still changing without re-reading the whole window; a playlist that has
/// ENDLIST will never change again, so requesting `_HLS_skip=YES` against it and then
/// requiring EXT-X-SKIP in the answer was demanding something the spec does not.
async fn check_playlist_delta_updates(playlists: &[MediaPlaylist]) -> (Vec<Issue>, Vec<DeltaReport>) {
    let mut issues = Vec::new();
    let mut reports = Vec::new();

    // Every rendition's Delta Update is requested at once. Awaiting them one at a time made
    // the report wait for one round-trip per rendition, and on a live presentation each of
    // those may block for up to the server's hold-back.
    let probes: Vec<(&MediaPlaylist, f64, String)> = playlists.iter()
        .filter_map(|pl| {
            let can_skip = delta_update_can_skip_until(pl)?;
            let sep = if pl.url.contains('?') { "&" } else { "?" };
            Some((pl, can_skip, format!("{}{}_HLS_skip=YES", pl.url, sep)))
        })
        .collect();
    let responses = futures::future::join_all(
        probes.iter().map(|(_, _, delta_url)| fetch_text(delta_url.clone())),
    ).await;

    for ((pl, can_skip, delta_url), response) in probes.into_iter().zip(responses) {
        let sc = pl.server_control.as_ref().expect("CAN-SKIP-UNTIL comes from SERVER-CONTROL");
        let hold_back = sc.hold_back.unwrap_or(0.0);
        let can_block_reload = sc.can_block_reload;

        match response {
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

                issues.extend(delta_response_issues(
                    &pl.name,
                    &delta_url,
                    delta_content,
                    &delta_pl,
                ));

                reports.push(DeltaReport {
                    name: pl.name.clone(),
                    media_type: pl.media_type.clone(),
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
                    delta_url: delta_url.clone(),
                    can_skip_until: can_skip,
                    hold_back,
                    can_block_reload,
                    full_segment_count: pl.segments.len(),
                    delta_segment_count: 0,
                    skipped_segments: 0,
                    delta_error: Some(err_msg.clone()),
                });
                issues.push(Issue::warn(format!(
                    "rfc8216bis §6.2.5.1: delta update request for '{}' failed: {}",
                    pl.name, err_msg
                )).for_check(CheckId::PlaylistFetch).in_rendition(pl.name.as_str()).at_uri(delta_url));
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

    // ── categorize_issues ─────────────────────────────────────────────────────

    fn group<'a>(groups: &'a [CheckGroup], name: &str) -> &'a CheckGroup {
        groups
            .iter()
            .find(|g| g.name == name)
            .unwrap_or_else(|| panic!("no '{name}' group in {:?}", groups.iter().map(|g| &g.name).collect::<Vec<_>>()))
    }

    #[test]
    fn check_defs_cover_every_check_id() {
        for id in CheckId::ALL {
            let matches = CHECK_DEFS.iter().filter(|d| d.id == *id).count();
            assert_eq!(matches, 1, "{id:?} must have exactly one row in CHECK_DEFS, found {matches}");
        }
        assert!(
            !CHECK_DEFS.iter().any(|d| d.id == CheckId::Unassigned),
            "the catch-all group is added separately and must not be a CHECK_DEFS row"
        );
    }

    #[test]
    fn discontinuity_finding_lands_in_its_own_group() {
        let issues = checks::check_discontinuity_sequence(&[
            {
                let mut pl = MediaPlaylist::new("v0".into(), "https://ex.com/v0.m3u8".into());
                pl.discontinuity_sequence = 0;
                pl
            },
            {
                let mut pl = MediaPlaylist::new("v1".into(), "https://ex.com/v1.m3u8".into());
                pl.discontinuity_sequence = 3;
                pl
            },
        ]);
        assert_eq!(issues.len(), 1, "expected one discontinuity finding: {issues:?}");

        let groups = categorize_issues(&issues, &RunInputs::everything());
        assert_eq!(group(&groups, "Discontinuity Sequence").issues.len(), 1);
        assert_eq!(group(&groups, "Discontinuity Sequence").status, "FAIL");
        assert!(
            groups.iter().all(|g| g.name == "Discontinuity Sequence" || g.issues.is_empty()),
            "no other group may claim the finding"
        );
    }

    #[test]
    fn target_duration_groups_do_not_steal_each_others_findings() {
        // Both messages name TARGETDURATION, which is how keyword matching used to file the
        // consistency finding under Target Duration Compliance and then drop it.
        let issues = vec![
            Issue::error("rfc8216bis §4.4.3.1: TARGETDURATION missing".into())
                .for_check(CheckId::TargetDurationCompliance),
            Issue::error("rfc8216bis §6.2.4: TARGETDURATION values differ".into())
                .for_check(CheckId::TargetDurationConsistency),
        ];
        let groups = categorize_issues(&issues, &RunInputs::everything());
        assert_eq!(group(&groups, "Target Duration Compliance").issues.len(), 1);
        assert_eq!(group(&groups, "Target Duration Consistency").issues.len(), 1);
    }

    #[test]
    fn ll_hls_findings_are_not_stolen_by_target_duration_compliance() {
        // CAN-SKIP-UNTIL and HOLD-BACK are both expressed as multiples of TARGETDURATION.
        let mut pl = MediaPlaylist::new("v".into(), "https://ex.com/v.m3u8".into());
        pl.target_duration = 6.0;
        pl.server_control = Some(ServerControl {
            can_skip_until: Some(12.0),
            hold_back: Some(6.0),
            part_hold_back: None,
            can_block_reload: true,
            can_skip_dateranges: false,
        });
        let issues = checks::check_ll_hls_compliance(&[pl]);
        assert!(
            issues.iter().any(|i| i.message.contains("CAN-SKIP-UNTIL")),
            "expected a CAN-SKIP-UNTIL finding: {issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.message.contains("HOLD-BACK")),
            "expected a HOLD-BACK finding: {issues:?}"
        );

        let groups = categorize_issues(&issues, &RunInputs::everything());
        assert_eq!(group(&groups, "LL-HLS Compliance").issues.len(), issues.len());
        assert!(group(&groups, "Target Duration Compliance").issues.is_empty());
    }

    #[test]
    fn fetch_failures_reach_the_report() {
        let issues = vec![
            Issue::warn("rfc8216bis §6.2: Could not fetch media playlist 'x': 404".into())
                .for_check(CheckId::PlaylistFetch),
        ];
        let groups = categorize_issues(&issues, &RunInputs::everything());
        assert_eq!(group(&groups, "Playlist Retrieval").issues.len(), 1);
        assert_eq!(group(&groups, "Playlist Retrieval").status, "WARN");
    }

    #[test]
    fn checks_the_run_held_nothing_for_are_reported_not_applicable() {
        // A media-playlist URL with nothing but segments: the multivariant, low-latency,
        // Delta Update and interstitial rows were all reported PASS, which credited the
        // stream with rules it was never measured against.
        let inputs = RunInputs::from_run(None, &[media_only_playlist()], false);
        let groups = categorize_issues(&[], &inputs);
        for name in [
            "BANDWIDTH Required",
            "STREAM-INF Consistency",
            "Media Group Membership",
            "I-Frame Playlists",
            "LL-HLS Compliance",
            "Playlist Delta Updates",
            "Encryption Consistency",
            "Segment Count",
        ] {
            assert_eq!(
                group(&groups, name).status,
                NOT_APPLICABLE,
                "'{name}' had nothing to read in a media-only run"
            );
        }
        assert_eq!(
            group(&groups, "EXTM3U Header").status,
            "PASS",
            "a check that reads every playlist did run"
        );
    }

    #[test]
    fn a_check_that_had_something_to_read_still_passes() {
        let master = parser::parse_master_playlist(
            "https://cdn.example.com/master.m3u8",
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000,CODECS=\"avc1.64001f\"\nv.m3u8\n",
        );
        let inputs = RunInputs::from_run(Some(&master), &[media_only_playlist()], false);
        let groups = categorize_issues(&[], &inputs);
        assert_eq!(group(&groups, "BANDWIDTH Required").status, "PASS");
        assert_eq!(
            group(&groups, "I-Frame Playlists").status,
            NOT_APPLICABLE,
            "this presentation declares no I-frame Variant Stream"
        );
    }

    #[test]
    fn a_finding_makes_its_check_applicable_whatever_the_inputs_say() {
        // Proof beats inference: the check plainly ran, so it cannot be reported N/A.
        let issues = vec![
            Issue::error("rfc8216bis §4.4.6.2: BANDWIDTH missing".into())
                .for_check(CheckId::BandwidthRequired),
        ];
        let groups = categorize_issues(&issues, &RunInputs::default());
        assert_eq!(group(&groups, "BANDWIDTH Required").status, "FAIL");
    }

    // ── audio_codec_of ────────────────────────────────────────────────────────

    #[test]
    fn audio_codec_is_found_whichever_order_codecs_are_listed_in() {
        assert_eq!(
            audio_codec_of("avc1.64001f,mp4a.40.2").as_deref(),
            Some("mp4a.40.2")
        );
        assert_eq!(
            audio_codec_of("mp4a.40.2,avc1.64001f").as_deref(),
            Some("mp4a.40.2"),
            "taking the second value labelled this audio group with a video codec"
        );
        assert_eq!(audio_codec_of("hvc1.2.4.L123.B0, ec-3").as_deref(), Some("ec-3"));
        assert_eq!(
            audio_codec_of("mp4a.40.2").as_deref(),
            Some("mp4a.40.2"),
            "an audio-only Variant Stream lists one codec"
        );
    }

    #[test]
    fn a_codecs_attribute_with_no_audio_value_yields_none() {
        assert_eq!(audio_codec_of("avc1.64001f").as_deref(), None);
        assert_eq!(audio_codec_of("hvc1.2.4.L123.B0,wvtt").as_deref(), None);
        assert_eq!(audio_codec_of("").as_deref(), None);
    }

    #[test]
    fn an_unrecognised_codec_beside_a_video_one_is_taken_as_the_audio() {
        // Formats are registered faster than this list is updated, so a value that is plainly
        // not video and not a text track is still reported rather than dropped.
        assert_eq!(audio_codec_of("avc1.64001f,xyz1.2").as_deref(), Some("xyz1.2"));
    }

    #[test]
    fn a_finding_that_names_no_check_is_still_reported() {
        let issues = vec![Issue::error("something nothing claims".into())];
        let groups = categorize_issues(&issues, &RunInputs::everything());
        let other = group(&groups, UNGROUPED_NAME);
        assert_eq!(other.issues.len(), 1, "an unclaimed finding must not be dropped");
        assert_eq!(other.status, "FAIL");
    }

    #[test]
    fn every_finding_is_grouped_exactly_once() {
        let issues: Vec<Issue> = CheckId::ALL.iter()
            .map(|id| Issue::warn(format!("finding from {id:?}")).for_check(*id))
            .collect();
        let groups = categorize_issues(&issues, &RunInputs::everything());
        let grouped: usize = groups.iter().map(|g| g.issues.len()).sum();
        assert_eq!(grouped, issues.len(), "every finding must appear in exactly one group");
        assert!(
            !groups.iter().any(|g| g.name == UNGROUPED_NAME),
            "no finding from a known check may fall through to the catch-all group"
        );
    }

    // ── run_fetched_stream_checks ─────────────────────────────────────────────

    /// A live media playlist with a broken interstitial, a segment URI with no EXTINF, a
    /// genuine SCTE-35 break and no #EXTM3U, so one fixture exercises each of the paths that
    /// used to be reachable only through a multivariant playlist.
    fn media_only_playlist() -> MediaPlaylist {
        let content = "#EXT-X-TARGETDURATION:4\n\
             #EXT-X-VERSION:9\n\
             #EXT-X-MEDIA-SEQUENCE:10\n\
             #EXT-X-DATERANGE:ID=\"ad-1\",CLASS=\"com.apple.hls.interstitial\",\
             START-DATE=\"2024-01-15T12:00:00Z\"\n\
             #EXT-X-DATERANGE:ID=\"0x30-1-99\",START-DATE=\"2024-01-15T12:00:04Z\",\
             SCTE35-OUT=0xFC\n\
             #EXTINF:4.0,\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:00Z\n\
             s0.m4s\n\
             #EXTINF:4.0,\ns1.m4s\n\
             #EXTINF:4.0,\ns2.m4s\n\
             orphan.m4s\n";
        let url = "https://cdn.example.com/media.m3u8";
        let mut pl = MediaPlaylist::new("media".to_string(), url.to_string());
        parse_media_playlist(url, content, &mut pl);
        pl
    }

    fn report_for(master: Option<&MasterPlaylist>, playlists: &[MediaPlaylist]) -> ValidationReport {
        let mut report = ValidationReport::new();
        report.tolerance_ms = 100.0;
        run_fetched_stream_checks(master, playlists, &mut report);
        report
    }

    #[test]
    fn a_media_only_stream_is_checked_without_a_multivariant_playlist() {
        let playlists = vec![media_only_playlist()];
        let report = report_for(None, &playlists);

        let ids: Vec<CheckId> = report.issues.iter().map(|i| i.check_id).collect();
        assert!(
            ids.contains(&CheckId::Interstitials),
            "interstitial checks must run for a media-only URL: {:#?}",
            report.issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
        assert!(report.has_interstitials_data, "the UI needs the interstitial data too");
        assert_eq!(report.interstitials.len(), 1);
        assert!(
            ids.contains(&CheckId::SegmentStructure),
            "parser findings must reach the report for a media-only URL"
        );
        assert!(
            ids.contains(&CheckId::ExtM3uHeader),
            "media-playlist checks must run for a media-only URL"
        );
        assert!(report.has_scte35_data, "SCTE-35 collection must run for a media-only URL");
        assert_eq!(
            report.ad_breaks.len(),
            1,
            "the interstitial Date Range ad-1 must be excluded; only the genuine SCTE-35 \
             break 0x30-1-99 should remain: {:?}",
            report.ad_breaks.iter().map(|b| &b.id).collect::<Vec<_>>()
        );
        assert_eq!(
            report.ad_breaks[0].id,
            "0x30-1-99",
            "the surviving ad break must be the SCTE-35 Date Range, not the interstitial"
        );
        assert!(
            report.playlist_window_s > 0.0,
            "the playlist window must be computed for a media-only URL"
        );
    }

    #[test]
    fn multivariant_checks_run_only_when_there_is_a_multivariant_playlist() {
        // A master with no BANDWIDTH and a dangling AUDIO reference, so the master-only
        // checks have something to report when they are given one.
        let master = parser::parse_master_playlist(
            "https://cdn.example.com/master.m3u8",
            "#EXTM3U\n#EXT-X-VERSION:9\n\
             #EXT-X-STREAM-INF:AUDIO=\"missing\",CODECS=\"avc1.64001f\"\nv.m3u8\n",
        );
        let playlists = vec![media_only_playlist()];

        let with_master = report_for(Some(&master), &playlists);
        let master_ids: Vec<CheckId> = with_master.issues.iter().map(|i| i.check_id).collect();
        assert!(master_ids.contains(&CheckId::BandwidthRequired), "{master_ids:?}");
        assert!(master_ids.contains(&CheckId::RenditionGroupReferences), "{master_ids:?}");

        let media_only = report_for(None, &playlists);
        let media_ids: Vec<CheckId> = media_only.issues.iter().map(|i| i.check_id).collect();
        assert!(
            !media_ids.contains(&CheckId::BandwidthRequired)
                && !media_ids.contains(&CheckId::RenditionGroupReferences)
                && !media_ids.contains(&CheckId::StreamInfConsistency)
                && !media_ids.contains(&CheckId::MediaGroupMembership),
            "checks that read the multivariant playlist cannot run without one: {media_ids:?}"
        );
    }

    /// A multivariant playlist that declares an I-frame Variant Stream, so the §4.4.6.3 check
    /// is both reachable and applicable.
    fn master_with_iframe_variant() -> MasterPlaylist {
        parser::parse_master_playlist(
            "https://cdn.example.com/master.m3u8",
            "#EXTM3U\n#EXT-X-VERSION:9\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,CODECS=\"avc1.64001f\"\nv.m3u8\n\
             #EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=100,CODECS=\"avc1.64001f\",URI=\"trick.m3u8\"\n",
        )
    }

    /// A regular rendition and a trick-play rendition that both carry the Date Range 'ad-1'
    /// with a different DURATION, and where the trick-play playlist never declares
    /// EXT-X-I-FRAMES-ONLY. One fixture, one finding each from the two checks.
    fn iframe_and_daterange_playlists() -> Vec<MediaPlaylist> {
        let body = |duration: &str| format!(
            "#EXTM3U\n#EXT-X-VERSION:9\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:00Z\n\
             #EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\",\
             DURATION={duration}\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n#EXT-X-ENDLIST\n"
        );
        let mut video = MediaPlaylist::new("v".into(), "https://cdn.example.com/v.m3u8".into());
        parse_media_playlist(&video.url.clone(), &body("30.0"), &mut video);

        let mut trick =
            MediaPlaylist::new("trick".into(), "https://cdn.example.com/trick.m3u8".into());
        parse_media_playlist(&trick.url.clone(), &body("15.0"), &mut trick);
        trick.is_iframe = true;
        assert!(!trick.iframes_only, "the fixture is the playlist that forgot the tag");

        vec![video, trick]
    }

    #[test]
    fn the_iframe_playlist_check_is_wired_into_a_run() {
        // The check itself is unit-tested in `checks`; what this pins is that the run calls
        // it at all, which nothing else would notice if the call site were dropped.
        let master = master_with_iframe_variant();
        let report = report_for(Some(&master), &iframe_and_daterange_playlists());
        let iframe: Vec<&Issue> = report.issues.iter()
            .filter(|i| i.check_id == CheckId::IFramePlaylists)
            .collect();
        assert_eq!(
            iframe.len(),
            1,
            "§4.4.6.3 must be checked when a run has a multivariant playlist: {:#?}",
            report.issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
        assert_eq!(iframe[0].severity, Severity::Error);

        let inputs = RunInputs::from_run(Some(&master), &iframe_and_daterange_playlists(), false);
        let groups = categorize_issues(&report.issues, &inputs);
        assert_eq!(group(&groups, "I-Frame Playlists").status, "FAIL");
    }

    #[test]
    fn the_date_range_consistency_check_is_wired_into_a_run() {
        let master = master_with_iframe_variant();
        let report = report_for(Some(&master), &iframe_and_daterange_playlists());
        let dateranges: Vec<&Issue> = report.issues.iter()
            .filter(|i| i.check_id == CheckId::DateRangeConsistency)
            .collect();
        assert_eq!(
            dateranges.len(),
            1,
            "§6.2.4 Date Range consistency must be checked on every run: {:#?}",
            report.issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
        assert_eq!(dateranges[0].severity, Severity::Error);

        let inputs = RunInputs::from_run(Some(&master), &iframe_and_daterange_playlists(), false);
        let groups = categorize_issues(&report.issues, &inputs);
        assert_eq!(group(&groups, "Date Range Consistency").status, "FAIL");
    }

    #[test]
    fn run_media_checks_folds_parse_issues_into_the_report() {
        let mut pl = MediaPlaylist::new("v".into(), "https://ex.com/v.m3u8".into());
        pl.raw_content = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-ENDLIST\n".to_string();
        pl.target_duration = 4.0;
        pl.has_endlist = true;
        pl.parse_issues = vec![
            Issue::error("rfc8216bis §4.4.4.1: Media Segment URI on line 4 has no preceding \
                          EXTINF tag.".into())
                .for_check(CheckId::SegmentStructure),
        ];

        let mut report = ValidationReport::new();
        report.tolerance_ms = 100.0;
        run_media_checks(&[pl], &mut report);

        let folded: Vec<&Issue> = report.issues.iter()
            .filter(|i| i.check_id == CheckId::SegmentStructure)
            .collect();
        assert_eq!(
            folded.len(),
            1,
            "a segment the parser could not build must be reported: {:#?}",
            report.issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
        assert_eq!(folded[0].severity, Severity::Error);

        // And it survives into the grouped view the UI renders.
        let groups = categorize_issues(&report.issues, &RunInputs::everything());
        assert_eq!(group(&groups, "Segment Structure").issues.len(), 1);
    }

    // ── delta_response_issues ─────────────────────────────────────────────────

    fn delta_playlist(content: &str) -> MediaPlaylist {
        let url = "https://cdn.example.com/v.m3u8?_HLS_skip=YES";
        let mut pl = MediaPlaylist::new("v (delta)".to_string(), url.to_string());
        parse_media_playlist(url, content, &mut pl);
        pl
    }

    #[test]
    fn a_delta_response_that_skipped_nothing_is_accepted() {
        // SKIPPED-SEGMENTS=0 is a conforming answer when the window holds nothing older than
        // CAN-SKIP-UNTIL, so it must not be reported.
        let content = "#EXTM3U\n#EXT-X-VERSION:9\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:10\n#EXT-X-SKIP:SKIPPED-SEGMENTS=0\n\
             #EXTINF:4.0,\ns0.m4s\n";
        let delta_pl = delta_playlist(content);
        assert_eq!(delta_pl.skipped_segments, 0);
        let issues = delta_response_issues("v", "https://ex.com/v.m3u8?_HLS_skip=YES", content, &delta_pl);
        assert!(issues.is_empty(), "{issues:#?}");
    }

    #[test]
    fn a_delta_response_missing_ext_x_skip_errors() {
        let content = "#EXTM3U\n#EXT-X-VERSION:9\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:10\n#EXTINF:4.0,\ns0.m4s\n";
        let delta_pl = delta_playlist(content);
        let issues = delta_response_issues("v", "https://ex.com/v.m3u8?_HLS_skip=YES", content, &delta_pl);
        assert_eq!(issues.len(), 1, "{issues:#?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].check_id, CheckId::DeltaUpdates);
        assert!(issues[0].message.contains("EXT-X-SKIP"));
    }

    #[test]
    fn a_delta_response_below_version_9_errors() {
        let content = "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:10\n#EXT-X-SKIP:SKIPPED-SEGMENTS=4\n\
             #EXTINF:4.0,\ns0.m4s\n";
        let delta_pl = delta_playlist(content);
        let issues = delta_response_issues("v", "https://ex.com/v.m3u8?_HLS_skip=YES", content, &delta_pl);
        assert_eq!(issues.len(), 1, "{issues:#?}");
        assert_eq!(issues[0].check_id, CheckId::VersionCompatibility);
    }

    // ── delta_update_can_skip_until ───────────────────────────────────────────

    #[test]
    fn delta_updates_are_probed_only_for_live_playlists() {
        let mut live = MediaPlaylist::new("v".into(), "https://ex.com/v.m3u8".into());
        live.server_control = Some(ServerControl {
            can_skip_until: Some(36.0),
            hold_back: Some(18.0),
            part_hold_back: None,
            can_block_reload: true,
            can_skip_dateranges: false,
        });
        assert_eq!(delta_update_can_skip_until(&live), Some(36.0));

        let mut ended = live.clone();
        ended.has_endlist = true;
        assert_eq!(
            delta_update_can_skip_until(&ended),
            None,
            "a playlist with ENDLIST cannot serve a delta update"
        );

        let plain = MediaPlaylist::new("v".into(), "https://ex.com/v.m3u8".into());
        assert_eq!(delta_update_can_skip_until(&plain), None);
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
