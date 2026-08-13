//! Apple Authoring Spec §7 — Segmentation (playlist-visible).

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();

    for pl in ctx.playlists.iter().filter(|p| !p.is_iframe) {
        if pl.target_duration <= 0.0 || pl.segments.is_empty() {
            continue;
        }

        // §7.5–7.6 — TD SHOULD be ~6s; EXTINF nominal ~6s
        if (pl.target_duration - 6.0).abs() > 2.0 {
            issues.push(author_warn(
                "7.5",
                format!(
                    "'{}' TARGETDURATION {:.0}s is far from recommended ~6s",
                    pl.name, pl.target_duration
                ),
            ));
        }
        let avg: f64 = pl.segments.iter().map(|s| s.duration).sum::<f64>()
            / pl.segments.len() as f64;
        if (avg - 6.0).abs() > 2.0 {
            issues.push(author_warn(
                "7.6",
                format!(
                    "'{}' average EXTINF {avg:.2}s is far from recommended ~6s",
                    pl.name
                ),
            ));
        }

        // §7.7 — EXTINF MUST NOT exceed TD by >0.5s (stricter than RFC)
        for (i, seg) in pl.segments.iter().enumerate() {
            if seg.duration > pl.target_duration + 0.5 {
                issues.push(author_error(
                    "7.7",
                    format!(
                        "'{}' segment {} EXTINF {:.3}s exceeds TARGETDURATION {:.0} by >0.5s",
                        pl.name, i, seg.duration, pl.target_duration
                    ),
                ));
                break; // one sample enough
            }
        }
    }

    // Phase C hooks: IDR/ASP from samples
    if ctx.deep_checks {
        for s in ctx.segment_samples {
            if s.looks_like_ts && !s.has_idr_nal_hint {
                issues.push(author_warn(
                    "7.1",
                    format!(
                        "sampled segment {} of '{}' may lack IDR at start (best-effort)",
                        s.segment_index, s.playlist_name
                    ),
                ));
            }
        }
    }

    issues
}
