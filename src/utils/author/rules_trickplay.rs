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
        issues.push(author_warn(
            "6.1",
            "no EXT-X-I-FRAME-STREAM-INF entries for trick play",
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

    // §6.2–6.6 density/bitrate heuristics
    for iframe in &iframes {
        if let (Some(ibw), Some(res)) = (iframe.bandwidth, iframe.resolution.as_deref()) {
            // Find matching video resolution
            if let Some(video) = videos.iter().find(|v| v.resolution.as_deref() == Some(res)) {
                if let Some(vbw) = video.bandwidth {
                    // I-frame bitrate typically much lower; warn if higher than video
                    if ibw > vbw {
                        issues.push(author_warn(
                            "6.3",
                            format!(
                                "I-frame BANDWIDTH {ibw} exceeds matching video BANDWIDTH {vbw} at {res}"
                            ),
                        ));
                    }
                }
            }
        }
    }

    // Multiple I-frame bitrates (§6.4)
    if iframes.len() == 1 && videos.len() > 2 {
        issues.push(author_warn(
            "6.4",
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

    // §6.14 / 6.16 — HDR I-frame at resolutions; SDR I-frame MUST (*)
    if !ctx.policy.is_exempt("6.14") {
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
                issues.push(author_warn(
                    "6.14",
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
    for sample in ctx.segment_samples.iter().filter(|s| s.is_iframe_playlist) {
        let pl_has_map = ctx
            .iframe_playlists()
            .find(|p| p.name == sample.playlist_name)
            .is_some_and(playlist_has_map)
            || sample.looks_like_fmp4;
        if pl_has_map && !sample.has_moof {
            issues.push(author_error(
                "6.10",
                format!(
                    "fMP4 I-frame segment '{}' missing moof header",
                    sample.uri
                ),
            ));
        }
    }

    // visionOS 6.18/6.19 — spatial trick play notes
    if ctx.policy.profile == super::profile::AuthorProfile::VisionOs && !iframes.is_empty() {
        let stereo_video = videos.iter().any(|v| {
            v.req_video_layout
                .as_deref()
                .is_some_and(|l| l.to_ascii_lowercase().contains("stereo"))
        });
        if stereo_video {
            let stereo_iframe = iframes.iter().any(|v| {
                v.req_video_layout
                    .as_deref()
                    .is_some_and(|l| l.to_ascii_lowercase().contains("stereo"))
            });
            if !stereo_iframe {
                issues.push(author_warn(
                    "6.18",
                    "visionOS: stereo spatial video SHOULD have stereo I-frame playlists",
                ));
            }
        }
    }

    issues
}
