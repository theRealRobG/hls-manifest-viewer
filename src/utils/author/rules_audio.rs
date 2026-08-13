//! Apple Authoring Spec §2 — Audio (playlist-visible).

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(master) = ctx.master else {
        return issues;
    };

    let audio_tokens: Vec<String> = master
        .variants
        .iter()
        .filter(|v| !v.is_iframe)
        .filter_map(|v| v.codecs.as_ref())
        .flat_map(|c| {
            codec_tokens(c)
                .into_iter()
                .filter(|t| {
                    let l = t.to_ascii_lowercase();
                    l.starts_with("mp4a")
                        || is_ac3(t)
                        || is_ec3(t)
                        || is_apac(t)
                        || l.starts_with("ac-4")
                        || l.starts_with("opus")
                })
                .map(|s| s.to_string())
        })
        .collect();

    let media_audio = &master.media_renditions;
    let audio_renditions: Vec<_> = media_audio
        .iter()
        .filter(|r| r.media_type == "AUDIO")
        .collect();

    // §2.2 / 2.5 / 2.29 — allow-list (playlist-level soft)
    for tok in &audio_tokens {
        let l = tok.to_ascii_lowercase();
        let ok = l.starts_with("mp4a.40.")
            || is_ac3(tok)
            || is_ec3(tok)
            || is_apac(tok)
            || l.starts_with("ac-4")
            || l == "mp4a.40.2";
        if !ok && !l.starts_with("mp4a") {
            issues.push(author_warn(
                "2.2",
                format!("unusual audio CODECS token '{tok}'"),
            ));
        }
    }

    // §2.3 — stereo AAC-LC / HE-AAC MUST exist (*)
    if !ctx.policy.is_exempt("2.3") {
        let has_stereo_aac = audio_tokens.iter().any(|t| is_aac_lc_family(t))
            || audio_renditions.iter().any(|r| {
                r.channels
                    .as_deref()
                    .is_some_and(|c| c.starts_with('2') || c == "2")
            });
        // If there is any audio at all, require stereo AAC presence in CODECS
        if !audio_tokens.is_empty() || !audio_renditions.is_empty() {
            let has_aac = audio_tokens.iter().any(|t| is_aac_lc_family(t));
            if !has_aac && !has_stereo_aac {
                issues.push(author_error(
                    "2.3",
                    "stereo AAC-LC / HE-AACv1 / HE-AACv2 MUST be present for compatibility",
                ));
            } else if !has_aac {
                issues.push(author_warn(
                    "2.3",
                    "AUDIO CHANNELS suggest stereo but no AAC CODECS token found on variants",
                ));
            }
        }
    }

    // §2.4 — HE-AAC with declared BW > 64 kbps → Warn
    for v in master.variants.iter().filter(|v| !v.is_iframe) {
        if let (Some(codecs), Some(bw)) = (&v.codecs, v.average_bandwidth.or(v.bandwidth)) {
            // Only apply when the variant looks audio-only or when audio is muxed — use audio portion
            let audio = codec_tokens(codecs)
                .into_iter()
                .find(|t| is_he_aac(t));
            if audio.is_some() && v.resolution.is_none() && bw > 64_000 {
                issues.push(author_warn(
                    "2.4",
                    format!(
                        "HE-AAC audio-only variant '{}' declares BANDWIDTH {bw} > 64 kbps",
                        v.uri
                    ),
                ));
            }
        }
    }

    // §2.6 — if ec-3 present, ac-3 MUST also exist (*)
    if !ctx.policy.is_exempt("2.6") {
        let has_ec3 = audio_tokens.iter().any(|t| is_ec3(t));
        let has_ac3 = audio_tokens.iter().any(|t| is_ac3(t));
        if has_ec3 && !has_ac3 {
            issues.push(author_error(
                "2.6",
                "ec-3/ec+3 present but no ac-3 compatibility audio",
            ));
        }
    }

    // §2.12–2.14, 2.27–2.28 — DVS / speech CHARACTERISTICS
    for r in &audio_renditions {
        let chars = r.characteristics.as_deref().unwrap_or("");
        let is_dvs = chars.contains("public.accessibility.describes-video")
            || chars.contains("describes-video");
        let is_speech = chars.contains("public.accessibility.transcribes-spoken-dialog")
            || chars.contains("easy-to-read");
        if is_dvs || is_speech {
            if !r.autoselect {
                issues.push(author_warn(
                    "2.13",
                    format!(
                        "accessibility audio '{}' SHOULD have AUTOSELECT=YES",
                        r.name
                    ),
                ));
            }
            if r.language.is_none() {
                issues.push(author_error(
                    "2.14",
                    format!("accessibility audio '{}' MUST have LANGUAGE", r.name),
                ));
            }
            if r.name.is_empty() {
                issues.push(author_error(
                    "2.12",
                    "accessibility audio missing NAME",
                ));
            }
        }
    }

    // §2.15 — alt/DVS duration ≈ primary (VOD)
    {
        let primary = ctx.audio_playlists().find(|_p| {
            // heuristic: first non-DVS
            true
        });
        if let Some(primary) = primary {
            if primary.has_endlist {
                let primary_dur = playlist_duration_s(primary);
                for pl in ctx.audio_playlists() {
                    if pl.url == primary.url {
                        continue;
                    }
                    if !pl.has_endlist {
                        continue;
                    }
                    let d = playlist_duration_s(pl);
                    if primary_dur > 0.0 && (d - primary_dur).abs() > 1.0 {
                        issues.push(author_warn(
                            "2.15",
                            format!(
                                "audio playlist '{}' duration {d:.1}s differs from primary {primary_dur:.1}s",
                                pl.name
                            ),
                        ));
                    }
                }
            }
        }
    }

    // §2.26 — non-Dolby multichannel → suggest AAC multichannel
    for r in &audio_renditions {
        if let Some(ch) = &r.channels {
            let n: u32 = ch
                .split('/')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if n > 2 {
                let has_dolby = audio_tokens.iter().any(|t| is_ac3(t) || is_ec3(t) || t.to_ascii_lowercase().starts_with("ac-4"));
                let has_aac_mc = audio_tokens.iter().any(|t| is_aac_lc_family(t));
                if !has_dolby && !has_aac_mc {
                    issues.push(author_warn(
                        "2.26",
                        format!(
                            "multichannel audio '{}' without Dolby or AAC multichannel CODECS",
                            r.name
                        ),
                    ));
                }
            }
        }
    }

    // §2.31 — APAC immersive → stereo AAC SHOULD exist
    if audio_tokens.iter().any(|t| is_apac(t)) {
        let has_stereo_aac = audio_tokens.iter().any(|t| is_aac_lc_family(t));
        if !has_stereo_aac {
            issues.push(author_warn(
                "2.31",
                "APAC immersive present; stereo AAC SHOULD also exist",
            ));
        }
    }

    // Phase B loudness / ludt
    for entry in ctx.init_probes {
        if entry.probe.audio_sample_fourcc.is_some() && !entry.probe.has_ludt {
            // §2.19 SHOULD include loudness — Warn when audio init lacks ludt
            issues.push(author_warn(
                "2.19",
                format!("audio init '{}' missing ludt loudness box", entry.uri),
            ));
        }
    }

    issues
}
