//! Apple Authoring Spec §3 — Ads / interstitials (playlist-visible).

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        return issues;
    };

    // Look for interstitial-related DATERANGE in media playlists
    for pl in ctx.playlists {
        for line in pl.raw_content.lines() {
            let line = line.trim();
            if !line.starts_with("#EXT-X-DATERANGE:") {
                continue;
            }
            let attrs = crate::utils::validator::parser::parse_attributes(
                line.strip_prefix("#EXT-X-DATERANGE:").unwrap_or(""),
            );
            let class = attrs.get("CLASS").map(|s| s.as_str()).unwrap_or("");
            if class != "com.apple.hls.interstitial" {
                continue;
            }
            // §3.2–3.4 — when asset URI is present, we can only Warn about unknown asset codecs
            if attrs.contains_key("X-ASSET-URI") || attrs.contains_key("X-ASSET-LIST") {
                // Compare host variant codecs loosely — without fetching assets, emit SHOULD note
                if let Some(codecs) = &pl.codecs {
                    let _ = codecs;
                    issues.push(author_warn(
                        "3.2",
                        format!(
                            "interstitial on '{}' references an asset; ensure codecs/aspect/bandwidth match host variant",
                            pl.name
                        ),
                    ));
                }
            }

            // AirPlay §3.5 — similar frame-rate
            if ctx.policy.profile == super::profile::AuthorProfile::AirPlay2 {
                if let Some(fps) = pl.frame_rate {
                    let _ = fps;
                    issues.push(author_warn(
                        "3.5",
                        format!(
                            "AirPlay2: interstitial assets for '{}' SHOULD use frame-rate x, 2x, or x/2 of host ({:?})",
                            pl.name, pl.frame_rate
                        ),
                    ));
                }
            }
        }
    }

    let _ = master;
    issues
}
