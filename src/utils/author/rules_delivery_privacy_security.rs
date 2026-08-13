//! Apple Authoring Spec §10–12 — Delivery / Privacy / Security.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;
use url::Url;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();

    // §10.1 — Content-Encoding: gzip for playlists
    let mut metas = Vec::new();
    if let Some(m) = ctx.master_http {
        metas.push(("master", m));
    }
    for pl in ctx.playlists {
        metas.push((pl.name.as_str(), &pl.http_meta));
    }
    for (name, meta) in &metas {
        if meta.request_url.is_empty() && meta.final_url.is_empty() {
            continue;
        }
        let enc = meta
            .content_encoding
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();
        // Browsers may transparently decode gzip and omit Content-Encoding on the Response.
        // Treat missing encoding as Warn (not Error) when we cannot observe it reliably.
        if enc.is_empty() {
            issues.push(author_warn(
                "10.1",
                format!(
                    "playlist '{name}' response Content-Encoding is not 'gzip' (observed: none; browsers may hide transparent decode)"
                ),
            ));
        } else if !enc.split(',').any(|e| e.trim() == "gzip") {
            issues.push(author_error(
                "10.1",
                format!(
                    "playlist '{name}' Content-Encoding is '{enc}', expected gzip"
                ),
            ));
        }
    }

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

    // §11.4 — revealing path tokens over http
    for pl in ctx.playlists {
        let url = &pl.url;
        if url.starts_with("http://") {
            let lower = url.to_ascii_lowercase();
            if lower.contains("token=")
                || lower.contains("sig=")
                || lower.contains("auth=")
                || lower.contains("session")
            {
                issues.push(author_warn(
                    "11.4",
                    format!("revealing auth/token query over http on '{}'", pl.url),
                ));
            }
        }
    }

    // §12.1–12.3 — TLS not available in-browser (documented via probe_notes)
    // Emit a single Info so it appears in Author Security group
    issues.push(author_info(
        "12.1",
        "TLS cipher/certificate validation not available in-browser",
    ));

    // §12.4 — non-static segment URL patterns (weak heuristic)
    for pl in ctx.playlists {
        if pl.segments.len() < 3 {
            continue;
        }
        let mut dynamic = 0;
        for seg in &pl.segments {
            let u = seg.uri.to_ascii_lowercase();
            if u.contains("?")
                && (u.contains("exp=")
                    || u.contains("token=")
                    || u.contains("signature=")
                    || u.contains("hmac="))
            {
                dynamic += 1;
            }
        }
        if dynamic >= 2 {
            issues.push(author_warn(
                "12.4",
                format!(
                    "'{}' uses frequently rotating signed segment URLs (ensure CDN caching still works)",
                    pl.name
                ),
            ));
        }
    }

    issues
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
