//! Phase C — measured bitrate and deep bitstream heuristics from segment samples.

use std::collections::HashMap;

use super::context::{AuthoringContext, SegmentSample};
use super::helpers::*;
use crate::utils::validator::types::{Issue, MediaPlaylist, Severity};

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

            // §1.26 / 1.28 — average vs AVERAGE-BANDWIDTH
            if let Some(avg) = pl.average_bandwidth {
                let declared = avg as f64;
                let deviation = (measured_avg - declared).abs() / declared;
                if is_vod {
                    if deviation > 0.10 {
                        issues.push(author_issue(
                            rate_severity(&window, audio.is_accounted(), deviation),
                            "1.26",
                            format!(
                                "'{name}' measured avg {measured_avg:.0} bps is outside ±10% of AVERAGE-BANDWIDTH {avg}{sample_note}"
                            ),
                        ));
                    }
                } else if measured_avg > declared * 1.10 {
                    // Live: sampled window is short vs ~1h — Warn instead of Error
                    issues.push(author_warn(
                        "1.28",
                        format!(
                            "'{name}' sampled avg {measured_avg:.0} bps exceeds 110% of AVERAGE-BANDWIDTH {avg}{sample_note}"
                        ),
                    ));
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
                        issues.push(author_issue(
                            rate_severity(&window, audio.is_accounted(), deviation),
                            "1.27",
                            format!(
                                "'{name}' measured peak {peak:.0} bps exceeds BANDWIDTH {bw} by >10%{sample_note}"
                            ),
                        ));
                    } else if peak < declared * 0.90 {
                        // The asset's real peak may sit outside the sampled window, so an
                        // apparently over-declared BANDWIDTH is only informational.
                        issues.push(author_info(
                            "1.27",
                            format!(
                                "'{name}' measured peak {peak:.0} bps is >10% below BANDWIDTH {bw} — the peak may be elsewhere in the asset{sample_note}"
                            ),
                        ));
                    }
                } else if peak > declared * 1.25 {
                    bandwidth_flagged = true;
                    issues.push(author_issue(
                        rate_severity(&window, audio.is_accounted(), deviation),
                        "1.29",
                        format!(
                            "'{name}' measured peak {peak:.0} bps exceeds 125% of BANDWIDTH {bw}{sample_note}"
                        ),
                    ));
                }
            }

            // §1.30 — VOD peak SHOULD be ≤ 200% of average bit rate
            if is_vod && measured_avg > 0.0 && peak > measured_avg * 2.0 {
                issues.push(author_warn(
                    "1.30",
                    format!(
                        "'{name}' peak {peak:.0} bps is more than 200% of measured avg {measured_avg:.0} bps{sample_note}"
                    ),
                ));
            }

            // §9.13 — BANDWIDTH must cover the peak of the playable combination
            if let Some(bw) = pl.bandwidth {
                if !bandwidth_flagged && peak > bw as f64 * 1.01 {
                    issues.push(author_warn(
                        "9.13",
                        format!(
                            "'{name}' measured peak {peak:.0} bps exceeds declared BANDWIDTH {bw}{sample_note}"
                        ),
                    ));
                }
            }

            // §6.9 — I-frame peak bit rate calculation (report measured)
            if is_iframe {
                if let Some(bw) = pl.bandwidth {
                    if peak > bw as f64 * 1.10 {
                        issues.push(author_warn(
                            "6.9",
                            format!(
                                "I-frame '{name}' measured peak {peak:.0} bps exceeds BANDWIDTH {bw}{sample_note}"
                            ),
                        ));
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
                    "'{name}' samples are encrypted — IDR placement (§7.4) and IDR interval (§1.13) cannot be read from segment bytes"
                ),
            ));
        } else if is_video {
            // §1.13 — IDRs SHOULD be present every ~2 seconds
            let idr_total: usize = samples
                .iter()
                .map(|s| s.idr_count.max(usize::from(s.has_idr_nal_hint)))
                .sum();
            if idr_total > 0 {
                let interval = total_dur / idr_total as f64;
                if interval > 2.5 {
                    issues.push(author_warn(
                        "1.13",
                        format!(
                            "'{name}' estimated IDR interval ~{interval:.1}s exceeds ~2s recommendation (deep sample)"
                        ),
                    ));
                }
            }

            // §7.4 — video segments MUST start with an IDR
            let mut no_idr: Vec<usize> = Vec::new();
            for s in samples.iter().filter(|s| s.looks_like_fmp4 || s.looks_like_ts) {
                if !s.has_idr_nal_hint {
                    no_idr.push(s.segment_index);
                } else if !s.idr_at_start {
                    issues.push(author_error(
                        "7.4",
                        format!(
                            "video segment '{}[#{}]' has IDR but not near segment start",
                            s.playlist_name, s.segment_index
                        ),
                    ));
                }
            }
            if !no_idr.is_empty() {
                let list = no_idr
                    .iter()
                    .map(|i| format!("#{i}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                issues.push(author_warn(
                    "7.4",
                    format!(
                        "'{name}' — no IDR NAL detected at the start of {} sampled segment(s) ({list}, best-effort)",
                        no_idr.len()
                    ),
                ));
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

        // §7.3 / 7.1 — fMP4 tfdt continuity across contiguous samples
        let tfdt_pairs: Vec<_> = samples
            .iter()
            .filter_map(|s| s.tfdt_base_media_decode_time.map(|t| (s, t)))
            .collect();
        for w in tfdt_pairs.windows(2) {
            let (a, ta) = w[0];
            let (b, tb) = w[1];
            if b.segment_index != a.segment_index + 1 {
                continue;
            }
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
            // Compare EXTINF ratio to tfdt delta ratio when 3+ points unavailable —
            // with timescale from matching init, convert to seconds.
            if let Some(scale) = ctx
                .probe_for_playlist(name)
                .and_then(|e| e.probe.timescale.or(e.probe.movie_timescale))
            {
                let media_dur = (tb - ta) as f64 / scale as f64;
                let extinf = a.extinf_s;
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
        }

        // §8.1 — sum EXTINF vs media timeline within ~1 frame
        if let Some(scale) = ctx
            .probe_for_playlist(name)
            .and_then(|e| e.probe.timescale.or(e.probe.movie_timescale))
        {
            if let (Some(first), Some(last)) = (tfdt_pairs.first(), tfdt_pairs.last()) {
                if last.0.segment_index > first.0.segment_index {
                    let media_dur = (last.1 - first.1) as f64 / scale as f64;
                    // Sum EXTINF of segments from first inclusive through last exclusive
                    // (tfdt spans the media of intervening segments ending at last's start)
                    let extinf_sum: f64 = samples
                        .iter()
                        .filter(|s| {
                            s.segment_index >= first.0.segment_index
                                && s.segment_index < last.0.segment_index
                        })
                        .map(|s| s.extinf_s)
                        .sum();
                    let frame = pl
                        .and_then(|p| p.frame_rate)
                        .map(|fps| 1.0 / fps)
                        .unwrap_or(1.0 / 30.0);
                    if extinf_sum > 0.0 && (extinf_sum - media_dur).abs() > frame + 0.001 {
                        issues.push(author_error(
                            "8.1",
                            format!(
                                "'{name}' EXTINF sum {extinf_sum:.3}s vs tfdt span {media_dur:.3}s exceeds one frame (~{frame:.3}s)",
                            ),
                        ));
                    }
                }
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
                let any_asp = samples.iter().any(|s| s.has_asp_hint);
                if !any_asp {
                    issues.push(author_warn(
                        "7.9",
                        format!(
                            "'{name}' APAC samples — no ASP marker hint detected (best-effort)"
                        ),
                    ));
                }
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
                issues.push(author_warn(
                    "4.3",
                    "closed captions declared but no CEA-608/708 SEI (GA94) hint found in deep video samples",
                ));
            }
        }
    }

    issues
}
