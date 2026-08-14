//! Apple Authoring Spec §14 — Low-Latency HLS.

use super::context::AuthoringContext;
use super::helpers::*;
use super::severity::must;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();

    for pl in ctx.playlists {
        // Treat presence of PART-INF / parts as LL-HLS
        let is_ll = pl.part_target.is_some() || !pl.parts.is_empty();
        if !is_ll {
            continue;
        }
        // SERVER-CONTROL carries the delivery directives an LL-HLS playlist cannot do without,
        // so a missing tag is itself the finding rather than a reason to skip the playlist.
        let Some(sc) = &pl.server_control else {
            issues.push(author_error(
                "14.1",
                format!(
                    "LL-HLS playlist '{}' has partial segments but no EXT-X-SERVER-CONTROL (CAN-BLOCK-RELOAD=YES and PART-HOLD-BACK are required)",
                    pl.name
                ),
            ));
            continue;
        };

        // §14.3 — PART-HOLD-BACK ≥ 3× PART-TARGET
        if let (Some(phb), Some(pt)) = (sc.part_hold_back, pl.part_target) {
            if phb + f64::EPSILON < 3.0 * pt {
                issues.push(author_error(
                    "14.3",
                    format!(
                        "'{}' PART-HOLD-BACK ({phb}) MUST be ≥ 3× PART-TARGET ({pt})",
                        pl.name
                    ),
                ));
            }
        } else if pl.part_target.is_some() && sc.part_hold_back.is_none() {
            issues.push(author_error(
                "14.2",
                format!(
                    "'{}' has PART-TARGET but missing PART-HOLD-BACK",
                    pl.name
                ),
            ));
        }

        // Delta + CAN-SKIP-DATERANGES
        if sc.can_skip_until.is_some() && !sc.can_skip_dateranges {
            issues.push(author_warn(
                "14.5",
                format!(
                    "'{}' advertises CAN-SKIP-UNTIL without CAN-SKIP-DATERANGES",
                    pl.name
                ),
            ));
        }

        if !sc.can_block_reload {
            issues.push(author_issue(
                must(),
                "14.1",
                format!(
                    "LL-HLS playlist '{}' MUST set CAN-BLOCK-RELOAD=YES",
                    pl.name
                ),
            ));
        }
    }

    issues
}
