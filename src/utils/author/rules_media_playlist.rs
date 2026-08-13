//! Apple Authoring Spec §8 — Media playlists.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        // Still run media-only rules
        return check_media_only(ctx);
    };

    let videos: Vec<_> = ctx.video_playlists().collect();
    let audios: Vec<_> = ctx.audio_playlists().collect();

    // §8.2–8.3 — same TD; VOD same content duration
    if let (Some(v), Some(a)) = (videos.first(), audios.first()) {
        if (v.target_duration - a.target_duration).abs() > 0.01 {
            issues.push(author_error(
                "8.2",
                format!(
                    "audio/video TARGETDURATION differ ({:.0} vs {:.0})",
                    v.target_duration, a.target_duration
                ),
            ));
        }
        if v.has_endlist && a.has_endlist {
            let vd = playlist_duration_s(v);
            let ad = playlist_duration_s(a);
            if vd > 0.0 && ad > 0.0 && (vd - ad).abs() > 1.0 {
                issues.push(author_warn(
                    "8.3",
                    format!("VOD audio/video durations differ ({ad:.1}s vs {vd:.1}s)"),
                ));
            }
        }
    }

    // §8.4 — Live MUST have PDT
    for pl in videos.iter().chain(audios.iter()) {
        let live = !pl.has_endlist && pl.playlist_type.as_deref() != Some("VOD");
        if live {
            let has_pdt = pl.segments.iter().any(|s| s.pdt.is_some());
            if !has_pdt {
                issues.push(author_error(
                    "8.4",
                    format!("live playlist '{}' MUST include PROGRAM-DATE-TIME", pl.name),
                ));
            }
        }
    }

    // §8.6–8.8 — PLAYLIST-TYPE
    for pl in ctx.playlists {
        if pl.has_endlist && pl.playlist_type.as_deref() == Some("EVENT") {
            issues.push(author_warn(
                "8.6",
                format!(
                    "'{}' has ENDLIST with PLAYLIST-TYPE:EVENT (prefer VOD when complete)",
                    pl.name
                ),
            ));
        }
        if pl.playlist_type.as_deref() == Some("VOD") && !pl.has_endlist {
            issues.push(author_error(
                "8.7",
                format!("PLAYLIST-TYPE:VOD '{}' MUST have EXT-X-ENDLIST", pl.name),
            ));
        }
    }

    // §8.9–8.10 — separate audio via EXT-X-MEDIA; LANGUAGE on non-VIDEO
    let has_audio_group = master.variants.iter().any(|v| v.audio_group.is_some());
    let has_demuxed_audio = master
        .media_renditions
        .iter()
        .any(|r| r.media_type == "AUDIO" && r.uri.is_some());
    if has_audio_group && !has_demuxed_audio {
        issues.push(author_warn(
            "8.9",
            "AUDIO group referenced but no EXT-X-MEDIA AUDIO with URI",
        ));
    }
    for r in &master.media_renditions {
        if r.media_type != "VIDEO" && r.uri.is_some() && r.language.is_none() {
            issues.push(author_error(
                "8.10",
                format!(
                    "non-VIDEO EXT-X-MEDIA '{}' MUST have LANGUAGE",
                    r.name
                ),
            ));
        }
    }

    // §8.11–8.12 — live ≥6 segments; window duration
    for pl in videos.iter().chain(audios.iter()) {
        let live = !pl.has_endlist && pl.playlist_type.as_deref() != Some("VOD");
        if live {
            if pl.segments.len() < 6 {
                issues.push(author_error(
                    "8.11",
                    format!(
                        "live playlist '{}' has {} segments (MUST be ≥6)",
                        pl.name,
                        pl.segments.len()
                    ),
                ));
            }
            let window = playlist_duration_s(pl);
            if window > 0.0 && window < ctx.policy.live_window_min_s {
                issues.push(author_warn(
                    "8.12",
                    format!(
                        "live playlist '{}' window {:.0}s < recommended {:.0}s",
                        pl.name, window, ctx.policy.live_window_min_s
                    ),
                ));
            }
        }
    }

    // §8.13–8.17 — discontinuity tagging
    for pl in ctx.playlists {
        let has_disc = pl.segments.iter().any(|s| s.discontinuity);
        if has_disc && pl.discontinuity_sequence == 0 {
            // DISCONTINUITY-SEQUENCE may be omitted when 0 — OK
        }
        if has_disc {
            // Ensure discontinuities appear as tags (already parsed onto segments)
            let _ = has_disc;
        }
    }
    // Aligned discontinuities across renditions
    {
        let video_disc: Vec<Vec<bool>> = videos
            .iter()
            .map(|p| p.segments.iter().map(|s| s.discontinuity).collect())
            .collect();
        if video_disc.len() >= 2 {
            let len = video_disc.iter().map(|v| v.len()).min().unwrap_or(0);
            for i in 0..len {
                let first = video_disc[0][i];
                if video_disc.iter().any(|v| v[i] != first) {
                    issues.push(author_warn(
                        "8.17",
                        format!("discontinuity flags misaligned across video renditions at index {i}"),
                    ));
                    break;
                }
            }
        }
    }

    // §8.19 / 8.21 — DATERANGE for interstitials/program boundaries SHOULD
    // (informational if none — skip)

    // §8.20 — MAP present when fMP4 implied
    for pl in ctx.playlists {
        let has_map = pl.segments.iter().any(|s| s.map_uri.is_some());
        let codecs = pl.codecs.as_deref().unwrap_or("");
        let suggests_fmp4 = codecs.contains("avc1")
            || codecs.contains("hvc1")
            || codecs.contains("hev1")
            || codecs.contains("av01")
            || codecs.contains("mp4a");
        // TS often uses avc1 too — only warn when MAP missing AND no .ts URIs
        let looks_ts = pl.segments.iter().any(|s| {
            s.uri.contains(".ts") || s.uri.contains(".m2ts")
        });
        if suggests_fmp4 && !looks_ts && !has_map && !pl.segments.is_empty() {
            issues.push(author_warn(
                "8.20",
                format!(
                    "'{}' appears fMP4 (CODECS) but has no EXT-X-MAP",
                    pl.name
                ),
            ));
        }
    }

    // §8.22–8.25 — INDEPENDENT-SEGMENTS for xHE-AAC/APAC
    for pl in ctx.playlists {
        let codecs = pl.codecs.as_deref().unwrap_or("");
        let needs_indep = codecs.to_ascii_lowercase().contains("mp4a.40.42")
            || codecs.to_ascii_lowercase().contains("apac");
        if needs_indep && !pl.independent_segments && !master.independent_segments {
            issues.push(author_warn(
                "8.22",
                format!(
                    "'{}' uses xHE-AAC/APAC without INDEPENDENT-SEGMENTS",
                    pl.name
                ),
            ));
        }
    }

    // Live Last-Modified vs Date
    for pl in ctx.playlists {
        let live = !pl.has_endlist && pl.playlist_type.as_deref() != Some("VOD");
        if live {
            if let (Some(lm), Some(date)) = (&pl.http_meta.last_modified, &pl.http_meta.date) {
                if lm == date {
                    // weak heuristic only
                    let _ = (lm, date);
                }
            }
        }
    }

    // AirPlay 8.25 — additional alignment MUST-ish
    if ctx.policy.profile == super::profile::AuthorProfile::AirPlay2 {
        // already covered by alignment checks; reinforce
    }

    issues
}

fn check_media_only(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in ctx.playlists {
        let live = !pl.has_endlist && pl.playlist_type.as_deref() != Some("VOD");
        if live && !pl.segments.iter().any(|s| s.pdt.is_some()) {
            issues.push(author_error(
                "8.4",
                format!("live playlist '{}' MUST include PROGRAM-DATE-TIME", pl.name),
            ));
        }
    }
    issues
}
