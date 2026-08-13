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

    // Phase B: CENC pattern / tenc / AirPlay §1.41
    for entry in ctx.init_probes {
        let probe = &entry.probe;
        let enc = probe.had_encrypted_sample_entry
            || probe
                .video_sample_fourcc
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case("encv"))
            || probe
                .audio_sample_fourcc
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case("enca"))
            || probe.has_tenc
            || probe.scheme_type.as_deref().is_some_and(|s| {
                let s = s.to_ascii_lowercase();
                s == "cenc" || s == "cbcs" || s == "cens" || s == "cbc1"
            });

        if !enc {
            continue;
        }

        // §13.7 — encrypt:skip pattern of 1:9 (crypt=1, skip=9)
        if let (Some(crypt), Some(skip)) = (probe.crypt_byte_block, probe.skip_byte_block) {
            if !(crypt == 1 && skip == 9) {
                issues.push(author_error(
                    "13.7",
                    format!(
                        "CENC pattern {crypt}:{skip} on init '{}' MUST be encrypt:skip 1:9",
                        entry.uri
                    ),
                ));
            }
        } else if probe.has_tenc {
            // Pattern fields absent (tenc v0) — content-sensitive / full-sample more likely
            issues.push(author_warn(
                "13.7",
                format!(
                    "encrypted init '{}' has tenc without crypt/skip pattern (expect 1:9)",
                    entry.uri
                ),
            ));
        }

        // §13.9 — content-sensitive encryption MUST NOT be used (cbcs with pattern is OK;
        // "cens"/"cbc1" without standard pattern is suspicious — soft warn on unknown schemes)
        if let Some(scheme) = probe.scheme_type.as_deref() {
            let s = scheme.to_ascii_lowercase();
            if s == "cens" {
                issues.push(author_error(
                    "13.9",
                    format!(
                        "scheme_type '{scheme}' on init '{}' looks like content-sensitive CENC",
                        entry.uri
                    ),
                ));
            }
        }

        // AirPlay §1.41 — encrypted fMP4 MUST have senc OR saiz+saio (often in media segments)
        if ctx.policy.profile == super::profile::AuthorProfile::AirPlay2 {
            let in_init = probe.has_senc || (probe.has_saiz && probe.has_saio);
            let in_samples = ctx.segment_samples.iter().any(|s| {
                s.has_senc || (s.has_saiz && s.has_saio)
            });
            if !in_init && !in_samples {
                issues.push(author_error(
                    "1.41",
                    format!(
                        "AirPlay2: encrypted init '{}' missing senc or saiz+saio (check media segments)",
                        entry.uri
                    ),
                ));
            }
        } else if ctx.deep_checks {
            let related = ctx
                .segment_samples
                .iter()
                .any(|s| s.has_senc || (s.has_saiz && s.has_saio));
            if !related && !(probe.has_senc || (probe.has_saiz && probe.has_saio)) {
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

    // AirPlay SAMPLE-AES-CTR forbidden (playlist-level)
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
