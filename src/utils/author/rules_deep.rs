//! Phase C — measured bitrate and deep bitstream heuristics from segment samples.

use std::collections::HashMap;

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::{Issue, Severity};

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    if !ctx.deep_checks {
        return issues;
    }
    if ctx.segment_samples.is_empty() {
        issues.push(author_info(
            "1.26",
            "Deep checks enabled but no media segments were sampled",
        ));
        return issues;
    }

    let mut by_playlist: HashMap<&str, Vec<&super::context::SegmentSample>> = HashMap::new();
    for s in ctx.segment_samples {
        by_playlist
            .entry(s.playlist_name.as_str())
            .or_default()
            .push(s);
    }

    for (name, mut samples) in by_playlist {
        samples.sort_by_key(|s| s.segment_index);
        let pl = ctx.playlists.iter().find(|p| p.name == name);
        let is_vod = pl.is_some_and(|p| {
            p.has_endlist || p.playlist_type.as_deref() == Some("VOD")
        });
        let is_iframe = pl.is_some_and(|p| p.is_iframe) || samples.iter().any(|s| s.is_iframe_playlist);
        let is_video = pl.is_some_and(|p| p.media_type == "VIDEO" && !p.is_iframe);
        let is_audio = pl.is_some_and(|p| p.media_type == "AUDIO");

        let total_bytes: usize = samples.iter().map(|s| s.bytes).sum();
        let total_dur: f64 = samples.iter().map(|s| s.extinf_s).sum();
        if total_dur <= 0.0 {
            continue;
        }
        let measured_avg = (total_bytes as f64 * 8.0) / total_dur;
        let peak = samples
            .iter()
            .filter(|s| s.extinf_s > 0.0)
            .map(|s| (s.bytes as f64 * 8.0) / s.extinf_s)
            .fold(0.0_f64, f64::max);

        if let Some(pl) = pl {
            // §1.26 / 1.28 — average vs AVERAGE-BANDWIDTH
            if let Some(avg) = pl.average_bandwidth {
                let avg_f = avg as f64;
                if is_vod {
                    let delta = (measured_avg - avg_f).abs() / avg_f;
                    if delta > 0.10 {
                        issues.push(author_issue(
                            Severity::Error,
                            "1.26",
                            format!(
                                "'{name}' measured avg {:.0} bps is outside ±10% of AVERAGE-BANDWIDTH {avg} (deep sample)",
                                measured_avg
                            ),
                        ));
                    }
                } else if measured_avg > avg_f * 1.10 {
                    // Live: sampled window is short vs ~1h — Warn instead of Error
                    issues.push(author_warn(
                        "1.28",
                        format!(
                            "'{name}' sampled avg {:.0} bps exceeds 110% of AVERAGE-BANDWIDTH {avg} (short live sample)",
                            measured_avg
                        ),
                    ));
                }
            }

            // §1.27 / 1.29 — peak vs BANDWIDTH
            if let Some(bw) = pl.bandwidth {
                let bw_f = bw as f64;
                if is_vod {
                    let delta = (peak - bw_f).abs() / bw_f;
                    // Peak must be within 10% of BANDWIDTH for VOD — typically peak ≤ declared
                    if peak > bw_f * 1.10 {
                        issues.push(author_error(
                            "1.27",
                            format!(
                                "'{name}' measured peak {:.0} bps exceeds BANDWIDTH {bw} by >10% (deep sample)",
                                peak
                            ),
                        ));
                    } else if delta > 0.10 && peak < bw_f * 0.90 {
                        issues.push(author_warn(
                            "1.27",
                            format!(
                                "'{name}' measured peak {:.0} bps is >10% below BANDWIDTH {bw} (deep sample)",
                                peak
                            ),
                        ));
                    }
                } else if peak > bw_f * 1.25 {
                    issues.push(author_error(
                        "1.29",
                        format!(
                            "'{name}' measured peak {:.0} bps exceeds 125% of BANDWIDTH {bw} (live sample)",
                            peak
                        ),
                    ));
                }
            }

            // §1.30 — VOD peak SHOULD be ≤ 200% of average bit rate
            if is_vod && measured_avg > 0.0 && peak > measured_avg * 2.0 {
                issues.push(author_warn(
                    "1.30",
                    format!(
                        "'{name}' peak {:.0} bps is more than 200% of measured avg {:.0} bps",
                        peak, measured_avg
                    ),
                ));
            }

            // §9.13 — BANDWIDTH should cover peak of playable combination (soft: this rendition alone)
            if let Some(bw) = pl.bandwidth {
                if peak > bw as f64 * 1.01 {
                    issues.push(author_warn(
                        "9.13",
                        format!(
                            "'{name}' measured peak {:.0} bps exceeds declared BANDWIDTH {bw}",
                            peak
                        ),
                    ));
                }
            }

            // §6.9 — I-frame peak bit rate calculation (report measured)
            if is_iframe {
                if let Some(bw) = pl.bandwidth {
                    if peak > bw as f64 * 1.10 {
                        issues.push(author_warn(
                            "6.9",
                            format!(
                                "I-frame '{name}' measured peak {:.0} bps exceeds BANDWIDTH {bw}",
                                peak
                            ),
                        ));
                    }
                }
            }
        }

        // §1.13 — IDRs SHOULD be present every ~2 seconds
        if is_video {
            let idr_total: usize = samples.iter().map(|s| s.idr_count.max(usize::from(s.has_idr_nal_hint))).sum();
            if idr_total > 0 && total_dur > 0.0 {
                let interval = total_dur / idr_total as f64;
                if interval > 2.5 {
                    issues.push(author_warn(
                        "1.13",
                        format!(
                            "'{name}' estimated IDR interval ~{interval:.1}s exceeds ~2s recommendation (deep sample)"
                        ),
                    ));
                }
            } else if samples.iter().any(|s| s.looks_like_fmp4 || s.looks_like_ts)
                && samples.iter().all(|s| !s.has_idr_nal_hint)
            {
                issues.push(author_warn(
                    "1.13",
                    format!("'{name}' deep samples had no detectable IDR NALs"),
                ));
            }
        }

        // §7.4 — video segments MUST start with an IDR
        if is_video {
            for s in &samples {
                if (s.looks_like_fmp4 || s.looks_like_ts)
                    && s.has_idr_nal_hint
                    && !s.idr_at_start
                {
                    issues.push(author_error(
                        "7.4",
                        format!(
                            "video segment '{}[#{}]' has IDR but not near segment start",
                            s.playlist_name, s.segment_index
                        ),
                    ));
                } else if (s.looks_like_fmp4 || s.looks_like_ts) && !s.has_idr_nal_hint {
                    issues.push(author_warn(
                        "7.4",
                        format!(
                            "video segment '{}[#{}]' — no IDR NAL detected at start (best-effort)",
                            s.playlist_name, s.segment_index
                        ),
                    ));
                }
            }
        }

        // §7.2 — TS continuity counters
        for s in &samples {
            if s.looks_like_ts && s.ts_continuity_ok == Some(false) {
                issues.push(author_error(
                    "7.2",
                    format!(
                        "TS continuity counter discontinuity in '{}[#{}]'",
                        s.playlist_name, s.segment_index
                    ),
                ));
            }
        }

        // §7.3 / 7.1 — fMP4 tfdt continuity across contiguous samples
        let tfdt_pairs: Vec<_> = samples
            .iter()
            .filter_map(|s| s.tfdt_base_media_decode_time.map(|t| (s, t)))
            .collect();
        for w in tfdt_pairs.windows(2) {
            let (a, ta) = w[0];
            let (b, tb) = w[1];
            if b.segment_index != a.segment_index + 1 {
                continue;
            }
            if tb < ta {
                issues.push(author_error(
                    "7.3",
                    format!(
                        "'{name}' tfdt decreased from {ta} to {tb} between segments {} and {}",
                        a.segment_index, b.segment_index
                    ),
                ));
                continue;
            }
            // Compare EXTINF ratio to tfdt delta ratio when 3+ points unavailable —
            // with timescale from matching init, convert to seconds.
            if let Some(scale) = ctx
                .probe_for_playlist(name)
                .and_then(|e| e.probe.timescale.or(e.probe.movie_timescale))
            {
                let media_dur = (tb - ta) as f64 / scale as f64;
                let extinf = a.extinf_s;
                if extinf > 0.0 && (media_dur - extinf).abs() > 0.5 {
                    issues.push(author_warn(
                        "7.3",
                        format!(
                            "'{name}' tfdt delta {media_dur:.3}s vs EXTINF {extinf:.3}s between segments {}→{}",
                            a.segment_index, b.segment_index
                        ),
                    ));
                }
            }
        }

        // §8.1 — sum EXTINF vs media timeline within ~1 frame
        if let Some(scale) = ctx
            .probe_for_playlist(name)
            .and_then(|e| e.probe.timescale.or(e.probe.movie_timescale))
        {
            if let (Some(first), Some(last)) = (tfdt_pairs.first(), tfdt_pairs.last()) {
                if last.0.segment_index > first.0.segment_index {
                    let media_dur = (last.1 - first.1) as f64 / scale as f64;
                    // Sum EXTINF of segments from first inclusive through last exclusive
                    // (tfdt spans the media of intervening segments ending at last's start)
                    let extinf_sum: f64 = samples
                        .iter()
                        .filter(|s| {
                            s.segment_index >= first.0.segment_index
                                && s.segment_index < last.0.segment_index
                        })
                        .map(|s| s.extinf_s)
                        .sum();
                    let frame = pl
                        .and_then(|p| p.frame_rate)
                        .map(|fps| 1.0 / fps)
                        .unwrap_or(1.0 / 30.0);
                    if extinf_sum > 0.0 && (extinf_sum - media_dur).abs() > frame + 0.001 {
                        issues.push(author_error(
                            "8.1",
                            format!(
                                "'{name}' EXTINF sum {extinf_sum:.3}s vs tfdt span {media_dur:.3}s exceeds one frame (~{frame:.3}s)",
                            ),
                        ));
                    }
                }
            }
        }

        // §7.8 / 7.9 — xHE-AAC IPF / APAC ASP (best-effort)
        if is_audio {
            let codecs = pl
                .and_then(|p| p.codecs.as_deref())
                .unwrap_or("")
                .to_ascii_lowercase();
            if codecs.contains("mp4a.40.42") {
                issues.push(author_info(
                    "7.8",
                    format!(
                        "'{name}' is xHE-AAC — verify each segment starts with an Immediate Playout Frame (not fully detectable in-browser)"
                    ),
                ));
            }
            if codecs.contains("apac") {
                let any_asp = samples.iter().any(|s| s.has_asp_hint);
                if !any_asp {
                    issues.push(author_warn(
                        "7.9",
                        format!(
                            "'{name}' APAC samples — no ASP marker hint detected (best-effort)"
                        ),
                    ));
                }
            }
        }

        // Missing tfdt on fMP4
        for s in &samples {
            if s.looks_like_fmp4 && !s.has_tfdt {
                issues.push(author_warn(
                    "7.3",
                    format!(
                        "fMP4 sample '{}[#{}]' missing tfdt (best-effort)",
                        s.playlist_name, s.segment_index
                    ),
                ));
            }
        }
    }

    // §4.3 — closed captions MUST be in video media segments when declared
    if let Some(master) = ctx.master {
        let cc_declared = master.variants.iter().any(|v| {
            v.closed_captions
                .as_deref()
                .is_some_and(|c| c != "NONE" && !c.is_empty())
        }) || master
            .media_renditions
            .iter()
            .any(|r| r.media_type == "CLOSED-CAPTIONS");
        if cc_declared {
            let video_samples: Vec<_> = ctx
                .segment_samples
                .iter()
                .filter(|s| {
                    ctx.playlists.iter().any(|p| {
                        p.name == s.playlist_name && p.media_type == "VIDEO" && !p.is_iframe
                    })
                })
                .collect();
            if !video_samples.is_empty()
                && video_samples.iter().all(|s| !s.has_cc_sei_hint)
            {
                issues.push(author_warn(
                    "4.3",
                    "closed captions declared but no CEA-608/708 SEI (GA94) hint found in deep video samples",
                ));
            }
        }
    }

    issues
}
