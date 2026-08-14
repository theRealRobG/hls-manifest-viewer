//! Apple Authoring Spec §2 — Audio (playlist-visible).

use super::context::AuthoringContext;
use super::helpers::*;
use super::severity::must;
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

    // §2.12 / §2.13 / §2.27 / §2.28 — DVS and speech-intelligibility CHARACTERISTICS
    for r in &audio_renditions {
        let chars = r.characteristics.as_deref().unwrap_or("");
        let has_dvs_characteristic = chars.contains("public.accessibility.describes-video");
        let is_dvs = audio_is_dvs(r);
        let enhances_speech = audio_enhances_speech(r);

        // §2.12 — the descriptive-audio characteristic itself, spelled in full.
        if is_dvs && !has_dvs_characteristic {
            issues.push(author_issue(
                must(),
                "2.12",
                format!(
                    "descriptive audio '{}' MUST declare CHARACTERISTICS=\"public.accessibility.describes-video\"",
                    r.name
                ),
            ));
        }
        // §2.13 — DVS is selected by the accessibility preference, not by the picker.
        if is_dvs && !r.autoselect {
            issues.push(author_issue(
                must(),
                "2.13",
                format!("descriptive audio '{}' MUST have AUTOSELECT=YES", r.name),
            ));
        }
        // §2.27 — accessibility audio MUST be language-tagged.
        if (is_dvs || enhances_speech) && r.language.is_none() {
            issues.push(author_issue(
                must(),
                "2.27",
                format!("accessibility audio '{}' MUST have LANGUAGE", r.name),
            ));
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

    // §2.31 — APAC *immersive* audio is what asks for a stereo AAC companion. Plain
    // stereo APAC ("2/BED-2") is a two-channel rendition like any other, so the rule
    // needs an immersive antecedent before it has anything to say.
    if audio_tokens.iter().any(|t| is_apac(t)) {
        let immersive: Vec<&str> = audio_renditions
            .iter()
            .filter(|r| {
                r.channels
                    .as_deref()
                    .is_some_and(apac_channels_are_immersive)
            })
            .map(|r| r.name.as_str())
            .collect();
        let has_stereo_aac = audio_tokens.iter().any(|t| is_aac_lc_family(t));
        if !immersive.is_empty() && !has_stereo_aac {
            issues.push(author_warn(
                "2.31",
                format!(
                    "immersive APAC audio ({}) without a stereo AAC rendition, which SHOULD also be provided",
                    immersive.join(", ")
                ),
            ));
        }
    }

    // Phase B loudness / container from init probes
    for entry in ctx.init_probes {
        let probe = &entry.probe;
        let Some(fourcc) = probe.audio_sample_fourcc.as_deref() else {
            continue;
        };
        let fourcc_l = fourcc.to_ascii_lowercase();
        let is_apac = fourcc_l.starts_with("apac");
        let is_flac = fourcc_l == "flac" || fourcc_l == "alac";
        let is_xhe = entry.playlist_names.iter().any(|name| {
            ctx.playlists.iter().any(|pl| {
                pl.name == *name
                    && pl
                        .codecs
                        .as_deref()
                        .is_some_and(|c| c.to_ascii_lowercase().contains("mp4a.40.42"))
            })
        });

        // §2.19 — ludt SHOULD for fMP4 audio except APAC (SHALL NOT for APAC)
        if is_apac {
            if probe.has_ludt {
                issues.push(author_error(
                    "2.19",
                    format!("APAC init '{}' MUST NOT include a ludt loudness box", entry.uri),
                ));
            }
            // §2.30 — APAC MUST have in-stream loudness/DRC (not fully observable from init)
            issues.push(author_info(
                "2.30",
                format!(
                    "APAC init '{}': verify in-stream loudness/DRC metadata in samples",
                    entry.uri
                ),
            ));
        } else if probe.looks_like_fmp4_init() && !probe.has_ludt {
            issues.push(author_warn(
                "2.19",
                format!("audio init '{}' missing ludt loudness box", entry.uri),
            ));
            // §2.18 / 2.20 — without ludt, dialnorm / AAC loudness SHOULD exist (best-effort note)
            if fourcc_l == "ac-3" || fourcc_l == "ec-3" {
                issues.push(author_warn(
                    "2.20",
                    format!(
                        "Dolby init '{}' lacks ludt; dialnorm SHOULD be present in the bitstream",
                        entry.uri
                    ),
                ));
            } else if fourcc_l == "mp4a" {
                issues.push(author_warn(
                    "2.21",
                    format!(
                        "AAC init '{}' lacks ludt; dialog loudness SHOULD be signaled in the bitstream",
                        entry.uri
                    ),
                ));
            }
        }

        // §2.25 — xHE-AAC / ALAC / FLAC / APAC MUST be fMP4
        if (is_apac || is_flac || is_xhe) && !probe.looks_like_fmp4_init() {
            issues.push(author_error(
                "2.25",
                format!(
                    "codec '{fourcc}' on init '{}' MUST use fMP4 container",
                    entry.uri
                ),
            ));
        }

        // §2.1 — audio SHOULD be elementary or fMP4 (info when we only see odd brands)
        if probe.looks_like_fmp4_init() {
            // satisfied
        }
    }

    // Playlist-level §2.25 when MAP missing for xHE-AAC/APAC/FLAC
    for pl in ctx.audio_playlists() {
        let codecs = pl.codecs.as_deref().unwrap_or("").to_ascii_lowercase();
        let needs_fmp4 = codecs.contains("mp4a.40.42")
            || codecs.contains("apac")
            || codecs.contains("alac")
            || codecs.contains("flac");
        if needs_fmp4 && playlist_looks_like_ts(pl) && !playlist_has_map(pl) {
            issues.push(author_error(
                "2.25",
                format!(
                    "'{}' declares xHE-AAC/APAC/ALAC/FLAC but looks like TS without EXT-X-MAP",
                    pl.name
                ),
            ));
        }
    }

    issues
}

/// Whether an APAC rendition's CHANNELS attribute describes immersive audio.
///
/// The attribute's first parameter counts channels and its second names the spatial
/// components: a channel bed (`BED-n`), isolated audio objects (`ISO-n`) or Ambisonics
/// (`1OA`, `2OA`, `3OA`). Immersive audio is a combination of two or more of those, or a
/// bed wider than stereo; a plain `2/BED-2` rendition is stereo APAC.
fn apac_channels_are_immersive(channels: &str) -> bool {
    let mut parts = channels.split('/');
    let count: u32 = parts
        .next()
        .and_then(|c| c.trim().parse().ok())
        .unwrap_or(0);
    if count > 2 {
        return true;
    }
    let Some(spatial) = parts.next() else {
        return false;
    };
    spatial
        .split('+')
        .map(|id| id.trim().to_ascii_uppercase())
        .any(|id| {
            if let Some(bed) = id.strip_prefix("BED-") {
                return bed.parse::<u32>().is_ok_and(|n| n > 2);
            }
            let ambisonics = id
                .strip_suffix("OA")
                .is_some_and(|order| !order.is_empty() && order.chars().all(|c| c.is_ascii_digit()));
            id.starts_with("ISO-") || ambisonics
        })
}

#[cfg(test)]
mod tests {
    use super::super::context::{
        InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample,
    };
    use super::*;
    use crate::utils::validator::parser::parse_master_playlist;
    use crate::utils::validator::types::{MediaPlaylist, Severity};

    /// Run the audio rules over a hand-written multivariant playlist with the given
    /// EXT-X-MEDIA lines and no media playlists fetched.
    fn master_issues(media: &str) -> Vec<Issue> {
        let content = format!(
            "#EXTM3U\n{media}#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS=\"avc1.4d401f,mp4a.40.2\",FRAME-RATE=30,AUDIO=\"aud\"\nhttps://example.com/v.m3u8\n"
        );
        let master = parse_master_playlist("https://example.com/master.m3u8", &content);
        let opts = ValidateAuthorOptions::default();
        let playlists: Vec<MediaPlaylist> = Vec::new();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        check(&ctx)
    }

    fn find<'a>(issues: &'a [Issue], section: &str) -> Option<&'a Issue> {
        issues.iter().find(|i| i.message.contains(section))
    }

    const DVS: &str = "public.accessibility.describes-video";

    #[test]
    fn apac_channels_tell_immersive_from_stereo() {
        assert!(!apac_channels_are_immersive("2/BED-2"));
        assert!(!apac_channels_are_immersive("2"));
        assert!(apac_channels_are_immersive("12/BED-4+ISO-8"));
        assert!(apac_channels_are_immersive("2/BED-2+ISO-4"));
        assert!(apac_channels_are_immersive("2/3OA"));
        assert!(apac_channels_are_immersive("6/BED-6"));
    }

    /// An APAC-only ladder whose audio rendition declares `channels`.
    fn apac_issues(channels: &str) -> Vec<Issue> {
        let content = format!(
            "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"Immersive\",LANGUAGE=\"en\",AUTOSELECT=YES,CHANNELS=\"{channels}\",URI=\"a.m3u8\"\n#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS=\"hvc1.2.4.L123.B0,apac\",FRAME-RATE=30,AUDIO=\"aud\"\nhttps://example.com/v.m3u8\n"
        );
        let master = parse_master_playlist("https://example.com/master.m3u8", &content);
        let opts = ValidateAuthorOptions::default();
        let playlists: Vec<MediaPlaylist> = Vec::new();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), &playlists, &opts, &inits, &segs, &vtts);
        check(&ctx)
    }

    #[test]
    fn author_2_31_needs_an_immersive_apac_antecedent() {
        assert!(
            find(&apac_issues("2/BED-2"), "§2.31").is_none(),
            "stereo APAC is not the immersive audio §2.31 is about"
        );
        let immersive = apac_issues("12/BED-4+ISO-8");
        let issue = find(&immersive, "§2.31").expect("immersive APAC without stereo AAC");
        assert_eq!(issue.severity, Severity::Warn);
        assert!(issue.message.contains("Immersive"), "{}", issue.message);
    }

    #[test]
    fn author_2_13_makes_dvs_autoselect_an_error() {
        let issues = master_issues(&format!(
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English AD\",LANGUAGE=\"en\",CHARACTERISTICS=\"{DVS}\",URI=\"dvs.m3u8\"\n"
        ));
        let issue = find(&issues, "§2.13").expect("expected §2.13 AUTOSELECT issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(issue.message.contains("AUTOSELECT=YES"));
    }

    #[test]
    fn author_2_27_carries_the_dvs_language_requirement() {
        let issues = master_issues(&format!(
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English AD\",AUTOSELECT=YES,CHARACTERISTICS=\"{DVS}\",URI=\"dvs.m3u8\"\n"
        ));
        let issue = find(&issues, "§2.27").expect("expected §2.27 LANGUAGE issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(find(&issues, "§2.14").is_none(), "§2.14 was the old mislabel");
    }

    #[test]
    fn author_2_12_flags_descriptive_audio_without_the_characteristic() {
        let issues = master_issues(
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English (Audio Description)\",LANGUAGE=\"en\",AUTOSELECT=YES,URI=\"dvs.m3u8\"\n",
        );
        let issue = find(&issues, "§2.12").expect("expected §2.12 CHARACTERISTICS issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(issue.message.contains("public.accessibility.describes-video"));
    }

    #[test]
    fn author_2_12_and_2_13_accept_a_compliant_dvs_rendition() {
        let issues = master_issues(&format!(
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English AD\",LANGUAGE=\"en\",AUTOSELECT=YES,CHARACTERISTICS=\"{DVS}\",URI=\"dvs.m3u8\"\n"
        ));
        assert!(find(&issues, "§2.12").is_none(), "{issues:?}");
        assert!(find(&issues, "§2.13").is_none(), "{issues:?}");
        assert!(find(&issues, "§2.27").is_none(), "{issues:?}");
    }

    #[test]
    fn author_2_13_does_not_apply_to_speech_intelligibility_audio() {
        let issues = master_issues(
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English Dialogue\",LANGUAGE=\"en\",CHARACTERISTICS=\"public.accessibility.enhances-speech-intelligibility\",URI=\"speech.m3u8\"\n",
        );
        assert!(find(&issues, "§2.13").is_none(), "{issues:?}");
        assert!(find(&issues, "§2.12").is_none(), "{issues:?}");
    }

    #[test]
    fn author_2_12_ignores_ordinary_audio_renditions() {
        let issues = master_issues(
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",LANGUAGE=\"en\",AUTOSELECT=YES,DEFAULT=YES,URI=\"en.m3u8\"\n",
        );
        assert!(find(&issues, "§2.12").is_none(), "{issues:?}");
        assert!(find(&issues, "§2.27").is_none(), "{issues:?}");
    }
}
