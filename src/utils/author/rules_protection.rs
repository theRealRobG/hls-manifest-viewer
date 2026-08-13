//! Apple Authoring Spec §13 — Content protection.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();

    for pl in ctx.playlists {
        // §13.x METHOD / KEYFORMAT
        for method in &pl.encryption_methods {
            let m = method.to_ascii_uppercase();
            if m == "SAMPLE-AES-CTR" {
                issues.push(author_error(
                    "13.5",
                    format!(
                        "'{}' uses METHOD=SAMPLE-AES-CTR which is forbidden for Apple devices",
                        pl.name
                    ),
                ));
            }
            if m == "AES-128" {
                // allowed
            }
            if m.contains("SAMPLE-AES") && m != "SAMPLE-AES" && m != "SAMPLE-AES-CTR" {
                issues.push(author_warn(
                    "13.2",
                    format!("unusual encryption METHOD '{method}' on '{}'", pl.name),
                ));
            }
        }
        for fmt in &pl.key_formats {
            // Common: identity, com.apple.streamingkeydelivery, urn:uuid:edef8ba9-...
            let f = fmt.to_ascii_lowercase();
            if f.contains("widevine") && !f.contains("edef8ba9") {
                // soft
                let _ = f;
            }
        }

        // HDCP-LEVEL on STREAM-INF copied to playlist
        if let Some(hdcp) = &pl.hdcp_level {
            let h = hdcp.to_ascii_uppercase();
            if h != "NONE" && h != "TYPE-0" && h != "TYPE-1" {
                issues.push(author_warn(
                    "13.6",
                    format!("unexpected HDCP-LEVEL '{hdcp}' on '{}'", pl.name),
                ));
            }
        }
    }

    if let Some(master) = ctx.master {
        for v in &master.variants {
            if let Some(hdcp) = &v.hdcp_level {
                let h = hdcp.to_ascii_uppercase();
                if !(h == "NONE" || h == "TYPE-0" || h == "TYPE-1") {
                    issues.push(author_warn(
                        "13.6",
                        format!("unexpected HDCP-LEVEL '{hdcp}' on '{}'", v.uri),
                    ));
                }
            }
        }
    }

    // Phase B: CENC pattern / senc|saiz+saio
    for entry in ctx.init_probes {
        // Init may not have senc; check tenc indirectly via encrypted sample entry
        let enc = entry
            .probe
            .video_sample_fourcc
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case("encv"))
            || entry
                .probe
                .audio_sample_fourcc
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case("enca"));
        if enc {
            // §13.7 / 13.9 — pattern encryption expected in media; note if deep samples lack saiz/saio
            if ctx.deep_checks {
                let related = ctx
                    .segment_samples
                    .iter()
                    .any(|s| s.has_saiz && s.has_saio || s.has_senc);
                if !related {
                    issues.push(author_warn(
                        "13.9",
                        format!(
                            "encrypted init '{}' but sampled segments lack senc/saiz+saio",
                            entry.uri
                        ),
                    ));
                }
            }
        }
    }

    // AirPlay §1.41 / protection overlay — SAMPLE-AES preferred notes
    if ctx.policy.profile == super::profile::AuthorProfile::AirPlay2 {
        for pl in ctx.playlists {
            if pl.encryption_methods.is_empty() {
                continue;
            }
            if pl.encryption_methods.iter().any(|m| m == "SAMPLE-AES-CTR") {
                issues.push(author_error(
                    "1.41",
                    format!("AirPlay2: SAMPLE-AES-CTR forbidden on '{}'", pl.name),
                ));
            }
        }
    }

    issues
}
