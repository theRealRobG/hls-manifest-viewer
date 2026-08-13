//! Apple Authoring Spec §14 — Low-Latency HLS.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();

    for pl in ctx.playlists {
        let Some(sc) = &pl.server_control else {
            continue;
        };
        // Treat presence of PART-INF / parts as LL-HLS
        let is_ll = pl.part_target.is_some() || !pl.parts.is_empty();
        if !is_ll {
            continue;
        }

        // §14.1–14.5 — PART-HOLD-BACK ≥ 3× part-target
        if let (Some(phb), Some(pt)) = (sc.part_hold_back, pl.part_target) {
            if phb + f64::EPSILON < 3.0 * pt {
                issues.push(author_error(
                    "14.2",
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
            issues.push(author_warn(
                "14.1",
                format!(
                    "LL-HLS playlist '{}' SHOULD set CAN-BLOCK-RELOAD=YES",
                    pl.name
                ),
            ));
        }
    }

    issues
}
