//! Apple Authoring Spec §9 — Multivariant playlist.

use super::context::AuthoringContext;
use super::helpers::*;
use super::severity::{must, should};
use crate::utils::validator::types::{Issue, MasterPlaylist, MasterRendition, MediaRendition};

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        return issues;
    };

    let variants: Vec<_> = master.variants.iter().filter(|v| !v.is_iframe).collect();
    let iframes: Vec<_> = master.variants.iter().filter(|v| v.is_iframe).collect();

    // §9.1–9.2 — CODECS/RESOLUTION required on STREAM-INF
    for v in &variants {
        if v.codecs.is_none() {
            issues.push(author_error(
                "9.1",
                format!("STREAM-INF missing CODECS on '{}'", v.uri),
            ));
        }
        if v.resolution.is_none() {
            // audio-only may omit RESOLUTION
            let audio_only = v.codecs.as_deref().is_some_and(|c| {
                codec_tokens(c)
                    .iter()
                    .all(|t| video_codec_family(t).is_none())
            });
            if !audio_only {
                issues.push(author_error(
                    "9.2",
                    format!("video STREAM-INF missing RESOLUTION on '{}'", v.uri),
                ));
            }
        }
    }

    // §9.3–9.4 — CODECS/RESOLUTION required on I-FRAME-STREAM-INF
    for v in &iframes {
        if v.codecs.is_none() {
            issues.push(author_error(
                "9.3",
                format!("I-FRAME-STREAM-INF missing CODECS on '{}'", v.uri),
            ));
        }
        if v.resolution.is_none() {
            issues.push(author_error(
                "9.4",
                format!("I-FRAME-STREAM-INF missing RESOLUTION on '{}'", v.uri),
            ));
        }
    }

    // §9.5–9.6 — audio carried in its own renditions
    let demuxed_audio = master
        .media_renditions
        .iter()
        .any(|r| r.media_type == "AUDIO" && r.uri.is_some());

    // §9.5 — video and audio SHOULD be in separate streams
    let muxed_variants: Vec<&str> = variants
        .iter()
        .filter(|v| {
            if v.audio_group.is_some() {
                return false;
            }
            let Some(codecs) = v.codecs.as_deref() else {
                return false;
            };
            let tokens = codec_tokens(codecs);
            let has_video = tokens.iter().any(|t| video_codec_family(t).is_some());
            let has_audio = tokens.iter().any(|t| video_codec_family(t).is_none());
            has_video && has_audio
        })
        .map(|v| v.uri.as_str())
        .collect();
    if !muxed_variants.is_empty() {
        issues.push(author_issue(
            should(),
            "9.5",
            format!(
                "video and audio SHOULD be in separate streams; muxed variant(s) without an AUDIO group: {}",
                muxed_variants.join(", ")
            ),
        ));
    }

    // §9.6 — multichannel audio MUST be in separate audio streams
    let has_mc_audio = master.media_renditions.iter().any(|r| {
        r.media_type == "AUDIO"
            && r.channels
                .as_deref()
                .is_some_and(|c| c.split('/').next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(0) > 2)
    });
    if has_mc_audio && !demuxed_audio {
        issues.push(author_issue(
            must(),
            "9.6",
            "multichannel audio MUST be carried in separate audio streams (EXT-X-MEDIA with URI)",
        ));
    }

    // §9.9–9.10 — multiple bitrates; HD ≥2 at max resolution
    if variants.len() < 2 {
        issues.push(author_issue(
            must(),
            "9.9",
            "multiple bit rates MUST be provided; fewer than 2 video variants in the multivariant playlist",
        ));
    }
    {
        let max_h = variants
            .iter()
            .filter_map(|v| v.resolution.as_deref().and_then(parse_resolution))
            .map(|(_, h)| h)
            .max()
            .unwrap_or(0);
        if max_h >= 720 {
            let at_max = variants
                .iter()
                .filter(|v| {
                    v.resolution
                        .as_deref()
                        .and_then(parse_resolution)
                        .is_some_and(|(_, h)| h == max_h)
                })
                .count();
            if at_max < 2 {
                issues.push(author_warn(
                    "9.10",
                    format!("HD max resolution has only {at_max} variant(s); recommend ≥2"),
                ));
            }
        }
    }

    // §9.12 — EXT-X-INDEPENDENT-SEGMENTS on the master, or on every video media playlist
    if !master.independent_segments {
        for pl in ctx.video_playlists().filter(|p| !p.independent_segments) {
            issues.push(author_issue(
                must(),
                "9.12",
                format!(
                    "EXT-X-INDEPENDENT-SEGMENTS missing on the multivariant playlist and on '{}'",
                    pl.name
                ),
            ));
        }
    }

    // §9.13–9.14 — AVERAGE-BANDWIDTH MUST; BANDWIDTH sanity
    for v in &variants {
        if v.average_bandwidth.is_none() {
            issues.push(author_error(
                "9.14",
                format!(
                    "STREAM-INF missing AVERAGE-BANDWIDTH on variant '{}'",
                    v.uri
                ),
            ));
        }
        if let (Some(bw), Some(avg)) = (v.bandwidth, v.average_bandwidth) {
            if avg > bw {
                issues.push(author_warn(
                    "9.13",
                    format!(
                        "AVERAGE-BANDWIDTH ({avg}) > BANDWIDTH ({bw}) on '{}'",
                        v.uri
                    ),
                ));
            }
        }
    }

    // §9.15 — FRAME-RATE on video STREAM-INF
    for v in &variants {
        let audio_only = v.codecs.as_deref().is_some_and(|c| {
            codec_tokens(c)
                .iter()
                .all(|t| video_codec_family(t).is_none())
        });
        if !audio_only && v.frame_rate.is_none() {
            issues.push(author_issue(
                must(),
                "9.15",
                format!("video STREAM-INF MUST have FRAME-RATE; missing on '{}'", v.uri),
            ));
        }
    }
    // §9.16 — once any rendition declares an HDR VIDEO-RANGE, every video rendition needs one.
    // Audio-only variants have no video to describe, so they are not counted; I-frame
    // renditions carry video and are.
    {
        let video_renditions: Vec<&MasterRendition> = master
            .variants
            .iter()
            .filter(|v| v.is_iframe || !stream_inf_is_audio_only(v))
            .collect();
        let any_hdr = video_renditions
            .iter()
            .any(|v| is_hdr_range(v.video_range.as_deref()));
        let missing: Vec<&str> = video_renditions
            .iter()
            .filter(|v| v.video_range.is_none())
            .map(|v| v.uri.as_str())
            .collect();
        if any_hdr && !missing.is_empty() {
            issues.push(author_error(
                "9.16",
                format!(
                    "VIDEO-RANGE is required on every video rendition when any HDR rendition is present; missing on: {}",
                    missing.join(", ")
                ),
            ));
        }
    }

    // §9.17 — renditions of one language listed general → specific
    issues.extend(check_rendition_specificity_order(&master.media_renditions));

    // §9.18 — EXT-X-CONTENT-STEERING and the PATHWAY-IDs it selects between
    issues.extend(check_content_steering(master, &variants));

    // §9.19–9.20 — SCORE all or none; APAC profile/level
    {
        let with_score = variants.iter().filter(|v| v.score.is_some()).count();
        if with_score > 0 && with_score != variants.len() {
            issues.push(author_error(
                "9.19",
                "SCORE MUST be present on all variants or none",
            ));
        }
        for v in &variants {
            if let Some(codecs) = &v.codecs {
                for tok in codec_tokens(codecs) {
                    if is_apac(tok) && !tok.contains('.') {
                        issues.push(author_warn(
                            "9.20",
                            format!("APAC CODECS '{tok}' SHOULD include profile/level"),
                        ));
                    }
                }
            }
        }
    }

    // iOS §9.21–9.22
    if ctx.policy.require_192k_variant {
        let has_low = variants.iter().any(|v| {
            v.bandwidth.unwrap_or(u64::MAX) <= 192_000
                || v.average_bandwidth.unwrap_or(u64::MAX) <= 192_000
        });
        if !has_low {
            issues.push(author_warn(
                "9.21",
                "iOS: no variant at ≤192 kbps",
            ));
        }
    }

    // tvOS: no audio-only variants
    if ctx.policy.forbid_audio_only_variants {
        for v in &variants {
            let audio_only = v.resolution.is_none()
                && v.codecs.as_deref().is_some_and(|c| {
                    codec_tokens(c)
                        .iter()
                        .all(|t| video_codec_family(t).is_none())
                });
            if audio_only {
                issues.push(author_error(
                    "9.20",
                    format!("tvOS: audio-only variant '{}' is not allowed", v.uri),
                ));
            }
        }
    }

    // AirPlay §9.23–9.24
    if ctx.policy.require_full_codec_fps_ladders {
        use std::collections::HashSet;
        let mut keys = HashSet::new();
        for v in &variants {
            let fam = v
                .codecs
                .as_deref()
                .and_then(|c| codec_tokens(c).into_iter().find_map(video_codec_family))
                .unwrap_or("?");
            let fps = v.frame_rate.map(|f| format!("{f:.2}")).unwrap_or_default();
            keys.insert(format!("{fam}@{fps}"));
        }
        // Each codec/fps combo should have multiple bitrates — check counts
        use std::collections::HashMap;
        let mut counts: HashMap<String, usize> = HashMap::new();
        for v in &variants {
            let fam = v
                .codecs
                .as_deref()
                .and_then(|c| codec_tokens(c).into_iter().find_map(video_codec_family))
                .unwrap_or("?");
            let fps = v.frame_rate.map(|f| format!("{f:.2}")).unwrap_or_default();
            *counts.entry(format!("{fam}@{fps}")).or_default() += 1;
        }
        for (k, n) in counts {
            if n < 2 {
                issues.push(author_warn(
                    "9.23",
                    format!("AirPlay2: codec/fps ladder '{k}' has only {n} bitrate(s)"),
                ));
            }
        }
        let has_hdr = variants.iter().any(|v| is_hdr_range(v.video_range.as_deref()));
        if has_hdr {
            let has_dv = variants.iter().any(|v| {
                v.codecs.as_deref().is_some_and(|c| {
                    codec_tokens(c)
                        .iter()
                        .any(|t| video_codec_family(t) == Some("dv"))
                })
            });
            let has_hdr10 = variants.iter().any(|v| {
                v.video_range.as_deref().is_some_and(|r| r.eq_ignore_ascii_case("PQ"))
                    && v.codecs.as_deref().is_some_and(|c| {
                        codec_tokens(c)
                            .iter()
                            .any(|t| video_codec_family(t) == Some("hevc"))
                    })
            });
            if has_dv && !has_hdr10 {
                issues.push(author_warn(
                    "9.24",
                    "AirPlay2: Dolby Vision present; HDR10 (PQ HEVC) SHOULD also be offered",
                ));
            }
        }
    }

    issues
}

/// Whether a STREAM-INF describes audio only: no RESOLUTION and no video CODECS token.
/// A variant with neither attribute is treated as audio so video rules stay quiet about it;
/// §9.1–9.2 already report the missing attributes.
fn stream_inf_is_audio_only(v: &MasterRendition) -> bool {
    if v.resolution.is_some() {
        return false;
    }
    match v.codecs.as_deref() {
        Some(codecs) => codec_tokens(codecs)
            .iter()
            .all(|t| video_codec_family(t).is_none()),
        None => true,
    }
}

/// §9.17 — renditions that describe the same language SHOULD run general → specific, so a
/// player picking the first acceptable match lands on the plain rendition before the
/// specialised one. Renditions are only comparable inside one TYPE + GROUP-ID + primary
/// language subtag, which keeps unrelated tags such as `fr-CA` and `en` apart.
fn check_rendition_specificity_order(renditions: &[MediaRendition]) -> Vec<Issue> {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<(&str, &str, String), Vec<&MediaRendition>> = BTreeMap::new();
    for r in renditions {
        let Some(lang) = r.language.as_deref() else {
            continue;
        };
        let key = (
            r.media_type.as_str(),
            r.group_id.as_str(),
            primary_language(lang),
        );
        groups.entry(key).or_default().push(r);
    }

    let mut issues = Vec::new();
    for ((media_type, group_id, primary), members) in groups {
        let mut most_specific: Option<&MediaRendition> = None;
        for r in members {
            let Some(earlier) = most_specific else {
                most_specific = Some(r);
                continue;
            };
            if rendition_specificity(r) < rendition_specificity(earlier) {
                issues.push(author_warn(
                    "9.17",
                    format!(
                        "{media_type} group '{group_id}' lists '{}' ({}) after the more specific '{}' ({}); renditions for language '{primary}' SHOULD run general → specific",
                        r.name,
                        describe_specificity(r),
                        earlier.name,
                        describe_specificity(earlier),
                    ),
                ));
                break;
            }
            if rendition_specificity(r) > rendition_specificity(earlier) {
                most_specific = Some(r);
            }
        }
    }
    issues
}

/// Lowercased primary language subtag: `en-US` and `en` both group under `en`.
fn primary_language(lang: &str) -> String {
    lang.split('-').next().unwrap_or(lang).to_ascii_lowercase()
}

/// How specific a rendition is: LANGUAGE subtags past the primary one plus CHARACTERISTICS.
fn rendition_specificity(r: &MediaRendition) -> usize {
    let lang_subtags = r
        .language
        .as_deref()
        .map(|l| {
            l.split('-')
                .filter(|s| !s.trim().is_empty())
                .count()
                .saturating_sub(1)
        })
        .unwrap_or(0);
    lang_subtags + characteristics(r).count()
}

fn characteristics(r: &MediaRendition) -> impl Iterator<Item = &str> {
    r.characteristics
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn describe_specificity(r: &MediaRendition) -> String {
    let lang = r.language.as_deref().unwrap_or("?");
    let chars: Vec<&str> = characteristics(r).collect();
    if chars.is_empty() {
        format!("LANGUAGE={lang}")
    } else {
        format!("LANGUAGE={lang}, CHARACTERISTICS={}", chars.join(" "))
    }
}

/// §9.18 — a multivariant playlist that uses Content Steering identifies its pathways with
/// PATHWAY-ID, on the EXT-X-CONTENT-STEERING tag for the pathway to start on and on the
/// STREAM-INF tags that belong to each pathway.
fn check_content_steering(master: &MasterPlaylist, variants: &[&MasterRendition]) -> Vec<Issue> {
    let mut issues = Vec::new();
    let steering: Vec<&str> = master
        .raw_content
        .lines()
        .map(str::trim)
        .filter(|l| {
            l.to_ascii_uppercase()
                .starts_with("#EXT-X-CONTENT-STEERING:")
        })
        .collect();
    let with_pathway = variants.iter().filter(|v| v.pathway_id.is_some()).count();

    if steering.is_empty() {
        if with_pathway > 0 {
            issues.push(author_info(
                "9.18",
                "PATHWAY-ID is present on STREAM-INF but there is no EXT-X-CONTENT-STEERING tag, so the pathways are never selected between",
            ));
        }
        return issues;
    }

    for line in &steering {
        let attrs = crate::utils::validator::parser::parse_attributes(
            line.split_once(':').map(|(_, rest)| rest).unwrap_or(""),
        );
        let Some(initial) = attrs.get("PATHWAY-ID") else {
            issues.push(author_issue(
                should(),
                "9.18",
                "EXT-X-CONTENT-STEERING SHOULD carry PATHWAY-ID naming the pathway to start on",
            ));
            continue;
        };
        let known = variants
            .iter()
            .any(|v| v.pathway_id.as_deref() == Some(initial.as_str()));
        if !known {
            issues.push(author_warn(
                "9.18",
                format!(
                    "EXT-X-CONTENT-STEERING PATHWAY-ID '{initial}' does not match the PATHWAY-ID of any variant"
                ),
            ));
        }
    }

    if with_pathway != variants.len() {
        issues.push(author_warn(
            "9.18",
            format!(
                "Content Steering is in use but only {with_pathway} of {} variant(s) carry PATHWAY-ID",
                variants.len()
            ),
        ));
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::super::context::{
        InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample,
    };
    use super::*;
    use crate::utils::validator::parser::parse_master_playlist;
    use crate::utils::validator::types::{MediaPlaylist, Severity};

    /// Findings for one spec section from a multivariant playlist with no media playlists.
    fn issues_for(master_text: &str, section: &str) -> Vec<Issue> {
        let master = parse_master_playlist("https://example.com/master.m3u8", master_text);
        let playlists: Vec<MediaPlaylist> = Vec::new();
        let opts = ValidateAuthorOptions::default();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains(section))
            .collect()
    }

    /// Findings citing exactly `rule`. A citation is followed by a colon, so this keeps
    /// §9.2 from also matching §9.20.
    fn rule_issues(master_text: &str, rule: &str) -> Vec<Issue> {
        issues_for(master_text, &format!("§{rule}:"))
    }

    // ── §9.1–9.4 STREAM-INF / I-FRAME-STREAM-INF attributes ──────────────────

    /// A two-rung H.264 ladder with a trick-play rendition, carrying every attribute
    /// §9.1–9.4 asks for. Tests drop one attribute at a time from it.
    const COMPLETE_LADDER: &str = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=6000000,AVERAGE-BANDWIDTH=5000000,RESOLUTION=1920x1080,CODECS="avc1.640029",FRAME-RATE=30
v1080.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.64001f",FRAME-RATE=30
v720.m3u8
#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=200000,RESOLUTION=1920x1080,CODECS="avc1.640029",URI="iframe.m3u8"
"#;

    #[test]
    fn author_9_1_to_9_4_accept_a_fully_described_ladder() {
        for rule in ["9.1", "9.2", "9.3", "9.4"] {
            let issues = rule_issues(COMPLETE_LADDER, rule);
            assert!(
                issues.is_empty(),
                "§{rule} on a compliant ladder: {:?}",
                issues.iter().map(|i| &i.message).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn author_9_2_errors_when_a_video_variant_omits_resolution() {
        let issues = rule_issues(
            &COMPLETE_LADDER.replace("RESOLUTION=1280x720,", ""),
            "9.2",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("v720.m3u8"),
            "the finding should name the variant, got: {}",
            issues[0].message
        );
    }

    /// RESOLUTION describes a picture, so a variant carrying only audio has none to
    /// declare. §9.14's AVERAGE-BANDWIDTH requirement still applies to it.
    #[test]
    fn author_9_2_ignores_an_audio_only_variant() {
        let issues = rule_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.64001f",FRAME-RATE=30
v720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=128000,AVERAGE-BANDWIDTH=120000,CODECS="mp4a.40.2"
audio.m3u8
"#,
            "9.2",
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn author_9_3_errors_when_an_iframe_variant_omits_codecs() {
        let issues = rule_issues(
            &COMPLETE_LADDER.replace(r#"CODECS="avc1.640029",URI"#, "URI"),
            "9.3",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("I-FRAME-STREAM-INF")
                && issues[0].message.contains("iframe.m3u8"),
            "got: {}",
            issues[0].message
        );
    }

    /// An I-frame rendition always carries video, so unlike §9.2 there is no audio-only
    /// case to excuse a missing RESOLUTION.
    #[test]
    fn author_9_4_errors_when_an_iframe_variant_omits_resolution() {
        let issues = rule_issues(
            &COMPLETE_LADDER.replace("RESOLUTION=1920x1080,CODECS=\"avc1.640029\",URI", "CODECS=\"avc1.640029\",URI"),
            "9.4",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(issues[0].message.contains("iframe.m3u8"), "{issues:?}");
    }

    /// §9.2's audio-only excuse is read off CODECS, so a variant that declares neither
    /// attribute is reported under both rules: nothing says it carries no picture. The
    /// §9.16 rendition rules take the opposite reading of the same variant and treat it
    /// as audio, which is why each states its own test rather than sharing a helper.
    #[test]
    fn author_9_1_and_9_2_both_report_a_variant_that_declares_neither() {
        let content = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,FRAME-RATE=30
v720.m3u8
"#;
        for rule in ["9.1", "9.2"] {
            let issues = rule_issues(content, rule);
            assert_eq!(issues.len(), 1, "§{rule}: {issues:?}");
            assert_eq!(issues[0].severity, Severity::Error);
            assert!(issues[0].message.contains("v720.m3u8"), "{issues:?}");
        }
    }

    // ── §9.19 SCORE, §9.20 APAC profile/level ────────────────────────────────

    /// SCORE ranks the variants against each other, so a player can only use it when
    /// every variant carries one — a half-scored ladder has no ordering at all.
    #[test]
    fn author_9_19_errors_when_only_some_variants_carry_score() {
        // The 1080 rung is scored and the 720 rung is not.
        let partial = COMPLETE_LADDER.replace(
            r#"CODECS="avc1.640029",FRAME-RATE=30"#,
            r#"CODECS="avc1.640029",FRAME-RATE=30,SCORE=2.0"#,
        );
        let issues = rule_issues(&partial, "9.19");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("all variants or none"),
            "got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_9_19_accepts_score_on_every_variant_or_on_none() {
        assert!(rule_issues(COMPLETE_LADDER, "9.19").is_empty());
        let every = COMPLETE_LADDER.replace("FRAME-RATE=30", "FRAME-RATE=30,SCORE=2.0");
        let issues = rule_issues(&every, "9.19");
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn author_9_20_asks_apac_codecs_for_a_profile_and_level() {
        let bare = rule_issues(
            &COMPLETE_LADDER.replace(r#"CODECS="avc1.64001f""#, r#"CODECS="avc1.64001f,apac""#),
            "9.20",
        );
        assert_eq!(bare.len(), 1, "{bare:?}");
        assert_eq!(bare[0].severity, Severity::Warn);
        assert!(bare[0].message.contains("'apac'"), "got: {}", bare[0].message);

        let qualified = rule_issues(
            &COMPLETE_LADDER.replace(r#"CODECS="avc1.64001f""#, r#"CODECS="avc1.64001f,apac.31.00""#),
            "9.20",
        );
        assert!(qualified.is_empty(), "{qualified:?}");
    }

    #[test]
    fn author_9_16_ignores_audio_only_variants() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=6000000,AVERAGE-BANDWIDTH=5000000,RESOLUTION=1920x1080,CODECS="hvc1.2.4.L153.B0",FRAME-RATE=30,VIDEO-RANGE=PQ
v1080.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="hvc1.2.4.L123.B0",FRAME-RATE=30,VIDEO-RANGE=PQ
v720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=128000,AVERAGE-BANDWIDTH=120000,CODECS="mp4a.40.2"
audio.m3u8
"#,
            "§9.16",
        );
        assert!(
            issues.is_empty(),
            "an audio-only variant has no video range to declare: {issues:?}"
        );
    }

    #[test]
    fn author_9_16_flags_iframe_variant_missing_video_range() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=6000000,AVERAGE-BANDWIDTH=5000000,RESOLUTION=1920x1080,CODECS="hvc1.2.4.L153.B0",FRAME-RATE=30,VIDEO-RANGE=PQ
v1080.m3u8
#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=200000,RESOLUTION=1920x1080,CODECS="hvc1.2.4.L153.B0",URI="iframe.m3u8"
"#,
            "§9.16",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(issues[0].message.contains("iframe.m3u8"));
    }

    #[test]
    fn author_9_16_quiet_when_no_hdr_rendition_exists() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30
v720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1500000,AVERAGE-BANDWIDTH=1200000,RESOLUTION=960x540,CODECS="avc1.640029",FRAME-RATE=30
v540.m3u8
"#,
            "§9.16",
        );
        assert!(issues.is_empty(), "an all-SDR ladder may omit it: {issues:?}");
    }

    #[test]
    fn author_9_17_does_not_compare_unrelated_languages() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="Francais (Canada)",LANGUAGE="fr-CA",URI="fr-CA.m3u8"
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English",LANGUAGE="en",URI="en.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30,SUBTITLES="subs"
v720.m3u8
"#,
            "§9.17",
        );
        assert!(
            issues.is_empty(),
            "fr-CA and en describe different languages: {issues:?}"
        );
    }

    #[test]
    fn author_9_17_flags_specific_language_listed_before_general() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English (US)",LANGUAGE="en-US",URI="en-US.m3u8"
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English",LANGUAGE="en",URI="en.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30,SUBTITLES="subs"
v720.m3u8
"#,
            "§9.17",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(issues[0].message.contains("English"));
    }

    #[test]
    fn author_9_17_flags_characteristics_listed_before_plain_rendition() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English SDH",LANGUAGE="en",CHARACTERISTICS="public.accessibility.describes-music-and-sound,public.accessibility.transcribes-spoken-dialog",URI="en-sdh.m3u8"
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English",LANGUAGE="en",URI="en.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30,SUBTITLES="subs"
v720.m3u8
"#,
            "§9.17",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].message.contains("CHARACTERISTICS"));
    }

    #[test]
    fn author_9_17_accepts_general_before_specific() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English",LANGUAGE="en",URI="en.m3u8"
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English SDH",LANGUAGE="en",CHARACTERISTICS="public.accessibility.transcribes-spoken-dialog",URI="en-sdh.m3u8"
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English (US) SDH",LANGUAGE="en-US",CHARACTERISTICS="public.accessibility.transcribes-spoken-dialog",URI="en-US-sdh.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30,SUBTITLES="subs"
v720.m3u8
"#,
            "§9.17",
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn author_9_18_accepts_content_steering_with_matching_pathways() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-CONTENT-STEERING:SERVER-URI="https://example.com/steer.json",PATHWAY-ID="cdn-a"
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30,PATHWAY-ID="cdn-a"
a/v720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30,PATHWAY-ID="cdn-b"
b/v720.m3u8
"#,
            "§9.18",
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn author_9_18_flags_content_steering_without_pathway_id() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-CONTENT-STEERING:SERVER-URI="https://example.com/steer.json"
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30,PATHWAY-ID="cdn-a"
a/v720.m3u8
"#,
            "§9.18",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].message.contains("EXT-X-CONTENT-STEERING SHOULD carry PATHWAY-ID"));
    }

    #[test]
    fn author_9_18_flags_initial_pathway_no_variant_offers() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-CONTENT-STEERING:SERVER-URI="https://example.com/steer.json",PATHWAY-ID="cdn-c"
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30,PATHWAY-ID="cdn-a"
a/v720.m3u8
"#,
            "§9.18",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].message.contains("'cdn-c'"));
    }

    #[test]
    fn author_9_18_flags_variants_missing_pathway_id_under_steering() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-CONTENT-STEERING:SERVER-URI="https://example.com/steer.json",PATHWAY-ID="cdn-a"
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30,PATHWAY-ID="cdn-a"
a/v720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1500000,AVERAGE-BANDWIDTH=1200000,RESOLUTION=960x540,CODECS="avc1.640029",FRAME-RATE=30
v540.m3u8
"#,
            "§9.18",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].message.contains("1 of 2 variant(s) carry PATHWAY-ID"));
    }

    #[test]
    fn author_9_18_notes_pathway_id_without_content_steering() {
        let issues = issues_for(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=3000000,AVERAGE-BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS="avc1.640029",FRAME-RATE=30,PATHWAY-ID="cdn-a"
a/v720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1500000,AVERAGE-BANDWIDTH=1200000,RESOLUTION=960x540,CODECS="avc1.640029",FRAME-RATE=30,PATHWAY-ID="cdn-a"
v540.m3u8
"#,
            "§9.18",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Info);
        assert!(issues[0].message.contains("no EXT-X-CONTENT-STEERING"));
    }
}
