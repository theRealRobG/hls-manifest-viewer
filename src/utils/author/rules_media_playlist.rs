//! Apple Authoring Spec §8 — Media playlists.

use super::context::AuthoringContext;
use super::helpers::*;
use super::severity::must;
use crate::utils::validator::types::{Issue, MediaPlaylist};

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        // Still run media-only rules
        return check_media_only(ctx);
    };

    let videos: Vec<_> = ctx.video_playlists().collect();
    let audios: Vec<_> = ctx.audio_playlists().collect();

    // §8.2–8.3 — every A/V playlist MUST share one TARGETDURATION and, for VOD, one duration.
    // The first video playlist is the reference; with no video, the first audio playlist is.
    {
        let av: Vec<&MediaPlaylist> = videos.iter().copied().chain(audios.iter().copied()).collect();
        if let Some(&reference) = av.first() {
            for &pl in av.iter().skip(1) {
                if (pl.target_duration - reference.target_duration).abs() > 0.01 {
                    issues.push(author_issue(
                        must(),
                        "8.2",
                        format!(
                            "TARGETDURATION of '{}' ({:.0}) MUST match '{}' ({:.0})",
                            pl.name, pl.target_duration, reference.name, reference.target_duration
                        ),
                    ));
                }
            }

            let ref_duration = playlist_duration_s(reference);
            if reference.has_endlist && ref_duration > 0.0 {
                for &pl in av.iter().skip(1) {
                    if !pl.has_endlist {
                        continue;
                    }
                    let d = playlist_duration_s(pl);
                    if d > 0.0 && (d - ref_duration).abs() > 1.0 {
                        issues.push(author_issue(
                            must(),
                            "8.3",
                            format!(
                                "VOD duration of '{}' ({d:.1}s) MUST match '{}' ({ref_duration:.1}s)",
                                pl.name, reference.name
                            ),
                        ));
                    }
                }
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
        if pl.has_endlist && pl.playlist_type.is_none() {
            issues.push(author_error(
                "8.6",
                format!(
                    "static playlist '{}' has EXT-X-ENDLIST and MUST have EXT-X-PLAYLIST-TYPE:VOD",
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
    // §8.10 is the catch-all LANGUAGE requirement for non-VIDEO renditions. Sections that state
    // the same requirement for a narrower set of renditions own those renditions, so a missing
    // LANGUAGE is reported once under its most specific section: §4.7 covers SUBTITLES and
    // CLOSED-CAPTIONS, and §2.27 covers descriptive / speech-intelligibility audio.
    for r in &master.media_renditions {
        let covered_elsewhere = matches!(r.media_type.as_str(), "SUBTITLES" | "CLOSED-CAPTIONS")
            || audio_is_dvs(r)
            || audio_enhances_speech(r);
        if r.media_type != "VIDEO" && !covered_elsewhere && r.uri.is_some() && r.language.is_none() {
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
        let live = !pl.has_endlist && pl.playlist_type.as_deref() != Some("VOD");
        let has_disc = pl.segments.iter().any(|s| s.discontinuity);
        // A parsed value of 0 is ambiguous (the tag defaults to 0), so look for the tag itself.
        let has_disc_seq_tag = pl.raw_content.contains("#EXT-X-DISCONTINUITY-SEQUENCE");
        if live && has_disc && !has_disc_seq_tag {
            issues.push(author_error(
                "8.17",
                format!(
                    "live playlist '{}' has EXT-X-DISCONTINUITY and MUST include EXT-X-DISCONTINUITY-SEQUENCE",
                    pl.name
                ),
            ));
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

    // §8.20 — MAP MUST be present when using fMP4 (strengthened by Phase B init brands)
    for pl in ctx.playlists {
        let has_map = playlist_has_map(pl);
        let codecs = pl.codecs.as_deref().unwrap_or("");
        let suggests_fmp4 = codecs.contains("avc1")
            || codecs.contains("hvc1")
            || codecs.contains("hev1")
            || codecs.contains("av01")
            || codecs.contains("dvh")
            || codecs.contains("mp4a");
        let looks_ts = playlist_looks_like_ts(pl);
        let probe = ctx.probe_for_playlist(&pl.name);
        let init_says_fmp4 = probe.is_some_and(|e| e.probe.looks_like_fmp4_init());

        if (init_says_fmp4 || (suggests_fmp4 && !looks_ts)) && !has_map && !pl.segments.is_empty()
        {
            let sev_rule_msg = if init_says_fmp4 {
                author_error(
                    "8.20",
                    format!(
                        "'{}' uses fMP4 init but has no EXT-X-MAP",
                        pl.name
                    ),
                )
            } else {
                author_warn(
                    "8.20",
                    format!(
                        "'{}' appears fMP4 (CODECS) but has no EXT-X-MAP",
                        pl.name
                    ),
                )
            };
            issues.push(sev_rule_msg);
        }
    }

    // §8.23 / §8.25 — EXT-X-INDEPENDENT-SEGMENTS for xHE-AAC and APAC. §8.22 is the rule
    // about aligned segment boundaries across renditions, so each codec is reported under
    // the section that names it. Both are conditional on the segments starting with an IPF
    // or an ASP, which cannot be read from the playlist, hence the wording.
    for pl in ctx.playlists {
        let codecs = pl.codecs.as_deref().unwrap_or("").to_ascii_lowercase();
        let cited = if codecs.contains("mp4a.40.42") {
            Some(("8.23", "xHE-AAC", "an Immediate Playout Frame (IPF)"))
        } else if codecs.contains("apac") {
            Some(("8.25", "APAC", "an Audio Synchronization Packet (ASP)"))
        } else {
            None
        };
        let Some((section, codec, opener)) = cited else {
            continue;
        };
        if !pl.independent_segments && !master.independent_segments {
            issues.push(author_warn(
                section,
                format!(
                    "'{}' carries {codec} without EXT-X-INDEPENDENT-SEGMENTS, which SHOULD be present when its segments start with {opener}",
                    pl.name
                ),
            ));
        }
    }

    // §8.18 — media playlist requests SHOULD NOT be redirected. A client resolves every
    // relative URI against the response URL, so a redirect silently moves the base of the
    // whole rendition and costs a round trip on each reload.
    issues.extend(check_playlist_redirects(ctx));

    // §8.24 — a live playlist has to be regenerated often enough to stay ahead of the
    // player, so Last-Modified may not trail the response Date by more than three
    // target durations.
    issues.extend(check_live_playlist_freshness(ctx));

    // AirPlay 8.25 — additional alignment MUST-ish
    if ctx.policy.profile == super::profile::AuthorProfile::AirPlay2 {
        // already covered by alignment checks; reinforce
    }

    issues
}

/// §8.18 — at most one finding per stream: one CDN or origin rule redirects every
/// rendition, so listing each variant separately would say the same thing N times.
fn check_playlist_redirects(ctx: &AuthoringContext<'_>) -> Option<Issue> {
    let redirected: Vec<&MediaPlaylist> = ctx
        .playlists
        .iter()
        .filter(|pl| {
            let meta = &pl.http_meta;
            !meta.request_url.is_empty()
                && !meta.final_url.is_empty()
                && meta.request_url != meta.final_url
        })
        .collect();

    let first = redirected.first()?;
    let more = match redirected.len() {
        1 => String::new(),
        n => format!(" (and {} more)", n - 1),
    };
    Some(author_warn(
        "8.18",
        format!(
            "media playlist '{}'{more} was redirected ('{}' → '{}'); media playlist requests \
             SHOULD NOT be redirected, because the client resolves relative URIs against the \
             final URL and pays the extra round trip on every reload",
            first.name, first.http_meta.request_url, first.http_meta.final_url
        ),
    ))
}

/// §8.24 — at most one finding per stream, naming the stalest live playlist.
fn check_live_playlist_freshness(ctx: &AuthoringContext<'_>) -> Option<Issue> {
    /// How many target durations Last-Modified may trail the response Date by.
    const MAX_AGE_IN_TARGET_DURATIONS: f64 = 3.0;

    let mut stale: Vec<(&str, f64, f64)> = Vec::new();
    for pl in ctx.playlists {
        let live = !pl.has_endlist && pl.playlist_type.as_deref() != Some("VOD");
        if !live || pl.target_duration <= 0.0 {
            continue;
        }
        let (Some(last_modified), Some(date)) =
            (&pl.http_meta.last_modified, &pl.http_meta.date)
        else {
            continue;
        };
        let (Some(last_modified), Some(date)) =
            (http_date_to_epoch(last_modified), http_date_to_epoch(date))
        else {
            continue;
        };
        let age = date - last_modified;
        let allowed = pl.target_duration * MAX_AGE_IN_TARGET_DURATIONS;
        if age > allowed {
            stale.push((pl.name.as_str(), age, allowed));
        }
    }
    stale.sort_by(|a, b| b.1.total_cmp(&a.1));

    let &(name, age, allowed) = stale.first()?;
    let more = match stale.len() {
        1 => String::new(),
        n => format!(" (and {} more)", n - 1),
    };
    Some(author_warn(
        "8.24",
        format!(
            "live playlist '{name}'{more} was last modified {age:.0}s before its response Date, \
             more than three target durations ({allowed:.0}s), so the client was served a stale \
             playlist"
        ),
    ))
}

/// Month name from an HTTP-date to its 1-based number.
fn http_month(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let name = name.to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|m| name.starts_with(m))
        .map(|i| i as u32 + 1)
}

/// Epoch seconds for an HTTP-date such as `Sun, 06 Nov 1994 08:49:37 GMT`. The value is
/// reshaped into ISO 8601 so the playlist parser's date maths is the only implementation.
/// A header that already holds an ISO 8601 timestamp is parsed directly.
fn http_date_to_epoch(value: &str) -> Option<f64> {
    use crate::utils::validator::parser::parse_iso8601_to_epoch;

    let value = value.trim();
    let after_weekday = value.split_once(',').map_or(value, |(_, rest)| rest.trim());
    let mut fields = after_weekday.split_whitespace();
    let parsed = (|| {
        let day = fields.next()?;
        let month = http_month(fields.next()?)?;
        let year = fields.next()?;
        let time = fields.next()?;
        if day.len() > 2 || year.len() != 4 || time.len() != 8 {
            return None;
        }
        parse_iso8601_to_epoch(&format!("{year}-{month:02}-{day:0>2}T{time}Z"))
    })();
    parsed.or_else(|| parse_iso8601_to_epoch(value))
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
    // Both rules read only the HTTP response, so they apply to a playlist opened directly.
    issues.extend(check_playlist_redirects(ctx));
    issues.extend(check_live_playlist_freshness(ctx));
    issues
}

#[cfg(test)]
mod tests {
    use super::super::context::{
        InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample,
    };
    use super::*;
    use crate::utils::validator::parser::{parse_master_playlist, parse_media_playlist};
    use crate::utils::validator::types::{MasterPlaylist, Severity};

    fn master() -> MasterPlaylist {
        parse_master_playlist(
            "https://example.com/master.m3u8",
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS="avc1.4d401f,mp4a.40.2",FRAME-RATE=30
https://example.com/v.m3u8
"#,
        )
    }

    /// Parse `content` as the sole video playlist and keep only findings for `section`.
    fn playlist_issues(content: &str, section: &str) -> Vec<Issue> {
        let mut pl = MediaPlaylist::new("video/1280x720".into(), "https://example.com/v.m3u8".into());
        parse_media_playlist("https://example.com/v.m3u8", content, &mut pl);
        pl.media_type = "VIDEO".into();
        let master = master();
        let playlists = vec![pl];
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

    const VOD_BODY: &str = "#EXTINF:6.00000,\n0.m4s\n#EXTINF:6.00000,\n1.m4s\n#EXT-X-ENDLIST\n";

    #[test]
    fn author_8_6_errors_when_endlist_has_no_playlist_type() {
        let issues = playlist_issues(
            &format!("#EXTM3U\n#EXT-X-TARGETDURATION:6\n{VOD_BODY}"),
            "§8.6",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("EXT-X-PLAYLIST-TYPE:VOD"),
            "expected the required tag in the message, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_8_6_accepts_endlist_with_playlist_type_vod() {
        let issues = playlist_issues(
            &format!("#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-PLAYLIST-TYPE:VOD\n{VOD_BODY}"),
            "§8.6",
        );
        assert!(
            issues.is_empty(),
            "a declared VOD playlist satisfies §8.6, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    /// Live window with a discontinuity, optionally declaring the sequence tag.
    fn live_with_discontinuity(disc_seq: Option<u64>) -> String {
        let seq = disc_seq
            .map(|n| format!("#EXT-X-DISCONTINUITY-SEQUENCE:{n}\n"))
            .unwrap_or_default();
        let mut body = String::new();
        for i in 0..6 {
            if i == 3 {
                body.push_str("#EXT-X-DISCONTINUITY\n");
            }
            body.push_str(&format!(
                "#EXT-X-PROGRAM-DATE-TIME:2026-01-01T00:00:{i:02}.000Z\n#EXTINF:6.00000,\n{i}.m4s\n"
            ));
        }
        format!("#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-MEDIA-SEQUENCE:10\n{seq}{body}")
    }

    #[test]
    fn author_8_17_errors_when_live_discontinuity_has_no_sequence_tag() {
        let issues = playlist_issues(&live_with_discontinuity(None), "§8.17");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("EXT-X-DISCONTINUITY-SEQUENCE"),
            "expected the required tag in the message, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_8_17_accepts_declared_discontinuity_sequence_of_zero() {
        let issues = playlist_issues(&live_with_discontinuity(Some(0)), "§8.17");
        assert!(
            issues.is_empty(),
            "an explicit DISCONTINUITY-SEQUENCE:0 satisfies §8.17, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    fn issues_for(playlists: &[MediaPlaylist], section: &str) -> Vec<Issue> {
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

    /// A fetched playlist whose request landed on `final_url`.
    fn fetched(name: &str, request_url: &str, final_url: &str) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.into(), request_url.into());
        pl.media_type = "VIDEO".into();
        pl.http_meta.request_url = request_url.into();
        pl.http_meta.final_url = final_url.into();
        pl
    }

    #[test]
    fn author_8_18_reports_a_redirected_media_playlist_once() {
        let issues = issues_for(
            &[
                fetched(
                    "video/1280x720",
                    "https://example.com/v.m3u8",
                    "https://cdn.example.net/v.m3u8",
                ),
                fetched(
                    "video/640x360",
                    "https://example.com/v360.m3u8",
                    "https://cdn.example.net/v360.m3u8",
                ),
            ],
            "§8.18",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(
            issues[0].message.contains("cdn.example.net/v.m3u8") && issues[0].message.contains("and 1 more"),
            "expected one collapsed finding naming the redirect target, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_8_18_stays_quiet_without_a_redirect() {
        let served = fetched(
            "video/1280x720",
            "https://example.com/v.m3u8",
            "https://example.com/v.m3u8",
        );
        // A playlist that was never fetched over HTTP carries no URLs to compare.
        let synthetic = MediaPlaylist::new("video/640x360".into(), "https://example.com/v360.m3u8".into());
        assert!(issues_for(&[served, synthetic], "§8.18").is_empty());
    }

    /// Live playlist whose response reported `last_modified` and `date`.
    fn live_with_dates(name: &str, target_duration: f64, last_modified: &str, date: &str) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.into(), format!("https://example.com/{name}.m3u8"));
        pl.media_type = "VIDEO".into();
        pl.target_duration = target_duration;
        pl.http_meta.last_modified = Some(last_modified.into());
        pl.http_meta.date = Some(date.into());
        pl
    }

    #[test]
    fn author_8_24_reports_a_live_playlist_older_than_three_target_durations() {
        let issues = issues_for(
            &[live_with_dates(
                "video/1280x720",
                6.0,
                "Thu, 01 Jan 2026 00:00:00 GMT",
                "Thu, 01 Jan 2026 00:00:30 GMT",
            )],
            "§8.24",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(
            issues[0].message.contains("30s") && issues[0].message.contains("18s"),
            "expected the measured age and the allowance, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_8_24_accepts_a_playlist_refreshed_within_three_target_durations() {
        let issues = issues_for(
            &[live_with_dates(
                "video/1280x720",
                6.0,
                "Thu, 01 Jan 2026 00:00:00 GMT",
                "Thu, 01 Jan 2026 00:00:12 GMT",
            )],
            "§8.24",
        );
        assert!(
            issues.is_empty(),
            "12s is within three 6s target durations, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn author_8_24_ignores_vod_playlists() {
        let mut vod = live_with_dates(
            "video/1280x720",
            6.0,
            "Thu, 01 Jan 2026 00:00:00 GMT",
            "Fri, 02 Jan 2026 00:00:00 GMT",
        );
        vod.has_endlist = true;
        vod.playlist_type = Some("VOD".into());
        assert!(issues_for(&[vod], "§8.24").is_empty());
    }

    #[test]
    fn parses_http_dates_and_rejects_junk() {
        let epoch = http_date_to_epoch("Thu, 01 Jan 1970 00:00:00 GMT").expect("imf-fixdate");
        assert!(epoch.abs() < f64::EPSILON, "got {epoch}");
        let unpadded = http_date_to_epoch("Sun, 6 Nov 1994 08:49:37 GMT").expect("unpadded day");
        let padded = http_date_to_epoch("Sun, 06 Nov 1994 08:49:37 GMT").expect("padded day");
        assert_eq!(unpadded, padded);
        assert_eq!(
            http_date_to_epoch("2026-01-01T00:00:30.000Z"),
            Some(1_767_225_630.0)
        );
        assert!(http_date_to_epoch("not a date").is_none());
    }
}
