//! Apple Authoring Spec §6 — Trick play / I-frame playlists.

use super::context::AuthoringContext;
use super::helpers::*;
use super::severity::{must, should};
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        return issues;
    };

    let iframes: Vec<_> = master.variants.iter().filter(|v| v.is_iframe).collect();
    let videos: Vec<_> = master.variants.iter().filter(|v| !v.is_iframe).collect();

    // §6.1 — at least one I-FRAME-STREAM-INF
    if iframes.is_empty() && !videos.is_empty() {
        issues.push(author_issue(
            must(),
            "6.1",
            "I-frame playlists MUST be provided: no EXT-X-I-FRAME-STREAM-INF entries for trick play",
        ));
        return issues;
    }

    // §6.8 — I-frame media playlists contain EXT-X-I-FRAMES-ONLY
    for pl in ctx.iframe_playlists() {
        if !pl.iframes_only {
            issues.push(author_error(
                "6.8",
                format!(
                    "I-frame playlist '{}' missing EXT-X-I-FRAMES-ONLY",
                    pl.name
                ),
            ));
        }
    }

    // §6.5 — an I-frame rendition is one picture per second, so it should sit well below
    // the bit rate of the video rendition it shadows.
    for iframe in &iframes {
        if let (Some(ibw), Some(res)) = (iframe.bandwidth, iframe.resolution.as_deref()) {
            // Find matching video resolution
            if let Some(video) = videos.iter().find(|v| v.resolution.as_deref() == Some(res)) {
                if let Some(vbw) = video.bandwidth {
                    if ibw > vbw {
                        issues.push(author_issue(
                            should(),
                            "6.5",
                            format!(
                                "I-frame BANDWIDTH {ibw} exceeds matching video BANDWIDTH {vbw} at {res}"
                            ),
                        ));
                    }
                }
            }
        }
    }

    // §6.6 — more than one I-frame bit rate lets trick play adapt like the video ladder.
    if iframes.len() == 1 && videos.len() > 2 {
        issues.push(author_issue(
            should(),
            "6.6",
            "only one I-frame bitrate; multiple I-frame renditions are recommended",
        ));
    }

    // §6.13 — codec pairing; AirPlay MUST match
    for iframe in &iframes {
        let ic = iframe.codecs.as_deref().unwrap_or("");
        let ifam = codec_tokens(ic)
            .into_iter()
            .find_map(video_codec_family);
        if let Some(fam) = ifam {
            if fam == "mjpg" {
                continue; // §6.17 mjpg allowed
            }
            let video_has = videos.iter().any(|v| {
                v.codecs.as_deref().is_some_and(|c| {
                    codec_tokens(c)
                        .iter()
                        .any(|t| video_codec_family(t) == Some(fam))
                })
            });
            if !video_has {
                let sev = if ctx.policy.iframe_codec_match_must {
                    must()
                } else {
                    should()
                };
                issues.push(author_issue(
                    sev,
                    "6.13",
                    format!("I-frame CODECS family '{fam}' has no matching video variants"),
                ));
            }
        }
    }

    // §6.14 — at least one trick-play rendition SHOULD be H.264 for wide device support (*)
    if !ctx.policy.is_exempt("6.14") && iframes.iter().any(|i| i.codecs.is_some()) {
        let has_h264 = iframes.iter().any(|i| {
            i.codecs.as_deref().is_some_and(|c| {
                codec_tokens(c)
                    .iter()
                    .any(|t| video_codec_family(t) == Some("avc"))
            })
        });
        if !has_h264 {
            issues.push(author_issue(
                should(),
                "6.14",
                "no H.264 I-frame playlist; some trick play SHOULD be H.264 for compatibility",
            ));
        }
    }

    // §6.15 — HDR trick play is optional, but once any HDR I-frame rendition exists it
    // SHOULD cover every HDR video resolution. Streams with SDR-only trick play are fine. (*)
    if !ctx.policy.is_exempt("6.15")
        && iframes
            .iter()
            .any(|i| is_hdr_range(i.video_range.as_deref()))
    {
        let hdr_video_res: Vec<_> = videos
            .iter()
            .filter(|v| is_hdr_range(v.video_range.as_deref()))
            .filter_map(|v| v.resolution.as_deref())
            .collect();
        for res in hdr_video_res {
            let has = iframes.iter().any(|i| {
                i.resolution.as_deref() == Some(res) && is_hdr_range(i.video_range.as_deref())
            });
            if !has {
                issues.push(author_issue(
                    should(),
                    "6.15",
                    format!("HDR video at {res} lacks matching HDR I-frame playlist"),
                ));
            }
        }
    }

    if !ctx.policy.is_exempt("6.16") {
        let has_sdr_video = videos.iter().any(|v| !is_hdr_range(v.video_range.as_deref()));
        let has_sdr_iframe = iframes.iter().any(|v| !is_hdr_range(v.video_range.as_deref()));
        if has_sdr_video && !has_sdr_iframe {
            let sev = if ctx.policy.sdr_iframe_must {
                must()
            } else {
                should()
            };
            issues.push(author_issue(
                sev,
                "6.16",
                "SDR video present but no SDR I-frame playlist",
            ));
        }
    }

    // §6.11–6.12 — live I-frame TD == A/V TD; VOD MAY differ
    let video_tds: Vec<f64> = ctx.video_playlists().map(|p| p.target_duration).collect();
    let is_live = ctx
        .video_playlists()
        .any(|p| !p.has_endlist && p.playlist_type.as_deref() != Some("VOD"));
    if is_live {
        for pl in ctx.iframe_playlists() {
            if let Some(&vtd) = video_tds.first() {
                if (pl.target_duration - vtd).abs() > 0.01 {
                    issues.push(author_error(
                        "6.11",
                        format!(
                            "live I-frame TARGETDURATION {:.0} != A/V TARGETDURATION {:.0}",
                            pl.target_duration, vtd
                        ),
                    ));
                }
            }
        }
    }

    // §6.10 — fMP4 I-frame segments MUST include moof
    issues.extend(check_iframe_moof(ctx));

    // §6.18 (visionOS) — trick play content SHOULD be monoscopic and rectilinear. The rule
    // used to ask for the opposite, reporting a compliant monoscopic I-frame ladder next to
    // stereo video and staying quiet about the stereo trick play it should have named.
    if ctx.policy.profile == super::profile::AuthorProfile::VisionOs {
        for iframe in &iframes {
            let Some(layout) = iframe.req_video_layout.as_deref() else {
                continue;
            };
            let mut nonconforming: Vec<&str> = layout
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .filter(|t| !specifier_is_monoscopic_rectilinear(t))
                .collect();
            nonconforming.dedup();
            if nonconforming.is_empty() {
                continue;
            }
            issues.push(author_warn(
                "6.18",
                format!(
                    "trick play '{}' declares REQ-VIDEO-LAYOUT '{}'; trick play content SHOULD be monoscopic and rectilinear ({})",
                    iframe.uri,
                    layout,
                    nonconforming.join(", ")
                ),
            ));
        }
    }

    issues
}

/// Whether one REQ-VIDEO-LAYOUT specifier keeps trick play monoscopic and rectilinear
/// (§6.18). `CH-MONO` is the monoscopic channel specifier and `PROJ-RECT` the rectilinear
/// projection; a stereo channel or any other projection is what the rule is about.
fn specifier_is_monoscopic_rectilinear(specifier: &str) -> bool {
    matches!(
        specifier.to_ascii_uppercase().as_str(),
        "CH-MONO" | "PROJ-RECT"
    )
}

/// §6.10 — an fMP4 I-frame segment MUST carry a moof header.
///
/// I-frame playlists usually address their samples with EXT-X-BYTERANGE, and such a
/// range routinely starts inside the mdat, after the moof of the enclosing fragment.
/// A missing moof is therefore only a MUST violation when a whole segment was read
/// from a playlist that declares EXT-X-MAP; every other case is reported as a warning
/// that names the reason the evidence is inconclusive.
fn check_iframe_moof(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    for sample in ctx
        .segment_samples
        .iter()
        .filter(|s| s.is_iframe_playlist && !s.has_moof && !s.looks_like_ts)
    {
        let pl = ctx
            .iframe_playlists()
            .find(|p| p.name == sample.playlist_name);
        let declares_map = pl.is_some_and(playlist_has_map);
        if !declares_map && !sample.looks_like_fmp4 {
            // Nothing identifies this as fMP4 — it may be a JPEG trick-play sample (§6.17).
            continue;
        }
        let ranged = pl.is_some_and(|p| {
            p.segments
                .get(sample.segment_index)
                .is_some_and(|s| s.byterange.is_some())
        });
        if declares_map && sample.looks_like_fmp4 && !ranged {
            issues.push(author_error(
                "6.10",
                format!("fMP4 I-frame segment '{}' has no moof header", sample.uri),
            ));
        } else {
            let reason = if ranged {
                "the sample was read as an EXT-X-BYTERANGE, which can start after the moof"
            } else if !declares_map {
                "the playlist declares no EXT-X-MAP, so it may not be fMP4"
            } else {
                "the sample body did not look like fMP4"
            };
            issues.push(author_warn(
                "6.10",
                format!(
                    "no moof header found in I-frame segment '{}' — {reason}",
                    sample.uri
                ),
            ));
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
    use crate::utils::mp4_probe::SegmentScan;
    use crate::utils::validator::parser::parse_master_playlist;
    use crate::utils::validator::types::{MasterPlaylist, MediaPlaylist, Segment, Severity};

    fn master() -> MasterPlaylist {
        parse_master_playlist(
            "https://example.com/master.m3u8",
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS="avc1.4d401f",FRAME-RATE=30
https://example.com/v.m3u8
#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=100000,AVERAGE-BANDWIDTH=90000,RESOLUTION=1280x720,CODECS="avc1.4d401f",URI="https://example.com/i.m3u8"
"#,
        )
    }

    /// I-frame playlist whose single segment optionally has an EXT-X-MAP and a BYTERANGE.
    fn iframe_playlist(has_map: bool, byterange: Option<&str>) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new("iframe/1280x720".into(), "https://example.com/i.m3u8".into());
        pl.is_iframe = true;
        pl.iframes_only = true;
        pl.target_duration = 6.0;
        pl.has_endlist = true;
        pl.playlist_type = Some("VOD".into());
        pl.segments.push(Segment {
            uri: "0.m4s".into(),
            duration: 6.0,
            title: None,
            pdt: None,
            discontinuity: false,
            byterange: byterange.map(str::to_string),
            is_ad: false,
            map_uri: has_map.then(|| "init.mp4".to_string()),
        });
        pl
    }

    fn moof_issues(pl: MediaPlaylist, scan: SegmentScan) -> Vec<Issue> {
        let master = master();
        let sample = SegmentSample::from_scan(
            pl.name.clone(),
            0,
            "https://example.com/0.m4s".into(),
            6.0,
            50_000,
            true,
            &scan,
        );
        let playlists = vec![pl];
        let samples = vec![sample];
        let opts = ValidateAuthorOptions::default();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &samples, &vtts);
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains("§6.10"))
            .collect()
    }

    /// An mdat-only body: what a byterange fetch of one I-frame typically returns.
    fn mdat_only() -> SegmentScan {
        SegmentScan {
            looks_like_fmp4: true,
            has_moof: false,
            ..Default::default()
        }
    }

    /// visionOS §6.18 findings for a ladder whose stereo video is paired with an I-frame
    /// rendition declaring `iframe_layout`.
    fn vision_trickplay_issues(iframe_layout: &str) -> Vec<Issue> {
        let content = format!(
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=8000000,AVERAGE-BANDWIDTH=7000000,RESOLUTION=4096x4096,CODECS=\"hvc1.2.4.L153.B0\",FRAME-RATE=30,REQ-VIDEO-LAYOUT=\"CH-STEREO\"\nhttps://example.com/v.m3u8\n#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=100000,AVERAGE-BANDWIDTH=90000,RESOLUTION=1280x720,CODECS=\"hvc1.2.4.L123.B0\",REQ-VIDEO-LAYOUT=\"{iframe_layout}\",URI=\"https://example.com/i.m3u8\"\n"
        );
        let master = parse_master_playlist("https://example.com/master.m3u8", &content);
        let playlists: Vec<MediaPlaylist> = Vec::new();
        let opts = ValidateAuthorOptions {
            profile: super::super::profile::AuthorProfile::VisionOs,
            deep_checks: false,
        };
        let inits: Vec<InitProbeEntry> = Vec::new();
        let samples: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &samples, &vtts);
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains("§6.18"))
            .collect()
    }

    #[test]
    fn author_6_18_accepts_monoscopic_rectilinear_trick_play() {
        assert!(
            vision_trickplay_issues("CH-MONO,PROJ-RECT").is_empty(),
            "monoscopic rectilinear trick play is what §6.18 asks for"
        );
    }

    #[test]
    fn author_6_18_reports_stereo_trick_play() {
        let issues = vision_trickplay_issues("CH-STEREO");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(
            issues[0].message.contains("CH-STEREO")
                && issues[0].message.contains("monoscopic and rectilinear"),
            "got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_6_18_reports_a_projected_trick_play_rendition() {
        let issues = vision_trickplay_issues("CH-MONO,PROJ-EQUI");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].message.contains("PROJ-EQUI"), "{issues:?}");
    }

    #[test]
    fn author_6_10_warns_when_the_sample_was_a_byterange() {
        let issues = moof_issues(iframe_playlist(true, Some("12000@34000")), mdat_only());
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(
            issues[0].message.contains("BYTERANGE"),
            "expected the byterange caveat, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_6_10_errors_when_a_whole_fmp4_segment_has_no_moof() {
        let issues = moof_issues(iframe_playlist(true, None), mdat_only());
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
    }

    #[test]
    fn author_6_10_warns_when_the_playlist_declares_no_map() {
        let issues = moof_issues(iframe_playlist(false, None), mdat_only());
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(
            issues[0].message.contains("EXT-X-MAP"),
            "expected the missing-MAP caveat, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_6_10_accepts_a_sample_with_moof() {
        let issues = moof_issues(
            iframe_playlist(true, None),
            SegmentScan {
                looks_like_fmp4: true,
                has_moof: true,
                ..Default::default()
            },
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn author_6_10_ignores_transport_stream_and_unrecognised_samples() {
        let ts = moof_issues(
            iframe_playlist(false, None),
            SegmentScan {
                looks_like_ts: true,
                ..Default::default()
            },
        );
        assert!(ts.is_empty(), "{ts:?}");
        let opaque = moof_issues(iframe_playlist(false, None), SegmentScan::default());
        assert!(opaque.is_empty(), "{opaque:?}");
    }

    /// Run the trick-play rules over a hand-written multivariant playlist with no
    /// media playlists fetched.
    fn master_issues(content: &str) -> Vec<Issue> {
        let master = parse_master_playlist("https://example.com/master.m3u8", content);
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

    fn video(bandwidth: u32, res: &str, range: &str) -> String {
        format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={bandwidth},AVERAGE-BANDWIDTH={bandwidth},RESOLUTION={res},CODECS=\"avc1.4d401f\",VIDEO-RANGE={range},FRAME-RATE=30\nhttps://example.com/{res}-{range}.m3u8\n"
        )
    }

    fn iframe(bandwidth: u32, res: &str, range: &str, codecs: &str) -> String {
        format!(
            "#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH={bandwidth},RESOLUTION={res},CODECS=\"{codecs}\",VIDEO-RANGE={range},URI=\"https://example.com/iframe-{res}-{range}.m3u8\"\n"
        )
    }

    #[test]
    fn author_6_5_flags_iframe_bitrate_above_its_video() {
        let content = format!(
            "#EXTM3U\n{}{}",
            video(2_000_000, "1280x720", "SDR"),
            iframe(3_000_000, "1280x720", "SDR", "avc1.4d401f")
        );
        let issues = master_issues(&content);
        let issue = find(&issues, "§6.5").expect("expected §6.5 bitrate issue");
        assert_eq!(issue.severity, Severity::Warn);
        assert!(find(&issues, "§6.3").is_none(), "§6.3 was the old mislabel");
    }

    #[test]
    fn author_6_6_flags_a_single_iframe_bitrate() {
        let content = format!(
            "#EXTM3U\n{}{}{}{}",
            video(1_000_000, "640x360", "SDR"),
            video(2_000_000, "1280x720", "SDR"),
            video(5_000_000, "1920x1080", "SDR"),
            iframe(200_000, "1280x720", "SDR", "avc1.4d401f")
        );
        let issues = master_issues(&content);
        let issue = find(&issues, "§6.6").expect("expected §6.6 single-bitrate issue");
        assert_eq!(issue.severity, Severity::Warn);
        assert!(find(&issues, "§6.4").is_none(), "§6.4 was the old mislabel");
    }

    #[test]
    fn author_6_15_is_quiet_when_trick_play_is_sdr_only() {
        let content = format!(
            "#EXTM3U\n{}{}{}",
            video(5_000_000, "1920x1080", "PQ"),
            video(2_000_000, "1280x720", "SDR"),
            iframe(200_000, "1280x720", "SDR", "avc1.4d401f")
        );
        let issues = master_issues(&content);
        assert!(
            find(&issues, "§6.15").is_none(),
            "HDR trick play is optional, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_6_15_flags_incomplete_hdr_trick_play() {
        let content = format!(
            "#EXTM3U\n{}{}{}{}",
            video(5_000_000, "1920x1080", "PQ"),
            video(2_000_000, "1280x720", "PQ"),
            iframe(200_000, "1280x720", "PQ", "avc1.4d401f"),
            iframe(200_000, "1280x720", "SDR", "avc1.4d401f")
        );
        let issues = master_issues(&content);
        let issue = find(&issues, "§6.15").expect("expected §6.15 HDR coverage issue");
        assert_eq!(issue.severity, Severity::Warn);
        assert!(issue.message.contains("1920x1080"));
    }

    #[test]
    fn author_6_14_asks_for_h264_trick_play() {
        let content = format!(
            "#EXTM3U\n{}{}",
            video(2_000_000, "1280x720", "SDR"),
            iframe(200_000, "1280x720", "SDR", "hvc1.1.6.L93.B0")
        );
        let issues = master_issues(&content);
        let issue = find(&issues, "§6.14").expect("expected §6.14 H.264 trick-play issue");
        assert_eq!(issue.severity, Severity::Warn);
        assert!(issue.message.contains("H.264"));
    }

    #[test]
    fn author_6_14_accepts_an_h264_iframe_rendition() {
        let content = format!(
            "#EXTM3U\n{}{}",
            video(2_000_000, "1280x720", "SDR"),
            iframe(200_000, "1280x720", "SDR", "avc1.4d401f")
        );
        assert!(find(&master_issues(&content), "§6.14").is_none());
    }
}
