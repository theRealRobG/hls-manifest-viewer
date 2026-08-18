//! Phase C — measured bitrate and deep bitstream heuristics from segment samples.

use std::collections::HashMap;

use super::context::{AuthoringContext, SegmentSample};
use super::helpers::*;
use super::severity::must;
use crate::utils::mp4_probe::InitSegmentProbe;
use crate::utils::validator::types::{Confidence, Issue, MediaPlaylist, Severity};

/// The track whose media timeline a playlist's segments carry. A segment can hold
/// several tracks, and a timed-metadata one usually runs on its own timescale, so a
/// decode time is only comparable to EXTINF once the right track is picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimelineTrack {
    Video,
    Audio,
}

impl TimelineTrack {
    fn for_playlist(is_video: bool, is_iframe: bool, is_audio: bool) -> Option<Self> {
        if is_video || is_iframe {
            Some(Self::Video)
        } else if is_audio {
            Some(Self::Audio)
        } else {
            None
        }
    }

    fn tfdt(self, sample: &SegmentSample) -> Option<u64> {
        match self {
            Self::Video => sample.video_tfdt,
            Self::Audio => sample.audio_tfdt,
        }
    }

    fn timescale(self, probe: &InitSegmentProbe) -> Option<u32> {
        match self {
            Self::Video => probe.video_timescale(),
            Self::Audio => probe.audio_timescale(),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
        }
    }
}

/// Playlist duration from the start of segment `first` to the start of segment `last`.
///
/// The sum comes from the playlist rather than from the sampled segments: deep samples
/// stride across an asset, so adding their EXTINF values measures a handful of segments
/// while a decode-time delta measures every segment between them.
///
/// `None` when the indices do not describe an uninterrupted span — EXT-X-DISCONTINUITY
/// restarts the media timeline, which leaves the two sides of it incomparable.
fn extinf_span(pl: &MediaPlaylist, first: usize, last: usize) -> Option<f64> {
    if last <= first || last > pl.segments.len() {
        return None;
    }
    let span = &pl.segments[first..last];
    let resets = span.iter().skip(1).chain(pl.segments.get(last));
    if resets.into_iter().any(|s| s.discontinuity) {
        return None;
    }
    Some(span.iter().map(|s| s.duration).sum())
}

/// "3 sampled segment(s) (#0, #4, #8)" — findings from a deep sample name the segments
/// they were read from, so a report can be checked against the media it came from.
fn describe_segments(indices: &[usize]) -> String {
    let list = indices
        .iter()
        .map(|i| format!("#{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{} sampled segment(s) ({list})", indices.len())
}

/// Measured bit rate over a set of sampled segments.
#[derive(Debug, Clone, Copy, Default)]
struct MeasuredRate {
    avg: f64,
    peak: f64,
    duration: f64,
}

fn measure(samples: &[&SegmentSample]) -> MeasuredRate {
    let bytes: usize = samples.iter().map(|s| s.bytes).sum();
    let duration: f64 = samples.iter().map(|s| s.extinf_s).sum();
    if duration <= 0.0 {
        return MeasuredRate::default();
    }
    MeasuredRate {
        avg: (bytes as f64 * 8.0) / duration,
        peak: samples
            .iter()
            .filter(|s| s.extinf_s > 0.0)
            .map(|s| (s.bytes as f64 * 8.0) / s.extinf_s)
            .fold(0.0_f64, f64::max),
        duration,
    }
}

/// Which part of a playlist a deep sample covers.
struct SampleWindow {
    sampled: usize,
    total: usize,
    first: usize,
    last: usize,
}

/// Deviation wide enough to blame on the declaration rather than on sampling variance.
const GROSS_RATE_DEVIATION: f64 = 0.50;

impl SampleWindow {
    fn new(samples: &[&SegmentSample], total: usize) -> Self {
        Self {
            sampled: samples.len(),
            total,
            first: samples.first().map(|s| s.segment_index).unwrap_or(0),
            last: samples.last().map(|s| s.segment_index).unwrap_or(0),
        }
    }

    /// Most of the playlist was actually measured.
    fn covers_playlist(&self) -> bool {
        self.total == 0 || self.sampled * 2 >= self.total
    }

    /// Samples stride across the asset instead of clustering at its start.
    fn spans_playlist(&self) -> bool {
        self.total == 0 || (self.last + 1 - self.first) * 2 >= self.total
    }

    fn describe(&self) -> String {
        if self.total == 0 || self.sampled >= self.total {
            format!(" (sampled {} segment(s))", self.sampled)
        } else {
            format!(
                " (sampled {} of {} segments, #{}–#{})",
                self.sampled, self.total, self.first, self.last
            )
        }
    }
}

/// A declared rate can only be called wrong when the sample stands in for the asset: a short
/// prefix, or a measurement missing the audio it is played with, is reported as a warning.
fn rate_severity(window: &SampleWindow, audio_accounted: bool, deviation: f64) -> Severity {
    if !audio_accounted {
        Severity::Warn
    } else if window.covers_playlist()
        || (window.spans_playlist() && deviation > GROSS_RATE_DEVIATION)
    {
        Severity::Error
    } else {
        Severity::Warn
    }
}

/// The audio a variant is played with, which STREAM-INF BANDWIDTH has to cover.
enum AudioContribution {
    /// The variant's own segments already carry audio, or it declares no audio group.
    Muxed,
    /// Measured average rate of the audio rendition this variant would be played with.
    Measured { playlist: String, avg: f64 },
    /// Audio could not be added, for the given reason — the measurement is video-only.
    Unmeasured(&'static str),
}

impl AudioContribution {
    /// Audio bit rate to add to the video measurement. Audio is near-constant-rate, so its
    /// average also stands in for its share of the combination's peak.
    fn addend(&self) -> f64 {
        match self {
            Self::Measured { avg, .. } => *avg,
            _ => 0.0,
        }
    }

    fn is_accounted(&self) -> bool {
        !matches!(self, Self::Unmeasured(_))
    }

    fn note(&self) -> String {
        match self {
            Self::Muxed => String::new(),
            Self::Measured { playlist, avg } => {
                format!(", including audio '{playlist}' at ~{avg:.0} bps")
            }
            Self::Unmeasured(reason) => format!(", video only — {reason}"),
        }
    }
}

/// BANDWIDTH describes the playable combination, so a demuxed variant's measured video rate
/// has to gain the rate of the audio rendition it is played with.
fn audio_contribution(ctx: &AuthoringContext<'_>, pl: &MediaPlaylist) -> AudioContribution {
    // No AUDIO group means the variant is self-contained (muxed TS, or a video-only ladder).
    let Some(group) = pl.audio_group.as_deref() else {
        return AudioContribution::Muxed;
    };
    // Only an init showing a video track and no audio track proves the variant's own segments
    // are video-only: a variant carrying muxed audio can declare an AUDIO group as well, and
    // adding a rendition's audio on top of that would overstate its rate.
    match ctx.probe_for_playlist(&pl.name).map(|e| &e.probe) {
        Some(probe)
            if probe.video_sample_fourcc.is_some() && probe.audio_sample_fourcc.is_none() => {}
        Some(_) => return AudioContribution::Muxed,
        None => {
            return AudioContribution::Unmeasured(
                "the variant's segments could not be confirmed as video-only",
            )
        }
    }
    let in_group: Vec<&MediaPlaylist> = ctx
        .audio_playlists()
        .filter(|a| a.group_id.as_deref() == Some(group))
        .collect();
    let candidates = if in_group.is_empty() {
        ctx.audio_playlists().collect()
    } else {
        in_group
    };
    for audio in candidates {
        let samples: Vec<&SegmentSample> = ctx
            .segment_samples
            .iter()
            .filter(|s| s.playlist_name == audio.name)
            .collect();
        let rate = measure(&samples);
        if rate.duration > 0.0 {
            return AudioContribution::Measured {
                playlist: audio.name.clone(),
                avg: rate.avg,
            };
        }
    }
    AudioContribution::Unmeasured("no audio segments were sampled for the variant's AUDIO group")
}

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    if !ctx.deep_checks {
        return issues;
    }
    if ctx.segment_samples.is_empty() {
        issues.push(author_info(
            "1.26",
            "Deep checks enabled but no media segments were sampled",
        ));
        return issues;
    }

    let mut by_playlist: HashMap<&str, Vec<&super::context::SegmentSample>> = HashMap::new();
    for s in ctx.segment_samples {
        by_playlist
            .entry(s.playlist_name.as_str())
            .or_default()
            .push(s);
    }

    for (name, mut samples) in by_playlist {
        samples.sort_by_key(|s| s.segment_index);
        let pl = ctx.playlists.iter().find(|p| p.name == name);
        let is_vod = pl.is_some_and(|p| {
            p.has_endlist || p.playlist_type.as_deref() == Some("VOD")
        });
        let is_iframe = pl.is_some_and(|p| p.is_iframe) || samples.iter().any(|s| s.is_iframe_playlist);
        let is_video = pl.is_some_and(|p| p.media_type == "VIDEO" && !p.is_iframe);
        let is_audio = pl.is_some_and(|p| p.media_type == "AUDIO");

        let rate = measure(&samples);
        if rate.duration <= 0.0 {
            continue;
        }
        let total_dur = rate.duration;

        if let Some(pl) = pl {
            let audio = if is_video {
                audio_contribution(ctx, pl)
            } else {
                AudioContribution::Muxed
            };
            let window = SampleWindow::new(&samples, pl.segments.len());
            let sample_note = format!("{}{}", window.describe(), audio.note());
            let measured_avg = rate.avg + audio.addend();
            let peak = rate.peak + audio.addend();
            let mut bandwidth_flagged = false;

            // Every rate below is measured over the sampled window rather than over the
            // whole asset, so each finding records that its evidence is a sample however
            // wide the deviation looks. `rate_severity` has already weighed how much of the
            // playlist was read, so the severity it chose stands.

            // §1.26 / 1.28 — average vs AVERAGE-BANDWIDTH
            if let Some(avg) = pl.average_bandwidth {
                let declared = avg as f64;
                let deviation = (measured_avg - declared).abs() / declared;
                if is_vod {
                    if deviation > 0.10 {
                        issues.push(
                            author_issue(
                                rate_severity(&window, audio.is_accounted(), deviation),
                                "1.26",
                                format!(
                                    "'{name}' measured avg {measured_avg:.0} bps is outside ±10% of AVERAGE-BANDWIDTH {avg}{sample_note}"
                                ),
                            )
                            .with_confidence(Confidence::Sampled),
                        );
                    }
                } else if measured_avg > declared * 1.10 {
                    // Live: sampled window is short vs ~1h — Warn instead of Error
                    issues.push(
                        author_warn(
                            "1.28",
                            format!(
                                "'{name}' sampled avg {measured_avg:.0} bps exceeds 110% of AVERAGE-BANDWIDTH {avg}{sample_note}"
                            ),
                        )
                        .with_confidence(Confidence::Sampled),
                    );
                }
            }

            // §1.27 / 1.29 — peak vs BANDWIDTH
            if let Some(bw) = pl.bandwidth {
                let declared = bw as f64;
                let deviation = (peak - declared).abs() / declared;
                if is_vod {
                    // Peak must be within 10% of BANDWIDTH for VOD — typically peak ≤ declared
                    if peak > declared * 1.10 {
                        bandwidth_flagged = true;
                        issues.push(
                            author_issue(
                                rate_severity(&window, audio.is_accounted(), deviation),
                                "1.27",
                                format!(
                                    "'{name}' measured peak {peak:.0} bps exceeds BANDWIDTH {bw} by >10%{sample_note}"
                                ),
                            )
                            .with_confidence(Confidence::Sampled),
                        );
                    } else if peak < declared * 0.90 {
                        // The asset's real peak may sit outside the sampled window, so an
                        // apparently over-declared BANDWIDTH is only informational.
                        issues.push(
                            author_info(
                                "1.27",
                                format!(
                                    "'{name}' measured peak {peak:.0} bps is >10% below BANDWIDTH {bw} — the peak may be elsewhere in the asset{sample_note}"
                                ),
                            )
                            .with_confidence(Confidence::Sampled),
                        );
                    }
                } else if peak > declared * 1.25 {
                    bandwidth_flagged = true;
                    issues.push(
                        author_issue(
                            rate_severity(&window, audio.is_accounted(), deviation),
                            "1.29",
                            format!(
                                "'{name}' measured peak {peak:.0} bps exceeds 125% of BANDWIDTH {bw}{sample_note}"
                            ),
                        )
                        .with_confidence(Confidence::Sampled),
                    );
                }
            }

            // §1.30 — VOD peak SHOULD be ≤ 200% of average bit rate
            if is_vod && measured_avg > 0.0 && peak > measured_avg * 2.0 {
                issues.push(
                    author_warn(
                        "1.30",
                        format!(
                            "'{name}' peak {peak:.0} bps is more than 200% of measured avg {measured_avg:.0} bps{sample_note}"
                        ),
                    )
                    .with_confidence(Confidence::Sampled),
                );
            }

            // §9.13 — BANDWIDTH must cover the peak of the playable combination
            if let Some(bw) = pl.bandwidth {
                if !bandwidth_flagged && peak > bw as f64 * 1.01 {
                    issues.push(
                        author_warn(
                            "9.13",
                            format!(
                                "'{name}' measured peak {peak:.0} bps exceeds declared BANDWIDTH {bw}{sample_note}"
                            ),
                        )
                        .with_confidence(Confidence::Sampled),
                    );
                }
            }

            // §6.9 — I-frame peak bit rate calculation (report measured)
            if is_iframe {
                if let Some(bw) = pl.bandwidth {
                    if peak > bw as f64 * 1.10 {
                        issues.push(
                            author_warn(
                                "6.9",
                                format!(
                                    "I-frame '{name}' measured peak {peak:.0} bps exceeds BANDWIDTH {bw}{sample_note}"
                                ),
                            )
                            .with_confidence(Confidence::Sampled),
                        );
                    }
                }
            }
        }

        // NAL scanning cannot see inside encrypted samples, so IDR heuristics there would
        // only invent findings.
        let encrypted = samples.iter().any(|s| s.has_senc)
            || pl.is_some_and(|p| {
                p.encryption_methods
                    .iter()
                    .any(|m| !m.eq_ignore_ascii_case("NONE"))
            })
            || ctx.probe_for_playlist(name).is_some_and(|e| {
                e.probe.had_encrypted_sample_entry
                    || e.probe.has_tenc
                    || e.probe.scheme_type.is_some()
            });

        if is_video && encrypted {
            issues.push(author_info(
                "7.4",
                format!(
                    "'{name}' samples are encrypted — IDR placement (§7.4) and key-frame interval (§1.13) cannot be read from segment bytes"
                ),
            ));
        } else if is_video {
            // §1.13 — key frames SHOULD be present every two seconds. A CRA opens a GOP
            // just as an IDR does, so what is counted is random-access pictures: counting
            // IDRs alone reads an open-GOP encode as having almost no key frames and
            // reports an interval the content does not have.
            let irap_total: usize = samples
                .iter()
                .map(|s| s.irap_count.max(usize::from(s.has_irap_nal_hint)))
                .sum();
            if irap_total > 0 {
                let interval = total_dur / irap_total as f64;
                if interval > 2.5 {
                    // The interval is an average over the sampled segments, so it stands in
                    // for the asset rather than describing all of it.
                    issues.push(
                        author_warn(
                            "1.13",
                            format!(
                                "'{name}' averages one key frame (IRAP) every ~{interval:.1}s over {} sampled segment(s) totalling {total_dur:.1}s, above the ~2s recommendation",
                                samples.len()
                            ),
                        )
                        .with_confidence(Confidence::Sampled),
                    );
                }
            }

            // §7.4 — video segments MUST start with an IDR. A segment holding no IRAP at
            // all cannot be entered on a switch, which is the conclusive violation. A
            // segment that opens on a CRA or BLA is still randomly accessible, so it is
            // reported on its own rather than as a failure of the same weight.
            let mut no_irap: Vec<usize> = Vec::new();
            let mut irap_not_at_start: Vec<usize> = Vec::new();
            let mut open_gop: Vec<usize> = Vec::new();
            let mut open_gop_holds_no_idr = false;
            let mut read_beyond_video_track = false;
            for s in samples.iter().filter(|s| s.looks_like_fmp4 || s.looks_like_ts) {
                if !s.has_irap_nal_hint {
                    no_irap.push(s.segment_index);
                } else if !s.irap_at_start {
                    irap_not_at_start.push(s.segment_index);
                } else if !s.idr_at_start {
                    open_gop.push(s.segment_index);
                    open_gop_holds_no_idr |= !s.has_idr_nal_hint;
                    read_beyond_video_track |= !s.nal_scan_scoped_to_video;
                }
            }
            if !no_irap.is_empty() {
                // A segment with no random-access picture in it is a violation of the
                // segments that were read, so the error stands; what it cannot speak for
                // is the segments the sample skipped.
                issues.push(
                    author_error(
                        "7.4",
                        format!(
                            "'{name}' — no IRAP (IDR, CRA or BLA) NAL found in {}: video segments MUST start with an IDR",
                            describe_segments(&no_irap)
                        ),
                    )
                    .with_confidence(Confidence::Sampled),
                );
            }
            if !irap_not_at_start.is_empty() {
                // Where the first IRAP sits is inferred from sample offsets, which a
                // conforming segment can also fail, so this MUST is reported as a warning.
                issues.push(author_issue_with_confidence(
                    Confidence::Heuristic,
                    must(),
                    "7.4",
                    format!(
                        "'{name}' — the first IRAP sits well past the start of {}; video segments MUST start with an IDR (position read from sample offsets, best-effort)",
                        describe_segments(&irap_not_at_start)
                    ),
                ));
            }
            if !open_gop.is_empty() {
                let idrs = if open_gop_holds_no_idr {
                    " and carry no IDR at all"
                } else {
                    ""
                };
                // Without the video track's sample ranges the scan reads the whole payload,
                // where another track's bytes can pass for NAL syntax.
                let scope = if read_beyond_video_track {
                    " (read from the whole segment payload, so the opening picture could not be tied to the video track)"
                } else {
                    ""
                };
                let confidence = if read_beyond_video_track {
                    Confidence::Heuristic
                } else {
                    Confidence::Sampled
                };
                issues.push(
                    author_warn(
                        "7.4",
                        format!(
                            "'{name}' — {} open on a CRA or BLA rather than an IDR{idrs}; random access works, but §7.4 asks for an IDR{scope}",
                            describe_segments(&open_gop)
                        ),
                    )
                    .with_confidence(confidence),
                );
            }
        }

        // §7.2 — TS continuity counters
        for s in &samples {
            if s.looks_like_ts && s.ts_continuity_ok == Some(false) {
                issues.push(author_error(
                    "7.2",
                    format!(
                        "TS continuity counter discontinuity in '{}[#{}]'",
                        s.playlist_name, s.segment_index
                    ),
                ));
            }
        }

        // §7.3 / 7.1 / 8.1 — the media timeline against the playlist's own durations
        let timeline = TimelineTrack::for_playlist(is_video, is_iframe, is_audio);
        if let (Some(timeline), Some(pl)) = (timeline, pl) {
            let scale = ctx
                .probe_for_playlist(name)
                .and_then(|e| timeline.timescale(&e.probe));
            let points: Vec<_> = samples
                .iter()
                .filter_map(|s| timeline.tfdt(s).map(|t| (*s, t)))
                .collect();

            for w in points.windows(2) {
                let (a, ta) = w[0];
                let (b, tb) = w[1];
                let Some(extinf) = extinf_span(pl, a.segment_index, b.segment_index) else {
                    continue;
                };
                if tb < ta {
                    issues.push(author_error(
                        "7.3",
                        format!(
                            "'{name}' tfdt decreased from {ta} to {tb} between segments {} and {}",
                            a.segment_index, b.segment_index
                        ),
                    ));
                    continue;
                }
                let Some(scale) = scale else {
                    continue;
                };
                let media_dur = (tb - ta) as f64 / scale as f64;
                if extinf > 0.0 && (media_dur - extinf).abs() > 0.5 {
                    issues.push(author_warn(
                        "7.3",
                        format!(
                            "'{name}' tfdt delta {media_dur:.3}s vs EXTINF {extinf:.3}s between segments {}→{}",
                            a.segment_index, b.segment_index
                        ),
                    ));
                }
            }

            // §8.1 — EXTINF durations MUST match the media timeline to within a frame
            match (scale, points.first(), points.last()) {
                (Some(scale), Some(&(first, first_tfdt)), Some(&(last, last_tfdt)))
                    if last_tfdt >= first_tfdt =>
                {
                    if let Some(extinf_sum) =
                        extinf_span(pl, first.segment_index, last.segment_index)
                    {
                        let media_dur = (last_tfdt - first_tfdt) as f64 / scale as f64;
                        let frame = pl.frame_rate.map(|fps| 1.0 / fps).unwrap_or(1.0 / 30.0);
                        if extinf_sum > 0.0 && (extinf_sum - media_dur).abs() > frame + 0.001 {
                            issues.push(author_error(
                                "8.1",
                                format!(
                                    "'{name}' EXTINF total {extinf_sum:.3}s from segment #{} up to #{} vs {} tfdt span {media_dur:.3}s — more than one frame (~{frame:.3}s) apart",
                                    first.segment_index,
                                    last.segment_index,
                                    timeline.as_str(),
                                ),
                            ));
                        }
                    }
                }
                // Decode times were read but the track they belong to has no timescale,
                // so the media timeline cannot be turned into seconds to compare.
                (None, Some(_), _) => issues.push(author_info(
                    "8.1",
                    format!(
                        "'{name}' EXTINF durations were not checked against the media timeline — no {} track timescale in the init segment",
                        timeline.as_str(),
                    ),
                )),
                _ => {}
            }
        }

        // §7.8 / 7.9 — xHE-AAC IPF / APAC ASP (best-effort)
        if is_audio {
            let codecs = pl
                .and_then(|p| p.codecs.as_deref())
                .unwrap_or("")
                .to_ascii_lowercase();
            if codecs.contains("mp4a.40.42") {
                issues.push(author_info(
                    "7.8",
                    format!(
                        "'{name}' is xHE-AAC — verify each segment starts with an Immediate Playout Frame (not fully detectable in-browser)"
                    ),
                ));
            }
            if codecs.contains("apac") {
                // An Audio Synchronization Packet is a bitstream structure inside the APAC
                // payload, not a box or a fourCC. Searching the bytes for the ASCII "asp "
                // found unrelated data and missed real ASPs, so the rule reports what it is
                // rather than guessing.
                issues.push(author_info(
                    "7.9",
                    format!(
                        "'{name}' carries APAC — each segment MUST start with an Audio Synchronization Packet, which cannot be verified in-browser"
                    ),
                ));
            }
        }

        // Missing tfdt on fMP4
        for s in &samples {
            if s.looks_like_fmp4 && !s.has_tfdt {
                issues.push(author_warn(
                    "7.3",
                    format!(
                        "fMP4 sample '{}[#{}]' missing tfdt (best-effort)",
                        s.playlist_name, s.segment_index
                    ),
                ));
            }
        }
    }

    // §4.3 — closed captions MUST be in video media segments when declared
    if let Some(master) = ctx.master {
        let cc_declared = master.variants.iter().any(|v| {
            v.closed_captions
                .as_deref()
                .is_some_and(|c| c != "NONE" && !c.is_empty())
        }) || master
            .media_renditions
            .iter()
            .any(|r| r.media_type == "CLOSED-CAPTIONS");
        if cc_declared {
            let video_samples: Vec<_> = ctx
                .segment_samples
                .iter()
                .filter(|s| {
                    ctx.playlists.iter().any(|p| {
                        p.name == s.playlist_name && p.media_type == "VIDEO" && !p.is_iframe
                    })
                })
                .collect();
            if !video_samples.is_empty()
                && video_samples.iter().all(|s| !s.has_cc_sei_hint)
            {
                // The finding is the absence of a byte pattern in the segments that were
                // read: conforming captions can sit in segments the sample never opened.
                issues.push(
                    author_warn(
                        "4.3",
                        "closed captions declared but no CEA-608/708 SEI (GA94) hint found in deep video samples",
                    )
                    .with_confidence(Confidence::Heuristic),
                );
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::super::context::{InitProbeEntry, ValidateAuthorOptions, WebVttSample};
    use super::*;
    use crate::utils::validator::types::Segment;

    /// A VOD transport-stream rendition of `count` six-second segments, with BANDWIDTH
    /// declared to match what [`ts_sample`] measures so the rate rules stay quiet.
    fn ts_playlist(count: usize) -> MediaPlaylist {
        let mut pl =
            MediaPlaylist::new("video/1280x720".into(), "https://example.com/v.m3u8".into());
        pl.media_type = "VIDEO".into();
        pl.target_duration = 6.0;
        pl.has_endlist = true;
        pl.playlist_type = Some("VOD".into());
        pl.bandwidth = Some(1_000_000);
        pl.average_bandwidth = Some(1_000_000);
        for i in 0..count {
            pl.segments.push(Segment {
                uri: format!("{i}.ts"),
                duration: 6.0,
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

    /// One sampled transport-stream segment. `continuity_ok` is what the TS packet scan
    /// concluded about the segment's continuity counters: `None` means it was not read.
    fn ts_sample(index: usize, continuity_ok: Option<bool>) -> SegmentSample {
        SegmentSample {
            playlist_name: "video/1280x720".into(),
            segment_index: index,
            uri: format!("{index}.ts"),
            extinf_s: 6.0,
            bytes: 750_000,
            looks_like_ts: true,
            has_idr_nal_hint: true,
            idr_at_start: true,
            has_irap_nal_hint: true,
            irap_at_start: true,
            irap_count: 1,
            nal_scan_scoped_to_video: true,
            ts_continuity_ok: continuity_ok,
            ..Default::default()
        }
    }

    /// Deep findings citing exactly `rule`.
    fn rule_issues(samples: &[SegmentSample], rule: &str) -> Vec<Issue> {
        let playlists = vec![ts_playlist(samples.len().max(6))];
        let opts = ValidateAuthorOptions {
            profile: super::super::profile::AuthorProfile::None,
            deep_checks: true,
        };
        let inits: Vec<InitProbeEntry> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(None, &playlists, &opts, &inits, samples, &vtts);
        let needle = format!("§{rule}:");
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains(&needle))
            .collect()
    }

    #[test]
    fn author_7_2_errors_on_a_transport_stream_continuity_break() {
        let samples = vec![ts_sample(0, Some(true)), ts_sample(1, Some(false))];
        let issues = rule_issues(&samples, "7.2");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("video/1280x720[#1]"),
            "the finding should name the segment: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_7_2_stays_quiet_when_the_counters_are_continuous() {
        let samples = vec![ts_sample(0, Some(true)), ts_sample(1, Some(true))];
        assert!(rule_issues(&samples, "7.2").is_empty());
    }

    /// A continuity verdict of `None` is "not read", which is not evidence of a break.
    #[test]
    fn author_7_2_stays_quiet_when_continuity_was_not_measured() {
        let samples = vec![ts_sample(0, None), ts_sample(1, None)];
        assert!(rule_issues(&samples, "7.2").is_empty());
    }

    /// Phase C runs only when the caller asked for deep checks; nothing is sampled
    /// otherwise, so no measured finding can be reported.
    #[test]
    fn deep_checks_are_skipped_unless_requested() {
        let playlists = vec![ts_playlist(6)];
        let samples = vec![ts_sample(0, Some(false))];
        let opts = ValidateAuthorOptions::default();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(None, &playlists, &opts, &inits, &samples, &vtts);
        assert!(check(&ctx).is_empty());
    }
}
