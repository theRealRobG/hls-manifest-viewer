use std::collections::HashMap;
use url::Url;
use super::types::*;

/// Parse HLS attribute string: KEY=VALUE,KEY="VALUE",... into HashMap
pub fn parse_attributes(attr_string: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    let mut remaining = attr_string.trim();
    while !remaining.is_empty() {
        // Find key
        let eq_pos = match remaining.find('=') {
            Some(p) => p,
            None => break,
        };
        let key = remaining[..eq_pos].trim().to_uppercase();
        remaining = &remaining[eq_pos + 1..];
        // Parse value
        let value;
        if remaining.starts_with('"') {
            remaining = &remaining[1..];
            let end_quote = remaining.find('"').unwrap_or(remaining.len());
            value = remaining[..end_quote].to_string();
            remaining = if end_quote + 1 < remaining.len() {
                &remaining[end_quote + 1..]
            } else {
                ""
            };
        } else {
            let comma_pos = remaining.find(',').unwrap_or(remaining.len());
            value = remaining[..comma_pos].trim().to_string();
            remaining = if comma_pos < remaining.len() {
                &remaining[comma_pos..]
            } else {
                ""
            };
        }
        attrs.insert(key, value);
        // Skip comma
        remaining = remaining.trim_start_matches(',').trim_start();
    }
    attrs
}

/// Resolve a potentially relative URL against a base URL using RFC 3986 semantics
/// (same behavior as `url::Url::join` used by the manifest viewer).
pub fn resolve_url(base_url: &str, relative: &str) -> String {
    if let Ok(base) = Url::parse(base_url)
        && let Ok(joined) = base.join(relative)
    {
        return joined.to_string();
    }
    // Fallback for non-absolute bases used in unit tests (e.g. "master.m3u8").
    if relative.starts_with("http://") || relative.starts_with("https://") {
        return relative.to_string();
    }
    if let Some(last_slash) = base_url.rfind('/') {
        format!("{}/{}", &base_url[..last_slash], relative)
    } else {
        relative.to_string()
    }
}

/// Extract the value of a named query parameter from a URL string.
fn extract_query_param(url: &str, param: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k == param { Some(v.to_string()) } else { None }
    })
}

/// Parse a master playlist and extract variant stream info
pub fn parse_master_playlist(url: &str, content: &str) -> MasterPlaylist {
    let mut master = MasterPlaylist {
        url: url.to_string(),
        raw_content: content.to_string(),
        version: 1,
        variants: Vec::new(),
        media_renditions: Vec::new(),
        definitions: HashMap::new(),
    };
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("#EXT-X-VERSION:") {
            if let Ok(v) = line.split_once(':').unwrap().1.trim().parse::<u32>() {
                master.version = v;
            }
        } else if let Some(attr_str) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let attrs = parse_attributes(attr_str);
            i += 1;
            let uri = if i < lines.len() { lines[i].trim().to_string() } else { String::new() };
            master.variants.push(MasterRendition {
                uri: resolve_url(url, &uri),
                bandwidth: attrs.get("BANDWIDTH").and_then(|v| v.parse().ok()),
                average_bandwidth: attrs.get("AVERAGE-BANDWIDTH").and_then(|v| v.parse().ok()),
                codecs: attrs.get("CODECS").cloned(),
                resolution: attrs.get("RESOLUTION").cloned(),
                frame_rate: attrs.get("FRAME-RATE").and_then(|v| v.parse().ok()),
                audio_group: attrs.get("AUDIO").cloned(),
                subtitle_group: attrs.get("SUBTITLES").cloned(),
                closed_captions: attrs.get("CLOSED-CAPTIONS").cloned(),
                video_range: attrs.get("VIDEO-RANGE").cloned(),
                is_iframe: false,
            });
        } else if let Some(attr_str) = line.strip_prefix("#EXT-X-I-FRAME-STREAM-INF:") {
            let attrs = parse_attributes(attr_str);
            if let Some(uri) = attrs.get("URI") {
                master.variants.push(MasterRendition {
                    uri: resolve_url(url, uri),
                    bandwidth: attrs.get("BANDWIDTH").and_then(|v| v.parse().ok()),
                    average_bandwidth: attrs.get("AVERAGE-BANDWIDTH").and_then(|v| v.parse().ok()),
                    codecs: attrs.get("CODECS").cloned(),
                    resolution: attrs.get("RESOLUTION").cloned(),
                    frame_rate: attrs.get("FRAME-RATE").and_then(|v| v.parse().ok()),
                    audio_group: None,
                    subtitle_group: None,
                    closed_captions: None,
                    video_range: attrs.get("VIDEO-RANGE").cloned(),
                    is_iframe: true,
                });
            }
        } else if let Some(attr_str) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = parse_attributes(attr_str);
            master.media_renditions.push(MediaRendition {
                media_type: attrs.get("TYPE").cloned().unwrap_or_default(),
                group_id: attrs.get("GROUP-ID").cloned().unwrap_or_default(),
                name: attrs.get("NAME").cloned().unwrap_or_default(),
                uri: attrs.get("URI").map(|u| resolve_url(url, u)),
                language: attrs.get("LANGUAGE").cloned(),
                is_default: attrs.get("DEFAULT").is_some_and(|v| v == "YES"),
                autoselect: attrs.get("AUTOSELECT").is_some_and(|v| v == "YES"),
                channels: attrs.get("CHANNELS").cloned(),
            });
        } else if let Some(rest) = line.strip_prefix("#EXT-X-DEFINE:") {
            let attrs = parse_attributes(rest);
            if let (Some(name), Some(value)) = (attrs.get("NAME"), attrs.get("VALUE")) {
                // Inline definition — store directly
                master.definitions.insert(name.clone(), value.clone());
            } else if let (Some(param), Some(name)) = (attrs.get("QUERYPARAM"), attrs.get("NAME")) {
                // QUERYPARAM — value comes from the named query parameter of the playlist URL
                if let Some(value) = extract_query_param(url, param) {
                    master.definitions.insert(name.clone(), value);
                }
            }
        }
        i += 1;
    }
    master
}

/// Parse a media playlist from raw content, populating the MediaPlaylist struct
pub fn parse_media_playlist(url: &str, content: &str, pl: &mut MediaPlaylist) {
    pl.url = url.to_string();
    pl.raw_content = content.to_string();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_duration: Option<f64> = None;
    let mut current_title: Option<String> = None;
    let mut current_pdt: Option<f64> = None;
    let mut current_discontinuity = false;
    let mut current_byterange: Option<String> = None;
    let mut current_map_uri: Option<String> = None;
    let mut cumulative_duration: f64 = 0.0;
    let mut last_pdt: Option<f64> = None;

    for line in &lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("#EXT-X-VERSION:") {
            if let Some(v) = line.split_once(':').and_then(|(_, v)| v.trim().parse::<u32>().ok()) {
                pl.version = v;
            }
        } else if line.starts_with("#EXT-X-TARGETDURATION:") {
            if let Some(v) = line.split_once(':').and_then(|(_, v)| v.trim().parse::<f64>().ok()) {
                pl.target_duration = v;
            }
        } else if line.starts_with("#EXT-X-MEDIA-SEQUENCE:") {
            if let Some(v) = line.split_once(':').and_then(|(_, v)| v.trim().parse::<u64>().ok()) {
                pl.media_sequence = v;
            }
        } else if line.starts_with("#EXT-X-DISCONTINUITY-SEQUENCE:") {
            if let Some(v) = line.split_once(':').and_then(|(_, v)| v.trim().parse::<u64>().ok()) {
                pl.discontinuity_sequence = v;
            }
        } else if line.starts_with("#EXT-X-PLAYLIST-TYPE:") {
            pl.playlist_type = line.split_once(':').map(|(_, v)| v.trim().to_string());
        } else if line.starts_with("#EXT-X-ENDLIST") {
            pl.has_endlist = true;
        } else if let Some(val) = line.strip_prefix("#EXTINF:") {
            let comma_pos = val.find(',').unwrap_or(val.len());
            current_duration = val[..comma_pos].trim().parse::<f64>().ok();
            if comma_pos < val.len() {
                let title = val[comma_pos + 1..].trim();
                if !title.is_empty() {
                    current_title = Some(title.to_string());
                }
            }
        } else if let Some(dt_str) = line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
            current_pdt = parse_iso8601_to_epoch(dt_str);
            if current_pdt.is_some() {
                // New PDT anchor — reset cumulative so extrapolation starts fresh from this tag
                last_pdt = current_pdt;
                cumulative_duration = 0.0;
            }
        } else if line.starts_with("#EXT-X-DISCONTINUITY") && !line.contains(':') {
            current_discontinuity = true;
        } else if let Some(rest) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            current_byterange = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            let attrs = parse_attributes(rest);
            current_map_uri = attrs.get("URI").map(|u| resolve_url(url, u));
        } else if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            let attrs = parse_attributes(rest);
            if let Some(method) = attrs.get("METHOD") {
                pl.encryption_methods.insert(method.clone());
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-SERVER-CONTROL:") {
            let attrs = parse_attributes(rest);
            pl.server_control = Some(ServerControl {
                can_skip_until: attrs.get("CAN-SKIP-UNTIL").and_then(|v| v.parse().ok()),
                hold_back: attrs.get("HOLD-BACK").and_then(|v| v.parse().ok()),
                part_hold_back: attrs.get("PART-HOLD-BACK").and_then(|v| v.parse().ok()),
                can_block_reload: attrs.get("CAN-BLOCK-RELOAD").is_some_and(|v| v == "YES"),
            });
        } else if let Some(rest) = line.strip_prefix("#EXT-X-PART-INF:") {
            let attrs = parse_attributes(rest);
            pl.part_target = attrs.get("PART-TARGET").and_then(|v| v.parse().ok());
        } else if let Some(rest) = line.strip_prefix("#EXT-X-PART:") {
            let attrs = parse_attributes(rest);
            if let Some(uri) = attrs.get("URI") {
                pl.parts.push(PartialSegment {
                    uri: resolve_url(url, uri),
                    duration: attrs.get("DURATION").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                    independent: attrs.get("INDEPENDENT").is_some_and(|v| v == "YES"),
                    gap: attrs.get("GAP").is_some_and(|v| v == "YES"),
                });
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-PRELOAD-HINT:") {
            let attrs = parse_attributes(rest);
            if let Some(uri) = attrs.get("URI") {
                pl.preload_hint_uri = Some(resolve_url(url, uri));
            }
            pl.preload_hint_type = attrs.get("TYPE").cloned();
        } else if let Some(rest) = line.strip_prefix("#EXT-X-RENDITION-REPORT:") {
            let attrs = parse_attributes(rest);
            let uri = attrs.get("URI").map(|u| resolve_url(url, u)).unwrap_or_default();
            let last_msn = attrs.get("LAST-MSN").and_then(|v| v.parse::<i64>().ok()).unwrap_or(-1);
            let last_part = attrs.get("LAST-PART").and_then(|v| v.parse::<i64>().ok()).unwrap_or(-1);
            pl.rendition_reports.push(super::types::RenditionReport { uri, last_msn, last_part });
        } else if let Some(rest) = line.strip_prefix("#EXT-X-DEFINE:") {
            let attrs = parse_attributes(rest);
            // Only handle NAME+VALUE inline definitions; IMPORT and QUERYPARAM cannot be resolved here
            if let (Some(name), Some(value)) = (attrs.get("NAME"), attrs.get("VALUE")) {
                pl.definitions.insert(name.clone(), value.clone());
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-SKIP:") {
            let attrs = parse_attributes(rest);
            if let Some(skipped) = attrs.get("SKIPPED-SEGMENTS").and_then(|v| v.parse::<u64>().ok()) {
                pl.skipped_segments = skipped;
            }
        } else if !line.starts_with('#') {
            // Segment URI
            if let Some(dur) = current_duration {
                // RFC 8216 §6.3.3: extrapolate PDT forward using cumulative EXTINF durations.
                // Discontinuity without a new explicit PDT tag resets the extrapolation anchor.
                let seg_pdt = if current_pdt.is_some() {
                    // Explicit PDT — anchor already reset in the PDT-tag handler above
                    current_pdt
                } else if current_discontinuity {
                    // Discontinuity with no new PDT: lose the extrapolation reference
                    last_pdt = None;
                    cumulative_duration = 0.0;
                    None
                } else { last_pdt.map(|lp| lp + cumulative_duration) };
                pl.segments.push(Segment {
                    uri: resolve_url(url, line),
                    duration: dur,
                    title: current_title.take(),
                    pdt: seg_pdt,
                    discontinuity: current_discontinuity,
                    byterange: current_byterange.take(),
                    is_ad: false,
                    map_uri: current_map_uri.clone(),
                });
                cumulative_duration += dur;
                current_duration = None;
                current_discontinuity = false;
                current_pdt = None;
            }
        }
    }
}

/// Parse ISO 8601 datetime string to epoch seconds (basic implementation)
pub fn parse_iso8601_to_epoch(s: &str) -> Option<f64> {
    // Handle format: 2024-01-15T12:30:45.123Z or 2024-01-15T12:30:45.123+00:00
    let s = s.trim();
    // Try to parse date and time components manually
    if s.len() < 19 {
        return None;
    }
    let year: i64 = s[0..4].parse().ok()?;
    let month: u32 = s[5..7].parse().ok()?;
    let day: u32 = s[8..10].parse().ok()?;
    let hour: u32 = s[11..13].parse().ok()?;
    let min: u32 = s[14..16].parse().ok()?;
    let sec: u32 = s[17..19].parse().ok()?;
    let mut frac: f64 = 0.0;
    let mut rest = &s[19..];
    if rest.starts_with('.') {
        let end = rest[1..].find(|c: char| !c.is_ascii_digit()).map_or(rest.len(), |p| p + 1);
        frac = format!("0{}", &rest[..end]).parse().unwrap_or(0.0);
        rest = &rest[end..];
    }
    // Calculate timezone offset in seconds
    let tz_offset: i64 = if rest.is_empty() || rest == "Z" {
        0
    } else if rest.starts_with('+') || rest.starts_with('-') {
        let sign: i64 = if rest.starts_with('-') { -1 } else { 1 };
        let tz = &rest[1..];
        let (h, m) = if tz.contains(':') {
            let parts: Vec<&str> = tz.split(':').collect();
            (parts[0].parse::<i64>().unwrap_or(0), parts.get(1).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0))
        } else if tz.len() >= 4 {
            (tz[..2].parse::<i64>().unwrap_or(0), tz[2..4].parse::<i64>().unwrap_or(0))
        } else {
            (tz.parse::<i64>().unwrap_or(0), 0)
        };
        sign * (h * 3600 + m * 60)
    } else {
        0
    };
    // Days from epoch (simplified - doesn't handle all edge cases but good enough)
    let days = days_from_epoch(year, month, day);
    let epoch = days as f64 * 86400.0 + hour as f64 * 3600.0 + min as f64 * 60.0 + sec as f64 + frac - tz_offset as f64;
    Some(epoch)
}

fn days_from_epoch(year: i64, month: u32, day: u32) -> i64 {
    let mut y = year;
    let mut m = month as i64;
    if m <= 2 {
        y -= 1;
        m += 12;
    }
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_attributes ──────────────────────────────────────────────────────

    #[test]
    fn parse_attributes_unquoted_values() {
        let attrs = parse_attributes("BANDWIDTH=3000000,RESOLUTION=1920x1080");
        assert_eq!(attrs.get("BANDWIDTH"), Some(&"3000000".to_string()));
        assert_eq!(attrs.get("RESOLUTION"), Some(&"1920x1080".to_string()));
    }

    #[test]
    fn parse_attributes_quoted_string() {
        let attrs = parse_attributes(r#"ID="break-1",START-DATE="2024-01-01T00:00:00Z""#);
        assert_eq!(attrs.get("ID"), Some(&"break-1".to_string()));
        assert_eq!(attrs.get("START-DATE"), Some(&"2024-01-01T00:00:00Z".to_string()));
    }

    #[test]
    fn parse_attributes_mixed_quoted_and_unquoted() {
        let attrs = parse_attributes(r#"BANDWIDTH=5000000,CODECS="avc1.64001f,mp4a.40.2",RESOLUTION=1920x1080"#);
        assert_eq!(attrs.get("BANDWIDTH"), Some(&"5000000".to_string()));
        assert_eq!(attrs.get("CODECS"), Some(&"avc1.64001f,mp4a.40.2".to_string()));
        assert_eq!(attrs.get("RESOLUTION"), Some(&"1920x1080".to_string()));
    }

    #[test]
    fn parse_attributes_empty_string() {
        let attrs = parse_attributes("");
        assert!(attrs.is_empty());
    }

    #[test]
    fn parse_attributes_keys_are_uppercased() {
        let attrs = parse_attributes("bandwidth=1000");
        assert!(attrs.contains_key("BANDWIDTH"));
        assert!(!attrs.contains_key("bandwidth"));
    }

    #[test]
    fn parse_attributes_quoted_value_with_commas() {
        // The value is a quoted URL with query parameters (& inside quotes must not split)
        let attrs = parse_attributes(r#"X-ASSET-LIST="https://example.com/ads.json?a=1&b=2",DURATION=30"#);
        assert_eq!(
            attrs.get("X-ASSET-LIST"),
            Some(&"https://example.com/ads.json?a=1&b=2".to_string())
        );
        assert_eq!(attrs.get("DURATION"), Some(&"30".to_string()));
    }

    // ── resolve_url ───────────────────────────────────────────────────────────

    #[test]
    fn resolve_url_absolute_returns_unchanged() {
        let base = "https://cdn.example.com/master.m3u8";
        let abs = "https://other.cdn.com/stream.m3u8";
        assert_eq!(resolve_url(base, abs), abs);
    }

    #[test]
    fn resolve_url_relative_joined_to_base_directory() {
        let base = "https://cdn.example.com/hls/master.m3u8";
        assert_eq!(
            resolve_url(base, "video/1080p.m3u8"),
            "https://cdn.example.com/hls/video/1080p.m3u8"
        );
    }

    #[test]
    fn resolve_url_parent_directory_normalized() {
        let base = "https://ads.com/1234/main/mvp.m3u8";
        assert_eq!(
            resolve_url(base, "../media/3.m3u8"),
            "https://ads.com/1234/media/3.m3u8"
        );
    }

    #[test]
    fn resolve_url_root_relative() {
        let base = "https://cdn.example.com/hls/master.m3u8";
        assert_eq!(
            resolve_url(base, "/abs/path.m3u8"),
            "https://cdn.example.com/abs/path.m3u8"
        );
    }

    #[test]
    fn resolve_url_relative_no_slash_in_base_returns_relative() {
        // base has no slash → falls back to returning the relative as-is
        assert_eq!(resolve_url("master.m3u8", "audio.m3u8"), "audio.m3u8");
    }

    #[test]
    fn resolve_url_http_prefix_is_recognised_as_absolute() {
        let abs = "http://insecure.cdn.com/stream.m3u8";
        assert_eq!(resolve_url("https://other.com/master.m3u8", abs), abs);
    }

    // ── parse_iso8601_to_epoch ────────────────────────────────────────────────

    #[test]
    fn parse_iso8601_unix_epoch() {
        // 1970-01-01T00:00:00Z → epoch 0
        let epoch = parse_iso8601_to_epoch("1970-01-01T00:00:00Z").expect("must parse");
        assert!((epoch - 0.0).abs() < 0.001);
    }

    #[test]
    fn parse_iso8601_utc_suffix() {
        // Known date: 2024-01-15T12:30:45Z
        // days_from_epoch(2024,1,15) = 19737; seconds = 19737*86400 + 12*3600 + 30*60 + 45
        let expected = 19737.0 * 86400.0 + 12.0 * 3600.0 + 30.0 * 60.0 + 45.0;
        let epoch = parse_iso8601_to_epoch("2024-01-15T12:30:45Z").expect("must parse");
        assert!((epoch - expected).abs() < 0.001, "expected {expected} got {epoch}");
    }

    #[test]
    fn parse_iso8601_fractional_seconds() {
        let base = parse_iso8601_to_epoch("2024-01-15T12:30:45Z").expect("must parse");
        let with_frac = parse_iso8601_to_epoch("2024-01-15T12:30:45.500Z").expect("must parse");
        assert!((with_frac - base - 0.5).abs() < 0.001);
    }

    #[test]
    fn parse_iso8601_positive_timezone_offset() {
        // +01:00 is UTC+1 → subtract 3600s from the value to get UTC epoch
        let utc = parse_iso8601_to_epoch("2024-01-15T12:30:45Z").expect("must parse");
        let offset = parse_iso8601_to_epoch("2024-01-15T13:30:45+01:00").expect("must parse");
        assert!((utc - offset).abs() < 0.001);
    }

    #[test]
    fn parse_iso8601_negative_timezone_offset() {
        // -05:00 is UTC-5 → add 18000s
        let utc = parse_iso8601_to_epoch("2024-01-15T12:30:45Z").expect("must parse");
        let offset = parse_iso8601_to_epoch("2024-01-15T07:30:45-05:00").expect("must parse");
        assert!((utc - offset).abs() < 0.001);
    }

    #[test]
    fn parse_iso8601_too_short_returns_none() {
        assert!(parse_iso8601_to_epoch("2024-01-15").is_none());
        assert!(parse_iso8601_to_epoch("").is_none());
    }
}
