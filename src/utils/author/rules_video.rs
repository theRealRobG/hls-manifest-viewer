//! Apple Authoring Spec §1 — Video (playlist-visible + init/deep hooks).

use super::context::AuthoringContext;
use super::helpers::*;
use super::severity::{must, should};
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        return issues;
    };

    let variants: Vec<_> = master.variants.iter().filter(|v| !v.is_iframe).collect();

    // §1.1 — video CODECS tokens must be AVC/HEVC/DV/AV1 (mjpg allowed for I-frame)
    for v in &variants {
        if let Some(codecs) = &v.codecs {
            for tok in codec_tokens(codecs) {
                // Skip audio tokens in demuxed CODECS lists
                if tok.to_ascii_lowercase().starts_with("mp4a")
                    || is_ac3(tok)
                    || is_ec3(tok)
                    || is_apac(tok)
                    || tok.eq_ignore_ascii_case("ac-4")
                {
                    continue;
                }
                if video_codec_family(tok).is_none() {
                    issues.push(author_issue(
                        must(),
                        "1.1",
                        format!(
                            "unsupported video CODECS token '{tok}' on variant '{}'",
                            v.uri
                        ),
                    ));
                }
            }
        }
    }

    // §1.10 — prefer avc1/hvc1/dvh1 over avc3/hev1/dvhe
    for v in &variants {
        if let Some(codecs) = &v.codecs {
            for tok in codec_tokens(codecs) {
                if prefers_parameter_sets_in_sample_entry(tok) {
                    issues.push(author_warn(
                        "1.10",
                        format!(
                            "prefer in-sample-entry parameter sets (avc1/hvc1/dvh1) over '{tok}' on '{}'",
                            v.uri
                        ),
                    ));
                }
            }
        }
    }

    // §1.12 — if HEVC/DV/AV1 present, H.264 SHOULD also exist (*)
    if !ctx.policy.is_exempt("1.12") {
        let has_advanced = variants.iter().any(|v| {
            v.codecs.as_deref().is_some_and(|c| {
                codec_tokens(c).iter().any(|t| {
                    matches!(video_codec_family(t), Some("hevc" | "dv" | "av1"))
                })
            })
        });
        let has_avc = variants.iter().any(|v| {
            v.codecs
                .as_deref()
                .is_some_and(|c| codec_tokens(c).iter().any(|t| video_codec_family(t) == Some("avc")))
        });
        if has_advanced && !has_avc {
            issues.push(author_warn(
                "1.12",
                "HEVC/DV/AV1 present but no H.264 (avc1/avc3) variant for compatibility",
            ));
        }
    }

    // §1.18 / §1.19 — FRAME-RATE
    let is_vod = ctx
        .video_playlists()
        .any(|p| p.has_endlist || p.playlist_type.as_deref() == Some("VOD"));
    for v in &variants {
        if let Some(fps) = v.frame_rate {
            if fps > 60.0 {
                issues.push(author_error(
                    "1.19",
                    format!("FRAME-RATE {fps} exceeds 60 on '{}'", v.uri),
                ));
            } else if is_vod && !is_recommended_vod_framerate(fps) {
                issues.push(author_warn(
                    "1.18",
                    format!(
                        "VOD FRAME-RATE {fps} is outside recommended set on '{}'",
                        v.uri
                    ),
                ));
            }
        }
    }

    // §1.20 — HDR variants SHOULD/MUST include some HDR ≤30 fps
    let hdr_vars: Vec<_> = variants
        .iter()
        .filter(|v| is_hdr_range(v.video_range.as_deref()))
        .collect();
    if !hdr_vars.is_empty() {
        let has_le_30 = hdr_vars
            .iter()
            .any(|v| v.frame_rate.is_some_and(|f| f <= 30.0 + 0.02));
        if !has_le_30 {
            let sev = if ctx.policy.hdr_30fps_must {
                must()
            } else {
                should()
            };
            issues.push(author_issue(
                sev,
                "1.20",
                "HDR present but no HDR variant at ≤30 fps",
            ));
        }
    }

    // §1.23 — overlapping bandwidth ladders per codec family
    {
        use std::collections::HashMap;
        let mut by_family: HashMap<&str, Vec<u64>> = HashMap::new();
        for v in &variants {
            let Some(codecs) = &v.codecs else { continue };
            let Some(bw) = v.bandwidth else { continue };
            for tok in codec_tokens(codecs) {
                if let Some(fam) = video_codec_family(tok) {
                    if fam == "mjpg" {
                        continue;
                    }
                    by_family.entry(fam).or_default().push(bw);
                }
            }
        }
        let families: Vec<_> = by_family.keys().copied().collect();
        if families.len() >= 2 {
            // Check pairwise range overlap
            for i in 0..families.len() {
                for j in (i + 1)..families.len() {
                    let a = &by_family[families[i]];
                    let b = &by_family[families[j]];
                    let a_min = *a.iter().min().unwrap();
                    let a_max = *a.iter().max().unwrap();
                    let b_min = *b.iter().min().unwrap();
                    let b_max = *b.iter().max().unwrap();
                    let overlap = a_min <= b_max && b_min <= a_max;
                    if !overlap {
                        let sev = if ctx.policy.overlapping_ladder_must {
                            must()
                        } else {
                            should()
                        };
                        issues.push(author_issue(
                            sev,
                            "1.23",
                            format!(
                                "codec families '{}' and '{}' lack overlapping BANDWIDTH ladders",
                                families[i], families[j]
                            ),
                        ));
                    }
                }
            }
        }
    }

    // §1.24 — if HDR/HEVC present, SDR variants MUST exist (*)
    if !ctx.policy.is_exempt("1.24") {
        let has_hdr_or_hevc = variants.iter().any(|v| {
            is_hdr_range(v.video_range.as_deref())
                || v.codecs.as_deref().is_some_and(|c| {
                    codec_tokens(c)
                        .iter()
                        .any(|t| matches!(video_codec_family(t), Some("hevc" | "dv")))
                })
        });
        let has_sdr = variants.iter().any(|v| {
            !is_hdr_range(v.video_range.as_deref())
                && v.codecs.as_deref().is_some_and(|c| {
                    codec_tokens(c)
                        .iter()
                        .any(|t| video_codec_family(t) == Some("avc"))
                })
        });
        // Also accept explicit SDR VIDEO-RANGE or missing VIDEO-RANGE with AVC
        let has_sdr = has_sdr
            || variants.iter().any(|v| {
                matches!(
                    v.video_range.as_deref().map(|s| s.to_ascii_uppercase()).as_deref(),
                    Some("SDR") | None
                ) && v.codecs.as_deref().is_some_and(|c| {
                    codec_tokens(c)
                        .iter()
                        .any(|t| video_codec_family(t) == Some("avc"))
                })
            });
        if has_hdr_or_hevc && !has_sdr {
            issues.push(author_error(
                "1.24",
                "HDR/HEVC present but no SDR H.264 compatibility variants",
            ));
        }
    }

    // §1.32 — first compatible default near ~2000 kbps AVERAGE-BANDWIDTH
    if !variants.is_empty() {
        let target = if ctx.policy.profile == super::profile::AuthorProfile::Ios {
            // iOS 1.32a/b: Wi-Fi ~2000, cellular lower — warn if none near 2000 or 800
            2_000_000u64
        } else {
            2_000_000u64
        };
        let near = variants.iter().any(|v| {
            let bw = v.average_bandwidth.or(v.bandwidth).unwrap_or(0);
            bw > 0 && bw.abs_diff(target) <= 800_000
        });
        if !near {
            issues.push(author_warn(
                "1.32",
                "no variant near ~2000 kbps AVERAGE-BANDWIDTH for initial selection",
            ));
        }
        if ctx.policy.profile == super::profile::AuthorProfile::Ios {
            let cellular = variants.iter().any(|v| {
                let bw = v.average_bandwidth.or(v.bandwidth).unwrap_or(0);
                (400_000..=1_200_000).contains(&bw)
            });
            if !cellular {
                issues.push(author_warn(
                    "1.32",
                    "iOS: no lower-bitrate (~800 kbps) variant for cellular initial selection",
                ));
            }
        }
    }

    // §1.33 — same aspect ratio
    {
        let ratios: Vec<f64> = variants
            .iter()
            .filter_map(|v| v.resolution.as_deref().and_then(aspect_ratio))
            .collect();
        if ratios.len() >= 2 {
            let first = ratios[0];
            if ratios.iter().any(|r| (r - first).abs() > 0.02) {
                issues.push(author_warn(
                    "1.33",
                    "video variants declare inconsistent aspect ratios from RESOLUTION",
                ));
            }
        }
    }

    // §1.34 — if UHD present, some UHD ≤15 Mbps
    {
        let uhd: Vec<_> = variants
            .iter()
            .filter(|v| {
                v.resolution
                    .as_deref()
                    .and_then(parse_resolution)
                    .is_some_and(|(w, h)| w >= 3840 || h >= 2160)
            })
            .collect();
        if !uhd.is_empty() {
            let ok = uhd.iter().any(|v| v.bandwidth.unwrap_or(u64::MAX) <= 15_000_000);
            if !ok {
                issues.push(author_warn(
                    "1.34",
                    "UHD present but no UHD variant at ≤15 Mbps",
                ));
            }
        }
    }

    // §1.36 — MV-HEVC indicators only with stereo REQ-VIDEO-LAYOUT
    for v in &variants {
        let codecs = v.codecs.as_deref().unwrap_or("");
        let looks_mv = codecs.to_ascii_lowercase().contains("hvc1")
            && (codecs.contains("mv")
                || v.req_video_layout
                    .as_deref()
                    .is_some_and(|l| l.to_ascii_lowercase().contains("stereo")));
        // If CODECS mentions layered/MV style without layout — soft check via layout presence
        if let Some(layout) = &v.req_video_layout {
            if layout.to_ascii_lowercase().contains("stereo") {
                let hevc = codec_tokens(codecs)
                    .iter()
                    .any(|t| matches!(video_codec_family(t), Some("hevc" | "dv")));
                if !hevc {
                    issues.push(author_warn(
                        "1.36",
                        format!(
                            "REQ-VIDEO-LAYOUT stereo on '{}' without HEVC/DV CODECS",
                            v.uri
                        ),
                    ));
                }
            }
        }
        let _ = looks_mv;
    }

    // §1.37 — AV1 level ≤ 6.2
    for v in &variants {
        if let Some(codecs) = &v.codecs {
            for tok in codec_tokens(codecs) {
                if video_codec_family(tok) == Some("av1") && !av1_level_ok(tok) {
                    issues.push(author_error(
                        "1.37",
                        format!("AV1 CODECS '{tok}' exceeds level 6.2 on '{}'", v.uri),
                    ));
                }
            }
        }
    }

    // §1.38 — APMP mono → CODECS HEVC
    for v in &variants {
        if let Some(layout) = &v.req_video_layout {
            let l = layout.to_ascii_lowercase();
            if (l.contains("proj-equi") || l.contains("proj-rect")) && !l.contains("stereo") {
                let hevc = v.codecs.as_deref().is_some_and(|c| {
                    codec_tokens(c)
                        .iter()
                        .any(|t| video_codec_family(t) == Some("hevc"))
                });
                if !hevc {
                    issues.push(author_error(
                        "1.38",
                        format!(
                            "APMP mono projection on '{}' requires HEVC CODECS",
                            v.uri
                        ),
                    ));
                }
            }
        }
    }

    // Phase B: reinforce sample-entry preference / brands from init probes
    for entry in ctx.init_probes {
        if let Some(fourcc) = &entry.probe.video_sample_fourcc {
            if prefers_parameter_sets_in_sample_entry(fourcc) {
                issues.push(author_warn(
                    "1.10",
                    format!(
                        "init '{}' uses sample entry '{fourcc}' (prefer avc1/hvc1/dvh1)",
                        entry.uri
                    ),
                ));
            }
            if fourcc.to_ascii_lowercase().starts_with("avc") {
                if let (Some(max), Some(level)) =
                    (ctx.policy.h264_max_level, entry.probe.video_level.as_deref())
                {
                    // Soft compare numeric level strings (e.g. "5.1" vs "51")
                    let parse_lvl = |s: &str| -> Option<f64> {
                        if let Ok(v) = s.parse::<f64>() {
                            if v > 10.0 {
                                Some(v / 10.0)
                            } else {
                                Some(v)
                            }
                        } else {
                            None
                        }
                    };
                    if let (Some(have), Some(limit)) = (parse_lvl(level), parse_lvl(max)) {
                        if have > limit + 0.01 {
                            issues.push(author_warn(
                                "1.3",
                                format!(
                                    "H.264 level {level} in init '{}' exceeds {} profile max {max}",
                                    entry.uri,
                                    ctx.policy.profile.as_str()
                                ),
                            ));
                        }
                    }
                }
            }
        }
        if entry.probe.major_brand.is_none() && entry.probe.video_sample_fourcc.is_none() {
            issues.push(author_warn(
                "1.2",
                format!("could not parse ftyp/sample entry from init '{}'", entry.uri),
            ));
        }
    }

    // visionOS +1.40: when profile is visionOS and stereo, ensure REQ-VIDEO-LAYOUT
    if ctx.policy.profile == super::profile::AuthorProfile::VisionOs {
        for v in &variants {
            if v.req_video_layout.is_none() {
                issues.push(author_warn(
                    "1.40",
                    format!("visionOS: missing REQ-VIDEO-LAYOUT on '{}'", v.uri),
                ));
            }
        }
    }

    // AirPlay +1.41 — CENC pattern checks deferred to protection/init
    issues
}
