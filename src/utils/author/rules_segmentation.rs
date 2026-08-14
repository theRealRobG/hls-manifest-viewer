//! Apple Authoring Spec §7 — Segmentation (playlist-visible).

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::{Issue, MediaPlaylist};

/// Playlists carrying the audio/video media that the ~6 s recommendation is about.
/// Subtitle playlists segment on cue boundaries and trick-play playlists on I-frame
/// spacing, so neither is measured against §7.5–7.6.
fn is_av_media(pl: &MediaPlaylist) -> bool {
    !pl.is_iframe && (pl.media_type == "VIDEO" || pl.media_type == "AUDIO")
}

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();

    for pl in ctx.playlists.iter().filter(|p| !p.is_iframe) {
        if pl.target_duration <= 0.0 || pl.segments.is_empty() {
            continue;
        }

        // §7.5–7.6 — TD SHOULD be ~6s; EXTINF nominal ~6s
        if is_av_media(pl) {
            if (pl.target_duration - 6.0).abs() > 2.0 {
                issues.push(author_warn(
                    "7.5",
                    format!(
                        "'{}' TARGETDURATION {:.0}s is far from recommended ~6s",
                        pl.name, pl.target_duration
                    ),
                ));
            }
            let avg: f64 = pl.segments.iter().map(|s| s.duration).sum::<f64>()
                / pl.segments.len() as f64;
            if (avg - 6.0).abs() > 2.0 {
                issues.push(author_warn(
                    "7.6",
                    format!(
                        "'{}' average EXTINF {avg:.2}s is far from recommended ~6s",
                        pl.name
                    ),
                ));
            }
        }

        // §7.7 — EXTINF MUST NOT exceed TD by >0.5s (stricter than RFC)
        for (i, seg) in pl.segments.iter().enumerate() {
            if seg.duration > pl.target_duration + 0.5 {
                issues.push(author_error(
                    "7.7",
                    format!(
                        "'{}' segment {} EXTINF {:.3}s exceeds TARGETDURATION {:.0} by >0.5s",
                        pl.name, i, seg.duration, pl.target_duration
                    ),
                ));
                break; // one sample enough
            }
        }
    }

    // §7.1 (each segment starts with an IDR) is checked from sampled segment bytes by the
    // deep rules as §7.4, which knows which playlists carry unencrypted video.

    issues
}

#[cfg(test)]
mod tests {
    use super::super::context::{
        InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample,
    };
    use super::*;
    use crate::utils::validator::types::Segment;

    /// Playlist of `count` segments of `duration` seconds, with TARGETDURATION `td`.
    fn playlist(name: &str, media_type: &str, td: f64, duration: f64, count: usize) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.into(), format!("https://example.com/{name}.m3u8"));
        pl.media_type = media_type.into();
        pl.target_duration = td;
        pl.has_endlist = true;
        pl.playlist_type = Some("VOD".into());
        for i in 0..count {
            pl.segments.push(Segment {
                uri: format!("{i}.m4s"),
                duration,
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

    fn issues_for(playlists: &[MediaPlaylist]) -> Vec<Issue> {
        let opts = ValidateAuthorOptions::default();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(None, playlists, &opts, &inits, &segs, &vtts);
        check(&ctx)
    }

    #[test]
    fn author_7_5_ignores_subtitle_segment_lengths() {
        // A whole-programme WebVTT playlist: one 300 s cue file is normal authoring.
        let subs = playlist("subtitles/English (subs)", "SUBTITLES", 300.0, 300.0, 1);
        let issues = issues_for(&[subs]);
        assert!(
            issues.is_empty(),
            "subtitle playlists are not judged against the ~6s recommendation, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_7_5_still_reports_video_target_duration() {
        let issues = issues_for(&[playlist("video/1280x720", "VIDEO", 2.0, 2.0, 4)]);
        assert!(
            issues.iter().any(|i| i.message.contains("§7.5")) && issues.iter().any(|i| i.message.contains("§7.6")),
            "expected §7.5 and §7.6 on a 2s video playlist, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_7_5_still_reports_audio_target_duration() {
        let issues = issues_for(&[playlist("audio/English (aud)", "AUDIO", 1.0, 1.0, 4)]);
        assert!(
            issues.iter().any(|i| i.message.contains("§7.5")),
            "expected §7.5 on a 1s audio playlist, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_7_5_ignores_iframe_playlists() {
        let mut iframe = playlist("iframe/1280x720", "VIDEO", 10.0, 10.0, 4);
        iframe.is_iframe = true;
        let issues = issues_for(&[iframe]);
        assert!(
            issues.is_empty(),
            "trick-play playlists are not judged against the ~6s recommendation, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_7_7_reports_extinf_over_target_duration_on_subtitles() {
        // Scoping §7.5–7.6 to A/V must not stop the hard EXTINF/TARGETDURATION check.
        let subs = playlist("subtitles/English (subs)", "SUBTITLES", 6.0, 30.0, 1);
        let issues = issues_for(&[subs]);
        assert!(
            issues.iter().any(|i| i.message.contains("§7.7")),
            "expected §7.7, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }
}
