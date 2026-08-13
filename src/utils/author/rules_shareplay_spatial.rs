//! Apple Authoring Spec §15–16 — SharePlay / Spatial.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        return issues;
    };

    // §15.1–15.3 — interstitial duration consistency across tags (best-effort)
    for pl in ctx.playlists {
        let mut planned: Vec<f64> = Vec::new();
        let mut limits: Vec<f64> = Vec::new();
        for line in pl.raw_content.lines() {
            let line = line.trim();
            if !line.starts_with("#EXT-X-DATERANGE:") {
                continue;
            }
            let attrs = crate::utils::validator::parser::parse_attributes(
                line.strip_prefix("#EXT-X-DATERANGE:").unwrap_or(""),
            );
            if attrs.get("CLASS").map(|s| s.as_str()) != Some("com.apple.hls.interstitial") {
                continue;
            }
            if let Some(p) = attrs.get("PLANNED-DURATION").and_then(|v| v.parse().ok()) {
                planned.push(p);
            }
            if let Some(p) = attrs.get("X-PLAYOUT-LIMIT").and_then(|v| v.parse().ok()) {
                limits.push(p);
            }
        }
        if planned.len() >= 2 {
            let first = planned[0];
            if planned.iter().any(|p| (p - first).abs() > 0.5) {
                issues.push(author_warn(
                    "15.2",
                    format!(
                        "interstitial PLANNED-DURATION values differ on '{}'",
                        pl.name
                    ),
                ));
            }
        }
        let _ = limits;
    }

    // §16.1 — REQ-VIDEO-LAYOUT on spatial variants
    for v in master.variants.iter().filter(|v| !v.is_iframe) {
        let spatial_codec = v.codecs.as_deref().is_some_and(|c| {
            let l = c.to_ascii_lowercase();
            l.contains("hvc1") && (l.contains("mv") || v.req_video_layout.is_some())
        });
        if v.req_video_layout
            .as_deref()
            .is_some_and(|l| l.to_ascii_lowercase().contains("stereo"))
            || spatial_codec
        {
            if v.req_video_layout.is_none() {
                issues.push(author_error(
                    "16.1",
                    format!("spatial variant '{}' MUST include REQ-VIDEO-LAYOUT", v.uri),
                ));
            }
        }
    }

    // §16.3 — discontinuity or dual sample-desc when switching layouts
    // Playlist-level: if multiple layouts exist, discontinuities SHOULD appear when switching
    {
        let layouts: Vec<_> = master
            .variants
            .iter()
            .filter_map(|v| v.req_video_layout.as_deref())
            .collect();
        if layouts.len() >= 2 {
            let unique: std::collections::HashSet<_> = layouts.iter().copied().collect();
            if unique.len() >= 2 {
                // Check media playlists for discontinuities
                let any_disc = ctx.playlists.iter().any(|p| p.segments.iter().any(|s| s.discontinuity));
                if !any_disc {
                    issues.push(author_warn(
                        "16.3",
                        "multiple REQ-VIDEO-LAYOUT values without discontinuities (or dual sample descriptions)",
                    ));
                }
            }
        }
    }

    // §16.5 — CH-MONO when mono sections declared
    for v in &master.variants {
        if let Some(layout) = &v.req_video_layout {
            let l = layout.to_ascii_lowercase();
            if l.contains("mono") && !l.contains("ch-mono") && l.contains("stereo") {
                issues.push(author_warn(
                    "16.5",
                    format!(
                        "spatial layout on '{}' mentions mono sections; ensure CH-MONO signaling",
                        v.uri
                    ),
                ));
            }
        }
    }

    // Phase B: vexu presence for spatial
    let expects_spatial = master.variants.iter().any(|v| {
        v.req_video_layout
            .as_deref()
            .is_some_and(|l| l.to_ascii_lowercase().contains("stereo"))
    });
    if expects_spatial {
        let any_vexu = ctx.init_probes.iter().any(|e| e.probe.has_vexu);
        if !ctx.init_probes.is_empty() && !any_vexu {
            issues.push(author_warn(
                "16.2",
                "stereo spatial variants present but no vexu box found in init probes",
            ));
        }
    }

    // visionOS 16.6–16.7
    if ctx.policy.profile == super::profile::AuthorProfile::VisionOs && expects_spatial {
        issues.push(author_info(
            "16.6",
            "visionOS spatial playback: verify parallax/eye metadata in media (vexu children)",
        ));
    }

    issues
}
