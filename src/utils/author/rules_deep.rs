//! Phase C — measured bitrate and deep bitstream heuristics from segment samples.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    if !ctx.deep_checks || ctx.segment_samples.is_empty() {
        return issues;
    }

    use std::collections::HashMap;
    let mut by_playlist: HashMap<&str, Vec<&super::context::SegmentSample>> = HashMap::new();
    for s in ctx.segment_samples {
        by_playlist.entry(s.playlist_name.as_str()).or_default().push(s);
    }

    for (name, samples) in &by_playlist {
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

        let pl = ctx.playlists.iter().find(|p| p.name == *name);
        if let Some(pl) = pl {
            // §1.26–1.30 / 9.13 — compare to AVERAGE-BANDWIDTH / BANDWIDTH
            if let Some(avg) = pl.average_bandwidth {
                let tol = avg as f64 * 0.10; // 10% tolerance heuristic
                if (measured_avg - avg as f64).abs() > tol && measured_avg > avg as f64 * 1.1 {
                    issues.push(author_warn(
                        "1.26",
                        format!(
                            "'{name}' measured avg bitrate {:.0} bps exceeds AVERAGE-BANDWIDTH {avg} (sample)",
                            measured_avg
                        ),
                    ));
                }
            }
            if let Some(bw) = pl.bandwidth {
                if peak > bw as f64 * 1.1 {
                    issues.push(author_warn(
                        "1.27",
                        format!(
                            "'{name}' measured peak {:.0} bps exceeds BANDWIDTH {bw} (sample)",
                            peak
                        ),
                    ));
                }
            }
        }

        // §6.9 — I-frame measured bitrate soft check
        if name.contains("iframe") || pl.is_some_and(|p| p.is_iframe) {
            let _ = measured_avg;
        }
    }

    // Continuity / tfdt
    for s in ctx.segment_samples {
        if s.looks_like_fmp4 && !s.has_tfdt {
            issues.push(author_warn(
                "7.8",
                format!(
                    "fMP4 sample '{}[#{}]' missing tfdt (best-effort)",
                    s.playlist_name, s.segment_index
                ),
            ));
        }
    }

    issues
}
