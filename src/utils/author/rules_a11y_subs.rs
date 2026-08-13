//! Apple Authoring Spec §4–5 — Accessibility / Subtitles.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        return issues;
    };

    let has_cc = master.variants.iter().any(|v| {
        v.closed_captions
            .as_deref()
            .is_some_and(|c| c != "NONE" && !c.is_empty())
    }) || master
        .media_renditions
        .iter()
        .any(|r| r.media_type == "CLOSED-CAPTIONS");

    let subs: Vec<_> = master
        .media_renditions
        .iter()
        .filter(|r| r.media_type == "SUBTITLES")
        .collect();
    let a11y_subs = subs.iter().any(|r| {
        r.characteristics
            .as_deref()
            .is_some_and(|c| c.contains("public.accessibility"))
    });

    // §4.1
    if !has_cc && !a11y_subs && subs.is_empty() {
        issues.push(author_warn(
            "4.1",
            "no closed captions and no accessibility-marked subtitles",
        ));
    }

    // §4.2 / 5.2 — subtitle types; AirPlay WebVTT only
    for r in &subs {
        // CODECS on subtitle groups aren't always on MEDIA — check associated STREAM-INF
        let _ = r;
    }
    // Check subtitle playlists for CODECS if fetched — often not fetched in validate today.
    // Inspect MASTER for SUBTITLES GROUP and any CODECS on STREAM-INF isn't subtitle.
    // Soft: if AirPlay and any subtitle URI ends with .m3u8, assume WebVTT unless stpp in master text
    if ctx.policy.webvtt_only {
        if master.raw_content.contains("stpp") || master.raw_content.contains("im1t") {
            issues.push(author_error(
                "5.2",
                "AirPlay2 requires WebVTT subtitles; IMSC1/stpp detected",
            ));
        }
    }

    // §4.4–4.7, 5.4–5.11 — LANGUAGE, CHARACTERISTICS, AUTOSELECT, FORCED
    for r in master.media_renditions.iter().filter(|r| {
        r.media_type == "SUBTITLES"
            || r.media_type == "CLOSED-CAPTIONS"
            || r.media_type == "AUDIO"
    }) {
        if r.media_type != "VIDEO" && r.language.is_none() {
            // §8.10 also — LANGUAGE on non-VIDEO MEDIA
            if r.media_type == "SUBTITLES" || r.media_type == "CLOSED-CAPTIONS" {
                issues.push(author_error(
                    "5.4",
                    format!("{} '{}' MUST have LANGUAGE", r.media_type, r.name),
                ));
            }
        }
    }

    for r in &subs {
        if r.forced {
            // Forced + regular merge: there SHOULD be a non-forced sibling same language
            let lang = r.language.as_deref().unwrap_or("");
            let has_regular = subs.iter().any(|o| {
                !o.forced && o.language.as_deref() == Some(lang) && o.group_id == r.group_id
            });
            if !has_regular {
                issues.push(author_warn(
                    "5.7",
                    format!(
                        "FORCED subtitles '{}' SHOULD have a non-forced sibling in the same language",
                        r.name
                    ),
                ));
            }
        }
        if r.characteristics
            .as_deref()
            .is_some_and(|c| c.contains("public.accessibility"))
            && !r.autoselect
        {
            issues.push(author_warn(
                "5.6",
                format!(
                    "accessibility subtitles '{}' SHOULD have AUTOSELECT=YES",
                    r.name
                ),
            ));
        }
    }

    // §5.3 — WebVTT text files MUST include X-TIMESTAMP-MAP
    if ctx.webvtt_samples.is_empty() {
        let has_text_subs = master.media_renditions.iter().any(|r| {
            r.media_type == "SUBTITLES"
                && r.uri.is_some()
                && !master.raw_content.contains("stpp")
        });
        if has_text_subs {
            issues.push(author_info(
                "5.3",
                "subtitle renditions present but no WebVTT cue files were sampled (fMP4/IMSC or fetch failed)",
            ));
        }
    } else {
        for sample in ctx.webvtt_samples {
            if sample.has_webvtt_header && !sample.has_x_timestamp_map {
                issues.push(author_error(
                    "5.3",
                    format!(
                        "WebVTT '{}' missing X-TIMESTAMP-MAP",
                        sample.uri
                    ),
                ));
            } else if !sample.has_webvtt_header {
                issues.push(author_warn(
                    "5.3",
                    format!(
                        "subtitle sample '{}' does not look like a WebVTT text file",
                        sample.uri
                    ),
                ));
            }
        }
    }

    issues
}
