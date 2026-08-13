//! Apple Authoring Spec §9 — Multivariant playlist.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        return issues;
    };

    let variants: Vec<_> = master.variants.iter().filter(|v| !v.is_iframe).collect();
    let iframes: Vec<_> = master.variants.iter().filter(|v| v.is_iframe).collect();

    // §9.1–9.4 — CODECS/RESOLUTION required
    for v in variants.iter().chain(iframes.iter()) {
        if v.codecs.is_none() {
            issues.push(author_error(
                "9.1",
                format!("STREAM-INF missing CODECS on '{}'", v.uri),
            ));
        }
        if !v.is_iframe && v.resolution.is_none() {
            // audio-only may omit RESOLUTION
            let audio_only = v.codecs.as_deref().is_some_and(|c| {
                codec_tokens(c)
                    .iter()
                    .all(|t| video_codec_family(t).is_none())
            });
            if !audio_only {
                issues.push(author_error(
                    "9.2",
                    format!("video STREAM-INF missing RESOLUTION on '{}'", v.uri),
                ));
            }
        }
        if v.is_iframe && v.resolution.is_none() {
            issues.push(author_error(
                "9.3",
                format!("I-FRAME-STREAM-INF missing RESOLUTION on '{}'", v.uri),
            ));
        }
    }

    // §9.5–9.8 — demuxed audio expectations (soft)
    let has_mc_audio = master.media_renditions.iter().any(|r| {
        r.media_type == "AUDIO"
            && r.channels
                .as_deref()
                .is_some_and(|c| c.split('/').next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(0) > 2)
    });
    if has_mc_audio {
        let demuxed = master
            .media_renditions
            .iter()
            .any(|r| r.media_type == "AUDIO" && r.uri.is_some());
        if !demuxed {
            issues.push(author_warn(
                "9.5",
                "multichannel audio SHOULD be demuxed via EXT-X-MEDIA URI",
            ));
        }
    }

    // §9.9–9.10 — multiple bitrates; HD ≥2 at max resolution
    if variants.len() < 2 {
        issues.push(author_warn(
            "9.9",
            "fewer than 2 video bitrates in the multivariant playlist",
        ));
    }
    {
        let max_h = variants
            .iter()
            .filter_map(|v| v.resolution.as_deref().and_then(parse_resolution))
            .map(|(_, h)| h)
            .max()
            .unwrap_or(0);
        if max_h >= 720 {
            let at_max = variants
                .iter()
                .filter(|v| {
                    v.resolution
                        .as_deref()
                        .and_then(parse_resolution)
                        .is_some_and(|(_, h)| h == max_h)
                })
                .count();
            if at_max < 2 {
                issues.push(author_warn(
                    "9.10",
                    format!("HD max resolution has only {at_max} variant(s); recommend ≥2"),
                ));
            }
        }
    }

    // §9.11–9.12 — INDEPENDENT-SEGMENTS on master or all video media
    if !master.independent_segments {
        let videos: Vec<_> = ctx.video_playlists().collect();
        if !videos.is_empty() && videos.iter().any(|p| !p.independent_segments) {
            issues.push(author_warn(
                "9.11",
                "INDEPENDENT-SEGMENTS missing on master and some video media playlists",
            ));
        }
    }

    // §9.13–9.14 — AVERAGE-BANDWIDTH MUST; BANDWIDTH sanity
    for v in &variants {
        if v.average_bandwidth.is_none() {
            issues.push(author_error(
                "9.14",
                format!(
                    "STREAM-INF missing AVERAGE-BANDWIDTH on variant '{}'",
                    v.uri
                ),
            ));
        }
        if let (Some(bw), Some(avg)) = (v.bandwidth, v.average_bandwidth) {
            if avg > bw {
                issues.push(author_warn(
                    "9.13",
                    format!(
                        "AVERAGE-BANDWIDTH ({avg}) > BANDWIDTH ({bw}) on '{}'",
                        v.uri
                    ),
                ));
            }
        }
    }

    // §9.15–9.16 — FRAME-RATE; VIDEO-RANGE unless all SDR
    for v in &variants {
        let audio_only = v.codecs.as_deref().is_some_and(|c| {
            codec_tokens(c)
                .iter()
                .all(|t| video_codec_family(t).is_none())
        });
        if !audio_only && v.frame_rate.is_none() {
            issues.push(author_warn(
                "9.15",
                format!("video STREAM-INF missing FRAME-RATE on '{}'", v.uri),
            ));
        }
    }
    let any_hdr = variants
        .iter()
        .any(|v| is_hdr_range(v.video_range.as_deref()));
    let any_missing_vr = variants.iter().any(|v| v.video_range.is_none());
    if any_hdr && any_missing_vr {
        issues.push(author_error(
            "9.16",
            "VIDEO-RANGE required on all variants when any HDR is present",
        ));
    }

    // §9.17 — LANGUAGE ordered general→specific within group
    {
        use std::collections::HashMap;
        let mut by_group: HashMap<&str, Vec<&str>> = HashMap::new();
        for r in &master.media_renditions {
            if let Some(lang) = &r.language {
                by_group.entry(&r.group_id).or_default().push(lang.as_str());
            }
        }
        for (gid, langs) in by_group {
            // soft: if both "en" and "en-US", "en" should appear first
            let mut seen_specific = false;
            for lang in langs {
                if lang.contains('-') {
                    seen_specific = true;
                } else if seen_specific {
                    issues.push(author_warn(
                        "9.17",
                        format!(
                            "LANGUAGE order in group '{gid}' should list general tags before specific"
                        ),
                    ));
                    break;
                }
            }
        }
    }

    // §9.18–9.20 — PATHWAY-ID; SCORE all or none; APAC profile/level
    {
        let with_pathway = variants.iter().filter(|v| v.pathway_id.is_some()).count();
        if with_pathway > 0 && with_pathway != variants.len() {
            issues.push(author_warn(
                "9.18",
                "PATHWAY-ID present on some but not all variants",
            ));
        }
        let with_score = variants.iter().filter(|v| v.score.is_some()).count();
        if with_score > 0 && with_score != variants.len() {
            issues.push(author_error(
                "9.19",
                "SCORE MUST be present on all variants or none",
            ));
        }
        for v in &variants {
            if let Some(codecs) = &v.codecs {
                for tok in codec_tokens(codecs) {
                    if is_apac(tok) && !tok.contains('.') {
                        issues.push(author_warn(
                            "9.20",
                            format!("APAC CODECS '{tok}' SHOULD include profile/level"),
                        ));
                    }
                }
            }
        }
    }

    // iOS §9.21–9.22
    if ctx.policy.require_192k_variant {
        let has_low = variants.iter().any(|v| {
            v.bandwidth.unwrap_or(u64::MAX) <= 192_000
                || v.average_bandwidth.unwrap_or(u64::MAX) <= 192_000
        });
        if !has_low {
            issues.push(author_warn(
                "9.21",
                "iOS: no variant at ≤192 kbps",
            ));
        }
    }

    // tvOS: no audio-only variants
    if ctx.policy.forbid_audio_only_variants {
        for v in &variants {
            let audio_only = v.resolution.is_none()
                && v.codecs.as_deref().is_some_and(|c| {
                    codec_tokens(c)
                        .iter()
                        .all(|t| video_codec_family(t).is_none())
                });
            if audio_only {
                issues.push(author_error(
                    "9.20",
                    format!("tvOS: audio-only variant '{}' is not allowed", v.uri),
                ));
            }
        }
    }

    // AirPlay §9.23–9.24
    if ctx.policy.require_full_codec_fps_ladders {
        use std::collections::HashSet;
        let mut keys = HashSet::new();
        for v in &variants {
            let fam = v
                .codecs
                .as_deref()
                .and_then(|c| codec_tokens(c).into_iter().find_map(video_codec_family))
                .unwrap_or("?");
            let fps = v.frame_rate.map(|f| format!("{f:.2}")).unwrap_or_default();
            keys.insert(format!("{fam}@{fps}"));
        }
        // Each codec/fps combo should have multiple bitrates — check counts
        use std::collections::HashMap;
        let mut counts: HashMap<String, usize> = HashMap::new();
        for v in &variants {
            let fam = v
                .codecs
                .as_deref()
                .and_then(|c| codec_tokens(c).into_iter().find_map(video_codec_family))
                .unwrap_or("?");
            let fps = v.frame_rate.map(|f| format!("{f:.2}")).unwrap_or_default();
            *counts.entry(format!("{fam}@{fps}")).or_default() += 1;
        }
        for (k, n) in counts {
            if n < 2 {
                issues.push(author_warn(
                    "9.23",
                    format!("AirPlay2: codec/fps ladder '{k}' has only {n} bitrate(s)"),
                ));
            }
        }
        let has_hdr = variants.iter().any(|v| is_hdr_range(v.video_range.as_deref()));
        if has_hdr {
            let has_dv = variants.iter().any(|v| {
                v.codecs.as_deref().is_some_and(|c| {
                    codec_tokens(c)
                        .iter()
                        .any(|t| video_codec_family(t) == Some("dv"))
                })
            });
            let has_hdr10 = variants.iter().any(|v| {
                v.video_range.as_deref().is_some_and(|r| r.eq_ignore_ascii_case("PQ"))
                    && v.codecs.as_deref().is_some_and(|c| {
                        codec_tokens(c)
                            .iter()
                            .any(|t| video_codec_family(t) == Some("hevc"))
                    })
            });
            if has_dv && !has_hdr10 {
                issues.push(author_warn(
                    "9.24",
                    "AirPlay2: Dolby Vision present; HDR10 (PQ HEVC) SHOULD also be offered",
                ));
            }
        }
    }

    issues
}
