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
}
