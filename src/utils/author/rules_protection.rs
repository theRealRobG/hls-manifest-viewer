//! Apple Authoring Spec §13 — Content protection.

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::{Issue, MediaPlaylist};

/// KEYFORMAT that identifies FairPlay Streaming key delivery (§13.3).
const FPS_KEYFORMAT: &str = "com.apple.streamingkeydelivery";

/// One EXT-X-KEY / EXT-X-SESSION-KEY tag. The parsed playlist keeps METHOD and
/// KEYFORMAT as separate sets, so §13.2–13.4 re-read the raw tag to keep the
/// attributes of a single key paired.
struct KeyTag {
    method: String,
    keyformat: Option<String>,
    uri: Option<String>,
    has_iv: bool,
}

impl KeyTag {
    fn keyformat_is_fps(&self) -> bool {
        self.keyformat
            .as_deref()
            .is_some_and(|f| f.eq_ignore_ascii_case(FPS_KEYFORMAT))
    }

    /// `skd:` key URIs are the FPS key-delivery scheme, so they signal FairPlay
    /// intent even when KEYFORMAT is missing or wrong.
    fn uri_is_fps(&self) -> bool {
        self.uri
            .as_deref()
            .is_some_and(|u| u.trim().to_ascii_lowercase().starts_with("skd:"))
    }

    fn signals_fps(&self) -> bool {
        self.keyformat_is_fps() || self.uri_is_fps()
    }

    fn is_active(&self) -> bool {
        !self.method.is_empty() && self.method != "NONE"
    }
}

fn parse_key_tags(raw: &str, tag_prefix: &str) -> Vec<KeyTag> {
    raw.lines()
        .filter_map(|line| line.trim().strip_prefix(tag_prefix))
        .map(|rest| {
            let attrs = crate::utils::validator::parser::parse_attributes(rest);
            KeyTag {
                method: attrs
                    .get("METHOD")
                    .map(|m| m.trim().to_ascii_uppercase())
                    .unwrap_or_default(),
                keyformat: attrs.get("KEYFORMAT").cloned(),
                uri: attrs.get("URI").cloned(),
                has_iv: attrs.contains_key("IV"),
            }
        })
        .collect()
}

/// §13.2 / §13.3 / §13.4 — FairPlay signalling on a single key tag.
fn check_fps_key_tags(source: &str, tags: &[KeyTag], issues: &mut Vec<Issue>) {
    for tag in tags.iter().filter(|t| t.is_active() && t.signals_fps()) {
        if tag.method != "SAMPLE-AES" {
            issues.push(author_error(
                "13.2",
                format!(
                    "'{source}' signals FairPlay but uses METHOD={}; FPS content MUST use METHOD=SAMPLE-AES",
                    tag.method
                ),
            ));
        }
        if !tag.keyformat_is_fps() {
            let found = tag
                .keyformat
                .as_deref()
                .map(|f| format!("'{f}'"))
                .unwrap_or_else(|| "absent".to_string());
            issues.push(author_error(
                "13.3",
                format!(
                    "'{source}' uses an skd: FairPlay key URI but KEYFORMAT is {found}; \
                     FPS content MUST use KEYFORMAT=\"{FPS_KEYFORMAT}\""
                ),
            ));
        }
        if tag.has_iv {
            issues.push(author_warn(
                "13.4",
                format!(
                    "'{source}' sets IV on a FairPlay key; the IV attribute SHOULD NOT be used \
                     with FPS unless necessary for interoperability"
                ),
            ));
        }
    }
}

fn active_methods(pl: &MediaPlaylist) -> Vec<String> {
    let mut methods: Vec<String> = pl
        .encryption_methods
        .iter()
        .map(|m| m.trim().to_ascii_uppercase())
        .filter(|m| !m.is_empty() && m != "NONE")
        .collect();
    methods.sort();
    methods
}

/// §13.5 / §13.6 — HDCP-LEVEL expected for the resolution of encrypted variants.
fn check_hdcp_level(pl: &MediaPlaylist, issues: &mut Vec<Issue>) {
    let Some((width, height)) = pl.resolution.as_deref().and_then(parse_resolution) else {
        return;
    };
    let level = pl.hdcp_level.as_deref().map(|l| l.trim().to_ascii_uppercase());
    let described = level.clone().unwrap_or_else(|| "absent".to_string());
    let above_hd = width > 1920 || height > 1080;
    if above_hd {
        if level.as_deref() != Some("TYPE-1") {
            issues.push(author_warn(
                "13.6",
                format!(
                    "encrypted '{}' is {width}x{height} (greater than HD) but HDCP-LEVEL is {described}; SHOULD be TYPE-1",
                    pl.name
                ),
            ));
        }
    } else if width >= 1280 || height >= 720 {
        if !matches!(level.as_deref(), Some("TYPE-0") | Some("TYPE-1")) {
            issues.push(author_warn(
                "13.5",
                format!(
                    "encrypted '{}' is {width}x{height} (HD) but HDCP-LEVEL is {described}; SHOULD be TYPE-0",
                    pl.name
                ),
            ));
        }
    }
}

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();

    for pl in ctx.playlists {
        // §13.11 METHOD / §13.2 KEYFORMAT pairing
        for method in &pl.encryption_methods {
            let m = method.to_ascii_uppercase();
            if m == "SAMPLE-AES-CTR" {
                issues.push(author_error(
                    "13.11",
                    format!(
                        "'{}' uses METHOD=SAMPLE-AES-CTR, which SHALL NOT be used on Apple devices",
                        pl.name
                    ),
                ));
            }
            if m.contains("SAMPLE-AES") && m != "SAMPLE-AES" && m != "SAMPLE-AES-CTR" {
                issues.push(author_warn(
                    "13.2",
                    format!("unusual encryption METHOD '{method}' on '{}'", pl.name),
                ));
            }
        }

        let key_tags = parse_key_tags(&pl.raw_content, "#EXT-X-KEY:");
        if key_tags.is_empty() {
            // No raw playlist body to pair attributes against: fall back to the
            // collected sets, which can only catch a ladder where no key at all
            // uses SAMPLE-AES.
            let methods = active_methods(pl);
            let fps = pl
                .key_formats
                .iter()
                .any(|f| f.eq_ignore_ascii_case(FPS_KEYFORMAT));
            if fps && !methods.is_empty() && !methods.iter().any(|m| m == "SAMPLE-AES") {
                issues.push(author_error(
                    "13.2",
                    format!(
                        "'{}' signals FairPlay but uses METHOD={}; FPS content MUST use METHOD=SAMPLE-AES",
                        pl.name,
                        methods.join("/")
                    ),
                ));
            }
        } else {
            check_fps_key_tags(&pl.name, &key_tags, &mut issues);
        }

        // HDCP-LEVEL on STREAM-INF copied to playlist
        if let Some(hdcp) = &pl.hdcp_level {
            let h = hdcp.to_ascii_uppercase();
            if h != "NONE" && h != "TYPE-0" && h != "TYPE-1" {
                issues.push(author_warn(
                    "13.6",
                    format!("unexpected HDCP-LEVEL '{hdcp}' on '{}'", pl.name),
                ));
            }
        }

        // §13.5/§13.6 only bite on protected content. Trick-play variants are
        // skipped so an encrypted ladder reports one finding per resolution.
        if !pl.is_iframe && !active_methods(pl).is_empty() {
            check_hdcp_level(pl, &mut issues);
        }
    }

    if let Some(master) = ctx.master {
        for v in &master.variants {
            if let Some(hdcp) = &v.hdcp_level {
                let h = hdcp.to_ascii_uppercase();
                if !(h == "NONE" || h == "TYPE-0" || h == "TYPE-1") {
                    issues.push(author_warn(
                        "13.6",
                        format!("unexpected HDCP-LEVEL '{hdcp}' on '{}'", v.uri),
                    ));
                }
            }
        }

        let session_keys = parse_key_tags(&master.raw_content, "#EXT-X-SESSION-KEY:");
        check_fps_key_tags("multivariant playlist", &session_keys, &mut issues);
    }

    // Phase B: CENC pattern / tenc / AirPlay §1.41
    for entry in ctx.init_probes {
        let probe = &entry.probe;
        let enc = probe.had_encrypted_sample_entry
            || probe
                .video_sample_fourcc
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case("encv"))
            || probe
                .audio_sample_fourcc
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case("enca"))
            || probe.has_tenc
            || probe.scheme_type.as_deref().is_some_and(|s| {
                let s = s.to_ascii_lowercase();
                s == "cenc" || s == "cbcs" || s == "cens" || s == "cbc1"
            });

        if !enc {
            continue;
        }

        let scheme = probe
            .scheme_type
            .as_deref()
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();

        // §13.7 — pattern encryption MUST use encrypt:skip 1:9. Only `cbcs` video tracks
        // carry a pattern: `cenc` is full-sample AES-CTR, and audio tracks are routinely
        // encrypted without one, so neither is checked here.
        if scheme == "cbcs" && probe.video_sample_fourcc.is_some() {
            let (pattern, from_video_track) = if probe.has_video_tenc {
                (
                    (probe.video_crypt_byte_block, probe.video_skip_byte_block),
                    true,
                )
            } else {
                ((probe.crypt_byte_block, probe.skip_byte_block), false)
            };
            match pattern {
                (Some(crypt), Some(skip)) if crypt != 1 || skip != 9 => {
                    let msg = format!(
                        "CENC pattern {crypt}:{skip} on init '{}' MUST be encrypt:skip 1:9",
                        entry.uri
                    );
                    issues.push(if from_video_track {
                        author_error("13.7", msg)
                    } else {
                        author_warn(
                            "13.7",
                            format!("{msg} (probe could not attribute tenc to the video track)"),
                        )
                    });
                }
                (Some(_), Some(_)) => {}
                _ => issues.push(author_warn(
                    "13.7",
                    format!(
                        "cbcs video init '{}' has no tenc crypt/skip pattern (expect 1:9)",
                        entry.uri
                    ),
                )),
            }
        }

        // §13.9 — content-sensitive encryption MUST NOT be used (cbcs with pattern is OK;
        // "cens"/"cbc1" without standard pattern is suspicious — soft warn on unknown schemes)
        if scheme == "cens" {
            issues.push(author_error(
                "13.9",
                format!(
                    "scheme_type '{scheme}' on init '{}' looks like content-sensitive CENC",
                    entry.uri
                ),
            ));
        }

        // §13.9 / AirPlay §1.41 — subsample encryption info (senc, or saiz+saio) usually
        // lives in the media segments rather than the init. Only the samples of the
        // playlists that use this init count: scanning every sample in the stream would
        // let one compliant rendition hide a broken one.
        let init_has_info = probe.has_senc || (probe.has_saiz && probe.has_saio);
        let mut own_samples = 0usize;
        let mut own_samples_with_info = 0usize;
        for sample in ctx
            .segment_samples
            .iter()
            .filter(|s| entry.playlist_names.iter().any(|n| n == &s.playlist_name))
        {
            own_samples += 1;
            if sample.has_senc || (sample.has_saiz && sample.has_saio) {
                own_samples_with_info += 1;
            }
        }
        let has_info = init_has_info || own_samples_with_info > 0;
        let renditions = entry.playlist_names.join(", ");

        if ctx.policy.profile == super::profile::AuthorProfile::AirPlay2 {
            if !has_info {
                let where_to_look = if own_samples == 0 {
                    " (no segment of its playlist was sampled — check the media segments)"
                } else {
                    ""
                };
                issues.push(author_error(
                    "1.41",
                    format!(
                        "AirPlay2: encrypted init '{}' ('{renditions}') missing senc or saiz+saio{where_to_look}",
                        entry.uri
                    ),
                ));
            }
        } else if ctx.deep_checks && !has_info && own_samples > 0 {
            issues.push(author_warn(
                "13.9",
                format!(
                    "encrypted init '{}' but the {own_samples} sampled segment(s) of '{renditions}' lack senc/saiz+saio",
                    entry.uri
                ),
            ));
        }
    }

    // AirPlay SAMPLE-AES-CTR forbidden (playlist-level)
    if ctx.policy.profile == super::profile::AuthorProfile::AirPlay2 {
        for pl in ctx.playlists {
            if pl.encryption_methods.is_empty() {
                continue;
            }
            if pl.encryption_methods.iter().any(|m| m == "SAMPLE-AES-CTR") {
                issues.push(author_error(
                    "1.41",
                    format!("AirPlay2: SAMPLE-AES-CTR forbidden on '{}'", pl.name),
                ));
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::super::context::{
        InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample,
    };
    use super::*;
    use crate::utils::mp4_probe::{InitSegmentProbe, SegmentScan};
    use crate::utils::validator::types::Severity;

    /// Encrypted (cenc) video init belonging to one playlist.
    fn encrypted_init(uri: &str, playlist: &str) -> InitProbeEntry {
        InitProbeEntry {
            uri: uri.into(),
            byterange: None,
            playlist_names: vec![playlist.into()],
            media_types: vec!["VIDEO".into()],
            probe: InitSegmentProbe {
                video_sample_fourcc: Some("encv".into()),
                scheme_type: Some("cenc".into()),
                has_tenc: true,
                ..Default::default()
            },
        }
    }

    fn sample(playlist: &str, has_senc: bool) -> SegmentSample {
        SegmentSample::from_scan(
            playlist.into(),
            0,
            format!("https://example.com/{playlist}/0.m4s"),
            6.0,
            100_000,
            false,
            &SegmentScan {
                looks_like_fmp4: true,
                has_moof: true,
                has_senc,
                ..Default::default()
            },
        )
    }

    fn deep_issues(inits: &[InitProbeEntry], samples: &[SegmentSample]) -> Vec<Issue> {
        let playlists: Vec<MediaPlaylist> = Vec::new();
        let opts = ValidateAuthorOptions {
            deep_checks: true,
            ..Default::default()
        };
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(None, &playlists, &opts, inits, samples, &vtts);
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains("§13.9"))
            .collect()
    }

    #[test]
    fn author_13_9_reports_the_rendition_whose_own_samples_lack_senc() {
        let inits = vec![
            encrypted_init("https://example.com/720-init.mp4", "video/1280x720"),
            encrypted_init("https://example.com/360-init.mp4", "video/640x360"),
        ];
        let samples = vec![sample("video/1280x720", true), sample("video/640x360", false)];
        let issues = deep_issues(&inits, &samples);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(
            issues[0].message.contains("360-init.mp4") && issues[0].message.contains("video/640x360"),
            "a compliant rendition must not mask the broken one, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_13_9_accepts_a_rendition_whose_own_samples_carry_senc() {
        let inits = vec![encrypted_init("https://example.com/720-init.mp4", "video/1280x720")];
        let samples = vec![sample("video/1280x720", true)];
        assert!(deep_issues(&inits, &samples).is_empty());
    }

    #[test]
    fn author_13_9_stays_quiet_when_the_rendition_was_not_sampled() {
        let inits = vec![encrypted_init("https://example.com/720-init.mp4", "video/1280x720")];
        let samples = vec![sample("video/640x360", false)];
        let issues = deep_issues(&inits, &samples);
        assert!(
            issues.is_empty(),
            "another rendition's samples are no evidence about this init, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }
}
