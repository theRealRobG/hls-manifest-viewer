//! Apple Authoring Spec §1 — Video (playlist-visible + init/deep hooks).

use super::context::{AuthoringContext, InitProbeEntry};
use super::helpers::*;
use super::severity::{must, should};
use crate::utils::validator::types::{Issue, MasterRendition};

/// §1.3b fallback when no platform profile pins an H.264 level ceiling.
const DEFAULT_H264_MAX_LEVEL: &str = "5.2";
/// §1.6b HEVC Main 10 ceiling.
const HEVC_MAX_LEVEL: f64 = 5.1;
/// HEVC `general_profile_idc` for Main 10.
const HEVC_MAIN10_PROFILE_IDC: u32 = 2;
/// §1.32 AVERAGE-BANDWIDTH the default variant should sit near.
const DEFAULT_VARIANT_TARGET_BPS: u64 = 2_000_000;
/// How far from the §1.32 target a default variant may sit before it is reported.
const DEFAULT_VARIANT_TOLERANCE_BPS: u64 = 800_000;
/// §1.32 iOS cellular start: the spec pins no figure below the ~2000 kbps Wi-Fi
/// target, so this only asks the ladder to carry a rung around ~800 kbps.
const IOS_CELLULAR_BPS: std::ops::RangeInclusive<u64> = 400_000..=1_200_000;
/// §1.11 stand-in when a variant omits FRAME-RATE. §1.19 caps FRAME-RATE at 60, so
/// assuming the maximum keeps the required level an upper bound.
const ASSUMED_MAX_FRAME_RATE: f64 = 60.0;

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
                issues.push(author_conflict_aware_issue(
                    &ctx.policy,
                    must(),
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

    // §1.20 — HDR variants SHOULD/MUST include some HDR ≤30 fps. A variant that omits
    // FRAME-RATE states no rate at all, so it can neither satisfy nor fail this rule;
    // §9.15 reports the missing attribute on its own.
    {
        let hdr_frame_rates: Vec<f64> = variants
            .iter()
            .filter(|v| is_hdr_range(v.video_range.as_deref()))
            .filter_map(|v| v.frame_rate)
            .collect();
        let has_le_30 = hdr_frame_rates.iter().any(|f| *f <= 30.0 + 0.02);
        if !hdr_frame_rates.is_empty() && !has_le_30 {
            let sev = if ctx.policy.hdr_30fps_must {
                must()
            } else {
                should()
            };
            issues.push(author_conflict_aware_issue(
                &ctx.policy,
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
        // Any codec may carry the SDR ladder; §1.12 covers the H.264 preference.
        let has_sdr = variants.iter().any(|v| {
            matches!(
                v.video_range.as_deref().map(|s| s.to_ascii_uppercase()).as_deref(),
                Some("SDR") | None
            )
        });
        if has_hdr_or_hevc && !has_sdr {
            issues.push(author_error(
                "1.24",
                "HDR/HEVC present but no SDR compatibility variants",
            ));
        }
    }

    // §1.32 — the default variant SHOULD be near ~2000 kbps AVERAGE-BANDWIDTH. A client
    // with no bandwidth history starts on the first variant it can decode, so the subject
    // is that variant rather than whether the ladder happens to contain such a rung.
    for (family, v) in default_variants(&variants) {
        let Some(bw) = v.average_bandwidth.or(v.bandwidth).filter(|b| *b > 0) else {
            continue;
        };
        if bw.abs_diff(DEFAULT_VARIANT_TARGET_BPS) > DEFAULT_VARIANT_TOLERANCE_BPS {
            issues.push(author_conflict_aware_issue(
                &ctx.policy,
                should(),
                "1.32",
                format!(
                    "default {family} variant '{}' declares {} kbps; the first variant a client can play should be near ~{} kbps AVERAGE-BANDWIDTH",
                    v.uri,
                    bw / 1000,
                    DEFAULT_VARIANT_TARGET_BPS / 1000
                ),
            ));
        }
    }
    if ctx.policy.profile == super::profile::AuthorProfile::Ios && !variants.is_empty() {
        // iOS starts lower on cellular. The spec names no cellular figure, so this stays
        // an existence check on a low rung and leaves the ~2000 kbps target to Wi-Fi.
        let cellular = variants.iter().any(|v| {
            v.average_bandwidth
                .or(v.bandwidth)
                .is_some_and(|bw| IOS_CELLULAR_BPS.contains(&bw))
        });
        if !cellular {
            issues.push(author_conflict_aware_issue(
                &ctx.policy,
                should(),
                "1.32",
                "iOS: no lower-bitrate (~800 kbps) variant to start from on cellular",
            ));
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
                issues.push(author_conflict_aware_issue(
                    &ctx.policy,
                    should(),
                    "1.34",
                    "UHD present but no UHD variant at ≤15 Mbps",
                ));
            }
        }
    }

    // §1.36 — MV-HEVC MUST NOT be used for anything other than stereoscopic video, so a
    // variant that signals MV-HEVC without a stereo REQ-VIDEO-LAYOUT is the violation.
    for v in &variants {
        let init_is_mv = inits_for_variant(ctx, &v.uri)
            .iter()
            .any(|e| e.probe.has_lhvc);
        let codecs_are_mv = codecs_look_mv_hevc(v.codecs.as_deref().unwrap_or(""));
        if (!init_is_mv && !codecs_are_mv) || layout_is_stereo(v.req_video_layout.as_deref()) {
            continue;
        }
        let source = if init_is_mv {
            "an lhvC layered-HEVC configuration in its init"
        } else {
            "its CODECS"
        };
        let layout = match v.req_video_layout.as_deref() {
            Some(l) => format!("REQ-VIDEO-LAYOUT is '{l}'"),
            None => "REQ-VIDEO-LAYOUT is absent".to_string(),
        };
        issues.push(author_error(
            "1.36",
            format!(
                "'{}' signals MV-HEVC via {source} but {layout}; MV-HEVC must carry stereoscopic video only",
                v.uri
            ),
        ));
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

    // §1.9 — Dolby Vision MUST be Profile 5 (single-layer 10-bit HEVC) at Level ≤ 7
    for v in &variants {
        let Some(codecs) = &v.codecs else { continue };
        for tok in codec_tokens(codecs) {
            if video_codec_family(tok) != Some("dv") {
                continue;
            }
            let Some(dv) = parse_dv_codec(tok) else {
                issues.push(author_warn(
                    "1.9",
                    format!(
                        "Dolby Vision CODECS '{tok}' on '{}' omits the profile/level suffix",
                        v.uri
                    ),
                ));
                continue;
            };
            if dv.profile != 5 {
                issues.push(author_issue(
                    must(),
                    "1.9",
                    format!(
                        "Dolby Vision CODECS '{tok}' on '{}' is profile {}; only profile 5 is supported",
                        v.uri, dv.profile
                    ),
                ));
            }
            if dv.level > 7 {
                issues.push(author_issue(
                    must(),
                    "1.9",
                    format!(
                        "Dolby Vision CODECS '{tok}' on '{}' is level {}, above the maximum level 7",
                        v.uri, dv.level
                    ),
                ));
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

    // Profiles and levels already evaluated from init probes. The CODECS-derived
    // checks further down skip these so an fMP4 variant is only reported once.
    let mut init_avc_levels: Vec<f64> = Vec::new();
    let mut init_avc_profiles: Vec<u16> = Vec::new();
    let mut init_hevc_levels: Vec<f64> = Vec::new();
    let mut init_hevc_profiles: Vec<u32> = Vec::new();

    // Phase B: container / profile / level / HDR metadata from init probes
    for entry in ctx.init_probes {
        let probe = &entry.probe;
        let fourcc = probe.video_sample_fourcc.as_deref().unwrap_or("");
        let fourcc_l = fourcc.to_ascii_lowercase();
        let is_avc = fourcc_l.starts_with("avc");
        let is_hevc = fourcc_l.starts_with("hvc") || fourcc_l.starts_with("hev");
        let is_dv = fourcc_l.starts_with("dvh") || fourcc_l.starts_with("dvhe");
        let is_av1 = fourcc_l.starts_with("av01");

        if prefers_parameter_sets_in_sample_entry(fourcc) {
            issues.push(author_warn(
                "1.10",
                format!(
                    "init '{}' uses sample entry '{fourcc}' (prefer avc1/hvc1/dvh1)",
                    entry.uri
                ),
            ));
        }

        // §1.5 / §1.39 — the container itself, which these rules require to be fMP4. The
        // spec names no brand, so an init that parses as fMP4 satisfies them however its
        // packager spelled `ftyp`; the brands it declares are reported as probe notes.
        if (is_hevc || is_dv) && !probe.looks_like_fmp4_init() {
            issues.push(author_error(
                "1.5",
                format!("HEVC/DV init '{}' could not be parsed as fMP4", entry.uri),
            ));
        }
        if is_av1 && !probe.looks_like_fmp4_init() {
            issues.push(author_error(
                "1.39",
                format!("AV1 init '{}' could not be parsed as fMP4", entry.uri),
            ));
        }

        // §1.3 / 1.4 / 1.6 profile+level from avcC/hvcC
        if is_avc {
            if let Some(profile) = probe.video_profile.as_deref() {
                if let Ok(idc) = profile.trim().parse::<u16>() {
                    init_avc_profiles.push(idc);
                }
                // avcC carries a numeric profile_idc; older probes may carry a name.
                let name = h264_profile_name(profile)
                    .map(str::to_string)
                    .unwrap_or_else(|| profile.to_ascii_lowercase());
                if name.contains("baseline") || name.contains("main") || name.contains("extended")
                {
                    issues.push(author_warn(
                        "1.4",
                        format!(
                            "H.264 profile '{name}' in init '{}'; High Profile is preferred",
                            entry.uri
                        ),
                    ));
                }
            }
            if let Some(level) = probe.video_level.as_deref() {
                let max = ctx.policy.h264_max_level.unwrap_or(DEFAULT_H264_MAX_LEVEL);
                if let (Some(have), Some(limit)) =
                    (parse_codec_level(level), parse_codec_level(max))
                {
                    init_avc_levels.push(have);
                    if have > limit + 0.01 {
                        issues.push(author_error(
                            "1.3b",
                            format!(
                                "H.264 level {level} in init '{}' exceeds {} max {max}",
                                entry.uri,
                                ctx.policy.profile.as_str()
                            ),
                        ));
                    }
                }
            }
        }

        if is_hevc {
            // HEVC general_level_idc is typically level × 30 (e.g. 153 → 5.1).
            if let Some(raw) = probe.video_level.as_deref().and_then(|s| s.parse::<f64>().ok()) {
                let hevc_level = hevc_level_from_idc(raw);
                init_hevc_levels.push(hevc_level);
                if hevc_level > HEVC_MAX_LEVEL + 0.01 && !ctx.policy.is_exempt("1.6b") {
                    issues.push(hevc_level_issue(
                        ctx,
                        hevc_level,
                        format!("HEVC level {hevc_level} in init '{}'", entry.uri),
                    ));
                }
            }
            // general_profile_idc 2 is Main 10, which §1.6 requires.
            if let Some(idc) = probe
                .video_profile
                .as_deref()
                .and_then(|s| s.trim().parse::<u32>().ok())
            {
                init_hevc_profiles.push(idc);
                if idc != HEVC_MAIN10_PROFILE_IDC && !ctx.policy.is_exempt("1.6b") {
                    issues.push(author_conflict_aware_issue(
                        &ctx.policy,
                        must(),
                        "1.6b",
                        format!(
                            "HEVC profile {} in init '{}' is not Main 10",
                            hevc_profile_name(idc),
                            entry.uri
                        ),
                    ));
                }
            }
        }

        // §1.8 / 1.35 — HDR10 static metadata presence
        let playlist_is_hdr = entry.playlist_names.iter().any(|name| {
            ctx.playlists.iter().any(|pl| {
                pl.name == *name && is_hdr_range(pl.video_range.as_deref())
            })
        }) || variants.iter().any(|v| {
            is_hdr_range(v.video_range.as_deref())
                && v.codecs.as_deref().is_some_and(|c| {
                    codec_tokens(c).iter().any(|t| {
                        matches!(video_codec_family(t), Some("hevc" | "dv" | "av1"))
                    })
                })
        });
        if playlist_is_hdr && (is_hevc || is_dv) {
            if !probe.has_mdcv && !probe.has_clli {
                issues.push(author_warn(
                    "1.35",
                    format!(
                        "HDR content init '{}' missing mdcv/clli static metadata boxes",
                        entry.uri
                    ),
                ));
            } else if probe.has_mdcv || probe.has_clli {
                // §1.8 — metadata present in init (hvcC-adjacent) is preferred over sample SEI
                issues.push(author_info(
                    "1.8",
                    format!(
                        "HDR static metadata present in init '{}' (mdcv={}, clli={})",
                        entry.uri, probe.has_mdcv, probe.has_clli
                    ),
                ));
            }
        }

        if probe.major_brand.is_none()
            && probe.video_sample_fourcc.is_none()
            && probe.audio_sample_fourcc.is_none()
        {
            issues.push(author_warn(
                "1.2",
                format!("could not parse ftyp/sample entry from init '{}'", entry.uri),
            ));
        }
    }

    // §1.3b / §1.4 / §1.6b — profile and level from CODECS. MPEG-TS variants carry no
    // EXT-X-MAP, so no init is ever probed for them; when an init did report the same
    // profile or level it stays the primary source and the token is skipped here.
    for v in &variants {
        let Some(codecs) = &v.codecs else { continue };
        for tok in codec_tokens(codecs) {
            match video_codec_family(tok) {
                Some("avc") => {
                    let Some(avc) = parse_avc_codec(tok) else { continue };
                    let max = ctx.policy.h264_max_level.unwrap_or(DEFAULT_H264_MAX_LEVEL);
                    let already_reported = init_avc_levels
                        .iter()
                        .any(|l| (l - avc.level).abs() < 0.01);
                    if let Some(limit) = parse_codec_level(max) {
                        if !already_reported && avc.level > limit + 0.01 {
                            issues.push(author_error(
                                "1.3b",
                                format!(
                                    "H.264 CODECS '{tok}' on '{}' is level {:.1}, above the {} max {max}",
                                    v.uri,
                                    avc.level,
                                    ctx.policy.profile.as_str()
                                ),
                            ));
                        }
                    }
                    if !init_avc_profiles.contains(&avc.profile_idc) {
                        if let Some(name) = h264_profile_name(&avc.profile_idc.to_string()) {
                            if matches!(name, "baseline" | "main" | "extended") {
                                issues.push(author_warn(
                                    "1.4",
                                    format!(
                                        "H.264 CODECS '{tok}' on '{}' is {name} profile; High Profile is preferred",
                                        v.uri
                                    ),
                                ));
                            }
                        }
                    }
                }
                Some("hevc") if !ctx.policy.is_exempt("1.6b") => {
                    let Some(hevc) = parse_hevc_codec(tok) else { continue };
                    if let Some(level) = hevc.level {
                        let already_reported =
                            init_hevc_levels.iter().any(|l| (l - level).abs() < 0.01);
                        if !already_reported && level > HEVC_MAX_LEVEL + 0.01 {
                            issues.push(hevc_level_issue(
                                ctx,
                                level,
                                format!("HEVC CODECS '{tok}' on '{}' is level {level:.1}", v.uri),
                            ));
                        }
                    }
                    if let Some(idc) = hevc.profile_idc {
                        if idc != HEVC_MAIN10_PROFILE_IDC && !init_hevc_profiles.contains(&idc) {
                            issues.push(author_conflict_aware_issue(
                                &ctx.policy,
                                must(),
                                "1.6b",
                                format!(
                                    "HEVC CODECS '{tok}' on '{}' is profile {}, not Main 10",
                                    v.uri,
                                    hevc_profile_name(idc)
                                ),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    issues.extend(codecs_vs_hvcc_issues(ctx, &variants));

    // §1.3a — at least one H.264 variant ≤ High@L4.1 when H.264 present
    if !ctx.policy.is_exempt("1.3a") {
        let h264_levels: Vec<f64> = ctx
            .init_probes
            .iter()
            .filter(|e| {
                e.probe
                    .video_sample_fourcc
                    .as_deref()
                    .is_some_and(|c| c.to_ascii_lowercase().starts_with("avc"))
            })
            .filter_map(|e| e.probe.video_level.as_deref().and_then(parse_codec_level))
            .collect();
        if !h264_levels.is_empty() && !h264_levels.iter().any(|l| *l <= 4.1 + 0.01) {
            issues.push(author_warn(
                "1.3a",
                "no H.264 init ≤ High Profile Level 4.1 for maximum compatibility",
            ));
        }
    }

    // §1.11 — level SHOULD NOT exceed the resolution/fps requirement. Levels are read
    // from the inits that this variant's own playlist maps: comparing every init against
    // every variant reports the ladder's top level against its lowest rung.
    for v in &variants {
        let Some((width, height)) = v.resolution.as_deref().and_then(parse_resolution) else {
            continue;
        };
        let fps = v.frame_rate.unwrap_or(ASSUMED_MAX_FRAME_RATE);
        let required = h264_level_required_for(width, height, fps);
        let mut levels: Vec<(String, f64)> = inits_for_variant(ctx, &v.uri)
            .iter()
            .filter(|e| {
                e.probe
                    .video_sample_fourcc
                    .as_deref()
                    .is_some_and(|c| c.to_ascii_lowercase().starts_with("avc"))
            })
            .filter_map(|e| {
                let level = e.probe.video_level.as_deref().and_then(parse_codec_level)?;
                Some((format!("init '{}'", e.uri), level))
            })
            .collect();
        if levels.is_empty() {
            // MPEG-TS variants have no EXT-X-MAP, so no init is ever probed for them and
            // CODECS is the only level available, as in the §1.3b CODECS path above.
            levels = codec_tokens(v.codecs.as_deref().unwrap_or(""))
                .iter()
                .filter_map(|tok| {
                    parse_avc_codec(tok).map(|avc| (format!("CODECS '{tok}'"), avc.level))
                })
                .collect();
        }
        for (source, level) in levels {
            if level > required + 0.15 {
                issues.push(author_warn(
                    "1.11",
                    format!(
                        "H.264 level {level} from {source} exceeds the ~{required} needed for {}@{fps} on '{}'",
                        v.resolution.as_deref().unwrap_or("?"),
                        v.uri
                    ),
                ));
            }
        }
    }

    // Cross-check playlist CODECS families vs container (TS without MAP for HEVC/AV1)
    for pl in ctx.video_playlists() {
        let codecs = pl.codecs.as_deref().unwrap_or("");
        let has_hevc_family = codec_tokens(codecs)
            .iter()
            .any(|t| matches!(video_codec_family(t), Some("hevc" | "dv" | "av1")));
        if has_hevc_family && playlist_looks_like_ts(pl) && !playlist_has_map(pl) {
            issues.push(author_error(
                "1.5",
                format!(
                    "'{}' declares HEVC/DV/AV1 but looks like MPEG-TS without EXT-X-MAP",
                    pl.name
                ),
            ));
        }
    }

    // A missing REQ-VIDEO-LAYOUT belongs to §16.1, which asks for the attribute on the
    // variants that carry spatial video and reads that from the media rather than from the
    // platform profile. §1.40 is the visionOS rule about Dolby Vision stereo profiles, so
    // reporting the attribute under it named a requirement that does not exist, on flat
    // variants that need no layout.

    // AirPlay +1.41 — CENC pattern checks live in protection rules
    issues
}

/// A §1.6b level finding. The general ceiling is Main 10 Level 5.1, but immersive AIV as
/// §1.25 describes it — 4320×4320 at 90 fps — needs Level 6.1, so on such a stream a level
/// up to 6.1 is the encode the spec's own tiers ask for and is reported as the conflict it
/// is. A level the AIV guidance does not explain stays an error.
fn hevc_level_issue(ctx: &AuthoringContext<'_>, level: f64, subject: String) -> Issue {
    let ceiling = ctx.policy.hevc_error_level_ceiling(HEVC_MAX_LEVEL);
    if level > ceiling + 0.01 {
        return author_error(
            "1.6b",
            format!("{subject}, above the maximum Level {ceiling} for this content"),
        );
    }
    author_conflict_aware_issue(
        &ctx.policy,
        must(),
        "1.6b",
        format!("{subject}, above Main 10 Level {HEVC_MAX_LEVEL}"),
    )
}

/// The variant a client starts on for each video codec family: the first one listed,
/// since a client with no playback history picks the first variant it can decode.
/// Variants whose CODECS names no known video codec are grouped under `video`.
fn default_variants<'a>(
    variants: &[&'a MasterRendition],
) -> Vec<(&'static str, &'a MasterRendition)> {
    let mut defaults: Vec<(&'static str, &'a MasterRendition)> = Vec::new();
    for v in variants {
        let mut families: Vec<&'static str> = v
            .codecs
            .as_deref()
            .map(|c| {
                codec_tokens(c)
                    .iter()
                    .filter_map(|t| video_codec_family(t))
                    .filter(|f| *f != "mjpg")
                    .collect()
            })
            .unwrap_or_default();
        if families.is_empty() {
            families.push("video");
        }
        for family in families {
            if !defaults.iter().any(|(f, _)| *f == family) {
                defaults.push((family, v));
            }
        }
    }
    defaults
}

/// True when `playlist_url` is the resolved form of the (often relative) `variant_uri`.
fn playlist_url_matches(playlist_url: &str, variant_uri: &str) -> bool {
    fn normalize(s: &str) -> &str {
        s.split(['?', '#'])
            .next()
            .unwrap_or(s)
            .trim_start_matches("./")
    }
    let a = normalize(playlist_url);
    let b = normalize(variant_uri);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (long, short) = if a.len() > b.len() { (a, b) } else { (b, a) };
    long.strip_suffix(short)
        .is_some_and(|prefix| prefix.ends_with('/'))
}

/// Inits mapped by the media playlist behind `variant_uri`. Per-variant checks use this
/// so a finding is never reported against a variant the init does not belong to.
/// §9.1 — a CODECS attribute that names HEVC has to describe the HEVC the variant
/// actually delivers: a player picks the variant from this string alone (RFC 8216
/// §4.4.6.2), so a profile, tier or level that the `hvcC` contradicts either sends
/// unplayable media to a device that trusted it or hides playable media from one that
/// didn't. Only inits reached from this variant's own playlist are compared, and only
/// where both sides parsed.
fn codecs_vs_hvcc_issues(ctx: &AuthoringContext<'_>, variants: &[&MasterRendition]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for v in variants {
        let Some(codecs) = &v.codecs else { continue };
        for tok in codec_tokens(codecs) {
            if video_codec_family(tok) != Some("hevc") {
                continue;
            }
            let Some(declared) = parse_hevc_codec(tok) else {
                continue;
            };
            for entry in inits_for_variant(ctx, &v.uri) {
                let probe = &entry.probe;
                // dvh1/dvhe and MV-HEVC layers carry their own configuration records; an
                // hvcC only speaks for the hvc1/hev1 token when the sample entry agrees.
                let hevc_sample_entry = probe
                    .video_sample_fourcc
                    .as_deref()
                    .is_some_and(|c| matches!(c.to_ascii_lowercase().as_str(), "hvc1" | "hev1"));
                if !hevc_sample_entry {
                    continue;
                }
                let mut mismatches: Vec<String> = Vec::new();
                let actual_profile = probe
                    .video_profile
                    .as_deref()
                    .and_then(|s| s.trim().parse::<u32>().ok());
                if let (Some(want), Some(have)) = (declared.profile_idc, actual_profile) {
                    if want != have {
                        mismatches.push(format!(
                            "profile {} declared, {} in hvcC",
                            hevc_profile_name(want),
                            hevc_profile_name(have)
                        ));
                    }
                }
                let actual_tier = probe.video_tier.as_deref().and_then(HevcTier::from_flag);
                if let (Some(want), Some(have)) = (declared.tier, actual_tier) {
                    if want != have {
                        mismatches.push(format!(
                            "{} tier declared, {} tier in hvcC",
                            want.as_str(),
                            have.as_str()
                        ));
                    }
                }
                let actual_level = probe
                    .video_level
                    .as_deref()
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .map(hevc_level_from_idc);
                if let (Some(want), Some(have)) = (declared.level, actual_level) {
                    if (want - have).abs() > 0.01 {
                        mismatches.push(format!("level {want:.1} declared, {have:.1} in hvcC"));
                    }
                }
                if !mismatches.is_empty() {
                    issues.push(author_error(
                        "9.1",
                        format!(
                            "CODECS '{tok}' on '{}' does not describe init '{}': {}",
                            v.uri,
                            entry.uri,
                            mismatches.join("; ")
                        ),
                    ));
                }
            }
        }
    }
    issues
}

fn inits_for_variant<'a>(
    ctx: &AuthoringContext<'a>,
    variant_uri: &str,
) -> Vec<&'a InitProbeEntry> {
    let names: Vec<&str> = ctx
        .playlists
        .iter()
        .filter(|pl| playlist_url_matches(&pl.url, variant_uri))
        .map(|pl| pl.name.as_str())
        .collect();
    if names.is_empty() {
        return Vec::new();
    }
    ctx.init_probes
        .iter()
        .filter(|e| e.playlist_names.iter().any(|n| names.contains(&n.as_str())))
        .collect()
}

/// Best-effort MV-HEVC signal from CODECS: the layered HEVC sample entries plus the
/// `mv-hevc` spellings seen in the wild. An `lhvC` box in the init is the firm source.
fn codecs_look_mv_hevc(codecs: &str) -> bool {
    let lower = codecs.to_ascii_lowercase();
    if lower.contains("mv-hevc") || lower.contains("mvhevc") {
        return true;
    }
    codec_tokens(&lower)
        .iter()
        .any(|t| t.starts_with("lhv1") || t.starts_with("lhe1"))
}

/// REQ-VIDEO-LAYOUT naming a stereoscopic view, e.g. `CH-STEREO`.
fn layout_is_stereo(layout: Option<&str>) -> bool {
    layout.is_some_and(|l| l.to_ascii_lowercase().contains("stereo"))
}

#[cfg(test)]
mod tests {
    use super::super::context::{
        InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample,
    };
    use super::*;
    use crate::utils::mp4_probe::InitSegmentProbe;
    use crate::utils::validator::parser::parse_master_playlist;
    use crate::utils::validator::types::{MediaPlaylist, Severity};

    /// Two H.264 rungs whose default (first listed) sits far below the §1.32 target.
    const LOW_DEFAULT_LADDER: &str = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=500000,AVERAGE-BANDWIDTH=400000,RESOLUTION=640x360,CODECS="avc1.640015",FRAME-RATE=30
https://example.com/360.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2200000,AVERAGE-BANDWIDTH=2000000,RESOLUTION=1280x720,CODECS="avc1.64001f",FRAME-RATE=30
https://example.com/720.m3u8
"#;

    /// The same rungs with the ~2000 kbps one listed first, so the default is compliant.
    const TARGET_DEFAULT_LADDER: &str = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=2200000,AVERAGE-BANDWIDTH=2000000,RESOLUTION=1280x720,CODECS="avc1.64001f",FRAME-RATE=30
https://example.com/720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=500000,AVERAGE-BANDWIDTH=400000,RESOLUTION=640x360,CODECS="avc1.640015",FRAME-RATE=30
https://example.com/360.m3u8
"#;

    /// A 640×360 rung and a 1920×1080 rung, each with its own media playlist.
    const TWO_RUNG_LADDER: &str = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=2200000,AVERAGE-BANDWIDTH=2000000,RESOLUTION=640x360,CODECS="avc1.640015",FRAME-RATE=30
https://example.com/360.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=6000000,AVERAGE-BANDWIDTH=5000000,RESOLUTION=1920x1080,CODECS="avc1.640029",FRAME-RATE=30
https://example.com/1080.m3u8
"#;

    fn video_playlist(name: &str, url: &str) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.into(), url.into());
        pl.media_type = "VIDEO".into();
        pl.target_duration = 4.0;
        pl.has_endlist = true;
        pl
    }

    fn avc_init(uri: &str, playlist: &str, level: &str) -> InitProbeEntry {
        InitProbeEntry {
            uri: uri.into(),
            byterange: None,
            playlist_names: vec![playlist.into()],
            media_types: vec!["VIDEO".into()],
            probe: InitSegmentProbe {
                major_brand: Some("iso6".into()),
                video_sample_fourcc: Some("avc1".into()),
                video_profile: Some("100".into()),
                video_level: Some(level.into()),
                ..Default::default()
            },
        }
    }

    fn hevc_init(uri: &str, playlist: &str, has_lhvc: bool) -> InitProbeEntry {
        InitProbeEntry {
            uri: uri.into(),
            byterange: None,
            playlist_names: vec![playlist.into()],
            media_types: vec!["VIDEO".into()],
            probe: InitSegmentProbe {
                major_brand: Some("iso6".into()),
                compatible_brands: vec!["cmfc".into()],
                video_sample_fourcc: Some("hvc1".into()),
                has_lhvc,
                ..Default::default()
            },
        }
    }

    /// Run the video rules over `master_text` and keep only findings for `section`.
    fn section_issues(
        master_text: &str,
        playlists: &[MediaPlaylist],
        inits: &[InitProbeEntry],
        section: &str,
    ) -> Vec<Issue> {
        let master = parse_master_playlist("https://example.com/master.m3u8", master_text);
        let opts = ValidateAuthorOptions::default();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), playlists, &opts, inits, &segs, &vtts);
        let needle = format!("§{section}:");
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains(&needle))
            .collect()
    }

    // ── §1.32 default variant ────────────────────────────────────────────────

    #[test]
    fn author_1_32_reports_the_default_variant_not_the_ladder() {
        let issues = section_issues(LOW_DEFAULT_LADDER, &[], &[], "1.32");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0].message.contains("360.m3u8") && issues[0].message.contains("400 kbps"),
            "expected the first variant to be the subject, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_1_32_accepts_a_default_variant_at_the_target() {
        let issues = section_issues(TARGET_DEFAULT_LADDER, &[], &[], "1.32");
        assert!(
            issues.is_empty(),
            "a compliant default must not be reported because lower rungs exist: {issues:?}"
        );
    }

    // ── §9.1 CODECS against hvcC ─────────────────────────────────────────────

    /// One HEVC rung declaring Main 10, Main tier, Level 4.1 (`L123`).
    const HEVC_MASTER: &str = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=6000000,AVERAGE-BANDWIDTH=5000000,RESOLUTION=1920x1080,CODECS="hvc1.2.4.L123.B0",FRAME-RATE=30
https://example.com/hevc.m3u8
"#;

    /// An HEVC init whose hvcC reports `general_profile_idc`, `general_tier_flag` and
    /// `general_level_idc` exactly as the atom does.
    fn hvcc_init(profile_idc: &str, tier_flag: &str, level_idc: &str) -> InitProbeEntry {
        InitProbeEntry {
            uri: "https://example.com/hevc-init.mp4".into(),
            byterange: None,
            playlist_names: vec!["hevc".into()],
            media_types: vec!["VIDEO".into()],
            probe: InitSegmentProbe {
                major_brand: Some("iso6".into()),
                video_sample_fourcc: Some("hvc1".into()),
                video_profile: Some(profile_idc.into()),
                video_tier: Some(tier_flag.into()),
                video_level: Some(level_idc.into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn author_9_1_errors_when_codecs_level_contradicts_the_hvcc() {
        let playlists = vec![video_playlist("hevc", "https://example.com/hevc.m3u8")];
        // general_level_idc 153 is Level 5.1, not the Level 4.1 the CODECS string promises.
        let inits = vec![hvcc_init("2", "false", "153")];
        let issues = section_issues(HEVC_MASTER, &playlists, &inits, "9.1");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("level 4.1 declared, 5.1 in hvcC"),
            "got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_9_1_errors_when_codecs_tier_and_profile_contradict_the_hvcc() {
        let playlists = vec![video_playlist("hevc", "https://example.com/hevc.m3u8")];
        // Main profile at High tier behind a CODECS string claiming Main 10 at Main tier.
        let inits = vec![hvcc_init("1", "true", "123")];
        let issues = section_issues(HEVC_MASTER, &playlists, &inits, "9.1");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0]
                .message
                .contains("profile 2 (Main 10) declared, 1 (Main) in hvcC")
                && issues[0].message.contains("Main tier declared, High tier in hvcC"),
            "both mismatches belong in one finding, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_9_1_accepts_codecs_that_matches_the_hvcc() {
        let playlists = vec![video_playlist("hevc", "https://example.com/hevc.m3u8")];
        let inits = vec![hvcc_init("2", "false", "123")];
        let issues = section_issues(HEVC_MASTER, &playlists, &inits, "9.1");
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn author_9_1_stays_quiet_when_the_hvcc_was_not_read() {
        let playlists = vec![video_playlist("hevc", "https://example.com/hevc.m3u8")];
        let mut init = hvcc_init("2", "false", "123");
        init.probe.video_profile = None;
        init.probe.video_tier = None;
        init.probe.video_level = None;
        let issues = section_issues(HEVC_MASTER, &playlists, &[init], "9.1");
        assert!(
            issues.is_empty(),
            "an unread hvcC is not evidence of a mismatch: {issues:?}"
        );
    }

    // ── PROJ-AIV spec conflicts ──────────────────────────────────────────────

    /// An immersive AIV ladder as §1.25 describes one: 4320×4320 at 90 fps and 50 Mbps,
    /// stereo MV-HEVC at Level 6.1 (`general_level_idc` 183).
    const AIV_MASTER: &str = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=100000000,AVERAGE-BANDWIDTH=50000000,RESOLUTION=4320x4320,CODECS="hvc1.2.4.L183.B0",FRAME-RATE=90,VIDEO-RANGE=PQ,REQ-VIDEO-LAYOUT="CH-STEREO,PROJ-AIV"
https://example.com/aiv.m3u8
"#;
    /// The same shape without the immersive layout, which no part of the spec asks for.
    const NON_AIV_90FPS_MASTER: &str = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=100000000,AVERAGE-BANDWIDTH=50000000,RESOLUTION=4320x4320,CODECS="hvc1.2.4.L183.B0",FRAME-RATE=90,VIDEO-RANGE=PQ
https://example.com/fast.m3u8
"#;

    #[test]
    fn author_1_19_reports_aiv_frame_rate_as_a_spec_conflict() {
        let issues = section_issues(AIV_MASTER, &[], &[], "1.19");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Info);
        assert!(
            issues[0].message.contains("Spec conflict (PROJ-AIV)")
                && issues[0].message.contains("FRAME-RATE 90"),
            "got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_1_19_still_errors_on_90_fps_without_an_immersive_layout() {
        let issues = section_issues(NON_AIV_90FPS_MASTER, &[], &[], "1.19");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(!issues[0].message.contains("Spec conflict"));
    }

    #[test]
    fn author_1_6b_accepts_level_6_1_on_aiv_but_not_beyond() {
        let at_ceiling = section_issues(AIV_MASTER, &[], &[], "1.6b");
        assert!(
            at_ceiling.iter().all(|i| i.severity == Severity::Info),
            "Level 6.1 is the level the AIV tiers need, got: {at_ceiling:?}"
        );

        // Level 6.2 (`general_level_idc` 186) is not explained by the AIV tiers.
        let beyond = section_issues(
            &AIV_MASTER.replace("L183", "L186"),
            &[],
            &[],
            "1.6b",
        );
        assert!(
            beyond.iter().any(|i| i.severity == Severity::Error),
            "a level above 6.1 stays an error on AIV content, got: {beyond:?}"
        );
    }

    #[test]
    fn author_1_6b_still_errors_above_5_1_without_an_immersive_layout() {
        let issues = section_issues(NON_AIV_90FPS_MASTER, &[], &[], "1.6b");
        assert!(
            issues.iter().any(|i| i.severity == Severity::Error),
            "{issues:?}"
        );
    }

    #[test]
    fn aiv_bitrate_and_hdr_rules_become_spec_conflicts() {
        for section in ["1.20", "1.32", "1.34"] {
            let issues = section_issues(AIV_MASTER, &[], &[], section);
            assert_eq!(issues.len(), 1, "§{section}: {issues:?}");
            assert_eq!(issues[0].severity, Severity::Info, "§{section}");
            assert!(
                issues[0].message.contains("Spec conflict (PROJ-AIV)"),
                "§{section}: {}",
                issues[0].message
            );
        }
    }

    #[test]
    fn non_conflicting_rules_still_fail_on_aiv_content() {
        // §1.36 is about MV-HEVC carrying non-stereo video, which AIV does not excuse.
        let playlists = vec![video_playlist(
            "video/4320x4320 · 100000k",
            "https://example.com/aiv.m3u8",
        )];
        let inits = vec![hevc_init(
            "https://example.com/aiv-init.mp4",
            "video/4320x4320 · 100000k",
            true,
        )];
        let master = AIV_MASTER.replace(",REQ-VIDEO-LAYOUT=\"CH-STEREO,PROJ-AIV\"", "");
        let issues = section_issues(&master, &playlists, &inits, "1.36");
        assert!(
            issues.iter().any(|i| i.severity == Severity::Error),
            "{issues:?}"
        );
    }

    // ── §1.36 MV-HEVC ────────────────────────────────────────────────────────

    const MV_MASTER: &str = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=8000000,AVERAGE-BANDWIDTH=2000000,RESOLUTION=1920x1080,CODECS="hvc1.2.4.L123.B0",FRAME-RATE=30,VIDEO-RANGE=SDR
https://example.com/mv.m3u8
"#;
    const MV_STEREO_MASTER: &str = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=8000000,AVERAGE-BANDWIDTH=2000000,RESOLUTION=1920x1080,CODECS="hvc1.2.4.L123.B0",FRAME-RATE=30,VIDEO-RANGE=SDR,REQ-VIDEO-LAYOUT="CH-STEREO"
https://example.com/mv.m3u8
"#;

    #[test]
    fn author_1_36_errors_when_mv_hevc_is_not_stereo() {
        let playlists = vec![video_playlist(
            "video/1920x1080 · 8000k",
            "https://example.com/mv.m3u8",
        )];
        let inits = vec![hevc_init(
            "https://example.com/mv-init.mp4",
            "video/1920x1080 · 8000k",
            true,
        )];
        let issues = section_issues(MV_MASTER, &playlists, &inits, "1.36");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(issues[0].message.contains("REQ-VIDEO-LAYOUT is absent"));
    }

    #[test]
    fn author_1_36_accepts_mv_hevc_carrying_stereo_video() {
        let playlists = vec![video_playlist(
            "video/1920x1080 · 8000k",
            "https://example.com/mv.m3u8",
        )];
        let inits = vec![hevc_init(
            "https://example.com/mv-init.mp4",
            "video/1920x1080 · 8000k",
            true,
        )];
        assert!(section_issues(MV_STEREO_MASTER, &playlists, &inits, "1.36").is_empty());
    }

    #[test]
    fn author_1_36_ignores_plain_hevc_without_mv_signals() {
        let playlists = vec![video_playlist(
            "video/1920x1080 · 8000k",
            "https://example.com/mv.m3u8",
        )];
        let inits = vec![hevc_init(
            "https://example.com/mv-init.mp4",
            "video/1920x1080 · 8000k",
            false,
        )];
        assert!(section_issues(MV_MASTER, &playlists, &inits, "1.36").is_empty());
    }

    #[test]
    fn mv_hevc_codecs_hints() {
        assert!(codecs_look_mv_hevc("lhv1.2.4.L153.B0"));
        assert!(codecs_look_mv_hevc("hvc1.2.4.L153.B0,lhe1.2.4.L153.B0"));
        assert!(codecs_look_mv_hevc("MV-HEVC"));
        assert!(!codecs_look_mv_hevc("hvc1.2.4.L153.B0,mp4a.40.2"));
        assert!(!codecs_look_mv_hevc(""));
    }

    // ── §1.20 HDR frame rate ─────────────────────────────────────────────────

    #[test]
    fn author_1_20_skips_hdr_variants_without_frame_rate() {
        let master = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=8000000,AVERAGE-BANDWIDTH=2000000,RESOLUTION=1920x1080,CODECS="hvc1.2.4.L123.B0",VIDEO-RANGE=PQ
https://example.com/hdr.m3u8
"#;
        assert!(
            section_issues(master, &[], &[], "1.20").is_empty(),
            "an undeclared FRAME-RATE is not a rate above 30"
        );
    }

    #[test]
    fn author_1_20_still_reports_hdr_declared_above_30() {
        let master = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=8000000,AVERAGE-BANDWIDTH=2000000,RESOLUTION=1920x1080,CODECS="hvc1.2.4.L123.B0",FRAME-RATE=60,VIDEO-RANGE=PQ
https://example.com/hdr.m3u8
"#;
        assert_eq!(section_issues(master, &[], &[], "1.20").len(), 1);
    }

    // ── §1.11 level vs resolution ────────────────────────────────────────────

    fn two_rung_playlists() -> Vec<MediaPlaylist> {
        vec![
            video_playlist("video/640x360 · 2200k", "https://example.com/360.m3u8"),
            video_playlist("video/1920x1080 · 6000k", "https://example.com/1080.m3u8"),
        ]
    }

    #[test]
    fn author_1_11_charges_each_init_to_its_own_variant() {
        let playlists = two_rung_playlists();
        let inits = vec![
            avc_init(
                "https://example.com/360-init.mp4",
                "video/640x360 · 2200k",
                "30",
            ),
            avc_init(
                "https://example.com/1080-init.mp4",
                "video/1920x1080 · 6000k",
                "41",
            ),
        ];
        let issues = section_issues(TWO_RUNG_LADDER, &playlists, &inits, "1.11");
        assert!(
            issues.is_empty(),
            "the 1080p init's level must not be charged to the 360p rung: {issues:?}"
        );
    }

    #[test]
    fn author_1_11_reports_a_level_above_its_own_variant_requirement() {
        let playlists = two_rung_playlists();
        let inits = vec![
            avc_init(
                "https://example.com/360-init.mp4",
                "video/640x360 · 2200k",
                "51",
            ),
            avc_init(
                "https://example.com/1080-init.mp4",
                "video/1920x1080 · 6000k",
                "41",
            ),
        ];
        let issues = section_issues(TWO_RUNG_LADDER, &playlists, &inits, "1.11");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0].message.contains("360-init.mp4") && issues[0].message.contains("640x360"),
            "{}",
            issues[0].message
        );
    }

    #[test]
    fn author_1_11_falls_back_to_codecs_without_an_init() {
        let master = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=2200000,AVERAGE-BANDWIDTH=2000000,RESOLUTION=640x360,CODECS="avc1.640033",FRAME-RATE=30
https://example.com/360.m3u8
"#;
        let issues = section_issues(master, &[], &[], "1.11");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0].message.contains("CODECS 'avc1.640033'"),
            "{}",
            issues[0].message
        );
    }

    #[test]
    fn author_1_11_assumes_the_maximum_frame_rate_when_undeclared() {
        // Level 4.2 is what 1080p60 needs, so it must pass while FRAME-RATE is unknown.
        let master = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=6000000,AVERAGE-BANDWIDTH=2000000,RESOLUTION=1920x1080,CODECS="avc1.64002a"
https://example.com/1080.m3u8
"#;
        assert!(section_issues(master, &[], &[], "1.11").is_empty());
    }

    #[test]
    fn playlist_url_matching_resolves_relative_variant_uris() {
        assert!(playlist_url_matches(
            "https://example.com/v/360.m3u8",
            "360.m3u8"
        ));
        assert!(playlist_url_matches(
            "https://example.com/v/360.m3u8?token=1",
            "https://example.com/v/360.m3u8"
        ));
        assert!(!playlist_url_matches(
            "https://example.com/v/hd-360.m3u8",
            "360.m3u8"
        ));
        assert!(!playlist_url_matches("https://example.com/v/360.m3u8", ""));
    }
}
