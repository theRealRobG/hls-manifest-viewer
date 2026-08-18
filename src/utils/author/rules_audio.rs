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
        } else if !probe.has_ludt {
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
    }

    // §2.25 — xHE-AAC / ALAC / FLAC / APAC MUST be fMP4. An init that failed to parse has
    // no sample entry to name its codec, so the requirement is read from the CODECS of the
    // playlist(s) declaring the init. The init is not necessarily an audio-only one: a
    // muxed variant carries these codecs in the same init as its video.
    for entry in ctx.init_probes {
        if entry.probe.looks_like_fmp4_init() {
            continue;
        }
        let declared = declared_codecs_for_init(ctx, entry);
        if declared.fmp4_only_audio.is_empty() {
            continue;
        }
        issues.push(author_error(
            "2.25",
            format!(
                "init '{}' could not be parsed as fMP4 ({} {}, which MUST use fMP4)",
                entry.uri,
                declared.fmp4_only_audio.declaring_phrase(),
                declared.fmp4_only_audio.tokens()
            ),
        ));
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

    // ── §2.19 APAC loudness, §2.25 container ─────────────────────────────────

    /// A demuxed ladder: the AUDIO group's EXT-X-MEDIA carries a URI, which is what moves
    /// the audio out of the variant's own segments and into the group's init.
    const DEMUXED_MASTER: &str = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",LANGUAGE=\"en\",AUTOSELECT=YES,DEFAULT=YES,URI=\"a.m3u8\"\n#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS=\"avc1.4d401f,mp4a.40.42\",FRAME-RATE=30,AUDIO=\"aud\"\nhttps://example.com/v.m3u8\n";

    /// Audio findings citing exactly `rule` when the rules are run over `master_text`.
    /// The citation is followed by a colon, which keeps §2.2 from also matching §2.25
    /// and §2.27.
    fn rule_issues_for_master(
        master_text: &str,
        playlists: &[MediaPlaylist],
        inits: &[InitProbeEntry],
        rule: &str,
    ) -> Vec<Issue> {
        let master = parse_master_playlist("https://example.com/master.m3u8", master_text);
        let opts = ValidateAuthorOptions::default();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), playlists, &opts, inits, &segs, &vtts);
        let needle = format!("§{rule}:");
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains(&needle))
            .collect()
    }

    /// The same, over the demuxed ladder most of these tests are written against.
    fn rule_issues(playlists: &[MediaPlaylist], inits: &[InitProbeEntry], rule: &str) -> Vec<Issue> {
        rule_issues_for_master(DEMUXED_MASTER, playlists, inits, rule)
    }

    /// An APAC audio init, optionally carrying the `ludt` loudness box that §2.19 says
    /// APAC must not use — APAC signals its loudness in the bitstream instead.
    fn apac_init(has_ludt: bool) -> InitProbeEntry {
        InitProbeEntry {
            uri: "https://example.com/apac-init.mp4".into(),
            byterange: None,
            playlist_names: vec!["audio/English (aud)".into()],
            media_types: vec!["AUDIO".into()],
            probe: crate::utils::mp4_probe::InitSegmentProbe {
                major_brand: Some("iso6".into()),
                audio_sample_fourcc: Some("apac".into()),
                has_ludt,
                ..Default::default()
            },
        }
    }

    #[test]
    fn author_2_19_errors_when_an_apac_init_carries_a_ludt_box() {
        let issues = rule_issues(&[], &[apac_init(true)], "2.19");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("MUST NOT include a ludt"),
            "got: {}",
            issues[0].message
        );
    }

    /// The `ludt` requirement runs the other way for every other codec, so an APAC init
    /// without one is what the rule asks for rather than the warning AAC would earn.
    #[test]
    fn author_2_19_accepts_an_apac_init_without_a_ludt_box() {
        assert!(rule_issues(&[], &[apac_init(false)], "2.19").is_empty());
    }

    /// An audio rendition whose segments have `extension`, declaring `codecs`. `map`
    /// writes an EXT-X-MAP, which is what turns a playlist into an fMP4 one.
    fn audio_rendition(codecs: &str, extension: &str, map: bool) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(
            "audio/English (aud)".into(),
            "https://example.com/a.m3u8".into(),
        );
        pl.media_type = "AUDIO".into();
        pl.codecs = Some(codecs.into());
        pl.target_duration = 6.0;
        pl.has_endlist = true;
        pl.playlist_type = Some("VOD".into());
        for i in 0..2 {
            pl.segments.push(crate::utils::validator::types::Segment {
                uri: format!("{i}.{extension}"),
                duration: 6.0,
                title: None,
                pdt: None,
                discontinuity: false,
                byterange: None,
                is_ad: false,
                map_uri: map.then(|| "init.mp4".to_string()),
            });
        }
        pl
    }

    #[test]
    fn author_2_25_errors_when_xhe_aac_is_carried_in_transport_stream() {
        let issues = rule_issues(&[audio_rendition("mp4a.40.42", "ts", false)], &[], "2.25");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("EXT-X-MAP"),
            "got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_2_25_accepts_the_same_codecs_in_fmp4() {
        for codecs in ["mp4a.40.42", "apac", "alac", "flac"] {
            let issues = rule_issues(&[audio_rendition(codecs, "m4s", true)], &[], "2.25");
            assert!(issues.is_empty(), "{codecs}: {issues:?}");
        }
    }

    /// AAC-LC is allowed in a transport stream, so the container rule §2.25 states for the
    /// newer codecs does not reach it.
    #[test]
    fn author_2_25_ignores_aac_lc_in_transport_stream() {
        assert!(rule_issues(&[audio_rendition("mp4a.40.2", "ts", false)], &[], "2.25").is_empty());
    }

    /// An init whose bytes yielded no `ftyp` and no sample entry, so the codec it was
    /// meant to carry can only be read from the playlist declaring it.
    fn unparseable_audio_init() -> InitProbeEntry {
        InitProbeEntry {
            uri: "https://example.com/a-init.mp4".into(),
            byterange: None,
            playlist_names: vec!["audio/English (aud)".into()],
            media_types: vec!["AUDIO".into()],
            probe: crate::utils::mp4_probe::InitSegmentProbe::default(),
        }
    }

    #[test]
    fn author_2_25_errors_when_an_fmp4_only_codecs_init_is_not_fmp4() {
        for codecs in ["mp4a.40.42", "apac", "alac", "fLaC"] {
            let playlists = [audio_rendition(codecs, "m4s", true)];
            let issues = rule_issues(&playlists, &[unparseable_audio_init()], "2.25");
            assert_eq!(issues.len(), 1, "{codecs}: {issues:?}");
            assert_eq!(issues[0].severity, Severity::Error);
            assert!(
                issues[0].message.contains("fMP4") && issues[0].message.contains(codecs),
                "the finding should name the declared codec, got: {}",
                issues[0].message
            );
        }
    }

    #[test]
    fn author_2_25_accepts_an_init_that_parses_as_fmp4() {
        let playlists = [audio_rendition("apac", "m4s", true)];
        assert!(rule_issues(&playlists, &[apac_init(false)], "2.25").is_empty());
    }

    /// AAC-LC needs no fMP4 container, so an init it could not be read from is §1.2's
    /// business rather than §2.25's.
    #[test]
    fn author_2_25_ignores_an_unparseable_init_for_aac_lc() {
        let playlists = [audio_rendition("mp4a.40.2", "m4s", true)];
        assert!(rule_issues(&playlists, &[unparseable_audio_init()], "2.25").is_empty());
    }

    /// A video variant whose STREAM-INF CODECS names the audio codec alongside its own.
    /// `audio_group` is the AUDIO attribute: set on a demuxed ladder, absent when the
    /// variant carries its audio itself.
    fn video_variant(codecs: &str, audio_group: Option<&str>) -> MediaPlaylist {
        let mut pl =
            MediaPlaylist::new("video/1280x720".into(), "https://example.com/v.m3u8".into());
        pl.media_type = "VIDEO".into();
        pl.codecs = Some(codecs.into());
        pl.audio_group = audio_group.map(str::to_string);
        pl.target_duration = 6.0;
        pl.has_endlist = true;
        pl.playlist_type = Some("VOD".into());
        for i in 0..2 {
            pl.segments.push(crate::utils::validator::types::Segment {
                uri: format!("{i}.m4s"),
                duration: 6.0,
                title: None,
                pdt: None,
                discontinuity: false,
                byterange: None,
                is_ad: false,
                map_uri: Some("v-init.mp4".to_string()),
            });
        }
        pl
    }

    /// The video init of `video_variant`, with bytes nothing could be read from.
    fn unparseable_video_init() -> InitProbeEntry {
        InitProbeEntry {
            uri: "https://example.com/v-init.mp4".into(),
            byterange: None,
            playlist_names: vec!["video/1280x720".into()],
            media_types: vec!["VIDEO".into()],
            probe: crate::utils::mp4_probe::InitSegmentProbe::default(),
        }
    }

    /// A demuxed variant's CODECS names the codec of the audio group it points at, and
    /// those samples are in that group's own init. Reading the token off the video init
    /// would fail it under a rule about bytes it never carried.
    #[test]
    fn author_2_25_ignores_the_audio_codec_a_demuxed_variant_declares_for_its_group() {
        let playlists = [video_variant("avc1.640029,mp4a.40.42", Some("aud"))];
        let issues = rule_issues(&playlists, &[unparseable_video_init()], "2.25");
        assert!(
            issues.is_empty(),
            "the xHE-AAC belongs to the audio group's init, not this one: {issues:?}"
        );
    }

    /// With no AUDIO group the variant carries its own audio, so the xHE-AAC it declares
    /// really is in the init that failed to parse.
    #[test]
    fn author_2_25_errors_when_a_muxed_variants_init_is_not_fmp4() {
        let playlists = [video_variant("avc1.640029,mp4a.40.42", None)];
        let issues = rule_issues(&playlists, &[unparseable_video_init()], "2.25");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("mp4a.40.42") && !issues[0].message.contains("audio init"),
            "a muxed init holds video too, so it is not an audio init, got: {}",
            issues[0].message
        );
    }

    /// An `EXT-X-MEDIA:TYPE=AUDIO` with no URI names a group without moving anything into
    /// it: the audio is still in the variant's segments, which is what §8.9 warns about.
    /// The AUDIO attribute alone must not excuse this init from the container rule.
    #[test]
    fn author_2_25_errors_when_a_uri_less_audio_group_leaves_the_audio_muxed() {
        const MUXED_GROUP_MASTER: &str = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",LANGUAGE=\"en\",AUTOSELECT=YES,DEFAULT=YES\n#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS=\"avc1.640029,mp4a.40.42\",FRAME-RATE=30,AUDIO=\"aud\"\nhttps://example.com/v.m3u8\n";
        let playlists = [video_variant("avc1.640029,mp4a.40.42", Some("aud"))];
        let issues = rule_issues_for_master(
            MUXED_GROUP_MASTER,
            &playlists,
            &[unparseable_video_init()],
            "2.25",
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("mp4a.40.42"),
            "got: {}",
            issues[0].message
        );
    }

    /// An audio-only variant may point at the group its own URI serves. It then declares
    /// no RESOLUTION and one audio token, its EXT-X-MAP is the group's init, and standing
    /// down on the AUDIO attribute would leave the init nobody else declares unchecked.
    #[test]
    fn author_2_25_errors_when_an_audio_only_variant_points_at_its_own_group() {
        let mut pl = video_variant("mp4a.40.42", Some("aud"));
        pl.name = "audio/English (aud)".into();
        let inits = [InitProbeEntry {
            uri: "https://example.com/a-init.mp4".into(),
            byterange: None,
            playlist_names: vec![pl.name.clone()],
            media_types: vec!["AUDIO".into()],
            probe: crate::utils::mp4_probe::InitSegmentProbe::default(),
        }];
        let issues = rule_issues(&[pl], &inits, "2.25");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("mp4a.40.42"),
            "got: {}",
            issues[0].message
        );
    }

    /// I-frame playlists carry trick-play video alone, and often share the variant's
    /// EXT-X-MAP. A packager that copies the whole CODECS string onto the
    /// EXT-X-I-FRAME-STREAM-INF must not thereby hand the video init an audio token the
    /// demuxed variant referencing the same init was already excused from.
    ///
    /// Trick-play is recognised from either signal, and this playlist carries only its
    /// own EXT-X-I-FRAMES-ONLY tag — the master-side EXT-X-I-FRAME-STREAM-INF is what
    /// §1.2's counterpart test is written against.
    #[test]
    fn author_2_25_ignores_an_audio_token_copied_onto_an_iframe_playlist() {
        let video = video_variant("avc1.640029,mp4a.40.42", Some("aud"));
        let mut iframe = video_variant("avc1.640029,mp4a.40.42", None);
        iframe.name = "iframe/1280x720".into();
        iframe.iframes_only = true;
        let inits = [InitProbeEntry {
            uri: "https://example.com/v-init.mp4".into(),
            byterange: None,
            playlist_names: vec![video.name.clone(), iframe.name.clone()],
            media_types: vec!["VIDEO".into()],
            probe: crate::utils::mp4_probe::InitSegmentProbe::default(),
        }];
        let issues = rule_issues(&[video, iframe], &inits, "2.25");
        assert!(
            issues.is_empty(),
            "the xHE-AAC is in the audio group's init, and trick-play holds no audio: {issues:?}"
        );
    }

    /// A variant that forgot its video token still has a RESOLUTION, which is what tells
    /// it apart from an audio-only rendition. Reading it as audio-only would put the
    /// group's xHE-AAC back on the video init the group was demuxed out of.
    #[test]
    fn author_2_25_reads_a_variants_resolution_before_calling_it_audio_only() {
        let mut pl = video_variant("mp4a.40.42", Some("aud"));
        pl.resolution = Some("1920x1080".into());
        let issues = rule_issues(&[pl], &[unparseable_video_init()], "2.25");
        assert!(
            issues.is_empty(),
            "a variant with a RESOLUTION carries video, whatever its CODECS forgot: {issues:?}"
        );
    }

    /// Two rungs sharing one init both declare the codec it failed to parse, so the
    /// finding names both — and a list of two takes the plural verb.
    #[test]
    fn author_2_25_agrees_its_verb_with_the_playlists_it_names() {
        let low = video_variant("avc1.640029,mp4a.40.42", None);
        let mut high = video_variant("avc1.640029,mp4a.40.42", None);
        high.name = "video/1920x1080".into();
        let inits = [InitProbeEntry {
            uri: "https://example.com/v-init.mp4".into(),
            byterange: None,
            playlist_names: vec![low.name.clone(), high.name.clone()],
            media_types: vec!["VIDEO".into()],
            probe: crate::utils::mp4_probe::InitSegmentProbe::default(),
        }];
        let issues = rule_issues(&[low, high], &inits, "2.25");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0]
                .message
                .contains("'video/1280x720', 'video/1920x1080' declare mp4a.40.42")
                && !issues[0].message.contains("declares"),
            "two playlists take 'declare', got: {}",
            issues[0].message
        );
    }

    /// A finding names its evidence, and the playlists whose tokens were discounted are
    /// not it. Only the variant that really declared the codec belongs in the message.
    #[test]
    fn author_2_25_names_only_the_playlist_whose_codec_it_cites() {
        let muxed = video_variant("avc1.640029,mp4a.40.42", None);
        let mut demuxed = video_variant("avc1.640029,mp4a.40.42", Some("aud"));
        demuxed.name = "video/1920x1080".into();
        let inits = [InitProbeEntry {
            uri: "https://example.com/v-init.mp4".into(),
            byterange: None,
            playlist_names: vec![muxed.name.clone(), demuxed.name.clone()],
            media_types: vec!["VIDEO".into()],
            probe: crate::utils::mp4_probe::InitSegmentProbe::default(),
        }];
        let issues = rule_issues(&[muxed, demuxed], &inits, "2.25");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0]
                .message
                .contains("'video/1280x720' declares mp4a.40.42")
                && !issues[0].message.contains("video/1920x1080"),
            "got: {}",
            issues[0].message
        );
    }
}
