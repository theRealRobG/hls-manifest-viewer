//! Apple HLS Authoring Specification checks (the "Author" section).

#![allow(clippy::collapsible_if)]

mod context;
mod helpers;
mod profile;
mod run;
mod severity;

mod rules_a11y_subs;
mod rules_ads;
mod rules_audio;
mod rules_deep;
mod rules_delivery_privacy_security;
mod rules_llhls;
mod rules_media_playlist;
mod rules_multivariant;
mod rules_protection;
mod rules_segmentation;
mod rules_shareplay_spatial;
mod rules_trickplay;
mod rules_video;

pub use context::AuthoringContext;
pub use profile::AuthorProfile;
pub use run::{run_author_report, AuthorOptions, AuthorReport};

use crate::utils::validator::types::Issue;

/// Run all Authoring Spec checks against the provided context.
pub fn run_authoring_checks(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    issues.extend(rules_video::check(ctx));
    issues.extend(rules_audio::check(ctx));
    issues.extend(rules_ads::check(ctx));
    issues.extend(rules_a11y_subs::check(ctx));
    issues.extend(rules_trickplay::check(ctx));
    issues.extend(rules_segmentation::check(ctx));
    issues.extend(rules_media_playlist::check(ctx));
    issues.extend(rules_multivariant::check(ctx));
    issues.extend(rules_delivery_privacy_security::check(ctx));
    issues.extend(rules_protection::check(ctx));
    issues.extend(rules_llhls::check(ctx));
    issues.extend(rules_shareplay_spatial::check(ctx));
    issues.extend(rules_deep::check(ctx));
    // Rules walk hash maps internally, so the order findings arrive in is not
    // stable across runs. Sort worst-first, then by message, so the same stream
    // always produces the same report.
    issues.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.message.cmp(&b.message)));
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::validator::parser::parse_master_playlist;
    use crate::utils::validator::types::*;
    use context::{InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample};
    use profile::AuthorPolicy;

    fn master_from(content: &str) -> MasterPlaylist {
        parse_master_playlist("https://example.com/master.m3u8", content)
    }

    fn segment(uri: &str, duration: f64) -> Segment {
        Segment {
            uri: uri.into(),
            duration,
            title: None,
            pdt: None,
            discontinuity: false,
            byterange: None,
            is_ad: false,
            map_uri: None,
        }
    }

    fn sample(
        playlist: &str,
        segment_index: usize,
        extinf_s: f64,
        bytes: usize,
        scan: crate::utils::mp4_probe::SegmentScan,
    ) -> SegmentSample {
        SegmentSample::from_scan(
            playlist.to_string(),
            segment_index,
            format!("https://example.com/{segment_index}.m4s"),
            extinf_s,
            bytes,
            false,
            &scan,
        )
    }

    /// VOD video variant of a demuxed ladder, with `count` two-second segments.
    fn demuxed_video_playlist(count: usize) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new("video/1280x720".into(), "https://example.com/v.m3u8".into());
        pl.media_type = "VIDEO".into();
        pl.audio_group = Some("aud".into());
        pl.target_duration = 2.0;
        pl.has_endlist = true;
        pl.playlist_type = Some("VOD".into());
        for i in 0..count {
            pl.segments.push(segment(&format!("{i}.m4s"), 2.0));
        }
        pl
    }

    fn deep_opts() -> ValidateAuthorOptions {
        ValidateAuthorOptions {
            deep_checks: true,
            ..Default::default()
        }
    }

    fn issue_for<'a>(issues: &'a [Issue], section: &str) -> Option<&'a Issue> {
        issues.iter().find(|i| i.message.contains(section))
    }

    #[test]
    fn author_9_14_requires_average_bandwidth() {
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,CODECS="avc1.4d401e,mp4a.40.2"
https://example.com/a.m3u8
"#,
        );
        let playlists: Vec<MediaPlaylist> = Vec::new();
        let opts = ValidateAuthorOptions::default();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        assert!(
            issues.iter().any(|i| i.message.contains("§9.14")),
            "expected §9.14 issue, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_1_1_rejects_unknown_video_codec() {
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=800000,AVERAGE-BANDWIDTH=700000,RESOLUTION=640x360,CODECS="vp09.00.10.08,mp4a.40.2",FRAME-RATE=30
https://example.com/a.m3u8
"#,
        );
        let playlists = Vec::new();
        let opts = ValidateAuthorOptions::default();
        let inits = Vec::new();
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        assert!(issues.iter().any(|i| i.message.contains("§1.1")));
    }

    #[test]
    fn author_7_7_extinf_exceeds_td() {
        let mut pl = MediaPlaylist::new("v".into(), "https://example.com/v.m3u8".into());
        pl.target_duration = 6.0;
        pl.has_endlist = true;
        pl.playlist_type = Some("VOD".into());
        pl.segments.push(Segment {
            uri: "a.ts".into(),
            duration: 7.0,
            title: None,
            pdt: None,
            discontinuity: false,
            byterange: None,
            is_ad: false,
            map_uri: None,
        });
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=800000,AVERAGE-BANDWIDTH=700000,RESOLUTION=640x360,CODECS="avc1.4d401e,mp4a.40.2",FRAME-RATE=30
https://example.com/v.m3u8
"#,
        );
        let playlists = vec![pl];
        let opts = ValidateAuthorOptions::default();
        let inits = Vec::new();
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        assert!(issues.iter().any(|i| i.message.contains("§7.7")));
    }

    #[test]
    fn policy_unused_ok() {
        let _ = AuthorPolicy::for_profile(AuthorProfile::None, false);
    }

    #[test]
    fn author_8_20_errors_when_fmp4_init_lacks_map() {
        use crate::utils::mp4_probe::InitSegmentProbe;
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=800000,AVERAGE-BANDWIDTH=700000,RESOLUTION=640x360,CODECS="hvc1.1.6.L93.B0,mp4a.40.2",FRAME-RATE=30
https://example.com/v.m3u8
"#,
        );
        let mut pl = MediaPlaylist::new("video/640x360 · 800k".into(), "https://example.com/v.m3u8".into());
        pl.media_type = "VIDEO".into();
        pl.codecs = Some("hvc1.1.6.L93.B0,mp4a.40.2".into());
        pl.segments.push(Segment {
            uri: "a.m4s".into(),
            duration: 6.0,
            title: None,
            pdt: None,
            discontinuity: false,
            byterange: None,
            is_ad: false,
            map_uri: None,
        });
        let probe = InitSegmentProbe {
            major_brand: Some("iso9".into()),
            compatible_brands: vec!["iso6".into(), "cmfc".into()],
            video_sample_fourcc: Some("hvc1".into()),
            ..Default::default()
        };
        let inits = vec![InitProbeEntry {
            uri: "https://example.com/init.mp4".into(),
            byterange: None,
            playlist_names: vec![pl.name.clone()],
            media_types: vec!["VIDEO".into()],
            probe,
        }];
        let playlists = vec![pl];
        let opts = ValidateAuthorOptions::default();
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        assert!(
            issues.iter().any(|i| i.message.contains("§8.20") && i.message.contains("fMP4 init")),
            "expected §8.20 fMP4 MAP error, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_5_3_requires_x_timestamp_map() {
        let master = master_from(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English",LANGUAGE="en",URI="subs.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=800000,AVERAGE-BANDWIDTH=700000,RESOLUTION=640x360,CODECS="avc1.4d401e,mp4a.40.2",FRAME-RATE=30,SUBTITLES="subs"
https://example.com/v.m3u8
"#,
        );
        let playlists = Vec::new();
        let opts = ValidateAuthorOptions::default();
        let inits = Vec::new();
        let segs = Vec::new();
        let vtts = vec![WebVttSample {
            playlist_name: "subtitles/English (subs)".into(),
            uri: "https://example.com/a.vtt".into(),
            has_webvtt_header: true,
            has_x_timestamp_map: false,
        }];
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        assert!(
            issues.iter().any(|i| i.message.contains("§5.3") && i.message.contains("X-TIMESTAMP-MAP")),
            "expected §5.3 issue, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_1_24_accepts_hevc_sdr_ladder() {
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=6000000,AVERAGE-BANDWIDTH=5000000,RESOLUTION=1920x1080,CODECS="hvc1.2.4.L123.B0",VIDEO-RANGE=PQ,FRAME-RATE=30
https://example.com/hdr.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS="hvc1.1.6.L93.B0",VIDEO-RANGE=SDR,FRAME-RATE=30
https://example.com/sdr.m3u8
"#,
        );
        let playlists = Vec::new();
        let opts = ValidateAuthorOptions::default();
        let inits = Vec::new();
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        assert!(
            !issues.iter().any(|i| i.message.contains("§1.24")),
            "HEVC SDR variant should satisfy §1.24, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_1_24_flags_hdr_only_ladder() {
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=6000000,AVERAGE-BANDWIDTH=5000000,RESOLUTION=1920x1080,CODECS="hvc1.2.4.L123.B0",VIDEO-RANGE=PQ,FRAME-RATE=30
https://example.com/hdr.m3u8
"#,
        );
        let playlists = Vec::new();
        let opts = ValidateAuthorOptions::default();
        let inits = Vec::new();
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        assert!(
            issues.iter().any(|i| i.message.contains("§1.24")),
            "expected §1.24 issue, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_1_26_flags_vod_avg_bitrate_drift() {
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=640x360,CODECS="avc1.4d401e,mp4a.40.2",FRAME-RATE=30
https://example.com/v.m3u8
"#,
        );
        let mut pl = MediaPlaylist::new("video/640x360 · 1000k".into(), "https://example.com/v.m3u8".into());
        pl.media_type = "VIDEO".into();
        pl.bandwidth = Some(1_000_000);
        pl.average_bandwidth = Some(800_000);
        pl.has_endlist = true;
        pl.playlist_type = Some("VOD".into());
        let segs = vec![
            SegmentSample {
                playlist_name: pl.name.clone(),
                segment_index: 0,
                uri: "https://example.com/0.m4s".into(),
                extinf_s: 2.0,
                // 800kbps * 2s = 1_600_000 bits = 200_000 bytes would match; use 400_000 bytes → 1.6 Mbps
                bytes: 400_000,
                is_iframe_playlist: false,
                looks_like_fmp4: true,
                has_moof: true,
                has_idr_nal_hint: true,
                idr_at_start: true,
                idr_count: 1,
                has_tfdt: true,
                tfdt_base_media_decode_time: Some(0),
                looks_like_ts: false,
                has_senc: false,
                has_saiz: false,
                has_saio: false,
                ts_continuity_ok: None,
                has_cc_sei_hint: false,
                has_asp_hint: false,
            },
            SegmentSample {
                playlist_name: pl.name.clone(),
                segment_index: 1,
                uri: "https://example.com/1.m4s".into(),
                extinf_s: 2.0,
                bytes: 400_000,
                is_iframe_playlist: false,
                looks_like_fmp4: true,
                has_moof: true,
                has_idr_nal_hint: true,
                idr_at_start: true,
                idr_count: 1,
                has_tfdt: true,
                tfdt_base_media_decode_time: Some(60_000),
                looks_like_ts: false,
                has_senc: false,
                has_saiz: false,
                has_saio: false,
                ts_continuity_ok: None,
                has_cc_sei_hint: false,
                has_asp_hint: false,
            },
        ];
        let opts = ValidateAuthorOptions {
            deep_checks: true,
            ..Default::default()
        };
        let playlists = vec![pl];
        let inits = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        assert!(
            issues.iter().any(|i| i.message.contains("§1.26")),
            "expected §1.26 issue, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    fn demuxed_master() -> MasterPlaylist {
        master_from(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="English",LANGUAGE="en",DEFAULT=YES,CHANNELS="2",URI="a.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f,mp4a.40.2",FRAME-RATE=30,AUDIO="aud"
https://example.com/v.m3u8
"#,
        )
    }

    /// Video-only fMP4 init, which is what proves a variant carries no muxed audio.
    fn video_only_init(playlist_name: &str) -> InitProbeEntry {
        use crate::utils::mp4_probe::InitSegmentProbe;
        InitProbeEntry {
            uri: "https://example.com/v-init.mp4".into(),
            byterange: None,
            playlist_names: vec![playlist_name.to_string()],
            media_types: vec!["VIDEO".into()],
            probe: InitSegmentProbe {
                major_brand: Some("iso6".into()),
                compatible_brands: vec!["cmfc".into()],
                video_sample_fourcc: Some("avc1".into()),
                timescale: Some(90_000),
                ..Default::default()
            },
        }
    }

    fn audio_playlist() -> MediaPlaylist {
        let mut pl = MediaPlaylist::new("audio/English (aud)".into(), "https://example.com/a.m3u8".into());
        pl.media_type = "AUDIO".into();
        pl.group_id = Some("aud".into());
        pl.target_duration = 2.0;
        pl.has_endlist = true;
        pl.segments.push(segment("a0.m4s", 2.0));
        pl.segments.push(segment("a1.m4s", 2.0));
        pl
    }

    #[test]
    fn author_1_26_adds_demuxed_audio_to_measured_avg() {
        let master = demuxed_master();
        let mut video = demuxed_video_playlist(2);
        video.bandwidth = Some(1_000_000);
        // Video alone measures 800 kbps — only the audio rendition pushes it out of ±10%.
        video.average_bandwidth = Some(800_000);
        let audio = audio_playlist();
        let segs = vec![
            sample(&video.name, 0, 2.0, 200_000, Default::default()),
            sample(&video.name, 1, 2.0, 200_000, Default::default()),
            sample(&audio.name, 0, 2.0, 25_000, Default::default()),
            sample(&audio.name, 1, 2.0, 25_000, Default::default()),
        ];
        let inits = vec![video_only_init(&video.name)];
        let playlists = vec![video, audio];
        let opts = deep_opts();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        let found = issue_for(&issues, "§1.26").unwrap_or_else(|| {
            panic!(
                "expected §1.26 issue, got: {:?}",
                issues.iter().map(|i| &i.message).collect::<Vec<_>>()
            )
        });
        assert_eq!(found.severity, Severity::Error);
        assert!(
            found.message.contains("including audio") && found.message.contains("900000 bps"),
            "expected combined rate with audio, got: {}",
            found.message
        );
    }

    #[test]
    fn author_1_26_warns_when_demuxed_audio_was_not_sampled() {
        let master = demuxed_master();
        let mut video = demuxed_video_playlist(2);
        video.bandwidth = Some(1_000_000);
        video.average_bandwidth = Some(600_000);
        let segs = vec![
            sample(&video.name, 0, 2.0, 200_000, Default::default()),
            sample(&video.name, 1, 2.0, 200_000, Default::default()),
        ];
        let inits = vec![video_only_init(&video.name)];
        let playlists = vec![video, audio_playlist()];
        let opts = deep_opts();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        let found = issue_for(&issues, "§1.26").unwrap_or_else(|| {
            panic!(
                "expected §1.26 issue, got: {:?}",
                issues.iter().map(|i| &i.message).collect::<Vec<_>>()
            )
        });
        assert_eq!(found.severity, Severity::Warn);
        assert!(
            found.message.contains("video only"),
            "expected a video-only note, got: {}",
            found.message
        );
    }

    #[test]
    fn author_1_26_warns_when_sample_is_a_short_prefix() {
        let master = demuxed_master();
        let mut video = demuxed_video_playlist(50);
        video.bandwidth = Some(1_000_000);
        video.average_bandwidth = Some(700_000);
        let audio = audio_playlist();
        let segs = vec![
            sample(&video.name, 0, 2.0, 200_000, Default::default()),
            sample(&video.name, 1, 2.0, 200_000, Default::default()),
            sample(&audio.name, 0, 2.0, 25_000, Default::default()),
        ];
        let inits = vec![video_only_init(&video.name)];
        let playlists = vec![video, audio];
        let opts = deep_opts();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        let found = issue_for(&issues, "§1.26").unwrap_or_else(|| {
            panic!(
                "expected §1.26 issue, got: {:?}",
                issues.iter().map(|i| &i.message).collect::<Vec<_>>()
            )
        });
        assert_eq!(found.severity, Severity::Warn);
        assert!(
            found.message.contains("sampled 2 of 50 segments, #0–#1"),
            "expected the sample window in the message, got: {}",
            found.message
        );
    }

    /// Run the rules over a master with no media playlists or probes.
    fn master_only_issues(content: &str, profile: AuthorProfile) -> Vec<Issue> {
        let master = master_from(content);
        let playlists = Vec::new();
        let opts = ValidateAuthorOptions {
            profile,
            deep_checks: false,
        };
        let inits = Vec::new();
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        run_authoring_checks(&ctx)
    }

    fn expect_issue<'a>(issues: &'a [Issue], section: &str) -> &'a Issue {
        issue_for(issues, section).unwrap_or_else(|| {
            panic!(
                "expected {section} issue, got: {:?}",
                issues.iter().map(|i| &i.message).collect::<Vec<_>>()
            )
        })
    }

    #[test]
    fn issues_come_back_worst_first_then_by_message() {
        use std::cmp::Reverse;
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="English",LANGUAGE="en",DEFAULT=YES,CHANNELS="6"
#EXT-X-STREAM-INF:BANDWIDTH=1000000,RESOLUTION=1280x720,CODECS="avc1.4d401f,ec-3",AUDIO="aud"
https://example.com/v720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=500000,RESOLUTION=640x360,CODECS="avc1.4d401e,ec-3",AUDIO="aud"
https://example.com/v360.m3u8
"#,
            AuthorProfile::None,
        );
        assert!(issues.len() > 1, "need several issues to compare ordering");
        let keys: Vec<_> = issues
            .iter()
            .map(|i| (Reverse(i.severity), i.message.as_str()))
            .collect();
        assert!(
            keys.windows(2).all(|w| w[0] <= w[1]),
            "issues must be sorted, got: {:?}",
            issues
                .iter()
                .map(|i| (i.severity, &i.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_9_15_frame_rate_is_an_error() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f"
https://example.com/v720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=500000,AVERAGE-BANDWIDTH=400000,RESOLUTION=640x360,CODECS="avc1.4d401e"
https://example.com/v360.m3u8
"#,
            AuthorProfile::None,
        );
        let found = expect_issue(&issues, "§9.15");
        assert_eq!(found.severity, Severity::Error);
    }

    #[test]
    fn author_6_1_missing_iframe_playlists_is_an_error() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f",FRAME-RATE=30
https://example.com/v720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=500000,AVERAGE-BANDWIDTH=400000,RESOLUTION=640x360,CODECS="avc1.4d401e",FRAME-RATE=30
https://example.com/v360.m3u8
"#,
            AuthorProfile::None,
        );
        let found = expect_issue(&issues, "§6.1:");
        assert_eq!(found.severity, Severity::Error);
    }

    /// A subtitle rendition missing LANGUAGE breaks both §4.7 and §8.10. §4.7 is the more
    /// specific section, so it owns the finding and §8.10 stays quiet.
    #[test]
    fn missing_subtitle_language_is_reported_once_under_4_7() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English",AUTOSELECT=YES,URI="https://example.com/subs.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f",FRAME-RATE=30,SUBTITLES="subs"
https://example.com/v720.m3u8
"#,
            AuthorProfile::None,
        );
        let language_issues: Vec<&String> = issues
            .iter()
            .map(|i| &i.message)
            .filter(|m| m.contains("MUST have LANGUAGE"))
            .collect();
        assert_eq!(language_issues.len(), 1, "{language_issues:?}");
        assert!(language_issues[0].contains("§4.7"), "{language_issues:?}");
    }

    /// §8.10 still owns audio renditions, which no narrower section covers.
    #[test]
    fn missing_audio_language_is_still_reported_under_8_10() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="English",DEFAULT=YES,CHANNELS="2",URI="https://example.com/a.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f,mp4a.40.2",FRAME-RATE=30,AUDIO="aud"
https://example.com/v720.m3u8
"#,
            AuthorProfile::None,
        );
        let language_issues: Vec<&String> = issues
            .iter()
            .map(|i| &i.message)
            .filter(|m| m.contains("MUST have LANGUAGE"))
            .collect();
        assert_eq!(language_issues.len(), 1, "{language_issues:?}");
        assert!(language_issues[0].contains("§8.10"), "{language_issues:?}");
    }

    /// Descriptive audio missing LANGUAGE breaks §2.27 and §8.10; §2.27 is the narrower rule.
    #[test]
    fn missing_descriptive_audio_language_is_reported_once_under_2_27() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="English AD",AUTOSELECT=YES,CHARACTERISTICS="public.accessibility.describes-video",URI="https://example.com/dvs.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f,mp4a.40.2",FRAME-RATE=30,AUDIO="aud"
https://example.com/v720.m3u8
"#,
            AuthorProfile::None,
        );
        let language_issues: Vec<&String> = issues
            .iter()
            .map(|i| &i.message)
            .filter(|m| m.contains("MUST have LANGUAGE"))
            .collect();
        assert_eq!(language_issues.len(), 1, "{language_issues:?}");
        assert!(language_issues[0].contains("§2.27"), "{language_issues:?}");
    }

    #[test]
    fn author_9_9_single_bitrate_is_an_error() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f",FRAME-RATE=30
https://example.com/v720.m3u8
"#,
            AuthorProfile::None,
        );
        assert_eq!(expect_issue(&issues, "§9.9").severity, Severity::Error);
    }

    #[test]
    fn author_9_6_multichannel_audio_without_its_own_stream_is_an_error() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="English",LANGUAGE="en",DEFAULT=YES,CHANNELS="6"
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f,ec-3",FRAME-RATE=30,AUDIO="aud"
https://example.com/v720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=500000,AVERAGE-BANDWIDTH=400000,RESOLUTION=640x360,CODECS="avc1.4d401e,ec-3",FRAME-RATE=30,AUDIO="aud"
https://example.com/v360.m3u8
"#,
            AuthorProfile::None,
        );
        assert_eq!(expect_issue(&issues, "§9.6").severity, Severity::Error);
    }

    #[test]
    fn author_8_3_compares_every_playlist_not_just_the_first_pair() {
        let master = demuxed_master();
        // 8 s of video against 4 s of audio — only caught if audio is compared to video.
        let video = demuxed_video_playlist(4);
        let playlists = vec![video, audio_playlist()];
        let opts = ValidateAuthorOptions::default();
        let inits = Vec::new();
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        let found = expect_issue(&issues, "§8.3");
        assert_eq!(found.severity, Severity::Error);
        assert!(
            found.message.contains("8.0s"),
            "expected the video duration in the message, got: {}",
            found.message
        );
    }

    #[test]
    fn author_9_12_reports_each_playlist_missing_independent_segments() {
        let master = demuxed_master();
        let video = demuxed_video_playlist(2);
        let playlists = vec![video, audio_playlist()];
        let opts = ValidateAuthorOptions::default();
        let inits = Vec::new();
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        let found = expect_issue(&issues, "§9.12");
        assert_eq!(found.severity, Severity::Error);
        assert!(
            found.message.contains("video/1280x720"),
            "expected the offending playlist name, got: {}",
            found.message
        );
        assert!(
            !issues.iter().any(|i| i.message.contains("§9.11")),
            "§9.11 was a mislabel of §9.12 and should no longer be emitted"
        );
    }

    fn idr_issues(scan: crate::utils::mp4_probe::SegmentScan) -> Vec<Issue> {
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f",FRAME-RATE=30
https://example.com/v.m3u8
"#,
        );
        let mut video = demuxed_video_playlist(2);
        video.audio_group = None;
        let segs = vec![
            sample(&video.name, 0, 2.0, 200_000, scan.clone()),
            sample(&video.name, 1, 2.0, 200_000, scan),
        ];
        let playlists = vec![video];
        let opts = deep_opts();
        let inits = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        run_authoring_checks(&ctx)
            .into_iter()
            .filter(|i| i.message.contains("§7.4"))
            .collect()
    }

    #[test]
    fn author_7_4_reports_missing_idr_on_clear_video() {
        use crate::utils::mp4_probe::SegmentScan;
        let issues = idr_issues(SegmentScan {
            looks_like_fmp4: true,
            has_moof: true,
            ..Default::default()
        });
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Warn && i.message.contains("no IDR NAL detected")),
            "expected a §7.4 warning, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_7_4_skips_idr_checks_on_encrypted_video() {
        use crate::utils::mp4_probe::SegmentScan;
        let issues = idr_issues(SegmentScan {
            looks_like_fmp4: true,
            has_moof: true,
            has_senc: true,
            ..Default::default()
        });
        assert!(
            issues.iter().all(|i| i.severity == Severity::Info),
            "encrypted samples must not raise §7.4 findings, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
        assert!(
            issues.iter().any(|i| i.message.contains("encrypted")),
            "expected an informational note about encryption, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_1_9_flags_dolby_vision_profile_8() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=20000000,AVERAGE-BANDWIDTH=18000000,RESOLUTION=3840x2160,CODECS="dvh1.08.07,ec-3",VIDEO-RANGE=PQ,FRAME-RATE=23.976
https://example.com/dv.m3u8
"#,
            AuthorProfile::None,
        );
        let found = expect_issue(&issues, "§1.9");
        assert_eq!(found.severity, Severity::Error);
        assert!(
            found.message.contains("profile 8"),
            "expected the offending profile in the message, got: {}",
            found.message
        );
    }

    #[test]
    fn author_1_9_flags_dolby_vision_level_above_7() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=20000000,AVERAGE-BANDWIDTH=18000000,RESOLUTION=3840x2160,CODECS="dvh1.05.09,ec-3",VIDEO-RANGE=PQ,FRAME-RATE=23.976
https://example.com/dv.m3u8
"#,
            AuthorProfile::None,
        );
        assert!(
            issues
                .iter()
                .any(|i| i.message.contains("§1.9") && i.message.contains("level 9")),
            "expected a §1.9 level error, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_1_9_accepts_dolby_vision_profile_5_level_6() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=20000000,AVERAGE-BANDWIDTH=18000000,RESOLUTION=3840x2160,CODECS="dvh1.05.06,ec-3",VIDEO-RANGE=PQ,FRAME-RATE=23.976
https://example.com/dv.m3u8
"#,
            AuthorProfile::None,
        );
        assert!(
            !issues.iter().any(|i| i.message.contains("§1.9")),
            "profile 5 level 6 must satisfy §1.9, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_1_3b_flags_avc_codecs_level_above_platform_max() {
        // avc1.640034 is High Profile level 5.2; tvOS caps H.264 at level 5.1.
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=12000000,AVERAGE-BANDWIDTH=10000000,RESOLUTION=3840x2160,CODECS="avc1.640034,mp4a.40.2",FRAME-RATE=30
https://example.com/v.m3u8
"#,
            AuthorProfile::Tvos,
        );
        let found = expect_issue(&issues, "§1.3b");
        assert_eq!(found.severity, Severity::Error);
        assert!(
            found.message.contains("level 5.2") && found.message.contains("max 5.1"),
            "expected the measured and allowed levels, got: {}",
            found.message
        );
    }

    #[test]
    fn author_1_3b_accepts_avc_codecs_level_at_platform_max() {
        // avc1.640033 is level 5.1, exactly the tvOS ceiling.
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=12000000,AVERAGE-BANDWIDTH=10000000,RESOLUTION=3840x2160,CODECS="avc1.640033,mp4a.40.2",FRAME-RATE=30
https://example.com/v.m3u8
"#,
            AuthorProfile::Tvos,
        );
        assert!(
            !issues.iter().any(|i| i.message.contains("§1.3b")),
            "level 5.1 must satisfy the tvOS ceiling, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_1_3b_reports_codecs_level_once_when_init_agrees() {
        use crate::utils::mp4_probe::InitSegmentProbe;
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=12000000,AVERAGE-BANDWIDTH=10000000,RESOLUTION=3840x2160,CODECS="avc1.640034",FRAME-RATE=30
https://example.com/v.m3u8
"#,
        );
        let mut pl = demuxed_video_playlist(2);
        pl.audio_group = None;
        let inits = vec![InitProbeEntry {
            uri: "https://example.com/v-init.mp4".into(),
            byterange: None,
            playlist_names: vec![pl.name.clone()],
            media_types: vec!["VIDEO".into()],
            probe: InitSegmentProbe {
                major_brand: Some("iso6".into()),
                compatible_brands: vec!["cmfc".into()],
                video_sample_fourcc: Some("avc1".into()),
                video_profile: Some("100".into()),
                video_level: Some("52".into()),
                ..Default::default()
            },
        }];
        let playlists = vec![pl];
        let opts = ValidateAuthorOptions {
            profile: AuthorProfile::Tvos,
            deep_checks: false,
        };
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        let issues = run_authoring_checks(&ctx);
        let level_issues: Vec<_> = issues
            .iter()
            .filter(|i| i.message.contains("§1.3b"))
            .collect();
        assert_eq!(
            level_issues.len(),
            1,
            "init and CODECS must not both report level 5.2, got: {:?}",
            level_issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
        assert!(
            level_issues[0].message.contains("init"),
            "the init probe stays the primary source, got: {}",
            level_issues[0].message
        );
    }

    #[test]
    fn author_1_6b_flags_hevc_codecs_that_is_not_main10() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS="hvc1.1.6.L93.B0",VIDEO-RANGE=SDR,FRAME-RATE=30
https://example.com/sdr.m3u8
"#,
            AuthorProfile::None,
        );
        assert!(
            issues
                .iter()
                .any(|i| i.message.contains("§1.6b") && i.message.contains("Main 10")),
            "expected a §1.6b Main 10 error, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_1_6b_flags_hevc_codecs_level_above_5_1() {
        let issues = master_only_issues(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=30000000,AVERAGE-BANDWIDTH=25000000,RESOLUTION=3840x2160,CODECS="hvc1.2.4.L180.B0",VIDEO-RANGE=PQ,FRAME-RATE=30
https://example.com/uhd.m3u8
"#,
            AuthorProfile::None,
        );
        assert!(
            issues
                .iter()
                .any(|i| i.message.contains("§1.6b") && i.message.contains("level 6.0")),
            "expected a §1.6b level error, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    fn protection_issues(probe: crate::utils::mp4_probe::InitSegmentProbe) -> Vec<Issue> {
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f",FRAME-RATE=30
https://example.com/v.m3u8
"#,
        );
        let playlists = Vec::new();
        let opts = ValidateAuthorOptions::default();
        let inits = vec![InitProbeEntry {
            uri: "https://example.com/init.mp4".into(),
            byterange: None,
            playlist_names: vec!["video/1280x720".into()],
            media_types: vec!["VIDEO".into()],
            probe,
        }];
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        run_authoring_checks(&ctx)
            .into_iter()
            .filter(|i| i.message.contains("§13.7"))
            .collect()
    }

    #[test]
    fn author_13_7_ignores_cenc_audio_init_without_pattern() {
        use crate::utils::mp4_probe::InitSegmentProbe;
        let issues = protection_issues(InitSegmentProbe {
            audio_sample_fourcc: Some("mp4a".into()),
            scheme_type: Some("cenc".into()),
            has_tenc: true,
            ..Default::default()
        });
        assert!(
            issues.is_empty(),
            "cenc without a pattern is not a §13.7 finding, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_13_7_flags_cbcs_video_pattern_other_than_1_9() {
        use crate::utils::mp4_probe::InitSegmentProbe;
        let issues = protection_issues(InitSegmentProbe {
            video_sample_fourcc: Some("hvc1".into()),
            scheme_type: Some("cbcs".into()),
            has_tenc: true,
            has_video_tenc: true,
            crypt_byte_block: Some(2),
            skip_byte_block: Some(8),
            video_crypt_byte_block: Some(2),
            video_skip_byte_block: Some(8),
            ..Default::default()
        });
        assert!(
            issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("2:8")),
            "expected a §13.7 error for a 2:8 cbcs video pattern, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    /// 720p VOD variant carrying the given EXT-X-KEY line(s), parsed like a fetched playlist.
    fn key_tag_issues(key_lines: &str) -> Vec<Issue> {
        use crate::utils::validator::parser::parse_media_playlist;
        let master = master_from(
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=1000000,AVERAGE-BANDWIDTH=800000,RESOLUTION=1280x720,CODECS="avc1.4d401f",FRAME-RATE=30,HDCP-LEVEL=TYPE-0
https://example.com/v.m3u8
"#,
        );
        let content = format!(
            "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:6\n#EXT-X-PLAYLIST-TYPE:VOD\n\
             {key_lines}\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:6.00000,\n0.m4s\n#EXT-X-ENDLIST\n"
        );
        let mut pl = MediaPlaylist::new("video/1280x720".into(), "https://example.com/v.m3u8".into());
        parse_media_playlist("https://example.com/v.m3u8", &content, &mut pl);
        pl.media_type = "VIDEO".into();
        pl.resolution = Some("1280x720".into());
        pl.hdcp_level = Some("TYPE-0".into());
        let playlists = vec![pl];
        let opts = ValidateAuthorOptions::default();
        let inits = Vec::new();
        let segs = Vec::new();
        let vtts = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        run_authoring_checks(&ctx)
            .into_iter()
            .filter(|i| i.message.contains("§13."))
            .collect()
    }

    #[test]
    fn author_13_2_flags_fairplay_key_with_aes_128() {
        let issues = key_tag_issues(
            r#"#EXT-X-KEY:METHOD=AES-128,URI="skd://abc123",KEYFORMAT="com.apple.streamingkeydelivery",KEYFORMATVERSIONS="1""#,
        );
        let found = issue_for(&issues, "§13.2").unwrap_or_else(|| {
            panic!(
                "expected §13.2 issue, got: {:?}",
                issues.iter().map(|i| &i.message).collect::<Vec<_>>()
            )
        });
        assert_eq!(found.severity, Severity::Error);
        assert!(
            found.message.contains("METHOD=AES-128"),
            "expected the offending METHOD in the message, got: {}",
            found.message
        );
        assert!(
            issue_for(&issues, "§13.3").is_none(),
            "KEYFORMAT is correct, so §13.3 must stay quiet: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_13_11_labels_sample_aes_ctr() {
        let issues = key_tag_issues(
            r#"#EXT-X-KEY:METHOD=SAMPLE-AES-CTR,URI="skd://abc123",KEYFORMAT="com.apple.streamingkeydelivery""#,
        );
        let found = issue_for(&issues, "§13.11").unwrap_or_else(|| {
            panic!(
                "expected §13.11 issue, got: {:?}",
                issues.iter().map(|i| &i.message).collect::<Vec<_>>()
            )
        });
        assert_eq!(found.severity, Severity::Error);
        assert!(
            issue_for(&issues, "§13.5").is_none(),
            "SAMPLE-AES-CTR is §13.11, not §13.5: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_13_3_and_13_4_flag_fairplay_uri_without_keyformat() {
        let issues =
            key_tag_issues(r#"#EXT-X-KEY:METHOD=SAMPLE-AES,URI="skd://abc123",IV=0x0123456789ABCDEF0123456789ABCDEF"#);
        let keyformat = issue_for(&issues, "§13.3").unwrap_or_else(|| {
            panic!(
                "expected §13.3 issue, got: {:?}",
                issues.iter().map(|i| &i.message).collect::<Vec<_>>()
            )
        });
        assert_eq!(keyformat.severity, Severity::Error);
        let iv = issue_for(&issues, "§13.4").unwrap_or_else(|| {
            panic!(
                "expected §13.4 issue, got: {:?}",
                issues.iter().map(|i| &i.message).collect::<Vec<_>>()
            )
        });
        assert_eq!(iv.severity, Severity::Warn);
    }

    #[test]
    fn author_13_2_accepts_well_formed_fairplay_key() {
        let issues = key_tag_issues(
            r#"#EXT-X-KEY:METHOD=SAMPLE-AES,URI="skd://abc123",KEYFORMAT="com.apple.streamingkeydelivery",KEYFORMATVERSIONS="1""#,
        );
        assert!(
            issues.is_empty(),
            "a conforming FairPlay key must not raise §13 findings, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }
}
