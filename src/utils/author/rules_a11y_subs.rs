//! Apple Authoring Spec §4–5 — Accessibility / Subtitles.

use super::context::AuthoringContext;
use super::helpers::*;
use super::severity::{must, should};
use crate::utils::validator::types::Issue;

/// The accessibility CHARACTERISTICS §4.5 expects on a subtitle rendition.
struct A11ySubtitleChars {
    /// Any `public.accessibility.*` value — the track is offered as accessible.
    marked: bool,
    transcribes_dialog: bool,
    describes_music_and_sound: bool,
}

impl A11ySubtitleChars {
    fn of(characteristics: Option<&str>) -> Self {
        let c = characteristics.unwrap_or("");
        Self {
            marked: c.contains("public.accessibility"),
            transcribes_dialog: c.contains("transcribes-spoken-dialog"),
            describes_music_and_sound: c.contains("describes-music-and-sound"),
        }
    }

    /// Values §4.5 requires but the rendition does not declare.
    fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.transcribes_dialog {
            missing.push("public.accessibility.transcribes-spoken-dialog");
        }
        if !self.describes_music_and_sound {
            missing.push("public.accessibility.describes-music-and-sound");
        }
        missing
    }
}

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

    // §4.7 — LANGUAGE on subtitle / closed-caption renditions. §8.10 states the same
    // requirement for every non-VIDEO rendition but defers to this section for these two
    // types, so the missing attribute is reported once.
    for r in master
        .media_renditions
        .iter()
        .filter(|r| r.media_type == "SUBTITLES" || r.media_type == "CLOSED-CAPTIONS")
    {
        if r.language.is_none() {
            issues.push(author_issue(
                must(),
                "4.7",
                format!("{} '{}' MUST have LANGUAGE", r.media_type, r.name),
            ));
        }
    }

    // §4.5 / §4.6, §5.8 / §5.11 — CHARACTERISTICS, AUTOSELECT, FORCED
    for r in &subs {
        let chars = A11ySubtitleChars::of(r.characteristics.as_deref());
        if chars.marked {
            let missing = chars.missing();
            if !missing.is_empty() {
                issues.push(author_issue(
                    must(),
                    "4.5",
                    format!(
                        "accessibility subtitles '{}' MUST declare CHARACTERISTICS {}",
                        r.name,
                        missing.join(" and ")
                    ),
                ));
            }
            if !r.autoselect {
                issues.push(author_issue(
                    must(),
                    "4.6",
                    format!(
                        "accessibility subtitles '{}' MUST have AUTOSELECT=YES",
                        r.name
                    ),
                ));
            }
        }

        if r.forced {
            // §5.8 — forced and regular subtitles of a language are merged at playback,
            // so a forced-only language leaves nothing for a viewer who wants full subtitles.
            let lang = r.language.as_deref().unwrap_or("");
            let has_regular = subs.iter().any(|o| {
                !o.forced && o.language.as_deref() == Some(lang) && o.group_id == r.group_id
            });
            if !has_regular {
                issues.push(author_issue(
                    should(),
                    "5.8",
                    format!(
                        "FORCED subtitles '{}' SHOULD have a non-forced sibling in the same language",
                        r.name
                    ),
                ));
            }
            if !r.autoselect {
                issues.push(author_issue(
                    should(),
                    "5.11",
                    format!(
                        "FORCED subtitles '{}' SHOULD have AUTOSELECT=YES",
                        r.name
                    ),
                ));
            }
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

    // §5.5 / §5.6 — subtitle playlists vs the main content they accompany
    issues.extend(check_subtitle_playlists(ctx));

    issues
}

/// §5.5 (coverage) and §5.6 (live TARGETDURATION) for the subtitle playlists that were
/// fetched alongside the video ladder. Both rules need a video playlist to compare against,
/// so nothing is reported when either side is missing.
fn check_subtitle_playlists(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    use crate::utils::validator::types::MediaPlaylist;

    let mut issues = Vec::new();
    let subtitle_playlists: Vec<&MediaPlaylist> = ctx
        .playlists
        .iter()
        .filter(|p| p.media_type == "SUBTITLES")
        .collect();
    if subtitle_playlists.is_empty() {
        return issues;
    }
    // The longest video rendition is the yardstick: every variant should be the same length,
    // so taking the maximum avoids blaming subtitles for a truncated video rendition.
    let Some(main) = ctx
        .video_playlists()
        .filter(|p| !p.segments.is_empty())
        .max_by(|a, b| {
            playlist_duration_s(a)
                .partial_cmp(&playlist_duration_s(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    else {
        return issues;
    };
    let main_duration = playlist_duration_s(main);
    let main_is_vod = main.has_endlist || main.playlist_type.as_deref() == Some("VOD");

    for sub in subtitle_playlists {
        let sub_duration = playlist_duration_s(sub);
        // Allow a segment's worth of slack: subtitle segmentation rarely lines up exactly
        // with video segmentation at the tail of the presentation.
        let slack = main.target_duration.max(sub.target_duration).max(1.0);

        if main_is_vod && main_duration > 0.0 {
            if !sub.has_endlist {
                issues.push(author_warn(
                    "5.5",
                    format!(
                        "subtitle playlist '{}' has no EXT-X-ENDLIST while the video is complete — it may not cover the whole presentation",
                        sub.name
                    ),
                ));
            }
            let shortfall = main_duration - sub_duration;
            if shortfall > slack.max(main_duration * 0.05) {
                issues.push(author_error(
                    "5.5",
                    format!(
                        "subtitle playlist '{}' covers {:.1}s of {:.1}s of main content — subtitles MUST cover the entire presentation",
                        sub.name, sub_duration, main_duration
                    ),
                ));
            } else if shortfall > slack {
                issues.push(author_warn(
                    "5.5",
                    format!(
                        "subtitle playlist '{}' covers {:.1}s of {:.1}s of main content",
                        sub.name, sub_duration, main_duration
                    ),
                ));
            }
        }

        // §5.6 — live subtitles share the media TARGETDURATION so playlist reloads line up.
        if !main_is_vod && main.target_duration > 0.0 && sub.target_duration > 0.0 {
            if (sub.target_duration - main.target_duration).abs() > 0.5 {
                issues.push(author_error(
                    "5.6",
                    format!(
                        "live subtitle playlist '{}' EXT-X-TARGETDURATION ({}) MUST match the other media ('{}' is {})",
                        sub.name, sub.target_duration, main.name, main.target_duration
                    ),
                ));
            }
        }
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
    use crate::utils::validator::types::{MasterPlaylist, MediaPlaylist, Segment, Severity};

    fn master() -> MasterPlaylist {
        parse_master_playlist(
            "https://example.com/master.m3u8",
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English",LANGUAGE="en",AUTOSELECT=YES,DEFAULT=YES,URI="subs.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS="avc1.4d401f,mp4a.40.2",FRAME-RATE=30,SUBTITLES="subs"
https://example.com/v.m3u8
"#,
        )
    }

    fn playlist(name: &str, media_type: &str, target: f64, count: usize, live: bool) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.into(), format!("https://example.com/{name}.m3u8"));
        pl.media_type = media_type.into();
        pl.target_duration = target;
        pl.has_endlist = !live;
        if !live {
            pl.playlist_type = Some("VOD".into());
        }
        for i in 0..count {
            pl.segments.push(Segment {
                uri: format!("{i}.m4s"),
                duration: target,
                title: None,
                pdt: None,
                discontinuity: false,
                byterange: None,
                is_ad: false,
                map_uri: None,
            });
        }
        pl
    }

    fn subtitle_issues(playlists: &[MediaPlaylist], section: &str) -> Vec<Issue> {
        let master = master();
        let opts = ValidateAuthorOptions::default();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), playlists, &opts, &inits, &segs, &vtts);
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains(section))
            .collect()
    }

    #[test]
    fn author_5_5_accepts_full_coverage() {
        let playlists = vec![
            playlist("video/1280x720", "VIDEO", 6.0, 20, false),
            playlist("subtitles/English (subs)", "SUBTITLES", 6.0, 20, false),
        ];
        let issues = subtitle_issues(&playlists, "§5.5");
        assert!(
            issues.is_empty(),
            "matching durations satisfy §5.5, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_5_5_errors_on_short_subtitle_coverage() {
        let playlists = vec![
            playlist("video/1280x720", "VIDEO", 6.0, 20, false),
            playlist("subtitles/English (subs)", "SUBTITLES", 6.0, 10, false),
        ];
        let issues = subtitle_issues(&playlists, "§5.5");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(issues[0].message.contains("60.0s of 120.0s"));
    }

    #[test]
    fn author_5_5_tolerates_one_segment_of_slack() {
        let playlists = vec![
            playlist("video/1280x720", "VIDEO", 6.0, 20, false),
            playlist("subtitles/English (subs)", "SUBTITLES", 6.0, 19, false),
        ];
        assert!(subtitle_issues(&playlists, "§5.5").is_empty());
    }

    #[test]
    fn author_5_6_errors_when_live_subtitle_target_duration_differs() {
        let playlists = vec![
            playlist("video/1280x720", "VIDEO", 6.0, 10, true),
            playlist("subtitles/English (subs)", "SUBTITLES", 10.0, 6, true),
        ];
        let issues = subtitle_issues(&playlists, "§5.6");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(issues[0].message.contains("TARGETDURATION"));
    }

    #[test]
    fn author_5_6_accepts_matching_live_target_duration() {
        let playlists = vec![
            playlist("video/1280x720", "VIDEO", 6.0, 10, true),
            playlist("subtitles/English (subs)", "SUBTITLES", 6.0, 10, true),
        ];
        assert!(subtitle_issues(&playlists, "§5.6").is_empty());
    }

    #[test]
    fn author_5_5_and_5_6_skip_when_no_subtitle_playlist_was_fetched() {
        let playlists = vec![playlist("video/1280x720", "VIDEO", 6.0, 20, false)];
        assert!(subtitle_issues(&playlists, "§5.5").is_empty());
        assert!(subtitle_issues(&playlists, "§5.6").is_empty());
    }

    /// Rule numbering / severity checks below run against a hand-written multivariant
    /// playlist with no media playlists fetched.
    fn master_issues(media: &str) -> Vec<Issue> {
        let content = format!(
            "#EXTM3U\n{media}#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS=\"avc1.4d401f,mp4a.40.2\",FRAME-RATE=30,SUBTITLES=\"subs\"\nhttps://example.com/v.m3u8\n"
        );
        let master = parse_master_playlist("https://example.com/master.m3u8", &content);
        let opts = ValidateAuthorOptions::default();
        let playlists: Vec<MediaPlaylist> = Vec::new();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        check(&ctx)
    }

    fn find<'a>(issues: &'a [Issue], section: &str) -> Option<&'a Issue> {
        issues.iter().find(|i| i.message.contains(section))
    }

    const A11Y_CHARS: &str =
        "public.accessibility.transcribes-spoken-dialog,public.accessibility.describes-music-and-sound";

    #[test]
    fn author_4_7_flags_subtitles_without_language() {
        let issues = master_issues(
            "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"English\",AUTOSELECT=YES,URI=\"subs.m3u8\"\n",
        );
        let issue = find(&issues, "§4.7").expect("expected §4.7 LANGUAGE issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(issue.message.contains("LANGUAGE"));
        assert!(find(&issues, "§5.4").is_none(), "§5.4 was the old mislabel");
    }

    #[test]
    fn author_4_6_requires_autoselect_on_accessibility_subtitles() {
        let issues = master_issues(&format!(
            "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"English CC\",LANGUAGE=\"en\",CHARACTERISTICS=\"{A11Y_CHARS}\",URI=\"subs.m3u8\"\n"
        ));
        let issue = find(&issues, "§4.6").expect("expected §4.6 AUTOSELECT issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(issue.message.contains("AUTOSELECT=YES"));
        // §5.6 now belongs to the live subtitle TARGETDURATION rule.
        assert!(find(&issues, "§5.6").is_none());
    }

    #[test]
    fn author_4_5_requires_both_accessibility_characteristics() {
        let issues = master_issues(
            "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"English CC\",LANGUAGE=\"en\",AUTOSELECT=YES,CHARACTERISTICS=\"public.accessibility.transcribes-spoken-dialog\",URI=\"subs.m3u8\"\n",
        );
        let issue = find(&issues, "§4.5").expect("expected §4.5 CHARACTERISTICS issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(issue.message.contains("describes-music-and-sound"));
        assert!(!issue.message.contains("transcribes-spoken-dialog"));
    }

    #[test]
    fn author_4_5_and_4_6_accept_a_complete_accessibility_rendition() {
        let issues = master_issues(&format!(
            "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"English CC\",LANGUAGE=\"en\",AUTOSELECT=YES,CHARACTERISTICS=\"{A11Y_CHARS}\",URI=\"subs.m3u8\"\n"
        ));
        assert!(find(&issues, "§4.5").is_none(), "{issues:?}");
        assert!(find(&issues, "§4.6").is_none(), "{issues:?}");
    }

    #[test]
    fn author_5_8_and_5_11_flag_a_forced_only_language() {
        let issues = master_issues(
            "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"French (Forced)\",LANGUAGE=\"fr\",FORCED=YES,URI=\"fr-forced.m3u8\"\n",
        );
        let merge = find(&issues, "§5.8").expect("expected §5.8 forced/regular merge issue");
        assert_eq!(merge.severity, Severity::Warn);
        assert!(find(&issues, "§5.7").is_none(), "§5.7 was the old mislabel");

        let autoselect = find(&issues, "§5.11").expect("expected §5.11 AUTOSELECT issue");
        assert_eq!(autoselect.severity, Severity::Warn);
    }

    #[test]
    fn author_5_8_and_5_11_accept_forced_alongside_a_regular_rendition() {
        let issues = master_issues(
            "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"French (Forced)\",LANGUAGE=\"fr\",FORCED=YES,AUTOSELECT=YES,URI=\"fr-forced.m3u8\"\n\
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"French\",LANGUAGE=\"fr\",AUTOSELECT=YES,URI=\"fr.m3u8\"\n",
        );
        assert!(find(&issues, "§5.8").is_none(), "{issues:?}");
        assert!(find(&issues, "§5.11").is_none(), "{issues:?}");
    }
}
