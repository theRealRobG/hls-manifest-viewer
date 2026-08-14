//! Apple Authoring Spec §10–12 — Delivery / Privacy / Security.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;
use url::Url;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();

    // §10.1 — Content-Encoding: gzip for playlists
    issues.extend(check_playlist_content_encoding(ctx));

    // §10.2 — duplicate / failover variants SHOULD
    if let Some(master) = ctx.master {
        use std::collections::HashMap;
        let mut by_key: HashMap<String, usize> = HashMap::new();
        for v in master.variants.iter().filter(|v| !v.is_iframe) {
            let key = format!(
                "{}|{}|{}",
                v.resolution.as_deref().unwrap_or(""),
                v.bandwidth.unwrap_or(0),
                v.codecs.as_deref().unwrap_or("")
            );
            *by_key.entry(key).or_default() += 1;
        }
        let has_dup = by_key.values().any(|&n| n >= 2);
        if !has_dup && master.variants.iter().filter(|v| !v.is_iframe).count() >= 2 {
            issues.push(author_warn(
                "10.2",
                "no duplicate/failover variants with matching RESOLUTION/BANDWIDTH/CODECS",
            ));
        }
    }

    // §10.3 — localhost / file: media URLs
    for pl in ctx.playlists {
        for seg in &pl.segments {
            if looks_local(&seg.uri) {
                issues.push(author_warn(
                    "10.3",
                    format!("media URL looks like localhost/file: '{}'", seg.uri),
                ));
                break;
            }
        }
        if looks_local(&pl.url) {
            issues.push(author_warn(
                "10.3",
                format!("playlist URL looks like localhost/file: '{}'", pl.url),
            ));
        }
    }
    if let Some(m) = ctx.master {
        if looks_local(&m.url) {
            issues.push(author_warn(
                "10.3",
                format!("master URL looks like localhost/file: '{}'", m.url),
            ));
        }
    }

    // §11.1–11.3 — https SHOULD
    check_https(&mut issues, "11.1", ctx.master.map(|m| m.url.as_str()));
    for pl in ctx.playlists {
        check_https(&mut issues, "11.2", Some(&pl.url));
        if let Some(seg) = pl.segments.first() {
            check_https(&mut issues, "11.3", Some(&seg.uri));
        }
    }

    // §11.4 — content-revealing Media Segment URLs over an unencrypted transport
    issues.extend(check_revealing_segment_urls(ctx));

    // §12.1–12.3 — TLS not available in-browser (documented via probe_notes)
    // Emit a single Info so it appears in Author Security group
    issues.push(author_info(
        "12.1",
        "TLS cipher/certificate validation not available in-browser",
    ));

    // §12.4 — segment URLs SHOULD NOT be completely static.
    // Signed / tokenized URLs satisfy the rule and are never penalized. A single manifest
    // snapshot cannot prove that URLs rotate per device, so the only positive evidence of a
    // static scheme is segment URLs with no query at all.
    issues.extend(check_static_segment_urls(ctx));

    issues
}

/// §10.1 — playlists SHOULD be served gzip-encoded. A browser that accepted a gzip response
/// decodes it transparently and drops Content-Encoding from the Response we can read, so an
/// absent header proves nothing: report it once for the whole stream as Info. A header naming
/// some other encoding was really sent and is still worth an error.
fn check_playlist_content_encoding(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut metas = Vec::new();
    if let Some(m) = ctx.master_http {
        metas.push(("master", m));
    }
    for pl in ctx.playlists {
        metas.push((pl.name.as_str(), &pl.http_meta));
    }

    let mut unobserved = 0usize;
    for (name, meta) in &metas {
        if meta.request_url.is_empty() && meta.final_url.is_empty() {
            continue;
        }
        let enc = meta
            .content_encoding
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();
        if enc.is_empty() {
            unobserved += 1;
        } else if !enc.split(',').any(|e| e.trim() == "gzip") {
            issues.push(author_error(
                "10.1",
                format!("playlist '{name}' Content-Encoding is '{enc}', expected gzip"),
            ));
        }
    }
    if unobserved > 0 {
        issues.push(author_info(
            "10.1",
            format!(
                "no Content-Encoding observed on {unobserved} playlist response(s) — browsers decode gzip transparently and hide the header, so verify gzip delivery on the server or with curl"
            ),
        ));
    }
    issues
}

/// §11.4 — a Media Segment URL delivered over an unencrypted transport is visible to anyone
/// on the path, so it SHOULD NOT spell out what is being watched. Only obvious content
/// descriptors in the URL path are flagged; a signed or tokenized query is not a §11.4 concern.
fn check_revealing_segment_urls(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in ctx.playlists {
        let insecure_fallback = pl.url.to_ascii_lowercase().starts_with("http://");
        let revealing = pl.segments.iter().find_map(|seg| {
            if !segment_uri_is_insecure(&seg.uri, insecure_fallback) {
                return None;
            }
            revealing_path_token(&seg.uri).map(|token| (seg.uri.as_str(), token))
        });
        if let Some((uri, token)) = revealing {
            issues.push(author_warn(
                "11.4",
                format!(
                    "media segment URL over http reveals content ('{token}') on '{}' in '{}'; use https or a URL path that does not describe the content",
                    uri, pl.name
                ),
            ));
        }
    }
    issues
}

/// Whether a segment URI is fetched over plain http. A relative URI inherits the scheme of
/// the playlist that lists it.
fn segment_uri_is_insecure(uri: &str, playlist_is_insecure: bool) -> bool {
    let lower = uri.to_ascii_lowercase();
    if lower.starts_with("http://") {
        return true;
    }
    if lower.contains("://") || lower.starts_with("data:") || lower.starts_with("blob:") {
        return false;
    }
    playlist_is_insecure
}

/// The first content-describing word in a segment URL's path, if any. Words are matched whole
/// with any trailing digits removed, so `episode12` counts while `subtitles` is not read as a
/// `title`.
fn revealing_path_token(uri: &str) -> Option<String> {
    const MARKERS: &[&str] = &[
        "episode", "episodes", "season", "series", "show", "showname", "movie", "film", "title",
        "genre", "trailer",
    ];
    let path = url_path(uri).to_ascii_lowercase();
    path.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .find(|word| {
            let base = word.trim_end_matches(|c: char| c.is_ascii_digit());
            MARKERS.contains(&base) || is_season_episode(word)
        })
        .map(str::to_string)
}

/// The path of a URI, without scheme, host or query — the part an author chooses per asset.
fn url_path(uri: &str) -> &str {
    let without_query = uri.split('?').next().unwrap_or(uri);
    match without_query.split_once("://") {
        Some((_, rest)) => rest.split_once('/').map(|(_, path)| path).unwrap_or(""),
        None => without_query,
    }
}

/// Whether a path word is a `s01e02`-style season/episode designator.
fn is_season_episode(word: &str) -> bool {
    let Some(rest) = word.strip_prefix('s') else {
        return false;
    };
    let Some((season, episode)) = rest.split_once('e') else {
        return false;
    };
    !season.is_empty()
        && !episode.is_empty()
        && season.chars().all(|c| c.is_ascii_digit())
        && episode.chars().all(|c| c.is_ascii_digit())
}

/// At most one §12.4 finding for the whole stream, so a static URL scheme is reported once.
fn check_static_segment_urls(ctx: &AuthoringContext<'_>) -> Option<Issue> {
    /// Segments needed before a playlist's URL scheme is worth judging.
    const MIN_SEGMENTS: usize = 4;

    let mut inspected = 0usize;
    let mut any_signed = false;
    let mut any_query = false;
    for pl in ctx.playlists.iter().filter(|p| p.segments.len() >= MIN_SEGMENTS) {
        inspected += 1;
        for seg in &pl.segments {
            let Some((_, query)) = seg.uri.split_once('?') else {
                continue;
            };
            if query.is_empty() {
                continue;
            }
            any_query = true;
            if query.split('&').any(looks_like_signing_param) {
                any_signed = true;
            }
        }
    }
    if inspected == 0 || any_signed {
        return None;
    }
    if any_query {
        Some(author_info(
            "12.4",
            "segment URLs carry query parameters but no recognizable expiry/token/signature — confirm they are per-device or rotating",
        ))
    } else {
        Some(author_warn(
            "12.4",
            "segment URLs look completely static (no expiry, token or signature) — they SHOULD be per-device or rotating",
        ))
    }
}

/// Whether a `name=value` query pair names a per-device / expiring credential.
fn looks_like_signing_param(pair: &str) -> bool {
    const MARKERS: &[&str] = &[
        "exp", "token", "sig", "hmac", "auth", "session", "policy", "jwt", "nonce", "hdnts",
        "secure",
    ];
    let name = pair.split('=').next().unwrap_or("").to_ascii_lowercase();
    !name.is_empty() && MARKERS.iter().any(|m| name.contains(m))
}

fn looks_local(uri: &str) -> bool {
    let u = uri.to_ascii_lowercase();
    u.starts_with("file:")
        || u.contains("://127.0.0.1")
        || u.contains("://localhost")
        || u.starts_with("127.0.0.1")
        || u.starts_with("localhost")
}

fn check_https(issues: &mut Vec<Issue>, section: &str, url: Option<&str>) {
    let Some(url) = url else { return };
    if url.is_empty() {
        return;
    }
    if let Ok(parsed) = Url::parse(url) {
        if parsed.scheme() != "https" && parsed.scheme() != "blob" && parsed.scheme() != "data" {
            issues.push(author_warn(
                section,
                format!("URL SHOULD use https: '{url}'"),
            ));
        }
    } else if url.starts_with("http://") {
        issues.push(author_warn(
            section,
            format!("URL SHOULD use https: '{url}'"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::super::context::{
        InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample,
    };
    use super::*;
    use crate::utils::validator::types::{MediaPlaylist, Segment, Severity};

    fn segment(uri: &str) -> Segment {
        Segment {
            uri: uri.into(),
            duration: 4.0,
            title: None,
            pdt: None,
            discontinuity: false,
            byterange: None,
            is_ad: false,
            map_uri: None,
        }
    }

    fn video_playlist(name: &str, uri_for: impl Fn(usize) -> String) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.into(), "https://example.com/v.m3u8".into());
        pl.media_type = "VIDEO".into();
        pl.target_duration = 4.0;
        pl.has_endlist = true;
        for i in 0..5 {
            pl.segments.push(segment(&uri_for(i)));
        }
        pl
    }

    fn issues_for_section(playlists: &[MediaPlaylist], section: &str) -> Vec<Issue> {
        let opts = ValidateAuthorOptions::default();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(None, playlists, &opts, &inits, &segs, &vtts);
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains(section))
            .collect()
    }

    fn issues_for(playlists: &[MediaPlaylist]) -> Vec<Issue> {
        issues_for_section(playlists, "§12.4")
    }

    /// A fetched playlist: `url` is where it came from, `segments` are listed as written.
    fn fetched_playlist(name: &str, url: &str, segments: &[&str]) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.into(), url.into());
        pl.media_type = "VIDEO".into();
        pl.target_duration = 4.0;
        pl.has_endlist = true;
        pl.http_meta.request_url = url.into();
        pl.http_meta.final_url = url.into();
        pl.http_meta.content_encoding = Some("gzip".into());
        for uri in segments {
            pl.segments.push(segment(uri));
        }
        pl
    }

    #[test]
    fn author_12_4_does_not_penalize_signed_segment_urls() {
        let playlists = vec![video_playlist("video/720p", |i| {
            format!("https://example.com/{i}.m4s?exp=1712000000&hmac=abc123")
        })];
        assert!(
            issues_for(&playlists).is_empty(),
            "signed segment URLs satisfy §12.4"
        );
    }

    #[test]
    fn author_12_4_warns_once_for_static_segment_urls() {
        let playlists = vec![
            video_playlist("video/720p", |i| format!("https://example.com/720/{i}.m4s")),
            video_playlist("video/1080p", |i| {
                format!("https://example.com/1080/{i}.m4s")
            }),
        ];
        let issues = issues_for(&playlists);
        assert_eq!(issues.len(), 1, "§12.4 reports the stream once: {issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(issues[0].message.contains("completely static"));
    }

    #[test]
    fn author_12_4_notes_unrecognized_query_parameters() {
        let playlists = vec![video_playlist("video/720p", |i| {
            format!("https://example.com/{i}.m4s?cdn=edge1")
        })];
        let issues = issues_for(&playlists);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Info);
    }

    #[test]
    fn author_12_4_skips_short_playlists() {
        let mut pl = MediaPlaylist::new("video/720p".into(), "https://example.com/v.m3u8".into());
        pl.media_type = "VIDEO".into();
        pl.segments.push(segment("https://example.com/0.m4s"));
        assert!(issues_for(&[pl]).is_empty());
    }

    #[test]
    fn author_10_1_reports_unobservable_encoding_once() {
        let playlists: Vec<MediaPlaylist> = ["video/720p", "video/1080p", "audio/en"]
            .iter()
            .map(|name| {
                let mut pl =
                    fetched_playlist(name, "https://example.com/v.m3u8", &["https://e/0.m4s"]);
                pl.http_meta.content_encoding = None;
                pl
            })
            .collect();
        let issues = issues_for_section(&playlists, "§10.1");
        assert_eq!(issues.len(), 1, "one report for the stream: {issues:?}");
        assert_eq!(issues[0].severity, Severity::Info);
        assert!(issues[0].message.contains("3 playlist response(s)"));
    }

    #[test]
    fn author_10_1_still_errors_on_a_non_gzip_encoding() {
        let mut pl = fetched_playlist("video/720p", "https://example.com/v.m3u8", &["0.m4s"]);
        pl.http_meta.content_encoding = Some("br".into());
        let issues = issues_for_section(&[pl], "§10.1");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(issues[0].message.contains("'br'"));
    }

    #[test]
    fn author_10_1_accepts_gzip() {
        let pl = fetched_playlist("video/720p", "https://example.com/v.m3u8", &["0.m4s"]);
        assert!(issues_for_section(&[pl], "§10.1").is_empty());
    }

    #[test]
    fn author_11_4_flags_content_revealing_segment_path_over_http() {
        let pl = fetched_playlist(
            "video/720p",
            "http://example.com/v.m3u8",
            &["season2/episode4/0.m4s"],
        );
        let issues = issues_for_section(&[pl], "§11.4");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(issues[0].message.contains("season2"));
    }

    #[test]
    fn author_11_4_ignores_content_revealing_paths_over_https() {
        let pl = fetched_playlist(
            "video/720p",
            "https://example.com/v.m3u8",
            &["https://example.com/episode4/0.m4s"],
        );
        assert!(issues_for_section(&[pl], "§11.4").is_empty());
    }

    #[test]
    fn author_11_4_ignores_auth_query_parameters() {
        let pl = fetched_playlist(
            "video/720p",
            "http://example.com/v.m3u8",
            &["http://example.com/a/0.m4s?token=abc&sig=def"],
        );
        assert!(
            issues_for_section(&[pl], "§11.4").is_empty(),
            "signed URLs are not a §11.4 disclosure"
        );
    }

    #[test]
    fn author_11_4_flags_a_season_episode_designator() {
        let pl = fetched_playlist(
            "video/720p",
            "http://example.com/v.m3u8",
            &["http://example.com/s01e02/0.m4s"],
        );
        let issues = issues_for_section(&[pl], "§11.4");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].message.contains("s01e02"));
    }

    #[test]
    fn author_11_4_ignores_the_host_name() {
        let pl = fetched_playlist(
            "video/720p",
            "http://show.example.com/v.m3u8",
            &["http://show.example.com/a/0.m4s"],
        );
        assert!(
            issues_for_section(&[pl], "§11.4").is_empty(),
            "only the path an author chooses per asset is inspected"
        );
    }

    #[test]
    fn author_11_4_does_not_read_subtitles_as_a_title() {
        let pl = fetched_playlist(
            "subtitles/English (subs)",
            "http://example.com/s.m3u8",
            &["subtitles/en/0.vtt"],
        );
        assert!(issues_for_section(&[pl], "§11.4").is_empty());
    }
}
