//! Apple Authoring Spec §15–16 — SharePlay / Spatial.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        return issues;
    };

    // §15.2 — SharePlay keeps every participant on one timeline, so an interstitial's
    // length has to be knowable from the playlist. Breaks are free to differ in length
    // from each other, so only a break with no declared length at all is a finding.
    issues.extend(check_interstitial_durations(ctx));

    // §16.1 — spatial video MUST declare REQ-VIDEO-LAYOUT. MV-HEVC cannot be told apart
    // from plain HEVC by its CODECS string, so spatial video is recognised from the media
    // itself: a vexu or lhvC box in the init segment. That keeps the check reachable for
    // the very mistake it is about — a ladder that omits the attribute entirely.
    for pl in ctx.video_playlists() {
        if pl.req_video_layout.is_some() {
            continue;
        }
        let Some(entry) = ctx.probe_for_playlist(&pl.name) else {
            continue;
        };
        let evidence = if entry.probe.has_vexu {
            "a vexu box"
        } else if entry.probe.has_lhvc {
            "an lhvC (MV-HEVC) box"
        } else {
            continue;
        };
        issues.push(author_error(
            "16.1",
            format!(
                "'{}' carries spatial video ({evidence} in init '{}') but its variant MUST include REQ-VIDEO-LAYOUT",
                pl.name, entry.uri
            ),
        ));
    }

    // §16.3 — discontinuity or dual sample-desc when switching layouts
    // Playlist-level: if multiple layouts exist, discontinuities SHOULD appear when switching
    {
        let layouts: Vec<_> = master
            .variants
            .iter()
            .filter_map(|v| v.req_video_layout.as_deref())
            .collect();
        if layouts.len() >= 2 {
            let unique: std::collections::HashSet<_> = layouts.iter().copied().collect();
            if unique.len() >= 2 {
                // Check media playlists for discontinuities
                let any_disc = ctx.playlists.iter().any(|p| p.segments.iter().any(|s| s.discontinuity));
                if !any_disc {
                    issues.push(author_warn(
                        "16.3",
                        "multiple REQ-VIDEO-LAYOUT values without discontinuities (or dual sample descriptions)",
                    ));
                }
            }
        }
    }

    // §16.5 — monoscopic sections are signalled with the CH-MONO channel specifier, so a
    // layout that spells monoscopic video any other way ("MONO", "MONOSCOPIC") is an
    // authoring mistake. Each comma-separated specifier is judged on its own.
    for v in &master.variants {
        let Some(layout) = v.req_video_layout.as_deref() else {
            continue;
        };
        for token in layout.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            let lower = token.to_ascii_lowercase();
            if lower.contains("mono") && lower != "ch-mono" {
                issues.push(author_warn(
                    "16.5",
                    format!(
                        "REQ-VIDEO-LAYOUT on '{}' spells a monoscopic section as '{token}'; \
                         monoscopic video is signalled with CH-MONO",
                        v.uri
                    ),
                ));
            }
        }
    }

    // Phase B: §16.2 vexu MUST for spatial video
    let expects_spatial = master
        .variants
        .iter()
        .any(|v| is_spatial_layout(v.req_video_layout.as_deref()));
    if expects_spatial {
        // Only inits belonging to a spatial variant are required to carry vexu — a mixed
        // ladder's plain 2D variants must not be judged against §16.2.
        let spatial_playlists: Vec<&str> = ctx
            .playlists
            .iter()
            .filter(|p| is_spatial_layout(p.req_video_layout.as_deref()))
            .map(|p| p.name.as_str())
            .collect();
        let spatial_inits: Vec<_> = ctx
            .init_probes
            .iter()
            .filter(|e| {
                e.playlist_names
                    .iter()
                    .any(|n| spatial_playlists.contains(&n.as_str()))
            })
            .collect();
        if spatial_inits.is_empty() {
            issues.push(author_warn(
                "16.2",
                "spatial variants present but no init segment could be associated with one to check vexu",
            ));
        } else {
            for e in spatial_inits.iter().filter(|e| !e.probe.has_vexu) {
                issues.push(author_error(
                    "16.2",
                    format!(
                        "spatial video init '{}' ({}) MUST include a vexu box",
                        e.uri,
                        e.playlist_names.join(", ")
                    ),
                ));
            }
        }

        // §16.4 — parallax metadata SHOULD when subtitles exist (vexu children not fully parsed)
        let has_subs = master
            .media_renditions
            .iter()
            .any(|r| r.media_type == "SUBTITLES");
        if has_subs {
            issues.push(author_info(
                "16.4",
                "stereo/immersive content with subtitles SHOULD include parallax metadata in vexu",
            ));
        }
    }

    // §16.6 (visionOS) — all stereo video MUST be encoded using MV-HEVC, which an init
    // signals with an lhvC layered-HEVC configuration. The rule used to restate §16.4's
    // parallax advice here, which said nothing about the encode it is written about.
    if ctx.policy.profile == super::profile::AuthorProfile::VisionOs {
        for pl in ctx.video_playlists() {
            if !layout_is_stereo(pl.req_video_layout.as_deref()) {
                continue;
            }
            // Only an init that was probed can show whether the encode is MV-HEVC; without
            // one there is nothing to report either way.
            let Some(entry) = ctx.probe_for_playlist(&pl.name) else {
                continue;
            };
            if entry.probe.has_lhvc {
                continue;
            }
            issues.push(author_error(
                "16.6",
                format!(
                    "'{}' declares stereo video but its init '{}' carries no lhvC (MV-HEVC) configuration; stereo video MUST be encoded using MV-HEVC",
                    pl.name, entry.uri
                ),
            ));
        }
    }

    // §16.7 — immersive video MUST name both CH-STEREO and PROJ-AIV. The projection
    // specifier is what identifies the content as immersive, so PROJ-AIV without
    // CH-STEREO is the violation this can see. CH-STEREO on its own is ordinary stereo
    // video, which §16.7 says nothing about.
    for v in &master.variants {
        let Some(layout) = v.req_video_layout.as_deref() else {
            continue;
        };
        let specifiers: Vec<String> = layout
            .split(',')
            .map(|t| t.trim().to_ascii_uppercase())
            .filter(|t| !t.is_empty())
            .collect();
        let has = |want: &str| specifiers.iter().any(|s| s == want);
        if !has("PROJ-AIV") || has("CH-STEREO") {
            continue;
        }
        issues.push(author_error(
            "16.7",
            format!(
                "'{}' declares REQ-VIDEO-LAYOUT '{layout}', so its video is immersive, but immersive video MUST use both CH-STEREO and PROJ-AIV; 'CH-STEREO' is missing",
                v.uri
            ),
        ));
    }

    issues
}

/// A REQ-VIDEO-LAYOUT naming a stereoscopic channel (§16.6).
fn layout_is_stereo(layout: Option<&str>) -> bool {
    layout.is_some_and(|l| l.to_ascii_lowercase().contains("stereo"))
}

/// §15.2 — at most one finding for the whole stream, naming interstitials whose playout
/// length is not declared anywhere. Every participant of a SharePlay session schedules
/// the break from the playlist alone, so an unbounded interstitial desynchronises them.
fn check_interstitial_durations(ctx: &AuthoringContext<'_>) -> Option<Issue> {
    let mut undeclared: Vec<String> = Vec::new();
    for pl in ctx.playlists {
        for line in pl.raw_content.lines() {
            let Some(rest) = line.trim().strip_prefix("#EXT-X-DATERANGE:") else {
                continue;
            };
            let attrs = crate::utils::validator::parser::parse_attributes(rest);
            if attrs.get("CLASS").map(|s| s.as_str()) != Some("com.apple.hls.interstitial") {
                continue;
            }
            let declares_length = ["DURATION", "PLANNED-DURATION", "X-PLAYOUT-LIMIT"]
                .iter()
                .any(|k| attrs.contains_key(*k));
            if !declares_length {
                undeclared.push(
                    attrs
                        .get("ID")
                        .cloned()
                        .unwrap_or_else(|| format!("interstitial on '{}'", pl.name)),
                );
            }
        }
    }

    let first = undeclared.first()?;
    let more = match undeclared.len() {
        1 => String::new(),
        n => format!(" (and {} more)", n - 1),
    };
    Some(author_warn(
        "15.2",
        format!(
            "interstitial '{first}'{more} declares neither DURATION, PLANNED-DURATION nor \
             X-PLAYOUT-LIMIT; SharePlay participants need a known break length to stay on \
             one timeline"
        ),
    ))
}

/// Whether a REQ-VIDEO-LAYOUT value describes stereoscopic / immersive video (§16.1).
fn is_spatial_layout(layout: Option<&str>) -> bool {
    layout.is_some_and(|l| {
        let l = l.to_ascii_lowercase();
        l.contains("stereo") || l.contains("immersive")
    })
}

#[cfg(test)]
mod tests {
    use super::super::context::{
        InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample,
    };
    use super::*;
    use crate::utils::mp4_probe::InitSegmentProbe;
    use crate::utils::validator::parser::parse_master_playlist;
    use crate::utils::validator::types::{MediaPlaylist, Segment};

    /// A mixed ladder: one stereo variant plus one plain 2D variant.
    fn mixed_master() -> crate::utils::validator::types::MasterPlaylist {
        parse_master_playlist(
            "https://example.com/master.m3u8",
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=8000000,AVERAGE-BANDWIDTH=7000000,RESOLUTION=4096x4096,CODECS="hvc1.2.4.L153.B0",FRAME-RATE=30,REQ-VIDEO-LAYOUT="CH-STEREO"
https://example.com/stereo.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS="hvc1.1.6.L93.B0",FRAME-RATE=30
https://example.com/flat.m3u8
"#,
        )
    }

    fn video_playlist(name: &str, url: &str, layout: Option<&str>) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.into(), url.into());
        pl.media_type = "VIDEO".into();
        pl.target_duration = 4.0;
        pl.has_endlist = true;
        pl.req_video_layout = layout.map(str::to_string);
        pl.segments.push(Segment {
            uri: "0.m4s".into(),
            duration: 4.0,
            title: None,
            pdt: None,
            discontinuity: false,
            byterange: None,
            is_ad: false,
            map_uri: Some("init.mp4".into()),
        });
        pl
    }

    fn init(uri: &str, playlist: &str, has_vexu: bool) -> InitProbeEntry {
        InitProbeEntry {
            uri: uri.into(),
            byterange: None,
            playlist_names: vec![playlist.into()],
            media_types: vec!["VIDEO".into()],
            probe: InitSegmentProbe {
                video_sample_fourcc: Some("hvc1".into()),
                has_vexu,
                ..Default::default()
            },
        }
    }

    fn vexu_issues(playlists: &[MediaPlaylist], inits: &[InitProbeEntry]) -> Vec<Issue> {
        let master = mixed_master();
        let opts = ValidateAuthorOptions::default();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), playlists, &opts, inits, &segs, &vtts);
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains("§16.2"))
            .collect()
    }

    #[test]
    fn author_16_2_ignores_flat_variant_init_without_vexu() {
        let playlists = vec![
            video_playlist(
                "video/4096x4096 · 8000k",
                "https://example.com/stereo.m3u8",
                Some("CH-STEREO"),
            ),
            video_playlist("video/1280x720 · 2000k", "https://example.com/flat.m3u8", None),
        ];
        let inits = vec![
            init(
                "https://example.com/stereo-init.mp4",
                "video/4096x4096 · 8000k",
                true,
            ),
            init(
                "https://example.com/flat-init.mp4",
                "video/1280x720 · 2000k",
                false,
            ),
        ];
        let issues = vexu_issues(&playlists, &inits);
        assert!(
            issues.is_empty(),
            "a 2D variant's init must not be held to §16.2, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_16_2_errors_when_spatial_init_lacks_vexu() {
        let playlists = vec![
            video_playlist(
                "video/4096x4096 · 8000k",
                "https://example.com/stereo.m3u8",
                Some("CH-STEREO"),
            ),
            video_playlist("video/1280x720 · 2000k", "https://example.com/flat.m3u8", None),
        ];
        let inits = vec![
            init(
                "https://example.com/stereo-init.mp4",
                "video/4096x4096 · 8000k",
                false,
            ),
            init(
                "https://example.com/flat-init.mp4",
                "video/1280x720 · 2000k",
                true,
            ),
        ];
        let issues = vexu_issues(&playlists, &inits);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, crate::utils::validator::types::Severity::Error);
        assert!(issues[0].message.contains("stereo-init.mp4"));
    }

    /// visionOS §16.6 findings for a stereo variant whose init reports `has_lhvc`.
    fn vision_mv_hevc_issues(has_lhvc: bool) -> Vec<Issue> {
        let master = mixed_master();
        let playlists = vec![video_playlist(
            "video/4096x4096 · 8000k",
            "https://example.com/stereo.m3u8",
            Some("CH-STEREO"),
        )];
        let mut inits = vec![init(
            "https://example.com/stereo-init.mp4",
            "video/4096x4096 · 8000k",
            true,
        )];
        inits[0].probe.has_lhvc = has_lhvc;
        let opts = ValidateAuthorOptions {
            profile: super::super::profile::AuthorProfile::VisionOs,
            deep_checks: false,
        };
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains("§16.6"))
            .collect()
    }

    #[test]
    fn author_16_6_accepts_mv_hevc_stereo() {
        assert!(vision_mv_hevc_issues(true).is_empty());
    }

    #[test]
    fn author_16_6_errors_when_stereo_is_not_mv_hevc() {
        let issues = vision_mv_hevc_issues(false);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(
            issues[0].severity,
            crate::utils::validator::types::Severity::Error
        );
        assert!(issues[0].message.contains("lhvC"), "{issues:?}");
    }

    /// All findings for `section` from a master-plus-playlists run.
    fn issues_for(
        master: &crate::utils::validator::types::MasterPlaylist,
        playlists: &[MediaPlaylist],
        inits: &[InitProbeEntry],
        section: &str,
    ) -> Vec<Issue> {
        let opts = ValidateAuthorOptions::default();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(master), playlists, &opts, inits, &segs, &vtts);
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains(section))
            .collect()
    }

    /// Video playlist carrying `daterange` lines as its raw body.
    fn playlist_with_dateranges(daterange: &str) -> MediaPlaylist {
        let mut pl = video_playlist("video/1280x720 · 2000k", "https://example.com/flat.m3u8", None);
        pl.raw_content = format!("#EXTM3U\n#EXT-X-TARGETDURATION:4\n{daterange}");
        pl
    }

    const AD: &str = "#EXT-X-DATERANGE:ID=\"ad1\",CLASS=\"com.apple.hls.interstitial\",\
                      START-DATE=\"2026-01-01T00:00:00.000Z\",X-ASSET-URI=\"a.m3u8\"";

    #[test]
    fn author_15_2_ignores_ad_breaks_of_different_lengths() {
        let master = mixed_master();
        let pl = playlist_with_dateranges(
            "#EXT-X-DATERANGE:ID=\"a\",CLASS=\"com.apple.hls.interstitial\",\
             START-DATE=\"2026-01-01T00:00:00.000Z\",PLANNED-DURATION=15\n\
             #EXT-X-DATERANGE:ID=\"b\",CLASS=\"com.apple.hls.interstitial\",\
             START-DATE=\"2026-01-01T00:05:00.000Z\",PLANNED-DURATION=30\n",
        );
        let issues = issues_for(&master, &[pl], &[], "§15.2");
        assert!(
            issues.is_empty(),
            "two breaks may legitimately differ in length, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_15_2_reports_a_break_without_any_declared_length_once() {
        let master = mixed_master();
        let pl = playlist_with_dateranges(&format!("{AD}\n{}\n", AD.replace("ad1", "ad2")));
        let issues = issues_for(&master, &[pl], &[], "§15.2");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, crate::utils::validator::types::Severity::Warn);
        assert!(
            issues[0].message.contains("'ad1'") && issues[0].message.contains("and 1 more"),
            "expected one collapsed finding, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_16_1_errors_when_spatial_media_has_no_req_video_layout() {
        let master = mixed_master();
        let playlists = vec![video_playlist(
            "video/1280x720 · 2000k",
            "https://example.com/flat.m3u8",
            None,
        )];
        let inits = vec![init(
            "https://example.com/flat-init.mp4",
            "video/1280x720 · 2000k",
            true,
        )];
        let issues = issues_for(&master, &playlists, &inits, "§16.1");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, crate::utils::validator::types::Severity::Error);
        assert!(
            issues[0].message.contains("REQ-VIDEO-LAYOUT"),
            "expected the missing attribute, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_16_1_treats_an_mv_hevc_init_as_spatial() {
        let master = mixed_master();
        let playlists = vec![video_playlist(
            "video/1280x720 · 2000k",
            "https://example.com/flat.m3u8",
            None,
        )];
        let mut inits = vec![init(
            "https://example.com/flat-init.mp4",
            "video/1280x720 · 2000k",
            false,
        )];
        inits[0].probe.has_lhvc = true;
        let issues = issues_for(&master, &playlists, &inits, "§16.1");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0].message.contains("lhvC"),
            "expected the MV-HEVC evidence, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_16_1_accepts_declared_spatial_video_and_plain_2d() {
        let master = mixed_master();
        let playlists = vec![
            video_playlist(
                "video/4096x4096 · 8000k",
                "https://example.com/stereo.m3u8",
                Some("CH-STEREO"),
            ),
            video_playlist("video/1280x720 · 2000k", "https://example.com/flat.m3u8", None),
        ];
        let inits = vec![
            init(
                "https://example.com/stereo-init.mp4",
                "video/4096x4096 · 8000k",
                true,
            ),
            init(
                "https://example.com/flat-init.mp4",
                "video/1280x720 · 2000k",
                false,
            ),
        ];
        let issues = issues_for(&master, &playlists, &inits, "§16.1");
        assert!(
            issues.is_empty(),
            "a declared stereo variant and a 2D variant both satisfy §16.1, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    /// Master with a single variant declaring `layout`.
    fn master_with_layout(layout: &str) -> crate::utils::validator::types::MasterPlaylist {
        parse_master_playlist(
            "https://example.com/master.m3u8",
            &format!(
                r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=8000000,AVERAGE-BANDWIDTH=7000000,RESOLUTION=4096x4096,CODECS="hvc1.2.4.L153.B0",FRAME-RATE=30,REQ-VIDEO-LAYOUT="{layout}"
https://example.com/stereo.m3u8
"#
            ),
        )
    }

    #[test]
    fn author_16_7_requires_ch_stereo_alongside_proj_aiv() {
        let master = master_with_layout("CH-MONO,PROJ-AIV");
        let issues = issues_for(&master, &[], &[], "§16.7");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(
            issues[0].severity,
            crate::utils::validator::types::Severity::Error
        );
        assert!(
            issues[0].message.contains("CH-STEREO"),
            "{}",
            issues[0].message
        );
    }

    #[test]
    fn author_16_7_leaves_plain_stereo_video_alone() {
        // Stereo video without an immersive projection is not what §16.7 is written about.
        let master = master_with_layout("CH-STEREO");
        assert!(
            issues_for(&master, &[], &[], "§16.7").is_empty(),
            "CH-STEREO on its own does not make content immersive"
        );
    }

    #[test]
    fn author_16_7_accepts_a_complete_immersive_layout() {
        let master = master_with_layout("CH-STEREO,PROJ-AIV");
        assert!(issues_for(&master, &[], &[], "§16.7").is_empty());
    }

    #[test]
    fn author_16_7_ignores_a_layout_that_is_not_immersive() {
        let master = master_with_layout("CH-MONO,PROJ-EQUI");
        assert!(issues_for(&master, &[], &[], "§16.7").is_empty());
    }

    #[test]
    fn author_16_5_reports_a_monoscopic_specifier_that_is_not_ch_mono() {
        let master = master_with_layout("CH-STEREO,MONO");
        let issues = issues_for(&master, &[], &[], "§16.5");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0].message.contains("'MONO'") && issues[0].message.contains("CH-MONO"),
            "expected the offending specifier, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_16_5_accepts_ch_stereo_with_ch_mono_sections() {
        let master = master_with_layout("CH-STEREO,CH-MONO");
        let issues = issues_for(&master, &[], &[], "§16.5");
        assert!(
            issues.is_empty(),
            "CH-STEREO,CH-MONO is the correct signalling, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_16_2_warns_when_no_spatial_init_was_probed() {
        let playlists = vec![video_playlist(
            "video/1280x720 · 2000k",
            "https://example.com/flat.m3u8",
            None,
        )];
        let inits = vec![init(
            "https://example.com/flat-init.mp4",
            "video/1280x720 · 2000k",
            false,
        )];
        let issues = vexu_issues(&playlists, &inits);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, crate::utils::validator::types::Severity::Warn);
    }
}
