use std::collections::{HashMap, HashSet};
use super::types::*;

/// RFC 8216bis §4.4.1.1 — EXTM3U must be first line
pub fn check_extm3u_header(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in playlists {
        let first_line = pl.raw_content.lines().next().unwrap_or("");
        if first_line.trim() != "#EXTM3U" {
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "RFC 8216bis §4.4.1.1: Playlist '{}' does not start with #EXTM3U. \
                     First line: '{}'",
                    pl.name, first_line
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }
    issues
}

/// RFC 8216bis §4.4.3.1 — TARGETDURATION presence, per-segment compliance, and accuracy.
/// The spec says the EXTINF duration "when rounded to the nearest integer, MUST be less than
/// or equal to the Target Duration." (round-half-away-from-zero, matching common rounding.)
pub fn check_target_duration_compliance(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in playlists {
        if pl.target_duration <= 0.0 {
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "rfc8216bis §4.4.3.1: EXT-X-TARGETDURATION missing or zero in '{}'. \
                     Every Media Playlist MUST declare a positive TARGETDURATION.",
                    pl.name
                ),
                uri_note: None,
                ..Default::default()
            });
            continue;
        }
        let target_int = pl.target_duration as u64;
        for (idx, seg) in pl.segments.iter().enumerate() {
            // §4.4.3.1: "rounded to the nearest integer" — not ceil
            let rounded = seg.duration.round() as u64;
            if rounded > target_int {
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: idx as i32,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: Some(seg.uri.clone()),
                    uri_b: None,
                    message: format!(
                        "rfc8216bis §4.4.3.1: Segment {} in '{}' duration {:.6}s \
                         (round={}s) exceeds TARGETDURATION {}s.",
                        idx, pl.name, seg.duration, rounded, target_int
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
        }
        if let Some(max_extinf) = pl.segments.iter().map(|s| s.duration).reduce(f64::max) {
            let rounded_max = max_extinf.round() as u64;
            // If declared TARGETDURATION exceeds the rounded longest segment by more than 1s, warn
            if target_int > rounded_max + 1 {
                issues.push(Issue {
                    severity: Severity::Warn,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!(
                        "rfc8216bis §4.4.3.1: TARGETDURATION={}s in '{}' is more than 1s \
                         above the longest segment (longest={:.6}s, round={}s). \
                         Consider reducing TARGETDURATION for accuracy.",
                        target_int, pl.name, max_extinf, rounded_max
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
        }
    }
    issues
}

/// RFC 8216bis §6.2.4 — PDT coverage: if any segment has PDT, all should
pub fn check_pdt_coverage(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in playlists {
        if pl.segments.is_empty() {
            continue;
        }
        let has_any_pdt = pl.segments.iter().any(|s| s.pdt.is_some());
        let all_have_pdt = pl.segments.iter().all(|s| s.pdt.is_some());
        if has_any_pdt && !all_have_pdt {
            let missing_count = pl.segments.iter().filter(|s| s.pdt.is_none()).count();
            issues.push(Issue {
                severity: Severity::Warn,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "RFC 8216bis §6.2.4: Playlist '{}' has partial PDT coverage: \
                     {} of {} segments lack EXT-X-PROGRAM-DATE-TIME.",
                    pl.name, missing_count, pl.segments.len()
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }
    issues
}

/// RFC 8216bis §4.4.1.2, §4.4.2, §4.4.3 — Singleton tags must not appear more than once.
/// Covers: EXT-X-VERSION (§4.4.1.2), EXT-X-INDEPENDENT-SEGMENTS (§4.4.2.1),
/// EXT-X-START (§4.4.2.2), and all Media Playlist singleton tags (§4.4.3).
pub fn check_media_sequence_duplicate_tags(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    let singleton_tags = [
        "#EXT-X-VERSION:",
        "#EXT-X-INDEPENDENT-SEGMENTS",
        "#EXT-X-START:",
        "#EXT-X-TARGETDURATION:",
        "#EXT-X-MEDIA-SEQUENCE:",
        "#EXT-X-DISCONTINUITY-SEQUENCE:",
        "#EXT-X-PLAYLIST-TYPE:",
        "#EXT-X-I-FRAMES-ONLY",
        "#EXT-X-PART-INF:",
        "#EXT-X-SERVER-CONTROL:",
    ];
    for pl in playlists {
        for tag in &singleton_tags {
            let count = pl.raw_content.lines().filter(|l| l.trim().starts_with(tag)).count();
            if count > 1 {
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!(
                        "RFC 8216bis §4.4.3.2: Singleton tag '{}' appears {} times in '{}'. \
                         It MUST appear at most once.",
                        tag.trim_end_matches(':'), count, pl.name
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
        }
    }
    issues
}

/// RFC 8216bis §4.4.6.2 — CODECS consistency for same URI
pub fn check_codecs_attribute(master: &MasterPlaylist) -> Vec<Issue> {
    let mut issues = Vec::new();
    // Group variants by URI
    let mut uri_groups: HashMap<String, Vec<&MasterRendition>> = HashMap::new();
    for v in &master.variants {
        uri_groups.entry(v.uri.clone()).or_default().push(v);
    }
    for (uri, variants) in &uri_groups {
        if variants.len() < 2 {
            continue;
        }

        // When the same video URI is referenced with different AUDIO groups the
        // CODECS string legitimately includes the paired audio codec and will
        // therefore differ across entries.  Only flag a CODECS mismatch when
        // the entries share the same AUDIO group (or both have none), meaning
        // the difference cannot be explained by alternative audio renditions.
        let audio_groups_differ = {
            let audio_set: HashSet<Option<&str>> = variants.iter()
                .map(|v| v.audio_group.as_deref())
                .collect();
            audio_set.len() > 1
        };

        if !audio_groups_differ {
            // Same audio group → CODECS must be identical
            let codecs_set: HashSet<Option<&str>> = variants.iter()
                .map(|v| v.codecs.as_deref())
                .collect();
            if codecs_set.len() > 1 {
                let details: Vec<String> = variants.iter()
                    .map(|v| format!("CODECS={}", v.codecs.as_deref().unwrap_or("(none)")))
                    .collect();
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: None,
                    rendition_b: None,
                    uri_a: Some(uri.clone()),
                    uri_b: None,
                    message: format!(
                        "RFC 8216bis §4.4.5.2: Multiple EXT-X-STREAM-INF tags share URI '{}' \
                         and the same AUDIO group but have different CODECS values: {}. \
                         They MUST match.",
                        uri, details.join(", ")
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
        } else {
            // Different audio groups → only flag if the VIDEO codec portion differs.
            // The video codec is the first comma-separated token in the CODECS string.
            let video_codec_set: HashSet<&str> = variants.iter()
                .filter_map(|v| v.codecs.as_deref())
                .map(|c| c.split(',').next().unwrap_or(c).trim())
                .collect();
            if video_codec_set.len() > 1 {
                let details: Vec<String> = variants.iter()
                    .map(|v| format!(
                        "AUDIO={} CODECS={}",
                        v.audio_group.as_deref().unwrap_or("(none)"),
                        v.codecs.as_deref().unwrap_or("(none)")
                    ))
                    .collect();
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: None,
                    rendition_b: None,
                    uri_a: Some(uri.clone()),
                    uri_b: None,
                    message: format!(
                        "RFC 8216bis §4.4.5.2: Multiple EXT-X-STREAM-INF tags share URI '{}' \
                         with different AUDIO groups but also have mismatched video codecs: {}.",
                        uri, details.join(", ")
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
        }

        // BANDWIDTH consistency: all entries for the same URI+audio_group must agree.
        // Group by audio_group first, then check bandwidth within each group.
        let mut by_audio: HashMap<Option<&str>, Vec<&MasterRendition>> = HashMap::new();
        for v in variants {
            by_audio.entry(v.audio_group.as_deref()).or_default().push(v);
        }
        for (audio_grp, grp_variants) in &by_audio {
            let bw_set: HashSet<Option<u64>> = grp_variants.iter().map(|v| v.bandwidth).collect();
            if bw_set.len() > 1 {
                let details: Vec<String> = grp_variants.iter()
                    .map(|v| format!("BANDWIDTH={}", v.bandwidth.map_or("(none)".to_string(), |b| b.to_string())))
                    .collect();
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: None,
                    rendition_b: None,
                    uri_a: Some(uri.clone()),
                    uri_b: None,
                    message: format!(
                        "RFC 8216bis §4.4.5.2: Multiple EXT-X-STREAM-INF tags share URI '{}' \
                         (AUDIO={}) but have different BANDWIDTH values: {}. They MUST match.",
                        uri,
                        audio_grp.unwrap_or("(none)"),
                        details.join(", ")
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
        }
    }
    issues
}

/// RFC 8216bis §4.4.6.2 — BANDWIDTH is a REQUIRED attribute on EXT-X-STREAM-INF.
pub fn check_bandwidth_required(master: &MasterPlaylist) -> Vec<Issue> {
    let mut issues = Vec::new();
    for v in &master.variants {
        if v.bandwidth.is_none() {
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: None,
                rendition_b: None,
                uri_a: Some(v.uri.clone()),
                uri_b: None,
                message: format!(
                    "RFC 8216bis §4.4.6.2: EXT-X-STREAM-INF for URI '{}' is missing \
                     the BANDWIDTH attribute, which is REQUIRED.",
                    v.uri
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }
    issues
}

/// RFC 8216bis §4.4.6.1.1 — When a Playlist contains multiple Groups of the same TYPE,
/// every Group MUST contain the same set of member NAMEs.
pub fn check_media_group_membership(master: &MasterPlaylist) -> Vec<Issue> {
    let mut issues = Vec::new();

    // Collect all distinct media types that appear in more than one group
    let mut by_type: HashMap<&str, HashMap<&str, Vec<&MediaRendition>>> = HashMap::new();
    for r in &master.media_renditions {
        by_type
            .entry(r.media_type.as_str())
            .or_default()
            .entry(r.group_id.as_str())
            .or_default()
            .push(r);
    }

    for (media_type, groups) in &by_type {
        if groups.len() < 2 {
            continue; // only one group of this type — nothing to compare
        }

        // Build the union of member NAMEs across all groups of this type
        let all_names: HashSet<&str> = groups.values()
            .flat_map(|members| members.iter().map(|r| r.name.as_str()))
            .collect();

        for (group_id, members) in groups {
            let present: HashSet<&str> = members.iter().map(|r| r.name.as_str()).collect();
            let mut missing: Vec<&str> = all_names.difference(&present).copied().collect();
            if missing.is_empty() {
                continue;
            }
            missing.sort_unstable();
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: None,
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "rfc8216bis §4.4.6.1.1: {} Group '{}' is missing member(s) present in \
                     other groups of the same type: {}. All groups of the same TYPE MUST \
                     have the same set of members.",
                    media_type, group_id,
                    missing.iter().map(|n| format!("'{}'", n)).collect::<Vec<_>>().join(", ")
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }
    issues
}

/// RFC 8216bis §8 — Version compatibility
pub fn check_version_compatibility(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in playlists {
        let v = pl.version;
        let content = &pl.raw_content;
        // EXT-X-KEY with IV requires v2+
        if v < 2 && content.contains("#EXT-X-KEY:") && content.contains("IV=") {
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "RFC 8216bis §8: '{}' uses EXT-X-KEY with IV attribute \
                     which requires VERSION >= 2. Declared version: {}.",
                    pl.name, v
                ),
                uri_note: None,
                ..Default::default()
            });
        }
        // Floating-point EXTINF requires v3+
        if v < 3 {
            for seg in &pl.segments {
                if seg.duration.fract() != 0.0 {
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: None,
                        uri_b: None,
                        message: format!(
                            "RFC 8216bis §8: '{}' uses floating-point EXTINF ({:.3}s) \
                             which requires VERSION >= 3. Declared version: {}.",
                            pl.name, seg.duration, v
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                    break;
                }
            }
        }
        // EXT-X-BYTERANGE requires v4+
        if v < 4 && content.contains("#EXT-X-BYTERANGE:") {
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "RFC 8216bis §8: '{}' uses EXT-X-BYTERANGE \
                     which requires VERSION >= 4. Declared version: {}.",
                    pl.name, v
                ),
                uri_note: None,
                ..Default::default()
            });
        }
        // EXT-X-MAP requires v5+ (unless I-frames-only, which requires v4+)
        if v < 5 && content.contains("#EXT-X-MAP:") && !content.contains("#EXT-X-I-FRAMES-ONLY") {
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "RFC 8216bis §8: '{}' uses EXT-X-MAP without I-FRAMES-ONLY \
                     which requires VERSION >= 5. Declared version: {}.",
                    pl.name, v
                ),
                uri_note: None,
                ..Default::default()
            });
        }
        // EXT-X-SKIP requires v9+
        if v < 9 && content.contains("#EXT-X-SKIP:") {
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "RFC 8216bis §8: '{}' uses EXT-X-SKIP \
                     which requires VERSION >= 9. Declared version: {}.",
                    pl.name, v
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }
    issues
}

/// RFC 8216 §6.2.2 — Live playlists must have >= 3 segments
pub fn check_live_playlist_min_segments(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in playlists {
        if pl.has_endlist {
            continue;
        }
        let n = pl.segments.len();
        if n < 3 {
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "RFC 8216 §6.2.2: Live playlist '{}' contains only {} segment(s). \
                     A Live Media Playlist MUST retain at least 3 segments.",
                    pl.name, n
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }
    issues
}


/// RFC 8216 §6.2.4 — TARGETDURATION consistency across renditions
pub fn check_targetduration_consistency(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    if playlists.len() < 2 {
        return issues;
    }
    let td_values: HashMap<&str, f64> = playlists.iter()
        .filter(|pl| pl.target_duration > 0.0)
        .map(|pl| (pl.name.as_str(), pl.target_duration))
        .collect();
    if td_values.is_empty() {
        return issues;
    }
    let unique_tds: HashSet<u64> = td_values.values().map(|&v| v as u64).collect();
    if unique_tds.len() > 1 {
        let td_summary: Vec<String> = td_values.iter()
            .map(|(name, td)| format!("{}={:.0}s", name, td))
            .collect();
        issues.push(Issue::warn(format!(
            "RFC 8216 §6.2.4: EXT-X-TARGETDURATION values differ across renditions \
             ({} distinct values). All Media Playlists SHOULD share the same \
             TARGETDURATION. Details: {}",
            unique_tds.len(), td_summary.join(", ")
        )));
    }
    issues
}

/// RFC 8216bis §4.4.3.5 — PLAYLIST-TYPE / ENDLIST consistency.
/// VOD playlists MUST have EXT-X-ENDLIST. EVENT playlists are valid live
/// playlists that grow (segments can only be appended, never removed) and
/// also MUST have EXT-X-ENDLIST when the event is complete.
pub fn check_playlist_type_endlist(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in playlists {
        match pl.playlist_type.as_deref() {
            Some("VOD") if !pl.has_endlist => {
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!(
                        "RFC 8216bis §4.4.3.5: Playlist '{}' declares PLAYLIST-TYPE:VOD \
                         but is missing EXT-X-ENDLIST. A VOD playlist MUST end with EXT-X-ENDLIST.",
                        pl.name
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
            // EVENT playlists are live; EXT-X-ENDLIST is added when the event ends.
            // An EVENT playlist without ENDLIST is valid and expected during a live event.
            Some("EVENT") => { /* valid — no error */ }
            _ => {}
        }
    }
    issues
}

/// RFC 8216bis §4.4.4.4 — Encryption consistency across renditions
pub fn check_encryption_consistency(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    let method_map: HashMap<&str, &HashSet<String>> = playlists.iter()
        .filter(|pl| !pl.encryption_methods.is_empty())
        .map(|pl| (pl.name.as_str(), &pl.encryption_methods))
        .collect();
    if method_map.is_empty() {
        return issues;
    }
    // Per-playlist: mixed encryption
    for (&name, methods) in &method_map {
        if methods.len() > 1 {
            let is_normal_mix = (methods.contains("AES-128") && methods.contains("NONE") && methods.len() == 2)
                || (methods.contains("SAMPLE-AES") && methods.contains("NONE") && methods.len() == 2);
            if !is_normal_mix {
                let sorted: Vec<&String> = methods.iter().collect();
                issues.push(Issue::warn(format!(
                    "RFC 8216bis §4.4.4.4: Playlist '{}' uses multiple encryption methods: {:?}.",
                    name, sorted
                )));
            }
        }
    }
    // Cross-rendition consistency
    let mut all_methods: HashSet<&str> = HashSet::new();
    for methods in method_map.values() {
        for m in *methods {
            if m != "NONE" || methods.len() == 1 {
                all_methods.insert(m.as_str());
            }
        }
    }
    if all_methods.len() > 1 {
        issues.push(Issue::error(format!(
            "RFC 8216bis §6.2.4: Renditions use inconsistent encryption methods: {:?}. \
             All renditions SHOULD use the same encryption method.",
            all_methods
        )));
    }
    issues
}

/// Discontinuity sequence consistency across renditions
pub fn check_discontinuity_sequence(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    if playlists.len() < 2 {
        return issues;
    }
    let disc_seqs: HashSet<u64> = playlists.iter().map(|pl| pl.discontinuity_sequence).collect();
    if disc_seqs.len() > 1 {
        let details: Vec<String> = playlists.iter()
            .map(|pl| format!("{}={}", pl.name, pl.discontinuity_sequence))
            .collect();
        issues.push(Issue::warn(format!(
            "EXT-X-DISCONTINUITY-SEQUENCE values differ across renditions: {}. \
             They SHOULD be consistent for synchronized playback.",
            details.join(", ")
        )));
    }
    issues
}

/// Segment count comparison across renditions (non-live)
pub fn check_segment_count(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    if playlists.len() < 2 {
        return issues;
    }
    // Only compare VOD VIDEO renditions — audio encoders produce different segment
    // counts than video encoders even for the same content duration; comparing
    // across media types always produces false positives.
    let vod_playlists: Vec<&MediaPlaylist> = playlists.iter()
        .filter(|pl| pl.has_endlist && pl.media_type == "VIDEO" && !pl.is_iframe)
        .collect();
    if vod_playlists.len() < 2 {
        return issues;
    }
    let counts: HashSet<usize> = vod_playlists.iter().map(|pl| pl.segments.len()).collect();
    if counts.len() > 1 {
        let details: Vec<String> = vod_playlists.iter()
            .map(|pl| format!("{}={}", pl.name, pl.segments.len()))
            .collect();
        issues.push(Issue::warn(format!(
            "Segment count mismatch across VIDEO renditions: {}. \
             All video renditions should have the same number of segments.",
            details.join(", ")
        )));
    }
    issues
}

/// EXTINF duration drift between renditions (MSN-aligned)
/// Only compares VIDEO vs VIDEO — audio encoders produce different segment
/// boundaries than video encoders, making cross-type drift meaningless.
pub fn check_duration_drift(playlists: &[MediaPlaylist], tolerance_ms: f64) -> Vec<Issue> {
    let mut issues = Vec::new();
    if playlists.len() < 2 {
        return issues;
    }
    let tolerance_s = tolerance_ms / 1000.0;
    for i in 0..playlists.len() {
        for j in (i + 1)..playlists.len() {
            let pl_a = &playlists[i];
            let pl_b = &playlists[j];
            // Skip cross-type pairs and I-frame-only playlists
            if pl_a.media_type != pl_b.media_type
                || pl_a.media_type != "VIDEO"
                || pl_a.is_iframe || pl_b.is_iframe
            {
                continue;
            }
            for (seg_a, seg_b, msn) in overlapping_segments(pl_a, pl_b) {
                let diff = (seg_a.duration - seg_b.duration).abs();
                if diff > tolerance_s {
                    issues.push(Issue {
                        severity: Severity::Warn,
                        segment_index: msn as i32,
                        rendition_a: Some(pl_a.name.clone()),
                        rendition_b: Some(pl_b.name.clone()),
                        uri_a: Some(seg_a.uri.clone()),
                        uri_b: Some(seg_b.uri.clone()),
                        message: format!(
                            "EXTINF drift at MSN {}: '{}' has {:.3}s vs '{}' has {:.3}s \
                             (diff={:.3}s, tolerance={:.3}s).",
                            msn, pl_a.name, seg_a.duration, pl_b.name, seg_b.duration,
                            diff, tolerance_s
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                }
            }
        }
    }
    issues
}

/// PDT alignment across renditions (MSN-aligned)
/// Only compares VIDEO vs VIDEO — audio PDT is extrapolated from audio segment
/// durations which differ from video, causing apparent drift that is not real.
pub fn check_pdt_alignment(playlists: &[MediaPlaylist], tolerance_ms: f64) -> Vec<Issue> {
    let mut issues = Vec::new();
    if playlists.len() < 2 {
        return issues;
    }
    let tolerance_s = tolerance_ms / 1000.0;
    for i in 0..playlists.len() {
        for j in (i + 1)..playlists.len() {
            let pl_a = &playlists[i];
            let pl_b = &playlists[j];
            // Skip cross-type pairs and I-frame-only playlists
            if pl_a.media_type != pl_b.media_type
                || pl_a.media_type != "VIDEO"
                || pl_a.is_iframe || pl_b.is_iframe
            {
                continue;
            }
            for (seg_a, seg_b, msn) in overlapping_segments(pl_a, pl_b) {
                if let (Some(pdt_a), Some(pdt_b)) = (seg_a.pdt, seg_b.pdt) {
                    let diff = (pdt_a - pdt_b).abs();
                    if diff > tolerance_s {
                        issues.push(Issue {
                            severity: Severity::Warn,
                            segment_index: msn as i32,
                            rendition_a: Some(pl_a.name.clone()),
                            rendition_b: Some(pl_b.name.clone()),
                            uri_a: Some(seg_a.uri.clone()),
                            uri_b: Some(seg_b.uri.clone()),
                            message: format!(
                                "PDT misalignment at MSN {}: diff={:.3}s (tolerance={:.3}s).",
                                msn, diff, tolerance_s
                            ),
                            uri_note: None,
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
    issues
}

/// Cumulative EXTINF drift across renditions
/// Only compares VIDEO renditions — comparing video total to audio total is
/// meaningless since they use different segment boundaries.
///
/// Uses the MSN-aligned common window across all renditions so that concurrent live fetches
/// (which may return one more or fewer segment per rendition) do not trigger false positives.
pub fn check_cumulative_drift(playlists: &[MediaPlaylist], tolerance_ms: f64) -> Vec<Issue> {
    let mut issues = Vec::new();
    let video_pls: Vec<&MediaPlaylist> = playlists.iter()
        .filter(|pl| pl.media_type == "VIDEO" && !pl.is_iframe)
        .collect();
    if video_pls.len() < 2 {
        return issues;
    }
    let tolerance_s = tolerance_ms / 1000.0;

    // Find the MSN range that every VIDEO rendition has in common.
    // This eliminates false positives from concurrent live fetches where one rendition
    // arrives with one extra segment, adding ~TARGETDURATION of spurious drift.
    let mut overlap_start = 0u64;
    let mut overlap_end = u64::MAX;
    for pl in &video_pls {
        let start = pl.media_sequence + pl.skipped_segments;
        let end = start + pl.segments.len() as u64;
        overlap_start = overlap_start.max(start);
        overlap_end = overlap_end.min(end);
    }
    if overlap_start >= overlap_end {
        // No shared window at all — renditions are completely disjoint; skip.
        return issues;
    }
    let window_size = (overlap_end - overlap_start) as usize;

    // Sum EXTINF for each rendition over the common window only.
    let totals: Vec<(&str, f64)> = video_pls.iter().map(|pl| {
        let pl_start = pl.media_sequence + pl.skipped_segments;
        let sum: f64 = (overlap_start..overlap_end)
            .filter_map(|msn| {
                let idx = (msn - pl_start) as usize;
                pl.segments.get(idx).map(|s| s.duration)
            })
            .sum();
        (pl.name.as_str(), sum)
    }).collect();

    let max_total = totals.iter().map(|(_, t)| *t).fold(f64::NEG_INFINITY, f64::max);
    let min_total = totals.iter().map(|(_, t)| *t).fold(f64::INFINITY, f64::min);
    let drift = max_total - min_total;
    if drift > tolerance_s {
        let details: Vec<String> = totals.iter()
            .map(|(name, total)| format!("{}={:.3}s", name, total))
            .collect();
        issues.push(Issue::warn(format!(
            "Cumulative EXTINF drift across renditions: {:.3}s (tolerance={:.3}s) \
             over {} common segments (MSN {}-{}). Totals: {}",
            drift, tolerance_s, window_size, overlap_start, overlap_end - 1,
            details.join(", ")
        )));
    }
    issues
}

/// MSN-aligned segment pairing helper
fn overlapping_segments<'a>(
    pl_a: &'a MediaPlaylist,
    pl_b: &'a MediaPlaylist,
) -> Vec<(&'a Segment, &'a Segment, u64)> {
    let mut pairs = Vec::new();
    let start_a = pl_a.media_sequence + pl_a.skipped_segments;
    let end_a = start_a + pl_a.segments.len() as u64;
    let start_b = pl_b.media_sequence + pl_b.skipped_segments;
    let end_b = start_b + pl_b.segments.len() as u64;
    let overlap_start = start_a.max(start_b);
    let overlap_end = end_a.min(end_b);
    for msn in overlap_start..overlap_end {
        let idx_a = (msn - start_a) as usize;
        let idx_b = (msn - start_b) as usize;
        if idx_a < pl_a.segments.len() && idx_b < pl_b.segments.len() {
            pairs.push((&pl_a.segments[idx_a], &pl_b.segments[idx_b], msn));
        }
    }
    pairs
}

/// LL-HLS compliance checks (draft-pantos-hls-rfc8216bis §4.4.3–4.4.5, §6.2.5.2)
pub fn check_ll_hls_compliance(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();

    // ── Per-rendition checks ──────────────────────────────────────────────────
    for pl in playlists {
        let has_parts = !pl.parts.is_empty();
        let has_part_inf = pl.part_target.is_some();
        let has_server_control = pl.server_control.is_some();
        if !has_parts && !has_part_inf && !has_server_control {
            continue;
        }

        // 1. EXT-X-PART-INF / PART-TARGET required when parts exist
        if has_parts && !has_part_inf {
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "LL-HLS §6.2.5.2: '{}' contains EXT-X-PART tags but no EXT-X-PART-INF. \
                     PART-TARGET is required.",
                    pl.name
                ),
                uri_note: None,
                ..Default::default()
            });
        }

        // 2. CAN-BLOCK-RELOAD=YES required when parts exist
        if has_parts {
            let can_block = pl.server_control.as_ref().is_some_and(|sc| sc.can_block_reload);
            if !can_block {
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!(
                        "LL-HLS §4.4.3.8: '{}' has EXT-X-PART tags but EXT-X-SERVER-CONTROL \
                         CAN-BLOCK-RELOAD=YES is absent. This is REQUIRED for low-latency delivery.",
                        pl.name
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
        }

        // 3. Part durations must not exceed PART-TARGET
        if let Some(pt) = pl.part_target {
            for (idx, part) in pl.parts.iter().enumerate() {
                if part.duration > pt + 0.001 {
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: idx as i32,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: Some(part.uri.clone()),
                        uri_b: None,
                        message: format!(
                            "LL-HLS §4.4.4.9: Part {} in '{}' has duration {:.5}s exceeding \
                             PART-TARGET {:.5}s.",
                            idx, pl.name, part.duration, pt
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                }
            }
        }

        // 4. First EXT-X-PART of every parent segment MUST carry INDEPENDENT=YES
        {
            let mut pending_independent: Vec<bool> = Vec::new();
            let mut seg_idx: usize = 0;
            for line in pl.raw_content.lines() {
                let l = line.trim();
                if let Some(rest) = l.strip_prefix("#EXT-X-PART:") {
                    let attrs = super::parser::parse_attributes(rest);
                    let indep = attrs.get("INDEPENDENT").is_some_and(|v| v == "YES");
                    pending_independent.push(indep);
                } else if l.starts_with("#EXTINF:") {
                    if let Some(&first_indep) = pending_independent.first()
                        && !first_indep {
                            issues.push(Issue {
                                severity: Severity::Error,
                                segment_index: seg_idx as i32,
                                rendition_a: Some(pl.name.clone()),
                                rendition_b: None,
                                uri_a: None,
                                uri_b: None,
                                message: format!(
                                    "LL-HLS §4.4.4.9: First EXT-X-PART before segment {} in '{}' \
                                     must carry INDEPENDENT=YES.",
                                    seg_idx, pl.name
                                ),
                                uri_note: None,
                                ..Default::default()
                            });
                        }
                    pending_independent.clear();
                    seg_idx += 1;
                }
            }
        }

        // 5. EXT-X-PRELOAD-HINT with TYPE=PART should be present at playlist tail
        if has_parts {
            let has_part_hint = pl.preload_hint_uri.is_some()
                && pl.preload_hint_type.as_deref() == Some("PART");
            if !has_part_hint {
                issues.push(Issue {
                    severity: Severity::Warn,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!(
                        "LL-HLS §4.4.5.3: '{}' is missing EXT-X-PRELOAD-HINT with TYPE=PART \
                         at the playlist tail. Clients cannot prefetch the next partial segment.",
                        pl.name
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
        }

        // 6. EXT-X-RENDITION-REPORT should be present
        if has_parts && pl.rendition_reports.is_empty() {
            issues.push(Issue {
                severity: Severity::Warn,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: None,
                uri_b: None,
                message: format!(
                    "LL-HLS §4.4.5.4: '{}' has no EXT-X-RENDITION-REPORT tags. \
                     Each media playlist should report the last MSN/Part of every \
                     other rendition so clients can switch without extra fetches.",
                    pl.name
                ),
                uri_note: None,
                ..Default::default()
            });
        }

        // 7. SERVER-CONTROL: CAN-SKIP-UNTIL MUST be >= 6× TARGETDURATION (§4.4.3.8)
        if let Some(sc) = &pl.server_control
            && let Some(csu) = sc.can_skip_until
                && pl.target_duration > 0.0 && csu < pl.target_duration * 6.0 - 0.001 {
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: None,
                        uri_b: None,
                        message: format!(
                            "rfc8216bis §4.4.3.8: '{}' CAN-SKIP-UNTIL={:.3}s < 6× \
                             TARGETDURATION={:.3}s (MUST be ≥ {:.3}s).",
                            pl.name, csu, pl.target_duration, pl.target_duration * 6.0
                        ),
                        uri_note: Some(format!(
                            "ratio={:.2}×, minimum 6.00×", csu / pl.target_duration
                        )),
                        ..Default::default()
                    });
                }

        // 8. SERVER-CONTROL: PART-HOLD-BACK >= 2× PART-TARGET (MUST), >= 3× (SHOULD)
        if let Some(sc) = &pl.server_control {
            if let (Some(phb), Some(pt)) = (sc.part_hold_back, pl.part_target) {
                if phb < pt * 2.0 - 0.001 {
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: None,
                        uri_b: None,
                        message: format!(
                            "LL-HLS §4.4.3.8: '{}' PART-HOLD-BACK={:.5}s < 2× PART-TARGET={:.5}s \
                             (MUST be ≥ {:.5}s).",
                            pl.name, phb, pt, pt * 2.0
                        ),
                        uri_note: Some(format!("ratio={:.3}×, MUST be ≥ 2.000×", phb / pt)),
                        ..Default::default()
                    });
                } else if phb < pt * 3.0 - 0.001 {
                    issues.push(Issue {
                        severity: Severity::Warn,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: None,
                        uri_b: None,
                        message: format!(
                            "LL-HLS §4.4.3.8: '{}' PART-HOLD-BACK={:.5}s < 3× PART-TARGET={:.5}s \
                             (SHOULD be ≥ {:.5}s).",
                            pl.name, phb, pt, pt * 3.0
                        ),
                        uri_note: Some(format!("ratio={:.3}×, SHOULD be ≥ 3.000×", phb / pt)),
                        ..Default::default()
                    });
                }
            }
            // HOLD-BACK >= 3× TARGETDURATION
            if let Some(hb) = sc.hold_back
                && pl.target_duration > 0.0 && hb < pl.target_duration * 3.0 - 0.001 {
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: None,
                        uri_b: None,
                        message: format!(
                            "LL-HLS §4.4.3.8: '{}' HOLD-BACK={:.3}s < 3× TARGETDURATION={:.3}s \
                             (MUST be ≥ {:.3}s).",
                            pl.name, hb, pl.target_duration, pl.target_duration * 3.0
                        ),
                        uri_note: Some(format!(
                            "ratio={:.2}×, minimum 3.00×", hb / pl.target_duration
                        )),
                        ..Default::default()
                    });
                }
        }
    }

    // ── Cross-rendition: LAST-MSN skew in EXT-X-RENDITION-REPORT ─────────────
    {
        let mut msn_reports: HashMap<String, Vec<i64>> = HashMap::new();
        for pl in playlists {
            if pl.parts.is_empty() { continue; }
            for rr in &pl.rendition_reports {
                if rr.last_msn >= 0 {
                    msn_reports.entry(rr.uri.clone()).or_default().push(rr.last_msn);
                }
            }
        }
        for (uri, msn_list) in &msn_reports {
            if msn_list.len() < 2 { continue; }
            let max_msn = *msn_list.iter().max().unwrap();
            let min_msn = *msn_list.iter().min().unwrap();
            let skew = max_msn - min_msn;
            if skew > 1 {
                let short_uri = uri.rsplit('/').next().unwrap_or(uri.as_str());
                issues.push(Issue::warn(format!(
                    "LL-HLS §4.4.5.4: EXT-X-RENDITION-REPORT LAST-MSN skew of {} segments \
                     for '{}' (reported MSNs: {}–{}). All renditions should report the same \
                     LAST-MSN ±1 segment.",
                    skew, short_uri, min_msn, max_msn
                )));
            }
        }
    }

    // ── Cross-rendition: EXT-X-SERVER-CONTROL must be identical (§6.2.4) ─────
    {
        let ll_pls: Vec<&MediaPlaylist> = playlists.iter()
            .filter(|pl| !pl.parts.is_empty())
            .collect();
        if ll_pls.len() >= 2 {
            type ScKey = (u64, u64, bool, u64);
            let sc_configs: Vec<(&str, ScKey)> = ll_pls.iter().map(|pl| {
                let key = pl.server_control.as_ref().map_or(
                    (0, 0, false, 0),
                    |sc| (
                        sc.hold_back.unwrap_or(0.0) as u64,
                        sc.part_hold_back.unwrap_or(0.0) as u64,
                        sc.can_block_reload,
                        sc.can_skip_until.unwrap_or(0.0) as u64,
                    )
                );
                (pl.name.as_str(), key)
            }).collect();
            let unique: HashSet<ScKey> = sc_configs.iter().map(|(_, k)| *k).collect();
            if unique.len() > 1 {
                let detail: Vec<String> = sc_configs.iter().map(|(name, _)| name.to_string()).collect();
                issues.push(Issue::error(format!(
                    "LL-HLS §6.2.4: EXT-X-SERVER-CONTROL attributes differ across renditions [{}]. \
                     All Media Playlists MUST carry identical SERVER-CONTROL values.",
                    detail.join(", ")
                )));
            }
        }
    }

    issues
}

/// Extract an MSN embedded in a segment URI filename.
///
/// Supports two common CDN patterns:
///   1. Purely numeric filename:  …/151674692.m4v
///   2. Hyphenated CDN pattern:   …/20260715T225158-151674692-03-ts.m4v
///      (MSN is the second hyphen-delimited field when it is ≥ 6 digits)
fn extract_msn_from_uri(uri: &str) -> Option<u64> {
    let filename = uri.rsplit('/').next().unwrap_or("").split('.').next().unwrap_or("");
    if !filename.is_empty() && filename.chars().all(|c| c.is_ascii_digit()) {
        return filename.parse().ok();
    }
    let parts: Vec<&str> = filename.split('-').collect();
    if parts.len() >= 2
        && parts[1].chars().all(|c| c.is_ascii_digit())
        && parts[1].len() >= 6
    {
        return parts[1].parse().ok();
    }
    None
}

/// rfc8216bis §4.4.3.2 — EXT-X-MEDIA-SEQUENCE presence, ordering, and URI-MSN consistency
pub fn check_media_sequence_continuity(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in playlists {
        let segs = &pl.segments;
        if segs.is_empty() { continue; }

        let tag_absent = pl.media_sequence == 0
            && !pl.raw_content.contains("#EXT-X-MEDIA-SEQUENCE:");

        if tag_absent {
            if !pl.has_endlist && segs.len() > 1 {
                issues.push(Issue::warn(format!(
                    "rfc8216bis §4.4.3.2: Live playlist '{}' does not declare \
                     EXT-X-MEDIA-SEQUENCE. For live playlists the tag SHOULD be present \
                     so clients can track the sliding window.",
                    pl.name
                )));
            }
            if let Some(uri_msn) = extract_msn_from_uri(&segs[0].uri)
                && uri_msn != 0 {
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: 0,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: Some(segs[0].uri.clone()),
                        uri_b: None,
                        message: format!(
                            "rfc8216bis §4.4.3.2: EXT-X-MEDIA-SEQUENCE absent from '{}' \
                             but first segment URI implies MSN={}. Add EXT-X-MEDIA-SEQUENCE \
                             to declare the correct base MSN.",
                            pl.name, uri_msn
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                }
        }

        // EXT-X-MEDIA-SEQUENCE MUST appear before the first Media Segment URI
        {
            let mut tag_line: Option<usize> = None;
            let mut first_seg_line: Option<usize> = None;
            let mut after_extinf = false;
            for (i, line) in pl.raw_content.lines().enumerate() {
                let l = line.trim();
                if l.starts_with("#EXT-X-MEDIA-SEQUENCE:") && tag_line.is_none() {
                    tag_line = Some(i);
                }
                if after_extinf && !l.starts_with('#') && !l.is_empty() && first_seg_line.is_none() {
                    first_seg_line = Some(i);
                }
                after_extinf = l.starts_with("#EXTINF:");
            }
            if let (Some(tl), Some(sl)) = (tag_line, first_seg_line)
                && tl > sl {
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: None,
                        uri_b: None,
                        message: format!(
                            "rfc8216bis §4.4.3.2: EXT-X-MEDIA-SEQUENCE MUST appear before \
                             the first Media Segment URI in '{}' \
                             (tag at line {}, first segment at line {}).",
                            pl.name, tl + 1, sl + 1
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                }
        }

        // URI-embedded MSN consistency
        let base_msn = pl.media_sequence + pl.skipped_segments;
        for (idx, seg) in segs.iter().enumerate() {
            let expected = base_msn + idx as u64;
            if let Some(actual) = extract_msn_from_uri(&seg.uri)
                && actual != expected {
                    issues.push(Issue {
                        severity: Severity::Warn,
                        segment_index: idx as i32,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: Some(seg.uri.clone()),
                        uri_b: None,
                        message: format!(
                            "rfc8216bis §4.4.3.2: Segment MSN mismatch in '{}' at index {}: \
                             URI implies MSN={} but expected MSN={} \
                             (base={}, skipped={}, index={}).",
                            pl.name, idx, actual, expected,
                            pl.media_sequence, pl.skipped_segments, idx
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                }
        }
    }
    issues
}

/// HLS Interstitials validation (Appendix D)
pub fn check_interstitials(playlists: &[MediaPlaylist]) -> (Vec<Issue>, Vec<Interstitial>) {
    let mut issues = Vec::new();
    let mut interstitials = Vec::new();

    for pl in playlists {
        if pl.raw_content.is_empty() {
            continue;
        }
        // Per rfc8216bis section D.2 a DATERANGE with the same ID as a
        // previously-seen tag is an update; update tags don't need to
        // repeat X-ASSET-URI/X-ASSET-LIST, so we only validate + push
        // the first occurrence of each ID.
        let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for line in pl.raw_content.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("#EXT-X-DATERANGE:") else {
                continue;
            };
            let attrs = super::parser::parse_attributes(rest);
            let class = attrs.get("CLASS").cloned().unwrap_or_default();
            if !class.contains("com.apple.hls.interstitial") {
                continue;
            }
            let dr_id = attrs.get("ID").cloned().unwrap_or_default();
            let is_update = !dr_id.is_empty() && seen_ids.contains(&dr_id);
            if !is_update {
                seen_ids.insert(dr_id.clone());
            }
            if is_update {
                continue;
            }
            let start_date = attrs.get("START-DATE").cloned().unwrap_or_default();
            let asset_uri = attrs.get("X-ASSET-URI").cloned();
            let asset_list = attrs.get("X-ASSET-LIST").cloned();
            let resume_offset = attrs.get("X-RESUME-OFFSET").and_then(|v| v.parse::<f64>().ok());
            let playout_limit = attrs.get("X-PLAYOUT-LIMIT").and_then(|v| v.parse::<f64>().ok());
            // PLANNED-DURATION is present on the OUT tag; X-PLAYOUT-LIMIT may be on the IN tag only
            let planned_duration_s = attrs.get("PLANNED-DURATION").and_then(|v| v.parse::<f64>().ok());
            let snap = attrs.get("X-SNAP").cloned();
            let cue = attrs.get("X-CUE").cloned();
            let timeline_style = attrs.get("X-TIMELINE-STYLE").cloned();

            let mut entry_errors = Vec::new();

            // MUST have ID
            if dr_id.is_empty() {
                entry_errors.push("Missing required ID attribute".to_string());
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!("Interstitial: DATERANGE in '{}' missing ID (MUST)", pl.name),
                    uri_note: None,
                    ..Default::default()
                });
            }

            // MUST have START-DATE
            if start_date.is_empty() {
                entry_errors.push("Missing required START-DATE".to_string());
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!("Interstitial: [{}] missing START-DATE in '{}'", dr_id, pl.name),
                    uri_note: None,
                    ..Default::default()
                });
            }

            // MUST have X-ASSET-URI or X-ASSET-LIST
            if asset_uri.is_none() && asset_list.is_none() {
                entry_errors.push("Missing X-ASSET-URI or X-ASSET-LIST (MUST have one)".to_string());
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!("Interstitial: [{}] missing X-ASSET-URI/X-ASSET-LIST in '{}'", dr_id, pl.name),
                    uri_note: None,
                    ..Default::default()
                });
            }

            // MUST NOT have both
            if asset_uri.is_some() && asset_list.is_some() {
                entry_errors.push("Has both X-ASSET-URI and X-ASSET-LIST (MUST have only one)".to_string());
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!("Interstitial: [{}] has both X-ASSET-URI and X-ASSET-LIST in '{}' (MUST NOT)", dr_id, pl.name),
                    uri_note: None,
                    ..Default::default()
                });
            }

            // Validate X-SNAP — §D.2: enumerated-string-list, values IN and OUT only
            if let Some(ref s) = snap {
                for part in s.split(',') {
                    let p = part.trim();
                    if p != "IN" && p != "OUT" {
                        entry_errors.push(format!("X-SNAP value '{}' invalid (must be IN or OUT)", p));
                        issues.push(Issue {
                            severity: Severity::Warn,
                            segment_index: -1,
                            rendition_a: Some(pl.name.clone()),
                            rendition_b: None,
                            uri_a: None,
                            uri_b: None,
                            message: format!(
                                "Interstitial: [{}] X-SNAP='{}' invalid in '{}' \
                                 (rfc8216bis §D.2: values must be OUT and/or IN).",
                                dr_id, p, pl.name
                            ),
                            uri_note: None,
                            ..Default::default()
                        });
                    }
                }
            }

            // Validate X-RESTRICT — §D.2: enumerated-string-list, values SKIP and JUMP only
            if let Some(ref restrict) = attrs.get("X-RESTRICT").cloned() {
                for part in restrict.split(',') {
                    let p = part.trim();
                    if p != "SKIP" && p != "JUMP" {
                        entry_errors.push(format!("X-RESTRICT value '{}' invalid (must be SKIP or JUMP)", p));
                        issues.push(Issue {
                            severity: Severity::Warn,
                            segment_index: -1,
                            rendition_a: Some(pl.name.clone()),
                            rendition_b: None,
                            uri_a: None,
                            uri_b: None,
                            message: format!(
                                "Interstitial: [{}] X-RESTRICT='{}' invalid in '{}' \
                                 (rfc8216bis §D.2: values must be SKIP and/or JUMP).",
                                dr_id, p, pl.name
                            ),
                            uri_note: None,
                            ..Default::default()
                        });
                    }
                }
            }

            // Validate X-CONTENT-MAY-VARY — §D.2: valid values "YES" and "NO"
            if let Some(ref cmv) = attrs.get("X-CONTENT-MAY-VARY").cloned()
                && cmv != "YES" && cmv != "NO" {
                    entry_errors.push(format!("X-CONTENT-MAY-VARY='{}' invalid (must be YES or NO)", cmv));
                    issues.push(Issue {
                        severity: Severity::Warn,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: None,
                        uri_b: None,
                        message: format!(
                            "Interstitial: [{}] X-CONTENT-MAY-VARY='{}' invalid in '{}' \
                             (rfc8216bis §D.2: must be \"YES\" or \"NO\").",
                            dr_id, cmv, pl.name
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                }

            // Validate X-TIMELINE-OCCUPIES — §D.2: valid values "POINT" and "RANGE"
            if let Some(ref to) = attrs.get("X-TIMELINE-OCCUPIES").cloned()
                && to != "POINT" && to != "RANGE" {
                    entry_errors.push(format!("X-TIMELINE-OCCUPIES='{}' invalid (must be POINT or RANGE)", to));
                    issues.push(Issue {
                        severity: Severity::Warn,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: None,
                        uri_b: None,
                        message: format!(
                            "Interstitial: [{}] X-TIMELINE-OCCUPIES='{}' invalid in '{}' \
                             (rfc8216bis §D.2: must be \"POINT\" or \"RANGE\").",
                            dr_id, to, pl.name
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                }

            // Validate X-TIMELINE-STYLE — §D.2: valid values "HIGHLIGHT" and "PRIMARY"
            if let Some(ref ts) = attrs.get("X-TIMELINE-STYLE").cloned()
                && ts != "HIGHLIGHT" && ts != "PRIMARY" {
                    entry_errors.push(format!("X-TIMELINE-STYLE='{}' invalid (must be HIGHLIGHT or PRIMARY)", ts));
                    issues.push(Issue {
                        severity: Severity::Warn,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: None,
                        uri_b: None,
                        message: format!(
                            "Interstitial: [{}] X-TIMELINE-STYLE='{}' invalid in '{}' \
                             (rfc8216bis §D.2: must be \"HIGHLIGHT\" or \"PRIMARY\").",
                            dr_id, ts, pl.name
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                }

            // Validate X-SKIP-CONTROL-LABEL-ID — §D.3: characters must be [a-z][A-Z]'-''_' only
            if let Some(ref label_id) = attrs.get("X-SKIP-CONTROL-LABEL-ID").cloned() {
                let invalid_chars: Vec<char> = label_id.chars()
                    .filter(|&c| !c.is_ascii_alphabetic() && c != '-' && c != '_')
                    .collect();
                if !invalid_chars.is_empty() {
                    entry_errors.push(format!(
                        "X-SKIP-CONTROL-LABEL-ID='{}' contains invalid chars {:?}",
                        label_id, invalid_chars
                    ));
                    issues.push(Issue {
                        severity: Severity::Error,
                        segment_index: -1,
                        rendition_a: Some(pl.name.clone()),
                        rendition_b: None,
                        uri_a: None,
                        uri_b: None,
                        message: format!(
                            "Interstitial: [{}] X-SKIP-CONTROL-LABEL-ID='{}' contains \
                             invalid characters {:?} in '{}' \
                             (rfc8216bis §D.3: MUST be [a-z][A-Z]'-''_' only).",
                            dr_id, label_id, invalid_chars, pl.name
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                }
            }

            interstitials.push(Interstitial {
                rendition: pl.name.clone(),
                id: dr_id,
                start_date,
                asset_uri,
                asset_list,
                resume_offset,
                playout_limit,
                planned_duration_s,
                snap,
                cue,
                timeline_style,
                errors: entry_errors,
                start_offset_s: None,
                content_duration_s: 0.0,
                rendition_url: pl.url.clone(),
                definitions: pl.definitions.clone(),
            });
        }
    }

    // ── Second pass: merge IN-tag attributes (X-PLAYOUT-LIMIT, X-RESUME-OFFSET) ──────────────
    // The OUT DATERANGE carries X-ASSET-LIST, PLANNED-DURATION, CLASS, etc.
    // The paired IN DATERANGE (same ID, no CLASS) carries X-PLAYOUT-LIMIT and X-RESUME-OFFSET.
    // Build a map from ID → index of the FIRST occurrence in `interstitials`.
    let mut id_to_idx: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (idx, it) in interstitials.iter().enumerate() {
        id_to_idx.entry(it.id.clone()).or_insert(idx);
    }
    for pl in playlists {
        for line in pl.raw_content.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("#EXT-X-DATERANGE:") else { continue; };
            let attrs = super::parser::parse_attributes(rest);
            // Skip OUT tags (already processed above) — only want IN tags (no CLASS)
            if attrs.get("CLASS").is_some_and(|c| c.contains("com.apple.hls.interstitial")) {
                continue;
            }
            let id = attrs.get("ID").cloned().unwrap_or_default();
            if let Some(&idx) = id_to_idx.get(&id) {
                let it = &mut interstitials[idx];
                if it.playout_limit.is_none() {
                    it.playout_limit = attrs.get("X-PLAYOUT-LIMIT").and_then(|v| v.parse::<f64>().ok());
                }
                if it.resume_offset.is_none() {
                    it.resume_offset = attrs.get("X-RESUME-OFFSET").and_then(|v| v.parse::<f64>().ok());
                }
            }
        }
    }

    (issues, interstitials)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn make_playlist(name: &str, content: &str) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.to_string(), format!("https://cdn.example.com/{name}.m3u8"));
        pl.raw_content = content.to_string();
        pl
    }

    fn make_segment(uri: &str, duration: f64) -> Segment {
        Segment {
            uri: uri.to_string(),
            duration,
            title: None,
            pdt: None,
            discontinuity: false,
            byterange: None,
            is_ad: false,
            map_uri: None,
        }
    }

    fn make_segment_with_pdt(uri: &str, duration: f64, pdt: f64) -> Segment {
        Segment { pdt: Some(pdt), ..make_segment(uri, duration) }
    }

    // ── check_extm3u_header ───────────────────────────────────────────────────

    #[test]
    fn extm3u_header_passes_when_first_line_is_extm3u() {
        let pl = make_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n");
        let issues = check_extm3u_header(&[pl]);
        assert!(issues.is_empty(), "expected no issues, got: {:?}", issues);
    }

    #[test]
    fn extm3u_header_errors_when_first_line_is_missing() {
        let pl = make_playlist("v", "#EXT-X-TARGETDURATION:6\n#EXTM3U\n");
        let issues = check_extm3u_header(&[pl]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Error);
    }

    // ── check_target_duration_compliance ─────────────────────────────────────

    #[test]
    fn target_duration_missing_produces_error() {
        let pl = make_playlist("v", "#EXTM3U\n");
        let issues = check_target_duration_compliance(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("missing or zero")));
    }

    #[test]
    fn segment_within_target_duration_passes() {
        let mut pl = make_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n");
        pl.target_duration = 6.0;
        pl.segments = vec![make_segment("seg0.mp4", 5.9)];
        let issues = check_target_duration_compliance(&[pl]);
        assert!(issues.is_empty(), "expected no issues");
    }

    #[test]
    fn segment_rounding_to_target_passes() {
        // 5.5 rounds to 6 which equals TARGETDURATION:6 — should pass
        let mut pl = make_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n");
        pl.target_duration = 6.0;
        pl.segments = vec![make_segment("seg0.mp4", 5.5)];
        let issues = check_target_duration_compliance(&[pl]);
        assert!(issues.is_empty(), "5.5 rounds to 6 = TARGETDURATION, must pass");
    }

    #[test]
    fn segment_exceeding_target_duration_errors() {
        // 7.807 rounds to 8 > 6 → ERROR
        let mut pl = make_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n");
        pl.target_duration = 6.0;
        pl.segments = vec![make_segment("seg-bad.mp4", 7.807)];
        let issues = check_target_duration_compliance(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("exceeds TARGETDURATION")));
    }

    #[test]
    fn targetduration_much_larger_than_max_segment_warns() {
        // TARGETDURATION=10, max segment=3.9 → WARN because 10 > 3+1=4
        let mut pl = make_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:10\n");
        pl.target_duration = 10.0;
        pl.segments = vec![make_segment("seg0.mp4", 3.9), make_segment("seg1.mp4", 3.9)];
        let issues = check_target_duration_compliance(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Warn && i.message.contains("more than 1s above")));
    }

    // ── check_pdt_coverage ────────────────────────────────────────────────────

    #[test]
    fn pdt_coverage_all_present_no_issues() {
        let mut pl = make_playlist("v", "#EXTM3U\n");
        let base = 1_700_000_000.0_f64;
        pl.segments = vec![
            make_segment_with_pdt("s0.mp4", 4.0, base),
            make_segment_with_pdt("s1.mp4", 4.0, base + 4.0),
        ];
        let issues = check_pdt_coverage(&[pl]);
        assert!(issues.is_empty());
    }

    #[test]
    fn pdt_coverage_partial_warns() {
        let mut pl = make_playlist("v", "#EXTM3U\n");
        pl.segments = vec![
            make_segment_with_pdt("s0.mp4", 4.0, 1_700_000_000.0),
            make_segment("s1.mp4", 4.0),  // no PDT
        ];
        let issues = check_pdt_coverage(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Warn && i.message.contains("partial PDT")));
    }

    #[test]
    fn pdt_coverage_none_no_issues() {
        let mut pl = make_playlist("v", "#EXTM3U\n");
        pl.segments = vec![make_segment("s0.mp4", 4.0), make_segment("s1.mp4", 4.0)];
        let issues = check_pdt_coverage(&[pl]);
        assert!(issues.is_empty());
    }

    // ── check_media_sequence_duplicate_tags ───────────────────────────────────

    #[test]
    fn duplicate_targetduration_errors() {
        let content = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-TARGETDURATION:4\n";
        let pl = make_playlist("v", content);
        let issues = check_media_sequence_duplicate_tags(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("EXT-X-TARGETDURATION")));
    }

    #[test]
    fn single_targetduration_passes() {
        let content = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n";
        let pl = make_playlist("v", content);
        let issues = check_media_sequence_duplicate_tags(&[pl]);
        assert!(issues.is_empty());
    }

    // ── check_live_playlist_min_segments ─────────────────────────────────────

    #[test]
    fn live_playlist_with_two_segments_errors() {
        let mut pl = make_playlist("v", "#EXTM3U\n");
        pl.has_endlist = false;
        pl.segments = vec![make_segment("s0.mp4", 4.0), make_segment("s1.mp4", 4.0)];
        let issues = check_live_playlist_min_segments(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("at least 3 segments")));
    }

    #[test]
    fn vod_playlist_with_two_segments_passes_min_segment_check() {
        let mut pl = make_playlist("v", "#EXTM3U\n#EXT-X-ENDLIST\n");
        pl.has_endlist = true;
        pl.segments = vec![make_segment("s0.mp4", 4.0), make_segment("s1.mp4", 4.0)];
        let issues = check_live_playlist_min_segments(&[pl]);
        assert!(issues.is_empty(), "VOD playlist must not trigger live-segment count check");
    }

    // ── check_playlist_type_endlist ───────────────────────────────────────────

    #[test]
    fn vod_without_endlist_errors() {
        let mut pl = make_playlist("v", "#EXTM3U\n#EXT-X-PLAYLIST-TYPE:VOD\n");
        pl.playlist_type = Some("VOD".to_string());
        pl.has_endlist = false;
        let issues = check_playlist_type_endlist(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("PLAYLIST-TYPE:VOD")));
    }

    #[test]
    fn vod_with_endlist_passes() {
        let mut pl = make_playlist("v", "#EXTM3U\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-ENDLIST\n");
        pl.playlist_type = Some("VOD".to_string());
        pl.has_endlist = true;
        let issues = check_playlist_type_endlist(&[pl]);
        assert!(issues.is_empty());
    }

    #[test]
    fn event_without_endlist_passes() {
        // EVENT playlist without ENDLIST is valid during a live event
        let mut pl = make_playlist("v", "#EXTM3U\n#EXT-X-PLAYLIST-TYPE:EVENT\n");
        pl.playlist_type = Some("EVENT".to_string());
        pl.has_endlist = false;
        let issues = check_playlist_type_endlist(&[pl]);
        assert!(issues.is_empty());
    }

    // ── extract_msn_from_uri (private, tested within module) ─────────────────

    #[test]
    fn extract_msn_numeric_filename() {
        assert_eq!(extract_msn_from_uri("https://cdn.example.com/segs/151674692.mp4"), Some(151674692));
    }

    #[test]
    fn extract_msn_hyphenated_cdn_pattern() {
        assert_eq!(
            extract_msn_from_uri("https://cdn.example.com/20260715T225158-151674692-03-ts.m4v"),
            Some(151674692)
        );
    }

    #[test]
    fn extract_msn_short_field_not_matched() {
        // Second field has only 4 digits — too short to be a CDN MSN
        assert_eq!(extract_msn_from_uri("https://cdn.example.com/seg-1234-abc.mp4"), None);
    }

    // ── check_interstitials ───────────────────────────────────────────────────

    #[test]
    fn interstitial_valid_with_asset_uri() {
        let content = concat!(
            "#EXTM3U\n",
            "#EXT-X-DATERANGE:",
            r#"ID="ad-1",START-DATE="2024-01-15T12:00:00Z","#,
            r#"CLASS="com.apple.hls.interstitial","#,
            "X-ASSET-URI=\"https://ads.example.com/ad.m3u8\"\n",
        );
        let pl = make_playlist("v", content);
        let (issues, interstitials) = check_interstitials(&[pl]);
        assert!(issues.is_empty(), "valid interstitial must produce no issues: {:?}", issues);
        assert_eq!(interstitials.len(), 1);
        assert_eq!(interstitials[0].id, "ad-1");
        assert!(interstitials[0].asset_uri.is_some());
    }

    #[test]
    fn interstitial_missing_id_errors() {
        let content = concat!(
            "#EXTM3U\n",
            "#EXT-X-DATERANGE:",
            r#"START-DATE="2024-01-15T12:00:00Z","#,
            r#"CLASS="com.apple.hls.interstitial","#,
            "X-ASSET-URI=\"https://ads.example.com/ad.m3u8\"\n",
        );
        let pl = make_playlist("v", content);
        let (issues, _) = check_interstitials(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("missing ID")));
    }

    #[test]
    fn interstitial_missing_asset_errors() {
        let content = concat!(
            "#EXTM3U\n",
            "#EXT-X-DATERANGE:",
            r#"ID="ad-1",START-DATE="2024-01-15T12:00:00Z","#,
            "CLASS=\"com.apple.hls.interstitial\"\n",
        );
        let pl = make_playlist("v", content);
        let (issues, _) = check_interstitials(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("X-ASSET-URI/X-ASSET-LIST")));
    }

    #[test]
    fn interstitial_both_asset_uri_and_list_errors() {
        let content = concat!(
            "#EXTM3U\n",
            "#EXT-X-DATERANGE:",
            r#"ID="ad-1",START-DATE="2024-01-15T12:00:00Z","#,
            r#"CLASS="com.apple.hls.interstitial","#,
            "X-ASSET-URI=\"https://ads.example.com/ad.m3u8\",",
            "X-ASSET-LIST=\"https://ads.example.com/ads.json\"\n",
        );
        let pl = make_playlist("v", content);
        let (issues, _) = check_interstitials(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("both X-ASSET-URI and X-ASSET-LIST")));
    }

    #[test]
    fn interstitial_invalid_snap_value_warns() {
        let content = concat!(
            "#EXTM3U\n",
            "#EXT-X-DATERANGE:",
            r#"ID="ad-1",START-DATE="2024-01-15T12:00:00Z","#,
            r#"CLASS="com.apple.hls.interstitial","#,
            "X-ASSET-URI=\"https://ads.example.com/ad.m3u8\",",
            "X-SNAP=\"BEFORE\"\n",
        );
        let pl = make_playlist("v", content);
        let (issues, _) = check_interstitials(&[pl]);
        assert!(issues.iter().any(|i| i.severity == Severity::Warn && i.message.contains("X-SNAP")));
    }
    /// Regression: a second EXT-X-DATERANGE with the same ID and CLASS
    /// (per rfc8216bis §D.2 this is an "update" tag) must NOT trigger a
    /// false "Missing X-ASSET-URI or X-ASSET-LIST" error.
    #[test]
    fn interstitial_update_tag_no_false_asset_error() {
        // First DATERANGE: the real "OUT" tag with X-ASSET-LIST.
        // Second DATERANGE: same ID + CLASS, no asset attrs (update / IN tag).
        let content = concat!(
            "#EXTM3U\n",
            "#EXT-X-DATERANGE:",
            "ID=\"ad-sle-1\",",
            "START-DATE=\"2024-01-15T12:00:00Z\",",
            "CLASS=\"com.apple.hls.interstitial\",",
            "PLANNED-DURATION=30.0,",
            "X-ASSET-LIST=\"https://ads.example.com/assets.json\"\n",
            // Update tag — same ID, same CLASS, no asset attributes
            "#EXT-X-DATERANGE:",
            "ID=\"ad-sle-1\",",
            "START-DATE=\"2024-01-15T12:00:00Z\",",
            "CLASS=\"com.apple.hls.interstitial\",",
            "X-RESUME-OFFSET=0\n",
        );
        let pl = make_playlist("v1", content);
        let (issues, interstitials) = check_interstitials(&[pl]);
        // No errors — the update tag must not trigger a false positive
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == Severity::Error).collect();
        assert!(
            errors.is_empty(),
            "update DATERANGE tag must not produce errors, got: {:?}", errors
        );
        // Only one Interstitial should be created (the first occurrence)
        assert_eq!(interstitials.len(), 1, "update tag must not create a second Interstitial");
        assert!(interstitials[0].asset_list.is_some());
    }

}