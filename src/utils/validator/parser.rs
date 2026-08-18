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
///
/// §4.4.2.3 treats a parameter with no associated value as a parse failure, so an empty value
/// is reported as absent. Where more than one parameter matches, any of the values may be
/// used, and this takes the first.
fn extract_query_param(url: &str, param: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    let query = query.split_once('#').map_or(query, |(before, _)| before);
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k == param && !v.is_empty() { Some(v.to_string()) } else { None }
    })
}

/// §4.4.2.3 requires a QUERYPARAM value to be percent-decoded before substitution.
fn percent_decode(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .to_string()
}

/// Parse a master playlist and extract variant stream info.
///
/// Variant and media-rendition URIs are stored **raw** (as written in the playlist).
/// Callers must substitute EXT-X-DEFINE variables and resolve against the master URL
/// before fetching (see [`crate::utils::validator::absolute_fetch_uri`]).
pub fn parse_master_playlist(url: &str, content: &str) -> MasterPlaylist {
    let mut master = MasterPlaylist {
        url: url.to_string(),
        raw_content: content.to_string(),
        version: 1,
        variants: Vec::new(),
        media_renditions: Vec::new(),
        definitions: HashMap::new(),
        independent_segments: false,
        http_meta: PlaylistHttpMeta::default(),
    };
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("#EXT-X-VERSION:") {
            if let Ok(v) = line.split_once(':').unwrap().1.trim().parse::<u32>() {
                master.version = v;
            }
        } else if line == "#EXT-X-INDEPENDENT-SEGMENTS" {
            master.independent_segments = true;
        } else if let Some(attr_str) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let attrs = parse_attributes(attr_str);
            i += 1;
            let uri = if i < lines.len() { lines[i].trim().to_string() } else { String::new() };
            master.variants.push(MasterRendition {
                uri, // raw — callers must substitute + resolve before fetch
                bandwidth: attrs.get("BANDWIDTH").and_then(|v| v.parse().ok()),
                average_bandwidth: attrs.get("AVERAGE-BANDWIDTH").and_then(|v| v.parse().ok()),
                codecs: attrs.get("CODECS").cloned(),
                resolution: attrs.get("RESOLUTION").cloned(),
                frame_rate: attrs.get("FRAME-RATE").and_then(|v| v.parse().ok()),
                audio_group: attrs.get("AUDIO").cloned(),
                subtitle_group: attrs.get("SUBTITLES").cloned(),
                video_group: attrs.get("VIDEO").cloned(),
                closed_captions: attrs.get("CLOSED-CAPTIONS").cloned(),
                video_range: attrs.get("VIDEO-RANGE").cloned(),
                is_iframe: false,
                score: attrs.get("SCORE").and_then(|v| v.parse().ok()),
                hdcp_level: attrs.get("HDCP-LEVEL").cloned(),
                pathway_id: attrs.get("PATHWAY-ID").cloned(),
                req_video_layout: attrs.get("REQ-VIDEO-LAYOUT").cloned(),
            });
        } else if let Some(attr_str) = line.strip_prefix("#EXT-X-I-FRAME-STREAM-INF:") {
            let attrs = parse_attributes(attr_str);
            if let Some(uri) = attrs.get("URI") {
                master.variants.push(MasterRendition {
                    uri: uri.clone(), // raw — callers must substitute + resolve before fetch
                    bandwidth: attrs.get("BANDWIDTH").and_then(|v| v.parse().ok()),
                    average_bandwidth: attrs.get("AVERAGE-BANDWIDTH").and_then(|v| v.parse().ok()),
                    codecs: attrs.get("CODECS").cloned(),
                    resolution: attrs.get("RESOLUTION").cloned(),
                    frame_rate: attrs.get("FRAME-RATE").and_then(|v| v.parse().ok()),
                    audio_group: None,
                    subtitle_group: None,
                    video_group: None,
                    closed_captions: None,
                    video_range: attrs.get("VIDEO-RANGE").cloned(),
                    is_iframe: true,
                    score: attrs.get("SCORE").and_then(|v| v.parse().ok()),
                    hdcp_level: attrs.get("HDCP-LEVEL").cloned(),
                    pathway_id: attrs.get("PATHWAY-ID").cloned(),
                    req_video_layout: attrs.get("REQ-VIDEO-LAYOUT").cloned(),
                });
            }
        } else if let Some(attr_str) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = parse_attributes(attr_str);
            master.media_renditions.push(MediaRendition {
                media_type: attrs.get("TYPE").cloned().unwrap_or_default(),
                group_id: attrs.get("GROUP-ID").cloned().unwrap_or_default(),
                name: attrs.get("NAME").cloned().unwrap_or_default(),
                uri: attrs.get("URI").cloned(), // raw — callers must substitute + resolve before fetch
                language: attrs.get("LANGUAGE").cloned(),
                is_default: attrs.get("DEFAULT").is_some_and(|v| v == "YES"),
                autoselect: attrs.get("AUTOSELECT").is_some_and(|v| v == "YES"),
                channels: attrs.get("CHANNELS").cloned(),
                characteristics: attrs.get("CHARACTERISTICS").cloned(),
                forced: attrs.get("FORCED").is_some_and(|v| v == "YES"),
            });
        } else if let Some(rest) = line.strip_prefix("#EXT-X-DEFINE:") {
            let attrs = parse_attributes(rest);
            if let (Some(name), Some(value)) = (attrs.get("NAME"), attrs.get("VALUE")) {
                // Inline definition — store directly
                master.definitions.insert(name.clone(), value.clone());
            } else if let Some(name) = attrs.get("QUERYPARAM") {
                // §4.4.2.3: QUERYPARAM carries the Variable Name itself, and the value comes
                // from the query parameter of that name in this playlist's URI. There is no
                // accompanying NAME attribute — a tag carries exactly one of the three — so
                // requiring one here meant no QUERYPARAM variable was ever defined.
                if let Some(value) = extract_query_param(url, name) {
                    master.definitions.insert(name.clone(), percent_decode(&value));
                }
            }
        }
        i += 1;
    }
    master
}

/// Parse a media playlist from raw content, populating the MediaPlaylist struct.
///
/// Segment and EXT-X-MAP URIs are resolved against `url` **after** EXT-X-DEFINE substitution,
/// so a `{$VAR}` reference that expands to an absolute URL survives. Definitions are collected
/// as the playlist is read, which matches the spec requirement that a variable is defined
/// before it is used. Variables inherited from a multivariant playlist must already be in
/// `pl.definitions` on entry (see
/// [`crate::utils::validator::apply_master_definitions`]).
pub fn parse_media_playlist(url: &str, content: &str, pl: &mut MediaPlaylist) {
    pl.url = url.to_string();
    pl.raw_content = content.to_string();
    let lines: Vec<&str> = content.lines().collect();
    let pl_label = if pl.name.is_empty() { url.to_string() } else { pl.name.clone() };
    // A segment URI whose EXTINF is missing or unreadable cannot become a Segment. Rather
    // than drop the line, note why so the report can show it (rfc8216bis §4.4.4.1).
    let mut parse_issues: Vec<Issue> = Vec::new();
    let mut extinf_unreadable = false;
    let mut current_duration: Option<f64> = None;
    let mut current_title: Option<String> = None;
    let mut current_pdt: Option<f64> = None;
    let mut current_discontinuity = false;
    let mut current_byterange: Option<String> = None;
    let mut current_map_uri: Option<String> = None;
    let mut cumulative_duration: f64 = 0.0;
    let mut last_pdt: Option<f64> = None;

    for (line_no, line) in lines.iter().enumerate() {
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
        } else if line == "#EXT-X-INDEPENDENT-SEGMENTS" {
            pl.independent_segments = true;
        } else if line == "#EXT-X-I-FRAMES-ONLY" {
            pl.iframes_only = true;
        } else if let Some(val) = line.strip_prefix("#EXTINF:") {
            let comma_pos = val.find(',').unwrap_or(val.len());
            let raw_duration = val[..comma_pos].trim();
            // §4.2: a decimal-floating-point number is unsigned, so a negative duration is
            // not a duration at all and the segment it introduces cannot be placed on a
            // timeline. It is rejected here rather than carried through as a negative length
            // that would then subtract from every cumulative and drift measurement.
            let parsed = raw_duration.parse::<f64>().ok().filter(|d| d.is_finite());
            current_duration = parsed.filter(|d| *d >= 0.0);
            extinf_unreadable = current_duration.is_none();
            if extinf_unreadable {
                let reason = if parsed.is_some() {
                    format!(
                        "'{}', which is negative. §4.2 defines an EXTINF duration as a \
                         decimal-floating-point or decimal-integer number, both of which are \
                         unsigned",
                        raw_duration
                    )
                } else {
                    format!(
                        "'{}', which is not a decimal-floating-point or decimal-integer number",
                        raw_duration
                    )
                };
                parse_issues.push(Issue {
                    severity: Severity::Error,
                    check_id: CheckId::SegmentStructure,
                    segment_index: pl.segments.len() as i32,
                    rendition_a: Some(pl_label.clone()),
                    message: format!(
                        "rfc8216bis §4.4.4.1: EXTINF on line {} of '{}' has duration {}. \
                         The segment it introduces was skipped and is not covered by any \
                         other check.",
                        line_no + 1, pl_label, reason
                    ),
                    ..Default::default()
                });
            }
            if comma_pos < val.len() {
                let title = val[comma_pos + 1..].trim();
                if !title.is_empty() {
                    current_title = Some(title.to_string());
                }
            }
        } else if let Some(dt_str) = line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
            // Counted separately from Segment::pdt, which is extrapolated forward from the
            // last tag and so is present on segments that carry no tag of their own.
            pl.program_date_time_tags += 1;
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
            current_map_uri = attrs
                .get("URI")
                .map(|u| super::absolute_fetch_uri(url, u, &pl.definitions));
            if let Some(uri) = attrs.get("URI") {
                // Keep URI and BYTERANGE together and unresolved, so init probing can
                // substitute EXT-X-DEFINE variables and fetch the right bytes.
                let map = InitMap {
                    uri: uri.clone(),
                    byterange: attrs.get("BYTERANGE").cloned(),
                };
                if !pl.init_maps.contains(&map) {
                    pl.init_maps.push(map);
                }
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            let attrs = parse_attributes(rest);
            if let Some(method) = attrs.get("METHOD") {
                pl.encryption_methods.insert(method.clone());
            }
            if let Some(fmt) = attrs.get("KEYFORMAT") {
                pl.key_formats.insert(fmt.clone());
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-SERVER-CONTROL:") {
            let attrs = parse_attributes(rest);
            pl.server_control = Some(ServerControl {
                can_skip_until: attrs.get("CAN-SKIP-UNTIL").and_then(|v| v.parse().ok()),
                hold_back: attrs.get("HOLD-BACK").and_then(|v| v.parse().ok()),
                part_hold_back: attrs.get("PART-HOLD-BACK").and_then(|v| v.parse().ok()),
                can_block_reload: attrs.get("CAN-BLOCK-RELOAD").is_some_and(|v| v == "YES"),
                can_skip_dateranges: attrs.get("CAN-SKIP-DATERANGES").is_some_and(|v| v == "YES"),
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
            if let (Some(name), Some(value)) = (attrs.get("NAME"), attrs.get("VALUE")) {
                pl.definitions.insert(name.clone(), value.clone());
            } else if let Some(name) = attrs.get("QUERYPARAM") {
                // §4.4.2.3: the value is that of the query parameter of the same name in the
                // URI of *this* playlist, which is known whether or not a Multivariant
                // Playlist was read, so a media-playlist URL resolves these too. Where the
                // request was redirected the parameter may only be on the response URI, which
                // is where §4.4.2.3 says a client MUST then look for it.
                let from_uri = extract_query_param(url, name).or_else(|| {
                    Some(pl.http_meta.final_url.as_str())
                        .filter(|final_url| !final_url.is_empty() && *final_url != url)
                        .and_then(|final_url| extract_query_param(final_url, name))
                });
                match from_uri {
                    Some(raw) => {
                        pl.definitions.insert(name.clone(), percent_decode(&raw));
                    }
                    None => {
                        parse_issues.push(Issue {
                            severity: Severity::Error,
                            check_id: CheckId::VariableDefinitions,
                            rendition_a: Some(pl_label.clone()),
                            uri_a: Some(url.to_string()),
                            message: format!(
                                "rfc8216bis §4.4.2.3: EXT-X-DEFINE on line {} of '{}' declares \
                                 QUERYPARAM=\"{}\" but the playlist URI has no '{}' query \
                                 parameter with a value. A parser MUST fail to parse the \
                                 Playlist, so every {{${}}} reference in it is unresolved.",
                                line_no + 1, pl_label, name, name, name
                            ),
                            ..Default::default()
                        });
                    }
                }
            } else if let Some(name) = attrs.get("IMPORT")
                && !pl.definitions.contains_key(name)
            {
                // Imports are seeded from the Multivariant Playlist before the playlist is
                // read (see `apply_master_definitions`), so one still unresolved here either
                // names nothing the parent declared or was reached without a parent at all.
                parse_issues.push(Issue {
                    severity: Severity::Error,
                    check_id: CheckId::VariableDefinitions,
                    rendition_a: Some(pl_label.clone()),
                    uri_a: Some(url.to_string()),
                    message: format!(
                        "rfc8216bis §4.4.2.3: EXT-X-DEFINE on line {} of '{}' imports \
                         variable '{}', which no Multivariant Playlist read for this run \
                         declares. Where the IMPORT names no declared variable, or the Media \
                         Playlist was not loaded from a Multivariant Playlist, a parser MUST \
                         fail to parse the Playlist.",
                        line_no + 1, pl_label, name
                    ),
                    ..Default::default()
                });
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
                let uri = super::absolute_fetch_uri(url, line, &pl.definitions);
                pl.segments.push(Segment {
                    uri,
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
            } else if extinf_unreadable {
                // The unreadable EXTINF was already reported; this is the URI it introduced.
                extinf_unreadable = false;
            } else {
                parse_issues.push(Issue {
                    severity: Severity::Error,
                    check_id: CheckId::SegmentStructure,
                    segment_index: pl.segments.len() as i32,
                    rendition_a: Some(pl_label.clone()),
                    uri_a: Some(super::absolute_fetch_uri(url, line, &pl.definitions)),
                    message: format!(
                        "rfc8216bis §4.4.4.1: Media Segment URI on line {} of '{}' has no \
                         preceding EXTINF tag. Every Media Segment MUST be introduced by an \
                         EXTINF tag; this URI was skipped and is not covered by any other check.",
                        line_no + 1, pl_label
                    ),
                    ..Default::default()
                });
            }
        }
    }

    pl.parse_issues = parse_issues;
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

    #[test]
    fn parse_master_stores_raw_variant_uri() {
        let master = parse_master_playlist(
            "https://ex.com/a/master.m3u8",
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000\nv1/prog.m3u8\n",
        );
        assert_eq!(master.variants[0].uri, "v1/prog.m3u8");
        assert!(
            master.media_renditions.is_empty(),
            "no MEDIA tags expected"
        );
    }

    #[test]
    fn parse_master_stores_raw_media_uri() {
        let master = parse_master_playlist(
            "https://ex.com/a/master.m3u8",
            "#EXTM3U\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"en\",URI=\"audio/en.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,AUDIO=\"a\"\nv1/prog.m3u8\n",
        );
        assert_eq!(
            master.media_renditions[0].uri.as_deref(),
            Some("audio/en.m3u8")
        );
    }

    // ── parse_media_playlist EXT-X-MAP ────────────────────────────────────────

    #[test]
    fn parse_media_keeps_each_map_uri_with_its_own_byterange() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n\
             #EXT-X-MAP:URI=\"{$base}/init_a.mp4\",BYTERANGE=\"800@0\"\n\
             #EXTINF:4.0,\nseg1.m4s\n\
             #EXT-X-DISCONTINUITY\n\
             #EXT-X-MAP:URI=\"init_b.mp4\",BYTERANGE=\"1200@5000\"\n\
             #EXTINF:4.0,\nseg2.m4s\n",
            &mut pl,
        );
        assert_eq!(
            pl.init_maps,
            vec![
                InitMap {
                    // Kept raw so the {$base} variable can still be substituted.
                    uri: "{$base}/init_a.mp4".to_string(),
                    byterange: Some("800@0".to_string()),
                },
                InitMap {
                    uri: "init_b.mp4".to_string(),
                    byterange: Some("1200@5000".to_string()),
                },
            ]
        );
    }

    #[test]
    fn parse_media_records_a_repeated_map_once() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n\
             #EXT-X-MAP:URI=\"init.mp4\"\n\
             #EXTINF:4.0,\nseg1.m4s\n\
             #EXT-X-MAP:URI=\"init.mp4\"\n\
             #EXTINF:4.0,\nseg2.m4s\n",
            &mut pl,
        );
        assert_eq!(
            pl.init_maps,
            vec![InitMap {
                uri: "init.mp4".to_string(),
                byterange: None
            }]
        );
    }

    // ── parse_media_playlist EXT-X-DEFINE ─────────────────────────────────────

    #[test]
    fn parse_media_substitutes_define_in_segment_uri_before_resolving() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n\
             #EXT-X-DEFINE:NAME=\"base\",VALUE=\"https://cdn.example\"\n\
             #EXTINF:4.0,\n{$base}/seg0.m4s\n",
            &mut pl,
        );
        assert_eq!(pl.segments[0].uri, "https://cdn.example/seg0.m4s");
    }

    #[test]
    fn parse_media_substitutes_define_inherited_from_the_multivariant_playlist() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        pl.definitions
            .insert("base".to_string(), "https://cdn.example".to_string());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n\
             #EXT-X-MAP:URI=\"{$base}/init.mp4\"\n\
             #EXTINF:4.0,\n{$base}/seg0.m4s\n",
            &mut pl,
        );
        assert_eq!(pl.segments[0].uri, "https://cdn.example/seg0.m4s");
        assert_eq!(
            pl.segments[0].map_uri.as_deref(),
            Some("https://cdn.example/init.mp4")
        );
    }

    #[test]
    fn parse_media_still_resolves_relative_segment_uris() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n#EXTINF:4.0,\nseg0.m4s\n",
            &mut pl,
        );
        assert_eq!(pl.segments[0].uri, "https://ex.com/hls/seg0.m4s");
    }

    // ── parse_media_playlist segment structure ────────────────────────────────

    #[test]
    fn parse_media_reports_a_segment_uri_with_no_extinf() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n\
             #EXT-X-TARGETDURATION:4\n\
             #EXTINF:4.0,\nseg0.m4s\n\
             seg1.m4s\n\
             #EXTINF:4.0,\nseg2.m4s\n",
            &mut pl,
        );
        assert_eq!(
            pl.segments.len(),
            2,
            "a URI with no EXTINF cannot become a segment"
        );
        assert_eq!(pl.parse_issues.len(), 1, "{:?}", pl.parse_issues);
        let issue = &pl.parse_issues[0];
        assert_eq!(issue.severity, Severity::Error);
        assert_eq!(issue.check_id, CheckId::SegmentStructure);
        assert!(issue.message.contains("line 5"), "{}", issue.message);
        assert_eq!(issue.uri_a.as_deref(), Some("https://ex.com/hls/seg1.m4s"));
    }

    #[test]
    fn parse_media_reports_an_unreadable_extinf_duration_once() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n\
             #EXT-X-TARGETDURATION:4\n\
             #EXTINF:not-a-number,\nseg0.m4s\n\
             #EXTINF:4.0,\nseg1.m4s\n",
            &mut pl,
        );
        assert_eq!(pl.segments.len(), 1);
        assert_eq!(
            pl.parse_issues.len(),
            1,
            "the URI that follows must not be reported a second time: {:?}",
            pl.parse_issues
        );
        let issue = &pl.parse_issues[0];
        assert_eq!(issue.severity, Severity::Error);
        assert_eq!(issue.check_id, CheckId::SegmentStructure);
        assert!(issue.message.contains("not-a-number"), "{}", issue.message);
    }

    #[test]
    fn parse_media_of_a_well_formed_playlist_raises_nothing() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n\
             #EXT-X-TARGETDURATION:4\n\
             #EXT-X-MAP:URI=\"init.mp4\"\n\
             #EXTINF:4.0,\n\
             #EXT-X-BYTERANGE:1000@0\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:00Z\n\
             seg0.m4s\n\
             #EXT-X-ENDLIST\n",
            &mut pl,
        );
        assert_eq!(pl.segments.len(), 1);
        assert!(pl.parse_issues.is_empty(), "{:?}", pl.parse_issues);
    }

    #[test]
    fn parse_media_rejects_a_negative_extinf_duration() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n\
             #EXT-X-TARGETDURATION:4\n\
             #EXTINF:-4.0,\nseg0.m4s\n\
             #EXTINF:4.0,\nseg1.m4s\n",
            &mut pl,
        );
        assert_eq!(
            pl.segments.len(),
            1,
            "a negative duration cannot place a segment on a timeline: {:?}",
            pl.segments
        );
        assert_eq!(pl.parse_issues.len(), 1, "{:?}", pl.parse_issues);
        let issue = &pl.parse_issues[0];
        assert_eq!(issue.severity, Severity::Error);
        assert!(issue.message.contains("negative"), "{}", issue.message);
        assert!(issue.message.contains("§4.2"), "{}", issue.message);
    }

    #[test]
    fn parse_media_counts_program_date_time_tags_not_extrapolated_pdts() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:00Z\n\
             #EXTINF:4.0,\nseg0.m4s\n#EXTINF:4.0,\nseg1.m4s\n#EXTINF:4.0,\nseg2.m4s\n",
            &mut pl,
        );
        assert_eq!(pl.program_date_time_tags, 1, "one tag was written");
        assert!(
            pl.segments.iter().all(|s| s.pdt.is_some()),
            "every segment still carries an extrapolated PDT, which is why the count is needed"
        );
    }

    // ── EXT-X-DEFINE ──────────────────────────────────────────────────────────

    #[test]
    fn parse_media_resolves_a_queryparam_definition_from_its_own_uri() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        let url = "https://ex.com/hls/prog.m3u8?token=ab%20cd&other=1";
        parse_media_playlist(
            url,
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-DEFINE:QUERYPARAM=\"token\"\n\
             #EXTINF:4.0,\nseg0.m4s\n",
            &mut pl,
        );
        assert_eq!(
            pl.definitions.get("token").map(String::as_str),
            Some("ab cd"),
            "the value is the percent-decoded query parameter: {:?}",
            pl.definitions
        );
        assert!(pl.parse_issues.is_empty(), "{:?}", pl.parse_issues);
    }

    #[test]
    fn parse_media_reports_a_queryparam_the_uri_does_not_carry() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-DEFINE:QUERYPARAM=\"token\"\n\
             #EXTINF:4.0,\nseg0.m4s\n",
            &mut pl,
        );
        let issue = pl.parse_issues.iter()
            .find(|i| i.check_id == CheckId::VariableDefinitions)
            .unwrap_or_else(|| panic!("expected a §4.4.2.3 finding: {:?}", pl.parse_issues));
        assert_eq!(issue.severity, Severity::Error);
        assert!(issue.message.contains("QUERYPARAM"), "{}", issue.message);
    }

    #[test]
    fn parse_media_finds_a_queryparam_on_the_redirect_response_uri() {
        // §4.4.2.3: "If the URI is redirected, the client MUST look for the query parameter in
        // the 30x response URI." Tokenising redirects are how most signed streams work.
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        pl.http_meta.final_url = "https://cdn.ex.com/hls/prog.m3u8?token=abc".to_string();
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-DEFINE:QUERYPARAM=\"token\"\n\
             #EXTINF:4.0,\nseg0.m4s\n",
            &mut pl,
        );
        assert_eq!(pl.definitions.get("token").map(String::as_str), Some("abc"));
        assert!(pl.parse_issues.is_empty(), "{:?}", pl.parse_issues);
    }

    #[test]
    fn a_query_parameter_with_no_value_does_not_define_a_variable() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8?token=",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-DEFINE:QUERYPARAM=\"token\"\n\
             #EXTINF:4.0,\nseg0.m4s\n",
            &mut pl,
        );
        assert!(
            pl.parse_issues.iter().any(|i| i.check_id == CheckId::VariableDefinitions),
            "a matching parameter with no associated value is a parse failure: {:?}",
            pl.parse_issues
        );
    }

    #[test]
    fn parse_media_reports_an_import_with_no_multivariant_playlist_to_import_from() {
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-DEFINE:IMPORT=\"token\"\n\
             #EXTINF:4.0,\nseg0.m4s\n",
            &mut pl,
        );
        let issue = pl.parse_issues.iter()
            .find(|i| i.check_id == CheckId::VariableDefinitions)
            .unwrap_or_else(|| panic!("expected a §4.4.2.3 finding: {:?}", pl.parse_issues));
        assert_eq!(issue.severity, Severity::Error);
        assert!(issue.message.contains("IMPORT") || issue.message.contains("imports"),
            "{}", issue.message);
    }

    #[test]
    fn parse_media_accepts_an_import_the_multivariant_playlist_declares() {
        // The value arrives in `definitions` before the media playlist is parsed, which is how
        // the run resolves IMPORT against the multivariant playlist it came from.
        let mut pl = MediaPlaylist::new("v1".to_string(), String::new());
        pl.definitions.insert("token".to_string(), "abc".to_string());
        parse_media_playlist(
            "https://ex.com/hls/prog.m3u8",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-DEFINE:IMPORT=\"token\"\n\
             #EXTINF:4.0,\nseg0.m4s\n",
            &mut pl,
        );
        assert!(
            !pl.parse_issues.iter().any(|i| i.check_id == CheckId::VariableDefinitions),
            "{:?}", pl.parse_issues
        );
    }

    #[test]
    fn parse_master_resolves_a_queryparam_definition_from_its_own_uri() {
        let master = parse_master_playlist(
            "https://ex.com/a/master.m3u8?sid=xyz%2F1",
            "#EXTM3U\n#EXT-X-DEFINE:QUERYPARAM=\"sid\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000\nv1/prog.m3u8\n",
        );
        assert_eq!(master.definitions.get("sid").map(String::as_str), Some("xyz/1"));
    }

    // ── parse_master_playlist rendition group attributes ──────────────────────

    #[test]
    fn parse_master_reads_the_video_group_of_a_stream_inf() {
        let master = parse_master_playlist(
            "https://ex.com/a/master.m3u8",
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000,VIDEO=\"alt-angles\"\nv1/prog.m3u8\n",
        );
        assert_eq!(master.variants[0].video_group.as_deref(), Some("alt-angles"));
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
