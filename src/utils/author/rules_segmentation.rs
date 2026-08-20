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

/// Playlists named in an aggregated §7.5 / §7.6 finding before it is summarised.
const MAX_NAMED_PLAYLISTS: usize = 3;

/// One finding for the whole stream. A ladder segments every rendition the same way, so
/// reporting §7.5 or §7.6 per playlist repeats one authoring decision once per variant.
fn aggregated(section: &str, subject: &str, offenders: &[String], measured: usize) -> Option<Issue> {
    let named = offenders.iter().take(MAX_NAMED_PLAYLISTS);
    let more = match offenders.len().saturating_sub(MAX_NAMED_PLAYLISTS) {
        0 => String::new(),
        n => format!(", and {n} more"),
    };
    if offenders.is_empty() {
        return None;
    }
    Some(author_warn(
        section,
        format!(
            "{} of {measured} audio/video playlist(s) {subject}: {}{more}",
            offenders.len(),
            named.cloned().collect::<Vec<_>>().join("; "),
        ),
    ))
}

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut av_measured = 0usize;
    let mut long_target_duration: Vec<String> = Vec::new();
    let mut long_average_extinf: Vec<String> = Vec::new();

    for pl in ctx.playlists.iter().filter(|p| !p.is_iframe) {
        if pl.target_duration <= 0.0 || pl.segments.is_empty() {
            continue;
        }

        // §7.5–7.6 — TD SHOULD be ~6s; EXTINF nominal ~6s
        if is_av_media(pl) {
            av_measured += 1;
            if (pl.target_duration - 6.0).abs() > 2.0 {
                long_target_duration
                    .push(format!("'{}' at {:.0}s", pl.name, pl.target_duration));
            }
            let avg: f64 = pl.segments.iter().map(|s| s.duration).sum::<f64>()
                / pl.segments.len() as f64;
            if (avg - 6.0).abs() > 2.0 {
                long_average_extinf.push(format!("'{}' at {avg:.2}s", pl.name));
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

    issues.extend(aggregated(
        "7.5",
        "declare a TARGETDURATION far from the recommended ~6s",
        &long_target_duration,
        av_measured,
    ));
    issues.extend(aggregated(
        "7.6",
        "average an EXTINF far from the recommended ~6s",
        &long_average_extinf,
        av_measured,
    ));

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
    fn author_7_5_reports_a_whole_ladder_once() {
        let ladder: Vec<MediaPlaylist> = ["360", "720", "1080", "2160"]
            .iter()
            .map(|res| playlist(&format!("video/{res}"), "VIDEO", 2.0, 2.0, 4))
            .collect();
        let issues = issues_for(&ladder);
        let td: Vec<&Issue> = issues
            .iter()
            .filter(|i| i.message.contains("§7.5"))
            .collect();
        assert_eq!(td.len(), 1, "one finding for the ladder: {issues:?}");
        assert!(
            td[0].message.contains("4 of 4 audio/video playlist(s)")
                && td[0].message.contains("and 1 more"),
            "the finding should account for every playlist it covers, got: {}",
            td[0].message
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
