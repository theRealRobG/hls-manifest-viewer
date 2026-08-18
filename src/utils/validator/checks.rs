use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use super::types::*;

/// Record `id` as the producer of every finding that has not already named its own check.
///
/// Findings are grouped in the report by the check that raised them, so a check that
/// returns without naming itself would land in the catch-all group.
fn produced_by(id: CheckId, mut issues: Vec<Issue>) -> Vec<Issue> {
    for issue in &mut issues {
        if issue.check_id == CheckId::Unassigned {
            issue.check_id = id;
        }
    }
    issues
}

/// One per-segment finding, together with what a summary of a whole run of them needs.
struct SegmentFinding<T> {
    /// What makes this the same finding as the one before it, apart from the segment it
    /// names: the rendition, the limit that was broken, the pair being compared.
    ///
    /// Renditions are identified here by position in the playlist slice, not by display
    /// name: two renditions can share a NAME, and folding by name would merge their runs
    /// into one finding that named a single rendition.
    key: String,
    issue: Issue,
    /// Measurement kept so a folded run can describe its own extremes.
    data: T,
}

/// Fold runs of consecutive per-segment findings that say the same thing into one finding.
///
/// A rendition whose whole window overruns TARGETDURATION used to produce one row per
/// segment, which buried every other finding in the report. [`Issue`] can carry a count and
/// a segment range, and the report already renders one as "Segments 12–48 (×37)", but no
/// check ever wrote the range — so the fields stayed at their defaults and the folding the
/// UI was built for never happened.
///
/// A run continues while the key matches and the segment index is the previous one plus one;
/// `summary` is asked for the wording only once a run is longer than a single segment.
fn fold_consecutive<T>(
    findings: Vec<SegmentFinding<T>>,
    summary: impl Fn(&Issue, &[T]) -> String,
) -> Vec<Issue> {
    fn close<T>(mut open: Issue, data: Vec<T>, summary: &impl Fn(&Issue, &[T]) -> String) -> Issue {
        if open.count > 1 {
            open.message = summary(&open, &data);
            // A run spans many segments, so the one pair of segment URIs the first finding
            // named no longer describes it. The renditions still do.
            open.uri_a = None;
            open.uri_b = None;
        }
        open
    }

    let mut folded: Vec<Issue> = Vec::new();
    let mut run: Option<(String, Issue, Vec<T>)> = None;
    for finding in findings {
        let continues = run.as_ref().is_some_and(|(key, open, _)| {
            *key == finding.key && finding.issue.segment_index == open.seg_last + 1
        });
        if continues {
            let (_, open, data) = run.as_mut().expect("a run only continues if there is one");
            open.seg_last = finding.issue.segment_index;
            open.count += 1;
            data.push(finding.data);
            continue;
        }
        if let Some((_, open, data)) = run.take() {
            folded.push(close(open, data, &summary));
        }
        let index = finding.issue.segment_index;
        let open = Issue { seg_first: index, seg_last: index, count: 1, ..finding.issue };
        run = Some((finding.key, open, vec![finding.data]));
    }
    if let Some((_, open, data)) = run.take() {
        folded.push(close(open, data, &summary));
    }
    folded
}

/// The largest of a run's measurements, for a summary that describes its worst case.
fn largest(values: impl IntoIterator<Item = f64>) -> f64 {
    values.into_iter().fold(f64::NEG_INFINITY, f64::max)
}

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
                    "rfc8216bis §4.4.1.1: Playlist '{}' does not start with #EXTM3U. \
                     First line: '{}'",
                    pl.name, first_line
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }
    produced_by(CheckId::ExtM3uHeader, issues)
}

/// RFC 8216bis §4.4.1.1, §4.4.1.2, §4.4.2 — structural checks on the Multivariant Playlist.
///
/// The multivariant playlist used to be read only for its variant attributes, so a
/// multivariant playlist that was missing #EXTM3U, repeated a singleton tag or used
/// variable substitution below its declared version passed without comment.
pub fn check_master_structure(master: &MasterPlaylist) -> Vec<Issue> {
    let mut issues = Vec::new();
    let label = if master.url.is_empty() { "multivariant playlist" } else { master.url.as_str() };

    let first_line = master.raw_content.lines().next().unwrap_or("");
    if first_line.trim() != "#EXTM3U" {
        issues.push(Issue {
            severity: Severity::Error,
            check_id: CheckId::ExtM3uHeader,
            message: format!(
                "rfc8216bis §4.4.1.1: Multivariant Playlist '{}' does not start with \
                 #EXTM3U. First line: '{}'",
                label, first_line
            ),
            uri_a: Some(master.url.clone()),
            ..Default::default()
        });
    }

    // §4.4.1.2 EXT-X-VERSION, §4.4.2.1 EXT-X-INDEPENDENT-SEGMENTS, §4.4.2.2 EXT-X-START.
    for tag in ["#EXT-X-VERSION:", "#EXT-X-INDEPENDENT-SEGMENTS", "#EXT-X-START:"] {
        let count = master.raw_content.lines().filter(|l| l.trim().starts_with(tag)).count();
        if count > 1 {
            issues.push(Issue {
                severity: Severity::Error,
                check_id: CheckId::SingletonTags,
                message: format!(
                    "rfc8216bis §4.4.1.2/§4.4.2: Singleton tag '{}' appears {} times in \
                     Multivariant Playlist '{}'. It MUST appear at most once.",
                    tag.trim_end_matches(':'), count, label
                ),
                uri_a: Some(master.url.clone()),
                ..Default::default()
            });
        }
    }

    // §8: variable substitution requires VERSION >= 8.
    if master.version < 8 && master.raw_content.contains("#EXT-X-DEFINE:") {
        issues.push(Issue {
            severity: Severity::Error,
            check_id: CheckId::VersionCompatibility,
            message: format!(
                "rfc8216bis §8: Multivariant Playlist '{}' uses EXT-X-DEFINE \
                 (variable substitution) which requires VERSION >= 8. Declared version: {}.",
                label, master.version
            ),
            uri_a: Some(master.url.clone()),
            ..Default::default()
        });
    }

    issues
}

/// RFC 8216bis §4.4.6.2 — every AUDIO, SUBTITLES, CLOSED-CAPTIONS and VIDEO attribute on
/// EXT-X-STREAM-INF MUST match the GROUP-ID of an EXT-X-MEDIA tag of that TYPE.
pub fn check_rendition_group_references(master: &MasterPlaylist) -> Vec<Issue> {
    let mut issues = Vec::new();

    let group_ids = |media_type: &str| -> HashSet<&str> {
        master.media_renditions.iter()
            .filter(|r| r.media_type == media_type)
            .map(|r| r.group_id.as_str())
            .collect()
    };
    let audio_groups = group_ids("AUDIO");
    let subtitle_groups = group_ids("SUBTITLES");
    let caption_groups = group_ids("CLOSED-CAPTIONS");
    let video_groups = group_ids("VIDEO");

    for v in &master.variants {
        let refs: [(&str, Option<&str>, &HashSet<&str>); 4] = [
            ("AUDIO", v.audio_group.as_deref(), &audio_groups),
            ("SUBTITLES", v.subtitle_group.as_deref(), &subtitle_groups),
            // CLOSED-CAPTIONS=NONE is an enumerated string, not a group reference.
            (
                "CLOSED-CAPTIONS",
                v.closed_captions.as_deref().filter(|c| *c != "NONE"),
                &caption_groups,
            ),
            ("VIDEO", v.video_group.as_deref(), &video_groups),
        ];
        for (attr, referenced, defined) in refs {
            let Some(group) = referenced else { continue };
            if defined.contains(group) {
                continue;
            }
            let known = {
                let mut names: Vec<&str> = defined.iter().copied().collect();
                names.sort_unstable();
                if names.is_empty() { "(none)".to_string() } else { names.join(", ") }
            };
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: None,
                rendition_b: None,
                uri_a: Some(v.uri.clone()),
                uri_b: None,
                message: format!(
                    "rfc8216bis §4.4.6.2: EXT-X-STREAM-INF for URI '{}' has {}=\"{}\" but no \
                     EXT-X-MEDIA tag with TYPE={} declares that GROUP-ID. The attribute value \
                     MUST match the GROUP-ID of a Rendition Group of that type \
                     (declared {} groups: {}).",
                    v.uri, attr, group, attr, attr, known
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }

    produced_by(CheckId::RenditionGroupReferences, issues)
}

/// RFC 8216bis §4.4.3.1 — TARGETDURATION presence, per-segment compliance, and accuracy.
/// The spec says the EXTINF duration "when rounded to the nearest integer, MUST be less than
/// or equal to the Target Duration." (round-half-away-from-zero, matching common rounding.)
pub fn check_target_duration_compliance(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    // Segment overruns are folded into runs, so a rendition whose entire window is too long
    // is one finding naming the range rather than one finding per segment.
    let mut overruns: Vec<SegmentFinding<(f64, u64)>> = Vec::new();
    for (pl_index, pl) in playlists.iter().enumerate() {
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
                overruns.push(SegmentFinding {
                    key: format!("{}|{}", pl_index, target_int),
                    issue: Issue {
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
                    },
                    data: (seg.duration, target_int),
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
    issues.extend(fold_consecutive(overruns, |iss, run| {
        let target = run.first().map_or(0, |(_, target)| *target);
        let longest = largest(run.iter().map(|(duration, _)| *duration));
        format!(
            "rfc8216bis §4.4.3.1: Segments {}–{} in '{}' ({} consecutive segments) exceed \
             TARGETDURATION {}s; the longest is {:.6}s (round={}s). An EXTINF duration, \
             rounded to the nearest integer, MUST be less than or equal to the Target Duration.",
            iss.seg_first, iss.seg_last,
            iss.rendition_a.as_deref().unwrap_or("(unnamed)"),
            iss.count, target, longest, longest.round() as u64
        )
    }));
    produced_by(CheckId::TargetDurationCompliance, issues)
}

/// rfc8216bis §6.2.4, §4.4.5.1 — which playlists carry EXT-X-PROGRAM-DATE-TIME.
///
/// §6.2.4: if any Media Playlist of a Multivariant Playlist contains an
/// EXT-X-PROGRAM-DATE-TIME tag, then all of them MUST, with consistent mappings of date and
/// time to media timestamps. Whether the mappings agree is measured by
/// [`check_pdt_alignment`]; this check is about which playlists declare the tag at all.
///
/// §4.4.5.1: a Playlist that contains an EXT-X-DATERANGE tag MUST also contain at least one
/// EXT-X-PROGRAM-DATE-TIME tag, because a Date Range is positioned by wall-clock time and
/// there is nothing to anchor it to otherwise.
///
/// This reads the tag counts the parser recorded, not [`Segment::pdt`]. Every segment that
/// follows a single PDT tag carries an extrapolated `pdt`, so the old per-playlist "partial
/// coverage" rule was asking a question about extrapolated values: it could only fire for
/// segments *before* the first tag, called that a warning, and never looked across
/// renditions at the requirement §6.2.4 actually states.
pub fn check_pdt_coverage(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();

    for pl in playlists {
        if pl.program_date_time_tags == 0 && pl.raw_content.contains("#EXT-X-DATERANGE:") {
            issues.push(Issue {
                severity: Severity::Error,
                rendition_a: Some(pl.name.clone()),
                uri_a: Some(pl.url.clone()),
                message: format!(
                    "rfc8216bis §4.4.5.1: Playlist '{}' contains EXT-X-DATERANGE tags but no \
                     EXT-X-PROGRAM-DATE-TIME tag. A Playlist that contains an EXT-X-DATERANGE \
                     tag MUST also contain at least one EXT-X-PROGRAM-DATE-TIME tag, or a \
                     client has nothing to position the Date Range against.",
                    pl.name
                ),
                ..Default::default()
            });
        }
    }

    // §6.2.4 is a constraint on a presentation, so it needs more than one Media Playlist to
    // say anything: a media-playlist URL on its own is reported as N/A.
    if playlists.len() >= 2 {
        let (tagged, untagged): (Vec<&MediaPlaylist>, Vec<&MediaPlaylist>) = playlists.iter()
            .partition(|pl| pl.program_date_time_tags > 0);
        if !tagged.is_empty() && !untagged.is_empty() {
            let names = |pls: &[&MediaPlaylist]| {
                pls.iter().map(|pl| format!("'{}'", pl.name)).collect::<Vec<_>>().join(", ")
            };
            issues.push(Issue {
                severity: Severity::Error,
                uri_a: untagged.first().map(|pl| pl.url.clone()),
                message: format!(
                    "rfc8216bis §6.2.4: {} of {} Media Playlists declare \
                     EXT-X-PROGRAM-DATE-TIME and {} do not ({} have it, {} do not). If any \
                     Media Playlist in a Multivariant Playlist contains an \
                     EXT-X-PROGRAM-DATE-TIME tag, then all of them MUST, with consistent \
                     mappings of date and time to media timestamps — otherwise a client \
                     cannot line the renditions up on a wall clock.",
                    tagged.len(), playlists.len(), untagged.len(),
                    names(&tagged), names(&untagged)
                ),
                ..Default::default()
            });
        }
    }

    produced_by(CheckId::PdtCoverage, issues)
}

/// RFC 8216bis §4.4.1.2, §4.4.2, §4.4.3 — Singleton tags must not appear more than once.
/// Covers: EXT-X-VERSION (§4.4.1.2), EXT-X-INDEPENDENT-SEGMENTS (§4.4.2.1),
/// EXT-X-START (§4.4.2.2), and all Media Playlist singleton tags (§4.4.3).
pub fn check_media_sequence_duplicate_tags(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    // Each tag is cited by the section that defines it, rather than all of them by the two
    // sections that happen to bracket the list.
    let singleton_tags = [
        ("#EXT-X-VERSION:", "§4.4.1.2"),
        ("#EXT-X-INDEPENDENT-SEGMENTS", "§4.4.2.1"),
        ("#EXT-X-START:", "§4.4.2.2"),
        ("#EXT-X-TARGETDURATION:", "§4.4.3.1"),
        ("#EXT-X-MEDIA-SEQUENCE:", "§4.4.3.2"),
        ("#EXT-X-DISCONTINUITY-SEQUENCE:", "§4.4.3.3"),
        ("#EXT-X-PLAYLIST-TYPE:", "§4.4.3.5"),
        ("#EXT-X-I-FRAMES-ONLY", "§4.4.3.6"),
        ("#EXT-X-PART-INF:", "§4.4.3.7"),
        ("#EXT-X-SERVER-CONTROL:", "§4.4.3.8"),
    ];
    for pl in playlists {
        for (tag, section) in &singleton_tags {
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
                        "rfc8216bis {}: Tag '{}' appears {} times in '{}'. \
                         There MUST NOT be more than one Media Playlist tag of each type in \
                         any Media Playlist (§4.4.3).",
                        section, tag.trim_end_matches(':'), count, pl.name
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
        }
    }
    produced_by(CheckId::SingletonTags, issues)
}

/// RFC 8216bis §4.4.6.2, §6.2.4 — consistency of EXT-X-STREAM-INF tags that share a URI.
///
/// A Variant Stream is the combination of a URI and the Rendition Groups it pairs with, so
/// two STREAM-INF tags that share a URI but name different AUDIO, SUBTITLES or
/// CLOSED-CAPTIONS groups are different Variant Streams and their CODECS and BANDWIDTH are
/// expected to differ. Only entries that agree on all of those attributes are compared for
/// CODECS and BANDWIDTH, and a mismatch there is reported as a warning: the spec's
/// requirement is on what the attributes describe, and a player reads them per entry.
///
/// The video codec is different. §6.2.4 requires every Variant Stream of a presentation to
/// carry the same video encoding, so the same media URI described with two different video
/// codecs is a contradiction no choice of audio can explain, and that stays an error.
pub fn check_stream_inf_consistency(master: &MasterPlaylist) -> Vec<Issue> {
    let mut issues = Vec::new();

    // URI plus everything that makes two STREAM-INF entries the same Variant Stream.
    type VariantKey<'a> =
        (&'a str, Option<&'a str>, Option<&'a str>, Option<&'a str>, Option<&'a str>);
    fn variant_key(v: &MasterRendition) -> VariantKey<'_> {
        (
            v.uri.as_str(),
            v.audio_group.as_deref(),
            v.subtitle_group.as_deref(),
            v.closed_captions.as_deref(),
            v.video_group.as_deref(),
        )
    }
    // First comma-separated token of a CODECS string: the video codec.
    fn video_codec(codecs: &str) -> &str {
        codecs.split(',').next().unwrap_or(codecs).trim()
    }

    let mut by_variant: HashMap<VariantKey<'_>, Vec<&MasterRendition>> = HashMap::new();
    let mut by_uri: HashMap<&str, Vec<&MasterRendition>> = HashMap::new();
    for v in &master.variants {
        by_variant.entry(variant_key(v)).or_default().push(v);
        by_uri.entry(v.uri.as_str()).or_default().push(v);
    }

    for ((uri, audio, subtitles, captions, video), variants) in &by_variant {
        if variants.len() < 2 {
            continue;
        }
        let groups = format!(
            "AUDIO={}, SUBTITLES={}, CLOSED-CAPTIONS={}, VIDEO={}",
            audio.unwrap_or("(none)"),
            subtitles.unwrap_or("(none)"),
            captions.unwrap_or("(none)"),
            video.unwrap_or("(none)")
        );

        let codecs_set: HashSet<Option<&str>> = variants.iter().map(|v| v.codecs.as_deref()).collect();
        if codecs_set.len() > 1 {
            let details: Vec<String> = variants.iter()
                .map(|v| format!("CODECS={}", v.codecs.as_deref().unwrap_or("(none)")))
                .collect();
            issues.push(Issue {
                severity: Severity::Warn,
                uri_a: Some((*uri).to_string()),
                message: format!(
                    "rfc8216bis §4.4.6.2: Multiple EXT-X-STREAM-INF tags share URI '{}' and the \
                     same Rendition Groups ({}) but declare different CODECS values: {}. \
                     One of them misdescribes what the URI contains.",
                    uri, groups, details.join(", ")
                ),
                ..Default::default()
            });
        }

        let bw_set: HashSet<Option<u64>> = variants.iter().map(|v| v.bandwidth).collect();
        if bw_set.len() > 1 {
            let details: Vec<String> = variants.iter()
                .map(|v| format!(
                    "BANDWIDTH={}",
                    v.bandwidth.map_or("(none)".to_string(), |b| b.to_string())
                ))
                .collect();
            issues.push(Issue {
                severity: Severity::Warn,
                uri_a: Some((*uri).to_string()),
                message: format!(
                    "rfc8216bis §4.4.6.2: Multiple EXT-X-STREAM-INF tags share URI '{}' and the \
                     same Rendition Groups ({}) but declare different BANDWIDTH values: {}. \
                     BANDWIDTH is the peak bit rate of the same media in each case.",
                    uri, groups, details.join(", ")
                ),
                ..Default::default()
            });
        }
    }

    // §4.4.6.2: CLOSED-CAPTIONS=NONE is a statement about the whole Multivariant Playlist —
    // "all EXT-X-STREAM-INF tags MUST have this attribute with a value of NONE" — because
    // captions in one Variant Stream but not another can trigger playback inconsistencies.
    // I-frame Variant Streams are excluded: §4.4.6.3 does not define the attribute for them.
    {
        let regular: Vec<&MasterRendition> =
            master.variants.iter().filter(|v| !v.is_iframe).collect();
        let declares_none = regular.iter()
            .any(|v| v.closed_captions.as_deref() == Some("NONE"));
        let others: Vec<&MasterRendition> = regular.iter()
            .filter(|v| v.closed_captions.as_deref() != Some("NONE"))
            .copied()
            .collect();
        if declares_none && !others.is_empty() {
            let detail: Vec<String> = others.iter()
                .map(|v| format!(
                    "'{}' has CLOSED-CAPTIONS={}",
                    v.uri,
                    v.closed_captions.as_deref().unwrap_or("(absent)")
                ))
                .collect();
            issues.push(Issue {
                severity: Severity::Error,
                uri_a: others.first().map(|v| v.uri.clone()),
                message: format!(
                    "rfc8216bis §4.4.6.2: One EXT-X-STREAM-INF declares CLOSED-CAPTIONS=NONE \
                     but {} of {} others do not: {}. Where the value NONE is used, all \
                     EXT-X-STREAM-INF tags MUST carry it, since captions present in one \
                     Variant Stream but not another can trigger playback inconsistencies.",
                    others.len(), regular.len(), detail.join(", ")
                ),
                ..Default::default()
            });
        }
    }

    // Across all entries for a URI, the video codec cannot depend on the audio group.
    for (uri, variants) in &by_uri {
        if variants.len() < 2 {
            continue;
        }
        let video_codecs: HashSet<&str> = variants.iter()
            .filter_map(|v| v.codecs.as_deref())
            .map(video_codec)
            .collect();
        if video_codecs.len() > 1 {
            let details: Vec<String> = variants.iter()
                .map(|v| format!(
                    "AUDIO={} CODECS={}",
                    v.audio_group.as_deref().unwrap_or("(none)"),
                    v.codecs.as_deref().unwrap_or("(none)")
                ))
                .collect();
            issues.push(Issue {
                severity: Severity::Error,
                uri_a: Some((*uri).to_string()),
                message: format!(
                    "rfc8216bis §6.2.4: Multiple EXT-X-STREAM-INF tags share URI '{}' but \
                     declare different video codecs: {}. The same media cannot be encoded two \
                     ways, and every Variant Stream of a presentation MUST use the same video \
                     encoding.",
                    uri, details.join(", ")
                ),
                ..Default::default()
            });
        }
    }

    produced_by(CheckId::StreamInfConsistency, issues)
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
                    "rfc8216bis §4.4.6.2: EXT-X-STREAM-INF for URI '{}' is missing \
                     the BANDWIDTH attribute, which is REQUIRED.",
                    v.uri
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }
    produced_by(CheckId::BandwidthRequired, issues)
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
    produced_by(CheckId::MediaGroupMembership, issues)
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
                    "rfc8216bis §8: '{}' uses EXT-X-KEY with IV attribute \
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
                            "rfc8216bis §8: '{}' uses floating-point EXTINF ({:.3}s) \
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
                    "rfc8216bis §8: '{}' uses EXT-X-BYTERANGE \
                     which requires VERSION >= 4. Declared version: {}.",
                    pl.name, v
                ),
                uri_note: None,
                ..Default::default()
            });
        }
        // §8: EXT-X-MAP needs v6 on its own, and v5 in an I-frames-only playlist.
        if content.contains("#EXT-X-MAP:") {
            let iframes_only = content.contains("#EXT-X-I-FRAMES-ONLY");
            let required = if iframes_only { 5 } else { 6 };
            if v < required {
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!(
                        "rfc8216bis §8: '{}' uses EXT-X-MAP {} which requires \
                         VERSION >= {}. Declared version: {}.",
                        pl.name,
                        if iframes_only {
                            "in a playlist with EXT-X-I-FRAMES-ONLY"
                        } else {
                            "without EXT-X-I-FRAMES-ONLY"
                        },
                        required, v
                    ),
                    uri_note: None,
                    ..Default::default()
                });
            }
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
                    "rfc8216bis §8: '{}' uses EXT-X-SKIP \
                     which requires VERSION >= 9. Declared version: {}.",
                    pl.name, v
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }
    produced_by(CheckId::VersionCompatibility, issues)
}

/// What in a playlist says that the server has taken segments out of its window, worded so a
/// finding can quote it. `None` means the window has only ever grown.
///
/// §6.2.2 requires EXT-X-MEDIA-SEQUENCE to be incremented by one for every segment removed,
/// so the Media Sequence Number is the server's own record of how many it has dropped.
fn removal_evidence(pl: &MediaPlaylist) -> Option<String> {
    if pl.media_sequence > 0 {
        return Some(format!(
            "EXT-X-MEDIA-SEQUENCE:{} records that {} segment(s) have already been removed",
            pl.media_sequence, pl.media_sequence
        ));
    }
    if pl.skipped_segments > 0 {
        return Some(format!(
            "a Playlist Delta Update left {} segment(s) out of the response",
            pl.skipped_segments
        ));
    }
    None
}

/// rfc8216bis §6.2.2 — a playlist with no ENDLIST MUST hold at least three Target Durations.
///
/// The requirement is on the window's *duration*, not its segment count: a server "MUST NOT
/// remove a Media Segment from a Playlist file without an EXT-X-ENDLIST tag if that would
/// produce a Playlist whose duration is less than three times the Target Duration". Counting
/// segments instead failed a conforming playlist that carries two segments longer than one
/// and a half Target Durations each, and passed a playlist of three very short ones.
///
/// Segments dropped by a Playlist Delta Update are counted back in at TARGETDURATION each,
/// since EXT-X-SKIP replaced them rather than the server removing them from the window.
///
/// The prohibition is on *removing* a segment, so a short window only breaks it once
/// something has been removed (see [`removal_evidence`]). A playlist still at
/// MEDIA-SEQUENCE:0 has removed nothing and is simply near the start of its broadcast, and
/// EXT-X-PLAYLIST-TYPE:EVENT promises that segments will only ever be appended — both were
/// being failed for a rule neither can have broken.
pub fn check_live_playlist_min_segments(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in playlists {
        if pl.has_endlist || pl.target_duration <= 0.0 {
            continue;
        }
        // EVENT promises that segments are only ever appended, so it cannot have removed one.
        if pl.playlist_type.as_deref() == Some("EVENT") {
            continue;
        }
        let Some(evidence) = removal_evidence(pl) else {
            continue;
        };
        let listed: f64 = pl.segments.iter().map(|s| s.duration).sum();
        let skipped = pl.skipped_segments as f64 * pl.target_duration;
        let window = listed + skipped;
        let required = pl.target_duration * 3.0;
        if window < required - 0.001 {
            issues.push(Issue {
                severity: Severity::Error,
                segment_index: -1,
                rendition_a: Some(pl.name.clone()),
                rendition_b: None,
                uri_a: Some(pl.url.clone()),
                uri_b: None,
                message: format!(
                    "rfc8216bis §6.2.2: Live playlist '{}' holds {:.3}s across {} segment(s), \
                     less than three times its TARGETDURATION of {:.3}s (needs {:.3}s), and \
                     {}. A server MUST NOT remove a Media Segment from a playlist without \
                     EXT-X-ENDLIST if that leaves a window shorter than three Target \
                     Durations; a shorter window can stall playback.",
                    pl.name, window, pl.segments.len(), pl.target_duration, required, evidence
                ),
                uri_note: None,
                ..Default::default()
            });
        }
    }
    produced_by(CheckId::LivePlaylistWindow, issues)
}


/// RFC 8216bis §6.2.4 — every Media Playlist of a presentation MUST have the same
/// TARGETDURATION, so that a client switching renditions keeps the same reload interval.
///
/// The spec's own exception is for trick-play: an I-frame playlist with
/// EXT-X-PLAYLIST-TYPE:VOD may use a different target duration, so those renditions are
/// excluded from the comparison rather than counted against it.
pub fn check_targetduration_consistency(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    if playlists.len() < 2 {
        return issues;
    }
    let is_vod_iframe_playlist = |pl: &MediaPlaylist| {
        (pl.iframes_only || pl.is_iframe) && pl.playlist_type.as_deref() == Some("VOD")
    };
    let td_values: Vec<(&str, f64)> = playlists.iter()
        .filter(|pl| pl.target_duration > 0.0 && !is_vod_iframe_playlist(pl))
        .map(|pl| (pl.name.as_str(), pl.target_duration))
        .collect();
    if td_values.len() < 2 {
        return issues;
    }
    let unique_tds: HashSet<u64> = td_values.iter().map(|(_, v)| *v as u64).collect();
    if unique_tds.len() > 1 {
        let td_summary: Vec<String> = td_values.iter()
            .map(|(name, td)| format!("{}={:.0}s", name, td))
            .collect();
        issues.push(Issue::error(format!(
            "rfc8216bis §6.2.4: EXT-X-TARGETDURATION values differ across renditions \
             ({} distinct values). Every Media Playlist of a presentation MUST declare the \
             same TARGETDURATION, apart from I-frame playlists with PLAYLIST-TYPE:VOD, which \
             are excluded here. Details: {}",
            unique_tds.len(), td_summary.join(", ")
        )));
    }
    produced_by(CheckId::TargetDurationConsistency, issues)
}

/// rfc8216bis §4.4.3.5, §6.2.4 — PLAYLIST-TYPE / ENDLIST consistency.
///
/// Per playlist (§4.4.3.5): a VOD playlist MUST have EXT-X-ENDLIST. EVENT playlists are valid
/// live playlists that grow (segments can only be appended, never removed) and gain
/// EXT-X-ENDLIST when the event is complete, so one without it is expected rather than wrong.
///
/// Across playlists (§6.2.4): "If any Media Playlists have an EXT-X-PLAYLIST-TYPE tag, all
/// Media Playlists MUST have an EXT-X-PLAYLIST-TYPE tag with the same value." A presentation
/// where one rendition is VOD and another is EVENT, or where only some declare the tag, is
/// telling a client two different things about whether the window can change.
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
                        "rfc8216bis §4.4.3.5: Playlist '{}' declares PLAYLIST-TYPE:VOD \
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

    if playlists.len() >= 2 {
        let declared: HashSet<&str> = playlists.iter()
            .filter_map(|pl| pl.playlist_type.as_deref())
            .collect();
        let missing: Vec<&str> = playlists.iter()
            .filter(|pl| pl.playlist_type.is_none())
            .map(|pl| pl.name.as_str())
            .collect();
        if !declared.is_empty() && (declared.len() > 1 || !missing.is_empty()) {
            let detail: Vec<String> = playlists.iter()
                .map(|pl| format!(
                    "{}={}",
                    pl.name,
                    pl.playlist_type.as_deref().unwrap_or("(absent)")
                ))
                .collect();
            issues.push(Issue::error(format!(
                "rfc8216bis §6.2.4: EXT-X-PLAYLIST-TYPE is not declared the same way by every \
                 Media Playlist: {}. If any Media Playlist has the tag, all of them MUST have \
                 it with the same value.",
                detail.join(", ")
            )));
        }
    }

    produced_by(CheckId::PlaylistTypeEndlist, issues)
}

/// RFC 8216bis §4.4.4.4 — Encryption consistency across renditions.
///
/// Only playlists that declare an EXT-X-KEY are compared, so a rendition with no EXT-X-KEY at
/// all is absent from the comparison rather than being counted as METHOD=NONE.
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
                    "rfc8216bis §4.4.4.4: Playlist '{}' uses multiple encryption methods: {:?}.",
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
    // Renditions that differ in encryption method are worth reporting, but this is a
    // warning: the spec's requirement is that each Media Segment can be decrypted from its
    // own playlist, and a presentation that encrypts video while leaving, say, a subtitle
    // or I-frame rendition clear is deployed deliberately and plays.
    if all_methods.len() > 1 {
        let mut sorted: Vec<&str> = all_methods.iter().copied().collect();
        sorted.sort_unstable();
        issues.push(Issue::warn(format!(
            "rfc8216bis §4.4.4.4: Renditions declare different EXT-X-KEY METHOD values: {}. \
             Check this is intended — a client that can decrypt one rendition may not be able \
             to switch to another.",
            sorted.join(", ")
        )));
    }
    produced_by(CheckId::EncryptionConsistency, issues)
}

/// RFC 8216bis §6.2.4 — EXT-X-DISCONTINUITY-SEQUENCE MUST match across the renditions of a
/// presentation, because a client synchronises renditions by discontinuity sequence number
/// before it can align their timelines.
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
        issues.push(Issue::error(format!(
            "rfc8216bis §6.2.4: EXT-X-DISCONTINUITY-SEQUENCE values differ across renditions: \
             {}. Renditions of the same presentation MUST carry matching discontinuity \
             sequence numbers so clients can align their timelines.",
            details.join(", ")
        )));
    }
    produced_by(CheckId::DiscontinuitySequence, issues)
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
            "rfc8216bis §6.2.4: Segment count differs across VIDEO renditions: {}. \
             Each Variant Stream MUST present the same content, so completed renditions of \
             the same presentation are expected to segment it the same way.",
            details.join(", ")
        )));
    }
    produced_by(CheckId::SegmentCount, issues)
}

/// rfc8216bis §6.2.4 — EXTINF duration drift between renditions (MSN-aligned).
///
/// Matching content in Variant Streams MUST have matching timestamps, so segments that share
/// a Media Sequence Number are expected to have the same duration. Only VIDEO is compared
/// with VIDEO: audio encoders produce different segment boundaries than video encoders, which
/// makes cross-type drift meaningless.
pub fn check_duration_drift(playlists: &[MediaPlaylist], tolerance_ms: f64) -> Vec<Issue> {
    let mut findings: Vec<SegmentFinding<(f64, f64)>> = Vec::new();
    if playlists.len() < 2 {
        return Vec::new();
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
                    findings.push(SegmentFinding {
                        key: format!("{}|{}", i, j),
                        issue: Issue {
                            severity: Severity::Warn,
                            segment_index: msn as i32,
                            rendition_a: Some(pl_a.name.clone()),
                            rendition_b: Some(pl_b.name.clone()),
                            uri_a: Some(seg_a.uri.clone()),
                            uri_b: Some(seg_b.uri.clone()),
                            message: format!(
                                "rfc8216bis §6.2.4: EXTINF drift at MSN {}: '{}' has {:.3}s vs \
                                 '{}' has {:.3}s (diff={:.3}s, tolerance={:.3}s).",
                                msn, pl_a.name, seg_a.duration, pl_b.name, seg_b.duration,
                                diff, tolerance_s
                            ),
                            uri_note: None,
                            ..Default::default()
                        },
                        data: (diff, tolerance_s),
                    });
                }
            }
        }
    }
    let issues = fold_consecutive(findings, |iss, run| {
        let worst = largest(run.iter().map(|(diff, _)| *diff));
        let tolerance = run.first().map_or(0.0, |(_, tolerance)| *tolerance);
        format!(
            "rfc8216bis §6.2.4: EXTINF drift at MSN {}–{} ({} consecutive segments): '{}' and \
             '{}' differ by up to {:.3}s (tolerance={:.3}s). Matching content in Variant \
             Streams MUST have matching timestamps.",
            iss.seg_first, iss.seg_last, iss.count,
            iss.rendition_a.as_deref().unwrap_or("(unnamed)"),
            iss.rendition_b.as_deref().unwrap_or("(unnamed)"),
            worst, tolerance
        )
    });
    produced_by(CheckId::DurationDrift, issues)
}

/// rfc8216bis §6.2.4 — PDT alignment across renditions (MSN-aligned).
///
/// This is the "consistent mappings of date and time to media timestamps" half of the
/// EXT-X-PROGRAM-DATE-TIME requirement; which playlists declare the tag at all is
/// [`check_pdt_coverage`]. Only VIDEO is compared with VIDEO: audio PDT is extrapolated from
/// audio segment durations, which differ from video, causing apparent drift that is not real.
pub fn check_pdt_alignment(playlists: &[MediaPlaylist], tolerance_ms: f64) -> Vec<Issue> {
    let mut findings: Vec<SegmentFinding<(f64, f64)>> = Vec::new();
    if playlists.len() < 2 {
        return Vec::new();
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
                        findings.push(SegmentFinding {
                            key: format!("{}|{}", i, j),
                            issue: Issue {
                                severity: Severity::Warn,
                                segment_index: msn as i32,
                                rendition_a: Some(pl_a.name.clone()),
                                rendition_b: Some(pl_b.name.clone()),
                                uri_a: Some(seg_a.uri.clone()),
                                uri_b: Some(seg_b.uri.clone()),
                                message: format!(
                                    "rfc8216bis §6.2.4: PDT misalignment at MSN {} between '{}' \
                                     and '{}': diff={:.3}s (tolerance={:.3}s).",
                                    msn, pl_a.name, pl_b.name, diff, tolerance_s
                                ),
                                uri_note: None,
                                ..Default::default()
                            },
                            data: (diff, tolerance_s),
                        });
                    }
                }
            }
        }
    }
    let issues = fold_consecutive(findings, |iss, run| {
        let worst = largest(run.iter().map(|(diff, _)| *diff));
        let tolerance = run.first().map_or(0.0, |(_, tolerance)| *tolerance);
        format!(
            "rfc8216bis §6.2.4: PDT misalignment at MSN {}–{} ({} consecutive segments) \
             between '{}' and '{}': up to {:.3}s apart (tolerance={:.3}s). Renditions that \
             carry EXT-X-PROGRAM-DATE-TIME MUST map date and time to media timestamps \
             consistently.",
            iss.seg_first, iss.seg_last, iss.count,
            iss.rendition_a.as_deref().unwrap_or("(unnamed)"),
            iss.rendition_b.as_deref().unwrap_or("(unnamed)"),
            worst, tolerance
        )
    });
    produced_by(CheckId::PdtAlignment, issues)
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
            "rfc8216bis §6.2.4: Cumulative EXTINF drift across renditions: {:.3}s \
             (tolerance={:.3}s) over {} common segments (MSN {}-{}). Totals: {}",
            drift, tolerance_s, window_size, overlap_start, overlap_end - 1,
            details.join(", ")
        )));
    }
    produced_by(CheckId::CumulativeDrift, issues)
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
                    "rfc8216bis §4.4.3.7: '{}' contains EXT-X-PART tags but no \
                     EXT-X-PART-INF. PART-TARGET is required.",
                    pl.name
                ),
                uri_note: None,
                ..Default::default()
            });
        }

        // 2. PART-HOLD-BACK is REQUIRED once EXT-X-PART-INF is present (§4.4.3.8): it is
        //    what tells a client how far from the live edge it may start playing parts.
        if has_part_inf {
            let has_part_hold_back = pl.server_control.as_ref()
                .is_some_and(|sc| sc.part_hold_back.is_some());
            if !has_part_hold_back {
                issues.push(Issue {
                    severity: Severity::Error,
                    segment_index: -1,
                    rendition_a: Some(pl.name.clone()),
                    rendition_b: None,
                    uri_a: None,
                    uri_b: None,
                    message: format!(
                        "rfc8216bis §4.4.3.8: '{}' declares EXT-X-PART-INF but \
                         EXT-X-SERVER-CONTROL has no PART-HOLD-BACK. The attribute is REQUIRED \
                         when the playlist contains EXT-X-PART-INF.",
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
                            "rfc8216bis §4.4.4.9: Part {} in '{}' has duration {:.5}s exceeding \
                             PART-TARGET {:.5}s.",
                            idx, pl.name, part.duration, pt
                        ),
                        uri_note: None,
                        ..Default::default()
                    });
                }
            }
        }

        // 4. EXT-X-PRELOAD-HINT with TYPE=PART should be present at playlist tail
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
                        "rfc8216bis §4.4.5.3: '{}' is missing EXT-X-PRELOAD-HINT with TYPE=PART \
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
                    "rfc8216bis §4.4.5.4: '{}' has no EXT-X-RENDITION-REPORT tags. \
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
                            "rfc8216bis §4.4.3.8: '{}' PART-HOLD-BACK={:.5}s < 2× PART-TARGET={:.5}s \
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
                            "rfc8216bis §4.4.3.8: '{}' PART-HOLD-BACK={:.5}s < 3× PART-TARGET={:.5}s \
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
                            "rfc8216bis §4.4.3.8: '{}' HOLD-BACK={:.3}s < 3× TARGETDURATION={:.3}s \
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
                // §4.4.5.4 requires the report to be accurate, not to agree within any
                // particular margin; one segment of slack is this tool's own threshold for
                // "the renditions are being published together", so the finding says so.
                issues.push(
                    Issue::warn(format!(
                        "rfc8216bis §4.4.5.4: EXT-X-RENDITION-REPORT LAST-MSN skew of {} \
                         segments for '{}' (reported MSNs: {}–{}). Renditions published \
                         together normally report the same LAST-MSN within one segment; a \
                         wider spread suggests one rendition is falling behind.",
                        skew, short_uri, min_msn, max_msn
                    ))
                    .with_confidence(Confidence::Heuristic),
                );
            }
        }
    }

    issues.extend(server_control_identity(playlists));

    produced_by(CheckId::LlHls, issues)
}

/// rfc8216bis §6.2.4 — "If any Media Playlist in a Multivariant Playlist contains an
/// EXT-X-SERVER-CONTROL tag, then all Media Playlists in that Multivariant Playlist MUST
/// contain that tag, with the same attributes and values."
///
/// Three things kept this from holding servers to that. It compared only playlists that carry
/// EXT-X-PART, so an audio rendition with no parts could advertise a different HOLD-BACK
/// without being looked at, and a playlist missing the tag entirely was folded in as a row of
/// zeros rather than reported as missing it. It compared the values as `as u64`, so
/// HOLD-BACK=18.0 and HOLD-BACK=18.9 were the same value. And CAN-SKIP-DATERANGES was not
/// compared at all. The message named the renditions but not what differed between them,
/// which left the reader to diff the playlists by hand.
fn server_control_identity(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    /// The attributes of the tag, rendered so that two playlists agree only when every value
    /// does. Formatting keeps the comparison exact without needing `f64` to be `Hash`.
    fn describe(sc: &ServerControl) -> String {
        fn seconds(value: Option<f64>) -> String {
            value.map_or("(absent)".to_string(), |v| format!("{v}"))
        }
        format!(
            "CAN-SKIP-UNTIL={}, CAN-SKIP-DATERANGES={}, HOLD-BACK={}, PART-HOLD-BACK={}, \
             CAN-BLOCK-RELOAD={}",
            seconds(sc.can_skip_until),
            if sc.can_skip_dateranges { "YES" } else { "NO" },
            seconds(sc.hold_back),
            seconds(sc.part_hold_back),
            if sc.can_block_reload { "YES" } else { "NO" },
        )
    }

    if playlists.len() < 2 || !playlists.iter().any(|pl| pl.server_control.is_some()) {
        return Vec::new();
    }

    let described: Vec<(&str, Option<String>)> = playlists.iter()
        .map(|pl| (pl.name.as_str(), pl.server_control.as_ref().map(describe)))
        .collect();
    let distinct: HashSet<&Option<String>> = described.iter().map(|(_, d)| d).collect();
    if distinct.len() < 2 {
        return Vec::new();
    }

    let detail: Vec<String> = described.iter()
        .map(|(name, described)| match described {
            Some(values) => format!("'{name}' has {values}"),
            None => format!("'{name}' has no EXT-X-SERVER-CONTROL tag"),
        })
        .collect();
    vec![Issue::error(format!(
        "rfc8216bis §6.2.4: EXT-X-SERVER-CONTROL is not identical across the {} Media \
         Playlists of this presentation: {}. If any Media Playlist contains the tag, all of \
         them MUST contain it with the same attributes and values, or a client changes its \
         reload and latency behaviour simply by switching rendition.",
        playlists.len(), detail.join("; ")
    ))]
}

/// rfc8216bis §4.4.3.2 — EXT-X-MEDIA-SEQUENCE presence and position.
///
/// Nothing here reads a number out of a segment URI. §4.4.3.2 defines the Media Sequence
/// Number of the first segment as the value of this tag and nothing else; segment file names
/// are free-form, so an MSN inferred from one says nothing about conformance.
pub fn check_media_sequence_continuity(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in playlists {
        if pl.segments.is_empty() { continue; }

        let tag_absent = !pl.raw_content.contains("#EXT-X-MEDIA-SEQUENCE:");
        if tag_absent && !pl.has_endlist && pl.segments.len() > 1 {
            issues.push(Issue::warn(format!(
                "rfc8216bis §4.4.3.2: Live playlist '{}' does not declare \
                 EXT-X-MEDIA-SEQUENCE. For live playlists the tag SHOULD be present \
                 so clients can track the sliding window.",
                pl.name
            )));
        }

        // EXT-X-MEDIA-SEQUENCE MUST appear before the first Media Segment URI.
        //
        // The first segment URI is the first URI line after the first EXTINF, which may be
        // several lines later: BYTERANGE, KEY, MAP and PROGRAM-DATE-TIME tags are all
        // allowed between an EXTINF and the URI it introduces.
        {
            let mut tag_line: Option<usize> = None;
            let mut first_seg_line: Option<usize> = None;
            let mut seen_extinf = false;
            for (i, line) in pl.raw_content.lines().enumerate() {
                let l = line.trim();
                if l.starts_with("#EXT-X-MEDIA-SEQUENCE:") && tag_line.is_none() {
                    tag_line = Some(i);
                }
                if l.starts_with("#EXTINF:") {
                    seen_extinf = true;
                } else if seen_extinf
                    && first_seg_line.is_none()
                    && !l.starts_with('#')
                    && !l.is_empty()
                {
                    first_seg_line = Some(i);
                }
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
    }
    produced_by(CheckId::MediaSequenceTags, issues)
}

/// rfc8216bis §4.4.6.3 — the Media Playlist an EXT-X-I-FRAME-STREAM-INF points at "MUST
/// contain an EXT-X-I-FRAMES-ONLY tag".
///
/// A trick-play playlist that omits the tag looks like an ordinary Variant Stream of very long
/// segments, so a client either plays it as one or, having been told it is an I-frame playlist
/// by the multivariant playlist, seeks within segments that are not all I-frames.
pub fn check_iframe_playlists(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for pl in playlists.iter().filter(|pl| pl.is_iframe && !pl.iframes_only) {
        issues.push(Issue {
            severity: Severity::Error,
            rendition_a: Some(pl.name.clone()),
            uri_a: Some(pl.url.clone()),
            message: format!(
                "rfc8216bis §4.4.6.3: '{}' is referenced by an EXT-X-I-FRAME-STREAM-INF tag \
                 but contains no EXT-X-I-FRAMES-ONLY tag. The Playlist file identified by the \
                 URI attribute of that tag MUST contain one, or a client cannot tell that \
                 every segment is independently decodable.",
                pl.name
            ),
            ..Default::default()
        });
    }
    produced_by(CheckId::IFramePlaylists, issues)
}

/// The Date Ranges of one playlist: ID → the merged attribute/value pairs of every tag that
/// shares that ID.
type DateRanges = BTreeMap<String, BTreeMap<String, String>>;

/// Every Date Range in a playlist as ID → merged attribute/value pairs.
///
/// §4.4.5.1 lets a server augment a Date Range with later EXT-X-DATERANGE tags carrying the
/// same ID, so the pairs from every tag sharing an ID are merged into the one set that
/// §6.2.4 compares across renditions.
fn dateranges_of(pl: &MediaPlaylist) -> DateRanges {
    let mut ranges: DateRanges = BTreeMap::new();
    for line in pl.raw_content.lines() {
        let Some(rest) = line.trim().strip_prefix("#EXT-X-DATERANGE:") else {
            continue;
        };
        let attrs = super::parser::parse_attributes(rest);
        // A tag with no ID is not a Date Range that can be matched up with anything; the
        // missing REQUIRED attribute is reported by the check that reads the tag itself.
        let Some(id) = attrs.get("ID").cloned() else {
            continue;
        };
        let entry = ranges.entry(id).or_default();
        for (name, value) in attrs {
            entry.insert(name, value);
        }
    }
    ranges
}

/// The wall-clock span a playlist covers, when it declares enough EXT-X-PROGRAM-DATE-TIME to
/// say. A Date Range starting outside that span may legitimately be absent from the playlist:
/// live windows slide, and the renditions of one run were not fetched at the same instant.
fn pdt_window(pl: &MediaPlaylist) -> Option<(f64, f64)> {
    if pl.program_date_time_tags == 0 {
        return None;
    }
    let first = pl.segments.first()?.pdt?;
    let last = pl.segments.last()?;
    Some((first, last.pdt? + last.duration))
}

/// rfc8216bis §6.2.4 — "Any Playlist with Date Ranges MUST contain the same set of Date Ranges
/// as the others that do. The EXT-X-DATERANGE tags of corresponding Date Ranges MUST have the
/// same ID attribute value and contain the same set of attribute/value pairs."
///
/// Only playlists that carry Date Ranges are compared with each other, which is what the
/// requirement says; a rendition with none is not in the comparison. A Date Range whose
/// START-DATE falls outside another playlist's own PDT window is not required of that playlist
/// either, so a live window that has slid between two fetches is not reported as a difference.
///
/// Attribute differences come in two kinds and are reported as two findings. Carriers that
/// give the same attribute different *values* contradict each other outright. A carrier that
/// has simply not written an attribute another has may be part-way through the augmentation
/// §4.4.5.1 allows, which is only ruled out once every carrier has EXT-X-ENDLIST.
pub fn check_daterange_consistency(playlists: &[MediaPlaylist]) -> Vec<Issue> {
    let carriers: Vec<(&MediaPlaylist, DateRanges)> = playlists
        .iter()
        .map(|pl| (pl, dateranges_of(pl)))
        .filter(|(_, ranges)| !ranges.is_empty())
        .collect();
    if carriers.len() < 2 {
        return Vec::new();
    }

    let all_ids: BTreeSet<&str> = carriers.iter()
        .flat_map(|(_, ranges)| ranges.keys().map(String::as_str))
        .collect();

    // Missing Date Ranges are collected per playlist, and attribute differences per Date
    // Range, so a presentation that disagrees about a hundred Date Ranges produces a handful
    // of readable findings rather than a hundred rows.
    let mut missing_by_playlist: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    // Two carriers that give one attribute two different values contradict each other, and
    // nothing about how the presentation is being served can reconcile them. A carrier that
    // has not written an attribute another has is a weaker signal: §4.4.5.1 lets a server
    // augment a Date Range with later tags carrying the same ID, so a live or event
    // presentation part-way through that augmentation legitimately looks like this between
    // two fetches. The two are reported separately because only the first is firm.
    let mut differing: Vec<String> = Vec::new();
    let mut partial: Vec<String> = Vec::new();

    for id in all_ids {
        let present: Vec<(&MediaPlaylist, &BTreeMap<String, String>)> = carriers.iter()
            .filter_map(|(pl, ranges)| ranges.get(id).map(|attrs| (*pl, attrs)))
            .collect();
        let start = present.first()
            .and_then(|(_, attrs)| attrs.get("START-DATE"))
            .and_then(|d| super::parser::parse_iso8601_to_epoch(d));

        for (pl, ranges) in &carriers {
            if ranges.contains_key(id) {
                continue;
            }
            // Absent from a playlist whose window does not reach the Date Range: nothing to
            // report. Where either the window or the START-DATE is unknown, the requirement
            // applies as written.
            let outside_window = match (pdt_window(pl), start) {
                (Some((from, to)), Some(start)) => start < from || start > to,
                _ => false,
            };
            if !outside_window {
                missing_by_playlist.entry(pl.name.as_str()).or_default().push(id);
            }
        }

        let mut conflicting: Vec<&str> = Vec::new();
        let mut absent_somewhere: Vec<&str> = Vec::new();
        if present.len() >= 2 {
            let names: BTreeSet<&str> = present.iter()
                .flat_map(|(_, attrs)| attrs.keys().map(String::as_str))
                .collect();
            for name in names {
                let mut values: HashSet<&str> = HashSet::new();
                let mut absent = false;
                for (_, attrs) in &present {
                    match attrs.get(name) {
                        Some(value) => {
                            values.insert(value.as_str());
                        }
                        None => absent = true,
                    }
                }
                if values.len() > 1 {
                    conflicting.push(name);
                } else if absent {
                    absent_somewhere.push(name);
                }
            }
        }
        if !conflicting.is_empty() {
            differing.push(format!("'{}' differs in {}", id, conflicting.join(", ")));
        }
        if !absent_somewhere.is_empty() {
            partial.push(format!("'{}' carries {} in some playlists only",
                id, absent_somewhere.join(", ")));
        }
    }

    let mut issues = Vec::new();
    let total = carriers.iter().map(|(_, ranges)| ranges.len()).max().unwrap_or(0);
    for (name, mut ids) in missing_by_playlist {
        ids.sort_unstable();
        let shown: Vec<String> = ids.iter().take(8).map(|id| format!("'{id}'")).collect();
        let ellipsis = if ids.len() > shown.len() { ", …" } else { "" };
        issues.push(Issue {
            severity: Severity::Error,
            rendition_a: Some(name.to_string()),
            message: format!(
                "rfc8216bis §6.2.4: '{}' carries Date Ranges but is missing {} of the {} that \
                 other playlists carry: {}{}. Any Playlist with Date Ranges MUST contain the \
                 same set of Date Ranges as the others that do.",
                name, ids.len(), total, shown.join(", "), ellipsis
            ),
            ..Default::default()
        });
    }
    if !differing.is_empty() {
        let shown: Vec<&String> = differing.iter().take(5).collect();
        let ellipsis = if differing.len() > shown.len() {
            format!(", and {} more", differing.len() - shown.len())
        } else {
            String::new()
        };
        issues.push(Issue::error(format!(
            "rfc8216bis §6.2.4: {} Date Range(s) are described differently by the playlists \
             that carry them: {}{}. Corresponding EXT-X-DATERANGE tags MUST have the same ID \
             and contain the same set of attribute/value pairs.",
            differing.len(),
            shown.iter().map(|d| d.as_str()).collect::<Vec<_>>().join("; "),
            ellipsis
        )));
    }
    if !partial.is_empty() {
        let shown: Vec<&String> = partial.iter().take(5).collect();
        let ellipsis = if partial.len() > shown.len() {
            format!(", and {} more", partial.len() - shown.len())
        } else {
            String::new()
        };
        // Once every carrier has ENDLIST there is no further augmentation coming, so an
        // attribute one of them never wrote is a difference that will never be closed.
        let settled = carriers.iter().all(|(pl, _)| pl.has_endlist);
        let (severity, confidence, why) = if settled {
            (
                Severity::Error,
                Confidence::Measured,
                "Every playlist that carries them has EXT-X-ENDLIST, so no later tag can \
                 still add the missing attributes.",
            )
        } else {
            (
                Severity::Warn,
                Confidence::Heuristic,
                "At least one playlist that carries them has no EXT-X-ENDLIST, so this may \
                 be a Date Range still being augmented rather than a disagreement.",
            )
        };
        issues.push(Issue {
            severity,
            confidence,
            message: format!(
                "rfc8216bis §6.2.4: {} Date Range(s) are described by more attributes in some \
                 playlists than in others: {}{}. Corresponding EXT-X-DATERANGE tags MUST \
                 contain the same set of attribute/value pairs. {}",
                partial.len(),
                shown.iter().map(|d| d.as_str()).collect::<Vec<_>>().join("; "),
                ellipsis,
                why
            ),
            ..Default::default()
        });
    }

    produced_by(CheckId::DateRangeConsistency, issues)
}

/// One interstitial finding, before findings repeated across renditions are merged.
struct InterstitialFinding {
    /// The Date Range this is about, plus the rule it broke. A presentation repeats its
    /// EXT-X-DATERANGE tags in every rendition, so a single malformed Date Range used to be
    /// reported once per rendition — eight identical errors for one authoring mistake.
    key: String,
    rendition: String,
    severity: Severity,
    message: String,
}

/// Merge findings that name the same Date Range and the same rule, keeping the order they
/// were raised in and recording which renditions carried each one.
fn merge_interstitial_findings(findings: Vec<InterstitialFinding>) -> Vec<Issue> {
    let mut order: Vec<String> = Vec::new();
    let mut merged: HashMap<String, (Issue, Vec<String>)> = HashMap::new();
    for finding in findings {
        match merged.get_mut(&finding.key) {
            Some((_, renditions)) => {
                if !renditions.contains(&finding.rendition) {
                    renditions.push(finding.rendition);
                }
            }
            None => {
                order.push(finding.key.clone());
                let issue = Issue {
                    severity: finding.severity,
                    check_id: CheckId::Interstitials,
                    rendition_a: Some(finding.rendition.clone()),
                    message: finding.message,
                    ..Default::default()
                };
                merged.insert(finding.key, (issue, vec![finding.rendition]));
            }
        }
    }
    order.into_iter()
        .filter_map(|key| merged.remove(&key))
        .map(|(mut issue, renditions)| {
            if renditions.len() > 1 {
                issue.uri_note = Some(format!(
                    "Present in {} renditions: {}.",
                    renditions.len(),
                    renditions.join(", ")
                ));
            }
            issue
        })
        .collect()
}

/// HLS Interstitials validation (rfc8216bis Appendix D)
pub fn check_interstitials(playlists: &[MediaPlaylist]) -> (Vec<Issue>, Vec<Interstitial>) {
    let mut findings: Vec<InterstitialFinding> = Vec::new();
    let mut interstitials = Vec::new();

    for pl in playlists {
        if pl.raw_content.is_empty() {
            continue;
        }
        // Per rfc8216bis §D.2 a DATERANGE with the same ID as a previously-seen tag in the
        // same playlist is an update; update tags don't need to repeat
        // X-ASSET-URI/X-ASSET-LIST, so only the first occurrence of each ID is validated.
        //
        // This stays per-playlist because that is what "an update" means: the same ID in
        // another rendition is the same Date Range declared again, not an update to it.
        // Findings repeated that way are merged afterwards by their key.
        let mut seen_ids: HashSet<String> = HashSet::new();
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
            // What makes two findings the same finding. An ID-less Date Range cannot be told
            // apart by ID, so its identity falls back to the tag text: two Date Ranges that
            // both forgot their ID are two mistakes, not one.
            let identity = if dr_id.is_empty() {
                format!("<no ID>:{rest}")
            } else {
                dr_id.clone()
            };
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
                findings.push(InterstitialFinding {
                    key: format!("{identity}|ID"),
                    rendition: pl.name.clone(),
                    severity: Severity::Error,
                    message: format!(
                        "rfc8216bis §4.4.5.1: Interstitial DATERANGE in '{}' is missing ID, \
                         which is a REQUIRED attribute. Without it the Date Range cannot be \
                         updated, paired with its IN tag, or matched across renditions.",
                        pl.name
                    ),
                });
            }

            // MUST have START-DATE
            if start_date.is_empty() {
                entry_errors.push("Missing required START-DATE".to_string());
                findings.push(InterstitialFinding {
                    key: format!("{identity}|START-DATE"),
                    rendition: pl.name.clone(),
                    severity: Severity::Error,
                    message: format!(
                        "rfc8216bis §4.4.5.1: Interstitial [{dr_id}] is missing START-DATE, \
                         which is a REQUIRED attribute. A client cannot place the interstitial \
                         on the timeline without it."
                    ),
                });
            }

            // MUST have X-ASSET-URI or X-ASSET-LIST
            if asset_uri.is_none() && asset_list.is_none() {
                entry_errors.push("Missing X-ASSET-URI or X-ASSET-LIST (MUST have one)".to_string());
                findings.push(InterstitialFinding {
                    key: format!("{identity}|asset"),
                    rendition: pl.name.clone(),
                    severity: Severity::Error,
                    message: format!(
                        "rfc8216bis §D.2: Interstitial [{dr_id}] is missing \
                         X-ASSET-URI/X-ASSET-LIST. One of the two MUST be present, or there is \
                         nothing for the client to play at the interstitial."
                    ),
                });
            }

            // MUST NOT have both
            if asset_uri.is_some() && asset_list.is_some() {
                entry_errors.push("Has both X-ASSET-URI and X-ASSET-LIST (MUST have only one)".to_string());
                findings.push(InterstitialFinding {
                    key: format!("{identity}|asset-both"),
                    rendition: pl.name.clone(),
                    severity: Severity::Error,
                    message: format!(
                        "rfc8216bis §D.2: Interstitial [{dr_id}] has both X-ASSET-URI and \
                         X-ASSET-LIST. A Date Range MUST NOT carry both, and a client given \
                         both cannot tell which describes the assets to play."
                    ),
                });
            }

            // Validate X-SNAP — §D.2: enumerated-string-list, values IN and OUT only
            if let Some(ref s) = snap {
                for part in s.split(',') {
                    let p = part.trim();
                    if p != "IN" && p != "OUT" {
                        entry_errors.push(format!("X-SNAP value '{}' invalid (must be IN or OUT)", p));
                        findings.push(InterstitialFinding {
                            key: format!("{identity}|X-SNAP={p}"),
                            rendition: pl.name.clone(),
                            severity: Severity::Warn,
                            message: format!(
                                "rfc8216bis §D.2: Interstitial [{dr_id}] has X-SNAP='{p}'. \
                                 The attribute is an enumerated-string-list whose values are \
                                 OUT and IN; anything else is ignored by a client."
                            ),
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
                        findings.push(InterstitialFinding {
                            key: format!("{identity}|X-RESTRICT={p}"),
                            rendition: pl.name.clone(),
                            severity: Severity::Warn,
                            message: format!(
                                "rfc8216bis §D.2: Interstitial [{dr_id}] has X-RESTRICT='{p}'. \
                                 The attribute is an enumerated-string-list whose values are \
                                 SKIP and JUMP; anything else is ignored by a client."
                            ),
                        });
                    }
                }
            }

            // Validate X-CONTENT-MAY-VARY — §D.2: valid values "YES" and "NO"
            if let Some(ref cmv) = attrs.get("X-CONTENT-MAY-VARY").cloned()
                && cmv != "YES" && cmv != "NO" {
                    entry_errors.push(format!("X-CONTENT-MAY-VARY='{}' invalid (must be YES or NO)", cmv));
                    findings.push(InterstitialFinding {
                        key: format!("{identity}|X-CONTENT-MAY-VARY"),
                        rendition: pl.name.clone(),
                        severity: Severity::Warn,
                        message: format!(
                            "rfc8216bis §D.2: Interstitial [{dr_id}] has \
                             X-CONTENT-MAY-VARY='{cmv}', which is neither \"YES\" nor \"NO\", \
                             the only two values the attribute takes."
                        ),
                    });
                }

            // Validate X-TIMELINE-OCCUPIES — §D.2: valid values "POINT" and "RANGE"
            if let Some(ref to) = attrs.get("X-TIMELINE-OCCUPIES").cloned()
                && to != "POINT" && to != "RANGE" {
                    entry_errors.push(format!("X-TIMELINE-OCCUPIES='{}' invalid (must be POINT or RANGE)", to));
                    findings.push(InterstitialFinding {
                        key: format!("{identity}|X-TIMELINE-OCCUPIES"),
                        rendition: pl.name.clone(),
                        severity: Severity::Warn,
                        message: format!(
                            "rfc8216bis §D.2: Interstitial [{dr_id}] has \
                             X-TIMELINE-OCCUPIES='{to}', which is neither \"POINT\" nor \
                             \"RANGE\", the only two values the attribute takes."
                        ),
                    });
                }

            // Validate X-TIMELINE-STYLE — §D.2: valid values "HIGHLIGHT" and "PRIMARY"
            if let Some(ref ts) = attrs.get("X-TIMELINE-STYLE").cloned()
                && ts != "HIGHLIGHT" && ts != "PRIMARY" {
                    entry_errors.push(format!("X-TIMELINE-STYLE='{}' invalid (must be HIGHLIGHT or PRIMARY)", ts));
                    findings.push(InterstitialFinding {
                        key: format!("{identity}|X-TIMELINE-STYLE"),
                        rendition: pl.name.clone(),
                        severity: Severity::Warn,
                        message: format!(
                            "rfc8216bis §D.2: Interstitial [{dr_id}] has \
                             X-TIMELINE-STYLE='{ts}', which is neither \"HIGHLIGHT\" nor \
                             \"PRIMARY\", the only two values the attribute takes."
                        ),
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
                    findings.push(InterstitialFinding {
                        key: format!("{identity}|X-SKIP-CONTROL-LABEL-ID"),
                        rendition: pl.name.clone(),
                        severity: Severity::Error,
                        message: format!(
                            "rfc8216bis §D.3: Interstitial [{dr_id}] has \
                             X-SKIP-CONTROL-LABEL-ID='{label_id}', which contains {invalid_chars:?}. \
                             The value MUST contain only characters from [a-z], [A-Z], '-' \
                             and '_'."
                        ),
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
    //
    // Date Ranges with no ID are left out of the map: they cannot be paired with anything, and
    // keying them all under the empty string made every ID-less IN tag in the presentation
    // donate its X-PLAYOUT-LIMIT and X-RESUME-OFFSET to one unrelated interstitial.
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (idx, it) in interstitials.iter().enumerate() {
        if !it.id.is_empty() {
            id_to_idx.entry(it.id.clone()).or_insert(idx);
        }
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
            let Some(id) = attrs.get("ID") else { continue; };
            if let Some(&idx) = id_to_idx.get(id) {
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

    let issues = merge_interstitial_findings(findings);
    (produced_by(CheckId::Interstitials, issues), interstitials)
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

    /// Read a media playlist the way the validator does, so tests exercise the parser's own
    /// view of segments, PDTs and tag order rather than a hand-built one.
    fn parse_playlist(name: &str, content: &str) -> MediaPlaylist {
        let url = format!("https://cdn.example.com/{name}.m3u8");
        let mut pl = MediaPlaylist::new(name.to_string(), url.clone());
        super::super::parser::parse_media_playlist(&url, content, &mut pl);
        pl
    }

    /// A parsed media playlist whose display name and URL are set independently, for the
    /// folding rules that must not take two renditions sharing a NAME for one rendition.
    fn named_playlist(name: &str, url: &str, content: &str) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.to_string(), url.to_string());
        super::super::parser::parse_media_playlist(url, content, &mut pl);
        pl
    }

    fn parse_master(content: &str) -> MasterPlaylist {
        super::super::parser::parse_master_playlist("https://cdn.example.com/master.m3u8", content)
    }

    fn errors(issues: &[Issue]) -> Vec<&Issue> {
        issues.iter().filter(|i| i.severity == Severity::Error).collect()
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

    /// A playlist with one PDT tag at the top, which is how nearly every live stream carries
    /// PDT: the parser extrapolates a `pdt` onto every later segment from it.
    fn playlist_with_pdt(name: &str, first_pdt: &str) -> MediaPlaylist {
        parse_playlist(name, &format!(
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-PROGRAM-DATE-TIME:{first_pdt}\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n"
        ))
    }

    fn playlist_without_pdt(name: &str) -> MediaPlaylist {
        parse_playlist(name, "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n")
    }

    #[test]
    fn pdt_coverage_accepts_a_presentation_where_every_playlist_declares_pdt() {
        let playlists = vec![
            playlist_with_pdt("v-hi", "2024-01-15T12:00:00Z"),
            playlist_with_pdt("v-lo", "2024-01-15T12:00:00Z"),
        ];
        assert!(check_pdt_coverage(&playlists).is_empty());
    }

    #[test]
    fn pdt_coverage_accepts_a_presentation_where_none_declares_pdt() {
        // §6.2.4 only bites once one playlist carries the tag; PDT is not required of a
        // presentation that uses none.
        let playlists = vec![playlist_without_pdt("v-hi"), playlist_without_pdt("v-lo")];
        assert!(check_pdt_coverage(&playlists).is_empty());
    }

    #[test]
    fn pdt_coverage_errors_when_only_some_playlists_declare_pdt() {
        let playlists = vec![
            playlist_with_pdt("v-hi", "2024-01-15T12:00:00Z"),
            playlist_without_pdt("a-en"),
        ];
        let issues = check_pdt_coverage(&playlists);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 1, "expected one §6.2.4 finding: {issues:?}");
        assert!(errors[0].message.contains("§6.2.4"), "{:?}", errors[0].message);
        assert!(
            errors[0].message.contains("'v-hi'") && errors[0].message.contains("'a-en'"),
            "the finding must name which playlists have the tag and which do not: {:?}",
            errors[0].message
        );
    }

    #[test]
    fn pdt_coverage_does_not_read_extrapolated_segment_pdts() {
        // The segments after the single tag all carry an extrapolated Segment::pdt, and the
        // segments before it carry none. That is normal authoring, not a finding — the rule
        // this check now enforces is about tags, not about extrapolated fields.
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXTINF:4.0,\ns0.m4s\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:04Z\n\
             #EXTINF:4.0,\ns1.m4s\n#EXTINF:4.0,\ns2.m4s\n");
        assert!(pl.segments[0].pdt.is_none() && pl.segments[2].pdt.is_some());
        assert!(
            check_pdt_coverage(&[pl]).is_empty(),
            "one PDT tag part-way down a playlist is conforming"
        );
    }

    #[test]
    fn a_playlist_with_dateranges_must_carry_a_pdt_tag() {
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\"\n\
             #EXTINF:4.0,\ns0.m4s\n");
        let issues = check_pdt_coverage(&[pl]);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 1, "expected the §4.4.5.1 finding: {issues:?}");
        assert!(errors[0].message.contains("§4.4.5.1"), "{:?}", errors[0].message);

        // With a PDT tag to anchor it, the same Date Range is fine.
        let anchored = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\"\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:00Z\n\
             #EXTINF:4.0,\ns0.m4s\n");
        assert!(check_pdt_coverage(&[anchored]).is_empty());
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
    fn a_live_window_shorter_than_three_target_durations_errors() {
        // MEDIA-SEQUENCE:8 is the server's own record that it has removed eight segments,
        // which is what §6.2.2 forbids doing down to a window this short.
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n\
             #EXT-X-MEDIA-SEQUENCE:8\n\
             #EXTINF:6.0,\ns0.m4s\n#EXTINF:6.0,\ns1.m4s\n");
        let issues = check_live_playlist_min_segments(&[pl]);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 1, "12s of window against 18s required: {issues:?}");
        assert!(errors[0].message.contains("§6.2.2"), "{:?}", errors[0].message);
    }

    #[test]
    fn a_live_window_of_two_long_segments_is_accepted() {
        // §6.2.2 asks for three Target Durations of media, not three segments. Two segments
        // of one and a half Target Durations each satisfy it.
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXTINF:6.0,\ns0.m4s\n#EXTINF:6.0,\ns1.m4s\n");
        assert!(
            check_live_playlist_min_segments(&[pl]).is_empty(),
            "12s of window against 12s required"
        );
    }

    #[test]
    fn three_very_short_segments_do_not_satisfy_the_live_window() {
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n\
             #EXT-X-MEDIA-SEQUENCE:12\n\
             #EXTINF:1.0,\ns0.m4s\n#EXTINF:1.0,\ns1.m4s\n#EXTINF:1.0,\ns2.m4s\n");
        assert_eq!(
            errors(&check_live_playlist_min_segments(&[pl])).len(),
            1,
            "counting segments passed this playlist; 3s of media is not 18s"
        );
    }

    #[test]
    fn a_live_window_that_has_removed_nothing_yet_is_not_a_violation() {
        // §6.2.2 forbids *removing* a segment down to a window shorter than three Target
        // Durations. MEDIA-SEQUENCE:0 says nothing has been removed, so this is a broadcast
        // that has only just started, not a server breaking the rule.
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n\
             #EXT-X-MEDIA-SEQUENCE:0\n\
             #EXTINF:6.0,\ns0.m4s\n#EXTINF:6.0,\ns1.m4s\n");
        assert!(
            check_live_playlist_min_segments(&[pl]).is_empty(),
            "a window that has only ever grown cannot have been shortened by a removal"
        );

        // The same playlist with no EXT-X-MEDIA-SEQUENCE tag at all, which §6.2.2 requires
        // of any server that does remove segments.
        let untagged = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n\
             #EXTINF:6.0,\ns0.m4s\n#EXTINF:6.0,\ns1.m4s\n");
        assert!(check_live_playlist_min_segments(&[untagged]).is_empty());
    }

    #[test]
    fn an_event_playlist_with_a_short_window_is_not_a_violation() {
        // EXT-X-PLAYLIST-TYPE:EVENT promises segments are only ever appended, so however
        // short the window is, it was not produced by a removal.
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n\
             #EXT-X-PLAYLIST-TYPE:EVENT\n#EXT-X-MEDIA-SEQUENCE:8\n\
             #EXTINF:6.0,\ns0.m4s\n");
        assert_eq!(pl.playlist_type.as_deref(), Some("EVENT"));
        assert!(
            check_live_playlist_min_segments(&[pl]).is_empty(),
            "an EVENT playlist cannot have removed the segments it is being failed for"
        );
    }

    #[test]
    fn a_delta_update_that_skipped_segments_is_evidence_the_window_slid() {
        // No EXT-X-MEDIA-SEQUENCE increment, but the response left segments out, so the
        // window this playlist stands for is not the one it lists.
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n\
             #EXT-X-SKIP:SKIPPED-SEGMENTS=1\n#EXTINF:1.0,\ns0.m4s\n");
        let issues = check_live_playlist_min_segments(&[pl]);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 1, "7s of window against 18s required");
        assert!(
            errors[0].message.contains("Delta Update"),
            "the finding must say what evidence of removal it read: {:?}",
            errors[0].message
        );
    }

    #[test]
    fn segments_skipped_by_a_delta_update_still_count_towards_the_window() {
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n\
             #EXT-X-SKIP:SKIPPED-SEGMENTS=10\n\
             #EXTINF:6.0,\ns0.m4s\n");
        assert!(
            check_live_playlist_min_segments(&[pl]).is_empty(),
            "EXT-X-SKIP replaced the segments, the server did not remove them"
        );
    }

    #[test]
    fn vod_playlist_with_two_segments_passes_min_segment_check() {
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n\
             #EXTINF:6.0,\ns0.m4s\n#EXTINF:6.0,\ns1.m4s\n#EXT-X-ENDLIST\n");
        let issues = check_live_playlist_min_segments(&[pl]);
        assert!(issues.is_empty(), "a playlist with ENDLIST has no sliding window");
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

    // ── check identity ────────────────────────────────────────────────────────

    /// Every finding the checks can produce has to name the check that produced it, because
    /// the report groups findings by that name and anything unnamed lands in a catch-all row.
    #[test]
    fn every_check_names_itself_on_every_finding() {
        let broken = parse_playlist(
            "broken",
            "#EXT-X-TARGETDURATION:6\n\
             #EXT-X-VERSION:1\n\
             #EXT-X-TARGETDURATION:6\n\
             #EXT-X-KEY:METHOD=AES-128,URI=\"k\"\n\
             #EXT-X-MAP:URI=\"init.mp4\"\n\
             #EXT-X-DISCONTINUITY-SEQUENCE:2\n\
             #EXT-X-PART-INF:PART-TARGET=1.0\n\
             #EXT-X-SERVER-CONTROL:CAN-SKIP-UNTIL=6.0,HOLD-BACK=1.0\n\
             #EXT-X-DATERANGE:ID=\"ad-1\",CLASS=\"com.apple.hls.interstitial\",\
             START-DATE=\"2024-01-15T12:00:00Z\"\n\
             #EXT-X-PART:DURATION=2.0,URI=\"p0.m4s\"\n\
             #EXTINF:7.807,\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:00Z\n\
             s0.m4s\n\
             #EXT-X-DISCONTINUITY\n\
             #EXTINF:4.5,\ns1.m4s\n\
             orphan.m4s\n\
             #EXT-X-MEDIA-SEQUENCE:10\n",
        );
        let other = parse_playlist(
            "other",
            "#EXTM3U\n#EXT-X-VERSION:9\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:10\n#EXT-X-DISCONTINUITY-SEQUENCE:5\n\
             #EXT-X-KEY:METHOD=NONE\n#EXT-X-PLAYLIST-TYPE:VOD\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n",
        );
        let master = parse_master(
            "#EXT-X-VERSION:6\n#EXT-X-VERSION:7\n\
             #EXT-X-DEFINE:NAME=\"h\",VALUE=\"https://cdn\"\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a1\",NAME=\"en\",URI=\"a.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a2\",NAME=\"fr\",URI=\"b.m3u8\"\n\
             #EXT-X-STREAM-INF:AUDIO=\"missing\",CODECS=\"avc1.64001f\"\nv.m3u8\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,CODECS=\"hvc1.2.4.L153\"\nv.m3u8\n",
        );

        let playlists = [broken, other];
        let mut findings: Vec<Issue> = Vec::new();
        for pl in &playlists {
            findings.extend(pl.parse_issues.iter().cloned());
        }
        findings.extend(check_extm3u_header(&playlists));
        findings.extend(check_target_duration_compliance(&playlists));
        findings.extend(check_pdt_coverage(&playlists));
        findings.extend(check_media_sequence_duplicate_tags(&playlists));
        findings.extend(check_version_compatibility(&playlists));
        findings.extend(check_live_playlist_min_segments(&playlists));
        findings.extend(check_targetduration_consistency(&playlists));
        findings.extend(check_playlist_type_endlist(&playlists));
        findings.extend(check_encryption_consistency(&playlists));
        findings.extend(check_discontinuity_sequence(&playlists));
        findings.extend(check_segment_count(&playlists));
        findings.extend(check_duration_drift(&playlists, 100.0));
        findings.extend(check_pdt_alignment(&playlists, 100.0));
        findings.extend(check_cumulative_drift(&playlists, 100.0));
        findings.extend(check_ll_hls_compliance(&playlists));
        findings.extend(check_media_sequence_continuity(&playlists));
        findings.extend(check_interstitials(&playlists).0);
        findings.extend(check_master_structure(&master));
        findings.extend(check_stream_inf_consistency(&master));
        findings.extend(check_bandwidth_required(&master));
        findings.extend(check_media_group_membership(&master));
        findings.extend(check_rendition_group_references(&master));

        let unnamed: Vec<&str> = findings.iter()
            .filter(|i| i.check_id == CheckId::Unassigned)
            .map(|i| i.message.as_str())
            .collect();
        assert!(unnamed.is_empty(), "findings that name no check: {unnamed:#?}");

        // Every check the fixture reaches. The ids deliberately not listed are PdtAlignment
        // and SegmentCount, which need a second fully PDT-tagged / VOD-with-ENDLIST rendition
        // this fixture does not have, and DeltaUpdates and PlaylistFetch, which are only
        // produced by the fetching code in the parent module (covered by its own tests).
        let named: HashSet<CheckId> = findings.iter().map(|i| i.check_id).collect();
        let expected = [
            CheckId::BandwidthRequired,
            CheckId::CumulativeDrift,
            CheckId::DiscontinuitySequence,
            CheckId::DurationDrift,
            CheckId::EncryptionConsistency,
            CheckId::ExtM3uHeader,
            CheckId::Interstitials,
            CheckId::LlHls,
            CheckId::LivePlaylistWindow,
            CheckId::MediaGroupMembership,
            CheckId::MediaSequenceTags,
            CheckId::PdtCoverage,
            CheckId::PlaylistTypeEndlist,
            CheckId::RenditionGroupReferences,
            CheckId::SegmentStructure,
            CheckId::SingletonTags,
            CheckId::StreamInfConsistency,
            CheckId::TargetDurationCompliance,
            CheckId::TargetDurationConsistency,
            CheckId::VersionCompatibility,
        ];
        let missing: Vec<CheckId> = expected.iter()
            .filter(|id| !named.contains(id))
            .copied()
            .collect();
        assert!(
            missing.is_empty(),
            "these checks stopped reporting, or stopped naming themselves: {missing:?}"
        );
    }

    // ── check_media_sequence_continuity ───────────────────────────────────────

    #[test]
    fn segment_uri_numbers_are_not_read_as_media_sequence_numbers() {
        // §4.4.3.2 defines the MSN of the first segment as EXT-X-MEDIA-SEQUENCE and nothing
        // else. Numeric segment file names that do not line up with it are not a violation.
        let pl = parse_playlist(
            "v",
            "#EXTM3U\n\
             #EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:10\n\
             #EXTINF:4.0,\n151674692.m4v\n\
             #EXTINF:4.0,\n151674693.m4v\n\
             #EXTINF:4.0,\n20260715T225158-151674694-03-ts.m4v\n",
        );
        let issues = check_media_sequence_continuity(&[pl]);
        assert!(issues.is_empty(), "URI-derived MSNs must not be reported: {issues:?}");
    }

    #[test]
    fn media_sequence_without_the_tag_warns_on_live_only() {
        let live = parse_playlist(
            "live",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n",
        );
        let issues = check_media_sequence_continuity(&[live]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Warn, "the tag is a SHOULD for live playlists");

        let vod = parse_playlist(
            "vod",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n#EXT-X-ENDLIST\n",
        );
        assert!(check_media_sequence_continuity(&[vod]).is_empty());
    }

    #[test]
    fn media_sequence_after_the_first_segment_is_found_across_intervening_tags() {
        // The first segment URI is three lines below its EXTINF; the old scan only looked at
        // the line immediately after an EXTINF and so found no segment at all.
        let pl = parse_playlist(
            "v",
            "#EXTM3U\n\
             #EXT-X-TARGETDURATION:4\n\
             #EXT-X-KEY:METHOD=AES-128,URI=\"k\"\n\
             #EXTINF:4.0,\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:00Z\n\
             #EXT-X-BYTERANGE:1000@0\n\
             s0.m4s\n\
             #EXT-X-MEDIA-SEQUENCE:10\n\
             #EXTINF:4.0,\ns1.m4s\n",
        );
        let issues = check_media_sequence_continuity(&[pl]);
        assert!(
            errors(&issues).iter().any(|i| i.message.contains("MUST appear before")),
            "expected a tag-order error: {issues:?}"
        );
    }

    #[test]
    fn media_sequence_before_the_first_segment_passes_with_intervening_tags() {
        let pl = parse_playlist(
            "v",
            "#EXTM3U\n\
             #EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:10\n\
             #EXTINF:4.0,\n\
             #EXT-X-BYTERANGE:1000@0\n\
             s0.m4s\n",
        );
        assert!(check_media_sequence_continuity(&[pl]).is_empty());
    }

    // ── check_targetduration_consistency ─────────────────────────────────────

    #[test]
    fn targetduration_mismatch_across_renditions_errors() {
        let a = parse_playlist("v0", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\ns0.m4s\n");
        let b = parse_playlist("v1", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.0,\ns0.m4s\n");
        let issues = check_targetduration_consistency(&[a, b]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Error, "§6.2.4 states this as a MUST");
    }

    #[test]
    fn vod_iframe_playlist_may_declare_its_own_targetduration() {
        let video = parse_playlist(
            "v0",
            "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-PLAYLIST-TYPE:VOD\n\
             #EXTINF:6.0,\ns0.m4s\n#EXT-X-ENDLIST\n",
        );
        let trick = parse_playlist(
            "iframe",
            "#EXTM3U\n#EXT-X-TARGETDURATION:60\n#EXT-X-PLAYLIST-TYPE:VOD\n\
             #EXT-X-I-FRAMES-ONLY\n#EXTINF:60.0,\ni0.m4s\n#EXT-X-ENDLIST\n",
        );
        let issues = check_targetduration_consistency(&[video, trick]);
        assert!(issues.is_empty(), "§6.2.4 exempts VOD I-frame playlists: {issues:?}");
    }

    // ── check_discontinuity_sequence ──────────────────────────────────────────

    #[test]
    fn discontinuity_sequence_mismatch_errors() {
        let a = parse_playlist(
            "v0",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-DISCONTINUITY-SEQUENCE:2\n\
             #EXTINF:4.0,\ns0.m4s\n",
        );
        let b = parse_playlist(
            "v1",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-DISCONTINUITY-SEQUENCE:5\n\
             #EXTINF:4.0,\ns0.m4s\n",
        );
        let issues = check_discontinuity_sequence(&[a, b]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].check_id, CheckId::DiscontinuitySequence);
    }

    // ── check_encryption_consistency ──────────────────────────────────────────

    #[test]
    fn a_clear_rendition_alongside_an_encrypted_one_warns_but_does_not_fail() {
        let encrypted = parse_playlist(
            "v0",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-KEY:METHOD=AES-128,URI=\"k\"\n#EXTINF:4.0,\ns0.m4s\n",
        );
        let clear = parse_playlist(
            "iframe",
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-KEY:METHOD=NONE\n#EXTINF:4.0,\ni0.m4s\n",
        );
        let issues = check_encryption_consistency(&[encrypted, clear]);
        assert!(
            errors(&issues).is_empty(),
            "a mixed clear/encrypted presentation must not fail the run: {issues:?}"
        );
        assert!(issues.iter().any(|i| i.severity == Severity::Warn), "but it is worth a warning");
    }

    // ── check_version_compatibility ───────────────────────────────────────────

    #[test]
    fn map_without_iframes_only_requires_version_6() {
        let v5 = parse_playlist(
            "v",
            "#EXTM3U\n#EXT-X-VERSION:5\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:4.0,\ns0.m4s\n",
        );
        let issues = check_version_compatibility(&[v5]);
        assert!(
            errors(&issues).iter().any(|i| i.message.contains("VERSION >= 6")),
            "EXT-X-MAP outside an I-frame playlist needs v6: {issues:?}"
        );

        let v6 = parse_playlist(
            "v",
            "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:4.0,\ns0.m4s\n",
        );
        assert!(check_version_compatibility(&[v6]).is_empty());
    }

    #[test]
    fn map_in_an_iframes_only_playlist_is_allowed_at_version_5() {
        let pl = parse_playlist(
            "iframe",
            "#EXTM3U\n#EXT-X-VERSION:5\n#EXT-X-TARGETDURATION:60\n#EXT-X-I-FRAMES-ONLY\n\
             #EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:60.0,\ni0.m4s\n",
        );
        let issues = check_version_compatibility(&[pl]);
        assert!(issues.is_empty(), "§8 allows MAP at v5 with I-FRAMES-ONLY: {issues:?}");
    }

    // ── check_ll_hls_compliance ───────────────────────────────────────────────

    fn ll_playlist(server_control: &str, part_inf: bool, independent_first_part: bool) -> MediaPlaylist {
        let first_part = if independent_first_part { ",INDEPENDENT=YES" } else { "" };
        let content = format!(
            "#EXTM3U\n#EXT-X-VERSION:9\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:1\n\
             {server_control}\n\
             {}\
             #EXT-X-PART:DURATION=1.0,URI=\"p0.m4s\"{first_part}\n\
             #EXT-X-PART:DURATION=1.0,URI=\"p1.m4s\"\n\
             #EXTINF:4.0,\ns0.m4s\n",
            if part_inf { "#EXT-X-PART-INF:PART-TARGET=1.0\n" } else { "" },
        );
        parse_playlist("v", &content)
    }

    #[test]
    fn a_dependent_first_part_is_not_an_error() {
        // INDEPENDENT is an OPTIONAL attribute; §4.4.4.9 only recommends it on the first part.
        let pl = ll_playlist(
            "#EXT-X-SERVER-CONTROL:CAN-BLOCK-RELOAD=YES,PART-HOLD-BACK=3.0,HOLD-BACK=12.0",
            true,
            false,
        );
        let issues = check_ll_hls_compliance(&[pl]);
        assert!(
            !issues.iter().any(|i| i.message.contains("INDEPENDENT")),
            "a dependent first part must not be reported: {issues:?}"
        );
    }

    #[test]
    fn a_missing_can_block_reload_is_not_an_error() {
        let pl = ll_playlist(
            "#EXT-X-SERVER-CONTROL:PART-HOLD-BACK=3.0,HOLD-BACK=12.0",
            true,
            true,
        );
        let issues = check_ll_hls_compliance(&[pl]);
        assert!(
            !issues.iter().any(|i| i.message.contains("CAN-BLOCK-RELOAD")),
            "CAN-BLOCK-RELOAD is how a server advertises blocking reload, not a requirement \
             on the playlist: {issues:?}"
        );
    }

    #[test]
    fn part_inf_without_part_hold_back_errors() {
        let pl = ll_playlist(
            "#EXT-X-SERVER-CONTROL:CAN-BLOCK-RELOAD=YES,HOLD-BACK=12.0",
            true,
            true,
        );
        let issues = check_ll_hls_compliance(&[pl]);
        assert!(
            errors(&issues).iter().any(|i| i.message.contains("PART-HOLD-BACK")
                && i.message.contains("REQUIRED")),
            "§4.4.3.8 makes PART-HOLD-BACK REQUIRED alongside EXT-X-PART-INF: {issues:?}"
        );
    }

    #[test]
    fn part_hold_back_present_satisfies_the_requirement() {
        let pl = ll_playlist(
            "#EXT-X-SERVER-CONTROL:CAN-BLOCK-RELOAD=YES,PART-HOLD-BACK=3.0,HOLD-BACK=12.0",
            true,
            true,
        );
        let issues = check_ll_hls_compliance(&[pl]);
        assert!(
            !issues.iter().any(|i| i.message.contains("no PART-HOLD-BACK")),
            "unexpected PART-HOLD-BACK finding: {issues:?}"
        );
    }

    // ── check_stream_inf_consistency ──────────────────────────────────────────

    #[test]
    fn same_uri_and_groups_with_different_codecs_warns() {
        let master = parse_master(
            "#EXTM3U\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aac\",NAME=\"en\",URI=\"a.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,AUDIO=\"aac\",CODECS=\"avc1.64001f,mp4a.40.2\"\nv.m3u8\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,AUDIO=\"aac\",CODECS=\"avc1.64001f,mp4a.40.5\"\nv.m3u8\n",
        );
        let issues = check_stream_inf_consistency(&master);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn, "a CODECS mismatch must not fail the run");
        assert!(issues[0].message.contains("§4.4.6.2"), "citation: {}", issues[0].message);
    }

    #[test]
    fn same_uri_with_different_subtitle_groups_is_a_different_variant_stream() {
        // Two Variant Streams sharing a video URI but pairing it with different SUBTITLES
        // groups describe different presentations; their CODECS and BANDWIDTH may differ.
        let master = parse_master(
            "#EXTM3U\n\
             #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs-en\",NAME=\"en\",URI=\"en.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs-fr\",NAME=\"fr\",URI=\"fr.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,SUBTITLES=\"subs-en\",CODECS=\"avc1.64001f\"\nv.m3u8\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1200,SUBTITLES=\"subs-fr\",CODECS=\"avc1.64001f,mp4a.40.2\"\nv.m3u8\n",
        );
        let issues = check_stream_inf_consistency(&master);
        assert!(issues.is_empty(), "expected no findings: {issues:?}");
    }

    #[test]
    fn same_uri_with_different_video_codecs_errors() {
        let master = parse_master(
            "#EXTM3U\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aac\",NAME=\"en\",URI=\"a.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"ec3\",NAME=\"en\",URI=\"a2.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,AUDIO=\"aac\",CODECS=\"avc1.64001f,mp4a.40.2\"\nv.m3u8\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1400,AUDIO=\"ec3\",CODECS=\"hvc1.2.4.L153,ec-3\"\nv.m3u8\n",
        );
        let issues = check_stream_inf_consistency(&master);
        let errs = errors(&issues);
        assert_eq!(errs.len(), 1, "{issues:?}");
        assert!(errs[0].message.contains("§6.2.4"), "citation: {}", errs[0].message);
    }

    #[test]
    fn same_uri_with_different_video_groups_is_a_different_variant_stream() {
        // VIDEO pairs a Variant Stream with an alternative video rendition group the same way
        // AUDIO does, so it is part of what makes two STREAM-INF entries the same stream.
        let master = parse_master(
            "#EXTM3U\n\
             #EXT-X-MEDIA:TYPE=VIDEO,GROUP-ID=\"cam-main\",NAME=\"main\",URI=\"m.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=VIDEO,GROUP-ID=\"cam-alt\",NAME=\"alt\",URI=\"c.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,VIDEO=\"cam-main\",CODECS=\"avc1.64001f\"\nv.m3u8\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1200,VIDEO=\"cam-alt\",CODECS=\"avc1.64001f,mp4a.40.2\"\nv.m3u8\n",
        );
        let issues = check_stream_inf_consistency(&master);
        assert!(issues.is_empty(), "expected no findings: {issues:?}");
    }

    #[test]
    fn same_uri_and_groups_with_different_bandwidth_warns() {
        let master = parse_master(
            "#EXTM3U\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,CODECS=\"avc1.64001f\"\nv.m3u8\n\
             #EXT-X-STREAM-INF:BANDWIDTH=2000,CODECS=\"avc1.64001f\"\nv.m3u8\n",
        );
        let issues = check_stream_inf_consistency(&master);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(issues[0].message.contains("BANDWIDTH"));
    }

    // ── check_master_structure ────────────────────────────────────────────────

    #[test]
    fn master_without_extm3u_errors() {
        let master = parse_master("#EXT-X-STREAM-INF:BANDWIDTH=1000\nv.m3u8\n");
        let issues = check_master_structure(&master);
        assert!(
            errors(&issues).iter().any(|i| i.check_id == CheckId::ExtM3uHeader),
            "{issues:?}"
        );
    }

    #[test]
    fn master_repeating_a_singleton_tag_errors() {
        let master = parse_master(
            "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-VERSION:7\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000\nv.m3u8\n",
        );
        let issues = check_master_structure(&master);
        assert!(
            errors(&issues).iter().any(|i| i.check_id == CheckId::SingletonTags
                && i.message.contains("EXT-X-VERSION")),
            "{issues:?}"
        );
    }

    #[test]
    fn master_using_variable_substitution_below_version_8_errors() {
        let master = parse_master(
            "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-DEFINE:NAME=\"host\",VALUE=\"https://cdn\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000\n{$host}/v.m3u8\n",
        );
        let issues = check_master_structure(&master);
        assert!(
            errors(&issues).iter().any(|i| i.check_id == CheckId::VersionCompatibility
                && i.message.contains("VERSION >= 8")),
            "{issues:?}"
        );
    }

    #[test]
    fn a_well_formed_master_produces_no_structural_findings() {
        let master = parse_master(
            "#EXTM3U\n#EXT-X-VERSION:8\n#EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-DEFINE:NAME=\"host\",VALUE=\"https://cdn\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000\n{$host}/v.m3u8\n",
        );
        assert!(check_master_structure(&master).is_empty());
    }

    // ── check_rendition_group_references ──────────────────────────────────────

    #[test]
    fn a_dangling_audio_group_reference_errors() {
        let master = parse_master(
            "#EXTM3U\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aac\",NAME=\"en\",URI=\"a.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,AUDIO=\"missing\"\nv.m3u8\n",
        );
        let issues = check_rendition_group_references(&master);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(issues[0].message.contains("AUDIO=\"missing\""), "{}", issues[0].message);
    }

    #[test]
    fn dangling_subtitle_and_video_group_references_error() {
        let master = parse_master(
            "#EXTM3U\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,SUBTITLES=\"subs\",VIDEO=\"alt\"\nv.m3u8\n",
        );
        let issues = check_rendition_group_references(&master);
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert!(issues.iter().any(|i| i.message.contains("SUBTITLES=\"subs\"")));
        assert!(issues.iter().any(|i| i.message.contains("VIDEO=\"alt\"")));
    }

    #[test]
    fn resolvable_group_references_and_closed_captions_none_pass() {
        let master = parse_master(
            "#EXTM3U\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aac\",NAME=\"en\",URI=\"a.m3u8\"\n\
             #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"en\",URI=\"s.m3u8\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,AUDIO=\"aac\",SUBTITLES=\"subs\",\
             CLOSED-CAPTIONS=NONE\nv.m3u8\n",
        );
        let issues = check_rendition_group_references(&master);
        assert!(issues.is_empty(), "CLOSED-CAPTIONS=NONE is not a group reference: {issues:?}");
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

    #[test]
    fn one_broken_interstitial_is_reported_once_for_the_whole_presentation() {
        // The same DATERANGE appears in every rendition, as §6.2.4 requires. Reporting it
        // per rendition turned one authoring mistake into eight rows.
        let tag = concat!(
            "#EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\",",
            "CLASS=\"com.apple.hls.interstitial\"\n",
        );
        let content = format!("#EXTM3U\n{tag}");
        let playlists = vec![
            make_playlist("v-hi", &content),
            make_playlist("v-lo", &content),
            make_playlist("a-en", &content),
        ];
        let (issues, interstitials) = check_interstitials(&playlists);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 1, "one mistake, one finding: {issues:?}");
        assert_eq!(errors[0].rendition_a.as_deref(), Some("v-hi"));
        assert!(
            errors[0].uri_note.as_deref().is_some_and(|n| n.contains("3 renditions")),
            "the finding must still say where it was seen: {:?}", errors[0].uri_note
        );
        assert_eq!(
            interstitials.len(), 3,
            "the interstitial table is still per rendition"
        );
    }

    #[test]
    fn two_interstitials_with_no_id_are_two_findings() {
        // Keying findings by ID alone made every ID-less DATERANGE the same DATERANGE.
        let content = concat!(
            "#EXTM3U\n",
            "#EXT-X-DATERANGE:START-DATE=\"2024-01-15T12:00:00Z\",",
            "CLASS=\"com.apple.hls.interstitial\",X-ASSET-URI=\"https://ads.example.com/a.m3u8\"\n",
            "#EXT-X-DATERANGE:START-DATE=\"2024-01-15T12:30:00Z\",",
            "CLASS=\"com.apple.hls.interstitial\",X-ASSET-URI=\"https://ads.example.com/b.m3u8\"\n",
        );
        let (issues, interstitials) = check_interstitials(&[make_playlist("v", content)]);
        assert_eq!(
            errors(&issues).len(), 2,
            "two Date Ranges each missing an ID are two mistakes: {issues:?}"
        );
        assert_eq!(interstitials.len(), 2);
    }

    #[test]
    fn an_id_less_in_tag_does_not_donate_its_attributes_to_another_interstitial() {
        // The IN tag here belongs to nothing: it has no ID to pair it with the interstitial.
        // Keyed under the empty string, its X-RESUME-OFFSET used to be adopted by the first
        // interstitial that also happened to have no ID.
        let content = concat!(
            "#EXTM3U\n",
            "#EXT-X-DATERANGE:START-DATE=\"2024-01-15T12:00:00Z\",",
            "CLASS=\"com.apple.hls.interstitial\",X-ASSET-URI=\"https://ads.example.com/a.m3u8\"\n",
            "#EXT-X-DATERANGE:START-DATE=\"2024-01-15T12:00:30Z\",X-RESUME-OFFSET=12.5\n",
        );
        let (_, interstitials) = check_interstitials(&[make_playlist("v", content)]);
        assert_eq!(interstitials.len(), 1);
        assert_eq!(
            interstitials[0].resume_offset, None,
            "an unpaired IN tag must not lend its X-RESUME-OFFSET to an unrelated Date Range"
        );
    }

    #[test]
    fn an_in_tag_still_completes_the_interstitial_that_shares_its_id() {
        let content = concat!(
            "#EXTM3U\n",
            "#EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\",",
            "CLASS=\"com.apple.hls.interstitial\",X-ASSET-URI=\"https://ads.example.com/a.m3u8\"\n",
            "#EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:30Z\",",
            "X-RESUME-OFFSET=12.5,X-PLAYOUT-LIMIT=30\n",
        );
        let (_, interstitials) = check_interstitials(&[make_playlist("v", content)]);
        assert_eq!(interstitials[0].resume_offset, Some(12.5));
        assert_eq!(interstitials[0].playout_limit, Some(30.0));
    }

    // ── Issue consolidation ───────────────────────────────────────────────────

    #[test]
    fn a_run_of_overlong_segments_is_one_finding_naming_the_range() {
        let mut content = String::from("#EXTM3U\n#EXT-X-TARGETDURATION:4\n");
        for i in 0..5 {
            content.push_str(&format!("#EXTINF:9.0,\ns{i}.m4s\n"));
        }
        let issues = check_target_duration_compliance(&[parse_playlist("v", &content)]);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 1, "five overruns in a row are one finding: {issues:?}");
        let issue = errors[0];
        assert_eq!((issue.seg_first, issue.seg_last, issue.count), (0, 4, 5));
        assert!(issue.message.contains("Segments 0–4"), "{}", issue.message);
        assert!(
            issue.uri_a.is_none(),
            "a range of segments cannot be linked to one segment's URI"
        );
    }

    #[test]
    fn separate_runs_of_overlong_segments_stay_separate() {
        let content = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXTINF:9.0,\ns0.m4s\n#EXTINF:9.0,\ns1.m4s\n\
             #EXTINF:4.0,\ns2.m4s\n\
             #EXTINF:9.0,\ns3.m4s\n";
        let issues = check_target_duration_compliance(&[parse_playlist("v", content)]);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 2, "a conforming segment breaks the run: {issues:?}");
        assert_eq!((errors[0].seg_first, errors[0].seg_last, errors[0].count), (0, 1, 2));
        assert_eq!(errors[1].count, 1);
        assert_eq!(errors[1].segment_index, 3);
        assert!(
            errors[1].uri_a.is_some(),
            "a single-segment finding still links to its segment"
        );
    }

    #[test]
    fn a_run_of_drifting_segments_is_one_finding() {
        let hi = parse_playlist("v-hi", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:0\n\
             #EXTINF:4.5,\ns0.m4s\n#EXTINF:4.5,\ns1.m4s\n#EXTINF:4.5,\ns2.m4s\n");
        let lo = parse_playlist("v-lo", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:0\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n#EXTINF:4.0,\ns2.m4s\n");
        let issues = check_duration_drift(&[hi, lo], 100.0);
        assert_eq!(issues.len(), 1, "three drifting segments in a row: {issues:?}");
        assert_eq!(issues[0].count, 3);
        assert!(issues[0].message.contains("MSN 0–2"), "{}", issues[0].message);
    }

    #[test]
    fn a_run_of_misaligned_pdts_is_one_finding_naming_the_range() {
        let hi = parse_playlist("v-hi", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:00Z\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n#EXTINF:4.0,\ns2.m4s\n");
        let lo = parse_playlist("v-lo", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:02Z\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n#EXTINF:4.0,\ns2.m4s\n");
        let issues = check_pdt_alignment(&[hi, lo], 100.0);
        assert_eq!(issues.len(), 1, "three misaligned segments in a row: {issues:?}");
        assert_eq!((issues[0].seg_first, issues[0].seg_last, issues[0].count), (0, 2, 3));
        assert_eq!(issues[0].check_id, CheckId::PdtAlignment);
        assert!(issues[0].message.contains("MSN 0–2"), "{}", issues[0].message);
        assert!(
            issues[0].uri_a.is_none() && issues[0].uri_b.is_none(),
            "a range of segments cannot be linked to one pair of segment URIs"
        );
    }

    #[test]
    fn a_gap_between_misaligned_pdts_breaks_the_run() {
        // 'v-lo' re-anchors its second segment onto the same wall clock as 'v-hi' before
        // drifting again, so the segments either side of it are two findings, not one run.
        let hi = parse_playlist("v-hi", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:00Z\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n#EXTINF:4.0,\ns2.m4s\n\
             #EXTINF:4.0,\ns3.m4s\n");
        let lo = parse_playlist("v-lo", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:0\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:02Z\n#EXTINF:4.0,\ns0.m4s\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:04Z\n#EXTINF:4.0,\ns1.m4s\n\
             #EXT-X-PROGRAM-DATE-TIME:2024-01-15T12:00:10Z\n#EXTINF:4.0,\ns2.m4s\n\
             #EXTINF:4.0,\ns3.m4s\n");
        let issues = check_pdt_alignment(&[hi, lo], 100.0);
        assert_eq!(issues.len(), 2, "an aligned segment breaks the run: {issues:?}");
        assert_eq!(issues[0].count, 1);
        assert_eq!(issues[0].segment_index, 0);
        assert_eq!((issues[1].seg_first, issues[1].seg_last, issues[1].count), (2, 3, 2));
    }

    #[test]
    fn two_renditions_that_share_a_name_are_folded_apart() {
        // EXT-X-MEDIA NAME is not unique, and folding by it merged one rendition's run into
        // the next rendition's, reporting both under a single name.
        let overlong_then_fine = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXTINF:9.0,\ns0.m4s\n#EXTINF:9.0,\ns1.m4s\n#EXTINF:4.0,\ns2.m4s\n";
        let fine_then_overlong = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n#EXTINF:9.0,\ns2.m4s\n";
        let playlists = vec![
            named_playlist("v", "https://cdn.example.com/a/v.m3u8", overlong_then_fine),
            named_playlist("v", "https://cdn.example.com/b/v.m3u8", fine_then_overlong),
        ];
        let issues = check_target_duration_compliance(&playlists);
        let errors = errors(&issues);
        assert_eq!(
            errors.len(),
            2,
            "the second playlist's segment 2 continues the first playlist's run only if the \
             two are taken for the same rendition: {issues:?}"
        );
        assert_eq!((errors[0].seg_first, errors[0].seg_last, errors[0].count), (0, 1, 2));
        assert_eq!((errors[1].segment_index, errors[1].count), (2, 1));
    }

    #[test]
    fn two_rendition_pairs_that_share_a_name_are_folded_apart() {
        // The same hazard for the checks that compare a pair of renditions: two pairs that
        // spell out the same two names were folded into one finding covering both.
        let hi = named_playlist("v-hi", "https://cdn.example.com/hi.m3u8",
            "#EXTM3U\n#EXT-X-TARGETDURATION:5\n#EXT-X-MEDIA-SEQUENCE:0\n\
             #EXTINF:5.0,\ns0.m4s\n#EXTINF:5.0,\ns1.m4s\n#EXTINF:5.0,\ns2.m4s\n");
        let lo_a = named_playlist("v-lo", "https://cdn.example.com/a/lo.m3u8",
            "#EXTM3U\n#EXT-X-TARGETDURATION:5\n#EXT-X-MEDIA-SEQUENCE:0\n\
             #EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n#EXTINF:5.0,\ns2.m4s\n");
        let lo_b = named_playlist("v-lo", "https://cdn.example.com/b/lo.m3u8",
            "#EXTM3U\n#EXT-X-TARGETDURATION:5\n#EXT-X-MEDIA-SEQUENCE:0\n\
             #EXTINF:5.0,\ns0.m4s\n#EXTINF:5.0,\ns1.m4s\n#EXTINF:4.0,\ns2.m4s\n");
        let issues = check_duration_drift(&[hi, lo_a, lo_b], 100.0);
        assert_eq!(
            issues.len(),
            3,
            "three pairs drift, and two of them name the same two renditions: {issues:?}"
        );
        assert_eq!((issues[0].seg_first, issues[0].seg_last, issues[0].count), (0, 1, 2));
        assert_eq!((issues[1].segment_index, issues[1].count), (2, 1));
        assert_eq!((issues[2].seg_first, issues[2].seg_last, issues[2].count), (0, 2, 3));
    }

    // ── check_iframe_playlists ────────────────────────────────────────────────

    fn iframe_playlist(name: &str, iframes_only: bool) -> MediaPlaylist {
        let content = if iframes_only {
            "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-I-FRAMES-ONLY\n\
             #EXTINF:6.0,\ni0.m4s\n#EXT-X-ENDLIST\n"
        } else {
            "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\ni0.m4s\n#EXT-X-ENDLIST\n"
        };
        let mut pl = parse_playlist(name, content);
        pl.is_iframe = true;
        pl
    }

    #[test]
    fn an_iframe_variant_without_i_frames_only_errors() {
        let issues = check_iframe_playlists(&[iframe_playlist("trick", false)]);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 1, "{issues:?}");
        assert!(errors[0].message.contains("§4.4.6.3"), "{}", errors[0].message);
        assert_eq!(errors[0].check_id, CheckId::IFramePlaylists);
    }

    #[test]
    fn an_iframe_variant_that_declares_i_frames_only_passes() {
        assert!(check_iframe_playlists(&[iframe_playlist("trick", true)]).is_empty());
    }

    #[test]
    fn a_regular_variant_is_not_asked_for_i_frames_only() {
        let pl = parse_playlist("v", "#EXTM3U\n#EXT-X-TARGETDURATION:6\n\
             #EXTINF:6.0,\ns0.m4s\n#EXT-X-ENDLIST\n");
        assert!(check_iframe_playlists(&[pl]).is_empty());
    }

    // ── check_daterange_consistency ───────────────────────────────────────────

    /// A playlist whose window starts at `start` and carries the given DATERANGE lines.
    fn playlist_with_dateranges(name: &str, start: &str, ranges: &[&str]) -> MediaPlaylist {
        daterange_playlist(name, start, ranges, false)
    }

    /// The same, with EXT-X-ENDLIST, so the presentation can no longer add attributes to a
    /// Date Range it has already written.
    fn ended_playlist_with_dateranges(name: &str, start: &str, ranges: &[&str]) -> MediaPlaylist {
        daterange_playlist(name, start, ranges, true)
    }

    fn daterange_playlist(
        name: &str,
        start: &str,
        ranges: &[&str],
        ended: bool,
    ) -> MediaPlaylist {
        let mut content = format!(
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-PROGRAM-DATE-TIME:{start}\n"
        );
        for range in ranges {
            content.push_str(range);
            content.push('\n');
        }
        content.push_str("#EXTINF:4.0,\ns0.m4s\n#EXTINF:4.0,\ns1.m4s\n");
        if ended {
            content.push_str("#EXT-X-ENDLIST\n");
        }
        let pl = parse_playlist(name, &content);
        assert_eq!(pl.has_endlist, ended);
        pl
    }

    const AD_1: &str = "#EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\",\
                        DURATION=30.0";

    #[test]
    fn playlists_carrying_the_same_date_ranges_pass() {
        let playlists = vec![
            playlist_with_dateranges("v-hi", "2024-01-15T12:00:00Z", &[AD_1]),
            playlist_with_dateranges("v-lo", "2024-01-15T12:00:00Z", &[AD_1]),
        ];
        assert!(check_daterange_consistency(&playlists).is_empty());
    }

    #[test]
    fn a_playlist_with_date_ranges_missing_one_that_others_carry_errors() {
        let other = "#EXT-X-DATERANGE:ID=\"ad-2\",START-DATE=\"2024-01-15T12:00:04Z\"";
        let playlists = vec![
            playlist_with_dateranges("v-hi", "2024-01-15T12:00:00Z", &[AD_1, other]),
            playlist_with_dateranges("v-lo", "2024-01-15T12:00:00Z", &[AD_1]),
        ];
        let issues = check_daterange_consistency(&playlists);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 1, "{issues:?}");
        assert!(errors[0].message.contains("'ad-2'"), "{}", errors[0].message);
        assert_eq!(errors[0].rendition_a.as_deref(), Some("v-lo"));
    }

    #[test]
    fn a_playlist_with_no_date_ranges_at_all_is_left_out_of_the_comparison() {
        // §6.2.4 compares the playlists that carry Date Ranges with each other; a rendition
        // that carries none is not one of them.
        let playlists = vec![
            playlist_with_dateranges("v-hi", "2024-01-15T12:00:00Z", &[AD_1]),
            playlist_with_dateranges("a-en", "2024-01-15T12:00:00Z", &[]),
        ];
        assert!(check_daterange_consistency(&playlists).is_empty());
    }

    #[test]
    fn corresponding_date_ranges_that_disagree_about_an_attribute_error() {
        // Two values for one attribute contradict each other whether or not the presentation
        // is still running, so this stays an Error on live playlists.
        let shifted = "#EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\",\
                       DURATION=15.0";
        let playlists = vec![
            playlist_with_dateranges("v-hi", "2024-01-15T12:00:00Z", &[AD_1]),
            playlist_with_dateranges("v-lo", "2024-01-15T12:00:00Z", &[shifted]),
        ];
        assert!(playlists.iter().all(|pl| !pl.has_endlist));
        let issues = check_daterange_consistency(&playlists);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 1, "{issues:?}");
        assert!(
            errors[0].message.contains("'ad-1' differs in DURATION"),
            "the finding must name the attribute that differs: {}", errors[0].message
        );
    }

    #[test]
    fn an_attribute_only_some_live_playlists_carry_yet_is_a_warning() {
        // §4.4.5.1 lets a server augment a Date Range with a later tag carrying the same ID.
        // While the presentation is still running, one rendition having written DURATION and
        // another not having got there yet is that augmentation in progress, not a
        // disagreement — reporting it as an Error failed conforming live streams.
        let without_duration = "#EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\"";
        let playlists = vec![
            playlist_with_dateranges("v-hi", "2024-01-15T12:00:00Z", &[AD_1]),
            playlist_with_dateranges("v-lo", "2024-01-15T12:00:00Z", &[without_duration]),
        ];
        let issues = check_daterange_consistency(&playlists);
        assert!(errors(&issues).is_empty(), "a live window may still be augmented: {issues:?}");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert_eq!(issues[0].confidence, Confidence::Heuristic);
        assert!(
            issues[0].message.contains("'ad-1' carries DURATION in some playlists only"),
            "the finding must name the attribute and the Date Range: {}", issues[0].message
        );
    }

    #[test]
    fn an_attribute_only_some_completed_playlists_carry_errors() {
        // Every carrier has ENDLIST, so no later tag is coming and the sets of
        // attribute/value pairs will never match.
        let without_duration = "#EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\"";
        let playlists = vec![
            ended_playlist_with_dateranges("v-hi", "2024-01-15T12:00:00Z", &[AD_1]),
            ended_playlist_with_dateranges("v-lo", "2024-01-15T12:00:00Z", &[without_duration]),
        ];
        let issues = check_daterange_consistency(&playlists);
        let errors = errors(&issues);
        assert_eq!(errors.len(), 1, "{issues:?}");
        assert_eq!(errors[0].confidence, Confidence::Measured);
        assert!(
            errors[0].message.contains("DURATION")
                && errors[0].message.contains("EXT-X-ENDLIST"),
            "the finding must say why augmentation can be ruled out: {}", errors[0].message
        );
    }

    #[test]
    fn one_live_carrier_is_enough_to_soften_a_missing_attribute() {
        // Not every rendition of a live presentation is fetched at the same instant, so the
        // gate is on any carrier still being able to grow, not on all of them.
        let without_duration = "#EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\"";
        let playlists = vec![
            ended_playlist_with_dateranges("v-hi", "2024-01-15T12:00:00Z", &[AD_1]),
            playlist_with_dateranges("v-lo", "2024-01-15T12:00:00Z", &[without_duration]),
        ];
        let issues = check_daterange_consistency(&playlists);
        assert!(errors(&issues).is_empty(), "{issues:?}");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Warn);
    }

    #[test]
    fn a_date_range_outside_a_playlists_window_is_not_required_of_it() {
        // The live windows were fetched seconds apart, so one rendition has already dropped
        // the segments the older Date Range sits in.
        let old = "#EXT-X-DATERANGE:ID=\"ad-old\",START-DATE=\"2024-01-15T11:00:00Z\"";
        let playlists = vec![
            playlist_with_dateranges("v-hi", "2024-01-15T11:00:00Z", &[old, AD_1]),
            playlist_with_dateranges("v-lo", "2024-01-15T12:00:00Z", &[AD_1]),
        ];
        assert!(
            check_daterange_consistency(&playlists).is_empty(),
            "a sliding window is not a set difference"
        );
    }

    #[test]
    fn updates_to_a_date_range_are_merged_before_the_sets_are_compared() {
        // §4.4.5.1 lets a later tag with the same ID add attributes. One playlist writing them
        // as one tag and another as two is the same set of attribute/value pairs.
        let split_a = "#EXT-X-DATERANGE:ID=\"ad-1\",START-DATE=\"2024-01-15T12:00:00Z\"";
        let split_b = "#EXT-X-DATERANGE:ID=\"ad-1\",DURATION=30.0";
        let playlists = vec![
            playlist_with_dateranges("v-hi", "2024-01-15T12:00:00Z", &[AD_1]),
            playlist_with_dateranges("v-lo", "2024-01-15T12:00:00Z", &[split_a, split_b]),
        ];
        assert!(check_daterange_consistency(&playlists).is_empty());
    }

    // ── EXT-X-SERVER-CONTROL identity (§6.2.4) ────────────────────────────────

    fn playlist_with_server_control(name: &str, attrs: &str, with_parts: bool) -> MediaPlaylist {
        let mut content = format!(
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-SERVER-CONTROL:{attrs}\n"
        );
        if with_parts {
            content.push_str("#EXT-X-PART-INF:PART-TARGET=1.0\n");
        }
        content.push_str("#EXTINF:4.0,\ns0.m4s\n");
        if with_parts {
            content.push_str("#EXT-X-PART:DURATION=1.0,URI=\"s1.0.m4s\"\n");
        }
        parse_playlist(name, &content)
    }

    /// The §6.2.4 findings only, since the fixtures are deliberately imperfect low-latency
    /// playlists in other ways.
    fn identity_errors(playlists: &[MediaPlaylist]) -> Vec<String> {
        check_ll_hls_compliance(playlists).into_iter()
            .filter(|i| i.severity == Severity::Error && i.message.contains("§6.2.4"))
            .map(|i| i.message)
            .collect()
    }

    #[test]
    fn server_control_is_compared_between_playlists_that_have_no_parts() {
        // Only playlists carrying EXT-X-PART used to be compared, so an audio rendition with a
        // different HOLD-BACK was never looked at.
        let playlists = vec![
            playlist_with_server_control("v", "CAN-BLOCK-RELOAD=YES,HOLD-BACK=12.0", false),
            playlist_with_server_control("a", "CAN-BLOCK-RELOAD=YES,HOLD-BACK=24.0", false),
        ];
        let found = identity_errors(&playlists);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("HOLD-BACK=12") && found[0].contains("HOLD-BACK=24"),
            "the finding must name the values that differ: {}", found[0]
        );
    }

    #[test]
    fn server_control_values_are_compared_without_truncating_them() {
        // Compared as integers, 18.0 and 18.9 were the same value.
        let playlists = vec![
            playlist_with_server_control("v", "CAN-SKIP-UNTIL=24.0,HOLD-BACK=18.0", false),
            playlist_with_server_control("a", "CAN-SKIP-UNTIL=24.0,HOLD-BACK=18.9", false),
        ];
        assert_eq!(identity_errors(&playlists).len(), 1);
    }

    #[test]
    fn can_skip_dateranges_is_part_of_the_comparison() {
        let playlists = vec![
            playlist_with_server_control("v", "CAN-SKIP-UNTIL=24.0,CAN-SKIP-DATERANGES=YES", false),
            playlist_with_server_control("a", "CAN-SKIP-UNTIL=24.0", false),
        ];
        let found = identity_errors(&playlists);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("CAN-SKIP-DATERANGES"), "{}", found[0]);
    }

    #[test]
    fn a_playlist_missing_server_control_entirely_is_reported_as_missing_it() {
        let playlists = vec![
            playlist_with_server_control("v", "CAN-BLOCK-RELOAD=YES,HOLD-BACK=12.0", false),
            parse_playlist("a", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.0,\ns0.m4s\n"),
        ];
        let found = identity_errors(&playlists);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("'a' has no EXT-X-SERVER-CONTROL tag"),
            "a playlist without the tag is not a playlist of zeroes: {}", found[0]
        );
    }

    #[test]
    fn identical_server_control_across_playlists_passes() {
        let attrs = "CAN-BLOCK-RELOAD=YES,CAN-SKIP-UNTIL=24.0,HOLD-BACK=12.0";
        let playlists = vec![
            playlist_with_server_control("v", attrs, false),
            playlist_with_server_control("a", attrs, false),
        ];
        assert!(identity_errors(&playlists).is_empty());
    }

    // ── Cross-playlist EXT-X-PLAYLIST-TYPE (§6.2.4) ───────────────────────────

    #[test]
    fn playlists_that_disagree_about_playlist_type_error() {
        let vod = parse_playlist("v-hi", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-PLAYLIST-TYPE:VOD\n#EXTINF:4.0,\ns0.m4s\n#EXT-X-ENDLIST\n");
        let event = parse_playlist("v-lo", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-PLAYLIST-TYPE:EVENT\n#EXTINF:4.0,\ns0.m4s\n");
        let issues = check_playlist_type_endlist(&[vod, event]);
        let found: Vec<&Issue> = errors(&issues).into_iter()
            .filter(|i| i.message.contains("§6.2.4"))
            .collect();
        assert_eq!(found.len(), 1, "{issues:?}");
        assert!(
            found[0].message.contains("VOD") && found[0].message.contains("EVENT"),
            "{}", found[0].message
        );
    }

    #[test]
    fn a_playlist_type_declared_by_only_some_playlists_errors() {
        let typed = parse_playlist("v-hi", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXT-X-PLAYLIST-TYPE:VOD\n#EXTINF:4.0,\ns0.m4s\n#EXT-X-ENDLIST\n");
        let untyped = parse_playlist("v-lo", "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
             #EXTINF:4.0,\ns0.m4s\n#EXT-X-ENDLIST\n");
        let issues = check_playlist_type_endlist(&[typed, untyped]);
        assert!(
            errors(&issues).iter().any(|i| i.message.contains("§6.2.4")),
            "if any playlist declares the tag, all of them MUST: {issues:?}"
        );
    }

    #[test]
    fn playlists_that_agree_about_playlist_type_pass() {
        let content = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-PLAYLIST-TYPE:VOD\n\
             #EXTINF:4.0,\ns0.m4s\n#EXT-X-ENDLIST\n";
        let issues = check_playlist_type_endlist(&[
            parse_playlist("v-hi", content),
            parse_playlist("v-lo", content),
        ]);
        assert!(issues.is_empty(), "{issues:?}");
    }

    // ── CLOSED-CAPTIONS=NONE (§4.4.6.2) ───────────────────────────────────────

    #[test]
    fn closed_captions_none_must_be_declared_by_every_stream_inf() {
        let master = parse_master(
            "#EXTM3U\n\
             #EXT-X-MEDIA:TYPE=CLOSED-CAPTIONS,GROUP-ID=\"cc\",NAME=\"en\",\
             INSTREAM-ID=\"CC1\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,CLOSED-CAPTIONS=NONE\nlo.m3u8\n\
             #EXT-X-STREAM-INF:BANDWIDTH=2000,CLOSED-CAPTIONS=\"cc\"\nhi.m3u8\n",
        );
        let issues = check_stream_inf_consistency(&master);
        let found: Vec<&Issue> = errors(&issues).into_iter()
            .filter(|i| i.message.contains("CLOSED-CAPTIONS=NONE"))
            .collect();
        assert_eq!(found.len(), 1, "{issues:?}");
    }

    #[test]
    fn closed_captions_none_everywhere_passes() {
        let master = parse_master(
            "#EXTM3U\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1000,CLOSED-CAPTIONS=NONE\nlo.m3u8\n\
             #EXT-X-STREAM-INF:BANDWIDTH=2000,CLOSED-CAPTIONS=NONE\nhi.m3u8\n",
        );
        assert!(
            !check_stream_inf_consistency(&master).iter()
                .any(|i| i.message.contains("CLOSED-CAPTIONS=NONE")),
            "every Variant Stream declares NONE"
        );
    }
}