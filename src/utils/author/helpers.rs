//! Shared Author helpers: issue builders and codec classification.

use super::context::{AuthoringContext, InitProbeEntry};
use crate::utils::validator::types::{Confidence, Issue, MediaRendition, Severity};

pub fn author_issue(severity: Severity, section: &str, message: impl Into<String>) -> Issue {
    // A citation is the only part of a finding a reader can check, so a section the
    // catalog does not know is a bug in the rule rather than something to report.
    debug_assert!(
        super::catalog::is_citable(section),
        "{}",
        super::catalog::citation_problem(section).unwrap_or_default()
    );
    Issue::new(
        severity,
        -1,
        format!("Apple Authoring Spec §{section}: {}", message.into()),
    )
}

pub fn author_error(section: &str, message: impl Into<String>) -> Issue {
    author_issue(Severity::Error, section, message)
}

pub fn author_warn(section: &str, message: impl Into<String>) -> Issue {
    author_issue(Severity::Warn, section, message)
}

pub fn author_info(section: &str, message: impl Into<String>) -> Issue {
    author_issue(Severity::Info, section, message)
}

/// A finding whose evidence is weaker than a direct read of the stream.
///
/// Severity says how serious a violation would be; confidence says how sure the check is
/// that it happened, and severity alone cannot carry both. A heuristic read of a bitstream
/// reported as an `ERROR` reads exactly like a missing tag, so anything short of
/// [`Confidence::Measured`] is capped at a warning here: a MUST that was only inferred is
/// not grounds for failing a stream.
///
/// A rule that has already reasoned about its own sampling — §1.26 weighing how much of
/// the asset it measured, or §7.4 finding no random-access picture at all in the bytes it
/// read — keeps the severity it chose and records its confidence with
/// [`Issue::with_confidence`] instead.
pub fn author_issue_with_confidence(
    confidence: Confidence,
    severity: Severity,
    section: &str,
    message: impl Into<String>,
) -> Issue {
    author_issue(confidence.cap(severity), section, message).with_confidence(confidence)
}

/// A finding that the Authoring Spec's own immersive AIV guidance may contradict.
///
/// Immersive AIV content as §1.25 and §16.6 describe it — 4320×4320 at 90 fps, 25 to
/// 100 Mbps, MV-HEVC — cannot also satisfy the general limits on frame rate, HEVC level,
/// bit rate and HDR frame rate. On such a stream the general rule is reported for
/// information, naming the conflict, instead of failing a run over a stream that follows
/// the spec's own recommendation.
pub fn author_conflict_aware_issue(
    policy: &super::profile::AuthorPolicy,
    severity: Severity,
    section: &str,
    message: impl Into<String>,
) -> Issue {
    let message = message.into();
    if !policy.aiv_spec_conflict(section) {
        return author_issue(severity, section, message);
    }
    author_info(
        section,
        format!(
            "Spec conflict (PROJ-AIV): {message}. This stream declares immersive AIV video, \
             whose own guidance (§1.25 tiers at 4320×4320 and 90 fps, §16.6 MV-HEVC) cannot \
             also meet this general requirement, so it is reported for information"
        ),
    )
}

/// Descriptive-video audio. A rendition can only be recognised as descriptive audio from its
/// CHARACTERISTICS or, failing that, from how it is named.
pub fn audio_is_dvs(r: &MediaRendition) -> bool {
    let chars = r.characteristics.as_deref().unwrap_or("");
    chars.contains("describes-video") || name_suggests_dvs(&r.name)
}

/// Audio mixed to make dialogue easier to follow.
pub fn audio_enhances_speech(r: &MediaRendition) -> bool {
    r.characteristics
        .as_deref()
        .is_some_and(|c| c.contains("enhances-speech-intelligibility"))
}

/// NAME wording broadcasters use for described video, so a DVS rendition that omits
/// §2.12's CHARACTERISTICS is still recognised. Deliberately narrow: every rule keyed
/// off this is a MUST.
fn name_suggests_dvs(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("audio description")
        || n.contains("described")
        || n.contains("descriptive")
        || n.split(|c: char| !c.is_ascii_alphanumeric())
            .any(|word| word == "dvs")
}

/// Split a CODECS attribute into tokens.
pub fn codec_tokens(codecs: &str) -> Vec<&str> {
    codecs
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn video_codec_family(token: &str) -> Option<&'static str> {
    let t = token.to_ascii_lowercase();
    if t.starts_with("avc1") || t.starts_with("avc3") {
        Some("avc")
    } else if t.starts_with("hvc1") || t.starts_with("hev1") {
        Some("hevc")
    } else if t.starts_with("dvh1") || t.starts_with("dvhe") {
        Some("dv")
    } else if t.starts_with("av01") {
        Some("av1")
    } else if t.starts_with("mjpg") || t.starts_with("jpeg") {
        Some("mjpg")
    } else {
        None
    }
}

pub fn prefers_parameter_sets_in_sample_entry(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    t.starts_with("avc3") || t.starts_with("hev1") || t.starts_with("dvhe")
}

pub fn is_aac_lc_family(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    // MPEG-4 AAC object types: 2 = AAC-LC, 5 = HE-AACv1 (SBR), 29 = HE-AACv2
    t.starts_with("mp4a.40.2")
        || t.starts_with("mp4a.40.5")
        || t.starts_with("mp4a.40.29")
        || t == "mp4a.40.2"
}

pub fn is_he_aac(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    t.starts_with("mp4a.40.5") || t.starts_with("mp4a.40.29")
}

pub fn is_ac3(token: &str) -> bool {
    token.eq_ignore_ascii_case("ac-3")
}

pub fn is_ec3(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    t == "ec-3" || t.starts_with("ec+3")
}

pub fn is_apac(token: &str) -> bool {
    token.to_ascii_lowercase().starts_with("apac")
}

/// The audio codecs §2.25 only allows in fMP4: xHE-AAC, APAC, ALAC and FLAC.
pub fn audio_requires_fmp4(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    t.starts_with("mp4a.40.42")
        || t.starts_with("apac")
        || t.starts_with("alac")
        || t.starts_with("flac")
}

/// The codecs one container rule was told an init carries, and the playlists that said so.
///
/// A finding names its evidence, and after tokens have been discounted the playlists that
/// reference the init are no longer that evidence: the ones whose CODECS actually
/// contributed a token are. Those are kept alongside the tokens so the message can cite
/// them instead.
#[derive(Debug, Clone, Default)]
pub struct DeclaredTokens {
    tokens: Vec<String>,
    playlists: Vec<String>,
}

impl DeclaredTokens {
    fn push(&mut self, token: &str, playlist: &str) {
        if !self.tokens.iter().any(|t| t == token) {
            self.tokens.push(token.to_string());
        }
        if !self.playlists.iter().any(|p| p == playlist) {
            self.playlists.push(playlist.to_string());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// The tokens as a finding lists them: `hvc1.2.4.L123.B0, dvh1.05.03`.
    pub fn tokens(&self) -> String {
        self.tokens.join(", ")
    }

    /// The contributing playlists and a verb that agrees with them: `'video' declares`.
    pub fn declaring_phrase(&self) -> String {
        let names: Vec<String> = self.playlists.iter().map(|n| format!("'{n}'")).collect();
        let verb = if self.playlists.len() == 1 {
            "declares"
        } else {
            "declare"
        };
        format!("{} {verb}", names.join(", "))
    }
}

/// The codecs an init segment was meant to carry, grouped by the container rule that
/// applies to each.
#[derive(Debug, Clone, Default)]
pub struct DeclaredCodecs {
    /// §1.5 codecs: HEVC and Dolby Vision.
    pub hevc_or_dv: DeclaredTokens,
    /// §1.39 codec: AV1.
    pub av1: DeclaredTokens,
    /// §2.25 codecs: xHE-AAC, APAC, ALAC and FLAC.
    pub fmp4_only_audio: DeclaredTokens,
}

/// The fMP4-only codecs the playlists declaring `entry` say it carries.
///
/// A parsed init names its codec in its sample entry, but an init that could not be read
/// as fMP4 has no sample entry — and that is precisely the case the container rules
/// (§1.5, §1.39, §2.25) exist for. So the codec has to come from the CODECS attribute of
/// the playlist(s) that reference the init.
///
/// CODECS describes a whole presentation rather than one init, so a token only counts
/// against the init when the playlist carrying it is the one that init belongs to. A
/// demuxed variant lists the codec of the audio group it points at, whose samples live in
/// that group's own init; and an audio rendition inherits its CODECS from the audio slot
/// of a STREAM-INF, which holds a video token when the packager wrote the two the other
/// way round. Attributing either to this init blames it for a container rule about bytes
/// it was never meant to carry.
///
/// Standing down has to be as narrow as the shape that earns it, or a real violation is
/// lost. An AUDIO attribute on its own does not put the audio anywhere else:
///
/// * an `EXT-X-MEDIA:TYPE=AUDIO` with no URI is the muxed shape §8.9 warns about — the
///   samples are in the variant's own segments even though the variant names a group;
/// * an audio-only variant may point at the very group its own URI serves, so its init
///   *is* the audio init and its only codec token is the one being discounted;
/// * an I-frame playlist has no AUDIO attribute to reason about at all, and often shares
///   the variant's EXT-X-MAP. A packager that copies the whole CODECS string onto
///   `EXT-X-I-FRAME-STREAM-INF` would otherwise hand the video init an audio token it
///   cannot carry — trick-play segments hold video alone.
pub fn declared_codecs_for_init(
    ctx: &AuthoringContext<'_>,
    entry: &InitProbeEntry,
) -> DeclaredCodecs {
    let mut declared = DeclaredCodecs::default();
    let declaring = ctx
        .playlists
        .iter()
        .filter(|pl| entry.playlist_names.contains(&pl.name));
    for pl in declaring {
        let Some(codecs) = pl.codecs.as_deref() else {
            continue;
        };
        // A group whose renditions carry a URI is one the audio was actually moved to.
        let group_is_demuxed = pl.audio_group.as_deref().is_some_and(|group| {
            ctx.master.is_some_and(|m| {
                m.media_renditions
                    .iter()
                    .any(|r| r.media_type == "AUDIO" && r.group_id == group && r.uri.is_some())
            })
        });
        // No RESOLUTION and no video token: whatever group this playlist names, the audio
        // it declares is the only thing its own init could be holding.
        let carries_only_audio = pl.resolution.is_none()
            && !codec_tokens(codecs)
                .iter()
                .any(|t| video_codec_family(t).is_some());
        let is_iframe = pl.is_iframe || pl.iframes_only;
        let audio_lives_elsewhere = is_iframe || (group_is_demuxed && !carries_only_audio);
        let video_lives_elsewhere = pl.media_type == "AUDIO";
        for tok in codec_tokens(codecs) {
            match video_codec_family(tok) {
                Some("hevc" | "dv") if !video_lives_elsewhere => {
                    declared.hevc_or_dv.push(tok, &pl.name)
                }
                Some("av1") if !video_lives_elsewhere => declared.av1.push(tok, &pl.name),
                None if audio_requires_fmp4(tok) && !audio_lives_elsewhere => {
                    declared.fmp4_only_audio.push(tok, &pl.name)
                }
                _ => {}
            }
        }
    }
    declared
}

pub fn parse_resolution(res: &str) -> Option<(u32, u32)> {
    let (w, h) = res.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

pub fn aspect_ratio(res: &str) -> Option<f64> {
    let (w, h) = parse_resolution(res)?;
    if h == 0 {
        return None;
    }
    Some(w as f64 / h as f64)
}

pub fn is_hdr_range(vr: Option<&str>) -> bool {
    matches!(
        vr.map(|s| s.to_ascii_uppercase()).as_deref(),
        Some("PQ") | Some("HLG")
    )
}

pub fn playlist_duration_s(pl: &crate::utils::validator::types::MediaPlaylist) -> f64 {
    pl.segments.iter().map(|s| s.duration).sum()
}

/// Approximate VOD frame-rate set from §1.18.
pub fn is_recommended_vod_framerate(fps: f64) -> bool {
    const ALLOWED: &[f64] = &[23.976, 24.0, 25.0, 29.97, 30.0, 50.0, 59.94, 60.0];
    ALLOWED.iter().any(|&a| (a - fps).abs() < 0.02)
}

pub fn av1_level_ok(token: &str) -> bool {
    // av01.P.LL.T... where LL is two-digit level*10 (e.g. 32 = 3.2). Max 6.2 → 62.
    let t = token.to_ascii_lowercase();
    if !t.starts_with("av01.") {
        return true;
    }
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() < 3 {
        return true;
    }
    let Ok(level) = parts[2].parse::<u32>() else {
        return true;
    };
    level <= 62
}

/// Map an H.264 `profile_idc` (avcC `avc_profile_indication`) to its profile name.
pub fn h264_profile_name(profile_idc: &str) -> Option<&'static str> {
    match profile_idc.trim().parse::<u16>().ok()? {
        66 => Some("baseline"),
        77 => Some("main"),
        88 => Some("extended"),
        100 => Some("high"),
        110 => Some("high10"),
        122 => Some("high422"),
        244 => Some("high444"),
        _ => None,
    }
}

/// Human-readable HEVC profile from an `hvcC` / CODECS `general_profile_idc`.
pub fn hevc_profile_name(profile_idc: u32) -> String {
    match profile_idc {
        1 => "1 (Main)".to_string(),
        2 => "2 (Main 10)".to_string(),
        3 => "3 (Main Still Picture)".to_string(),
        4 => "4 (Format Range Extensions)".to_string(),
        other => other.to_string(),
    }
}

/// Parse H.264/HEVC level strings like "4.1", "41", "51", "5.1" into a float.
/// Two-digit level indications are scaled down, so "10" reads as level 1.0.
pub fn parse_codec_level(s: &str) -> Option<f64> {
    let cleaned = s.trim().trim_start_matches('L').trim_start_matches('l');
    if let Ok(v) = cleaned.parse::<f64>() {
        if v >= 10.0 {
            Some(v / 10.0)
        } else {
            Some(v)
        }
    } else {
        None
    }
}

/// HEVC `general_level_idc` → level number: the indication is level × 30, so 153 → 5.1.
/// Values of 10 or less are assumed to already be levels.
pub fn hevc_level_from_idc(raw: f64) -> f64 {
    if raw > 10.0 {
        raw / 30.0
    } else {
        raw
    }
}

/// H.264 profile_idc and level read from an `avc1`/`avc3` CODECS token.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AvcCodec {
    pub profile_idc: u16,
    pub level: f64,
}

/// Parse an `avc1`/`avc3` CODECS token. The RFC 6381 form is `avc1.PPCCLL`, three
/// hex bytes holding profile_idc, the constraint flags and level_idc (`41` → 4.1).
/// The legacy dotted decimal form `avc1.<profile_idc>.<level_idc>` is also accepted.
pub fn parse_avc_codec(token: &str) -> Option<AvcCodec> {
    let t = token.to_ascii_lowercase();
    let (fourcc, rest) = t.split_once('.')?;
    if !matches!(fourcc, "avc1" | "avc3") {
        return None;
    }
    if rest.len() == 6 && rest.chars().all(|c| c.is_ascii_hexdigit()) {
        let profile_idc = u16::from_str_radix(&rest[0..2], 16).ok()?;
        let level_idc = u16::from_str_radix(&rest[4..6], 16).ok()?;
        return Some(AvcCodec {
            profile_idc,
            level: f64::from(level_idc) / 10.0,
        });
    }
    let mut parts = rest.split('.');
    let profile_idc = parts.next()?.trim().parse::<u16>().ok()?;
    let level = parse_codec_level(parts.next()?)?;
    Some(AvcCodec { profile_idc, level })
}

/// HEVC tier: `general_tier_flag` is set for High tier and clear for Main tier, and the
/// CODECS string spells the same distinction as the `L` or `H` prefix on its level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HevcTier {
    Main,
    High,
}

impl HevcTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Main => "Main",
            Self::High => "High",
        }
    }

    /// The tier an init probe reports. `general_tier_flag` arrives as a boolean, so both
    /// its spellings and the tier names are accepted.
    pub fn from_flag(flag: &str) -> Option<Self> {
        match flag.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "high" => Some(Self::High),
            "false" | "0" | "main" => Some(Self::Main),
            _ => None,
        }
    }
}

/// HEVC `general_profile_idc` (1 = Main, 2 = Main 10), tier and level read from an
/// `hvc1`/`hev1` CODECS token. Any field may be absent from a short token.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HevcCodec {
    pub profile_idc: Option<u32>,
    pub tier: Option<HevcTier>,
    pub level: Option<f64>,
}

/// Parse an `hvc1`/`hev1` CODECS token such as `hvc1.2.4.L153.B0`. The first
/// element is the profile (optionally prefixed by a profile space letter) and the
/// tier/level element is a `L`/`H` prefix followed by `general_level_idc`.
pub fn parse_hevc_codec(token: &str) -> Option<HevcCodec> {
    let t = token.to_ascii_lowercase();
    let (fourcc, rest) = t.split_once('.')?;
    if !matches!(fourcc, "hvc1" | "hev1") {
        return None;
    }
    fn strip_alpha(s: &str) -> &str {
        s.trim_start_matches(|c: char| c.is_ascii_alphabetic())
    }
    let parts: Vec<&str> = rest.split('.').collect();
    let profile_idc = parts.first().and_then(|p| strip_alpha(p).parse::<u32>().ok());
    let tier_and_level = parts.iter().find(|p| {
        let mut chars = p.chars();
        matches!(chars.next(), Some('l') | Some('h')) && chars.all(|c| c.is_ascii_digit())
    });
    let tier = tier_and_level.and_then(|p| match p.chars().next() {
        Some('l') => Some(HevcTier::Main),
        Some('h') => Some(HevcTier::High),
        _ => None,
    });
    let level = tier_and_level
        .and_then(|p| strip_alpha(p).parse::<f64>().ok())
        .map(hevc_level_from_idc);
    Some(HevcCodec {
        profile_idc,
        tier,
        level,
    })
}

/// Dolby Vision profile and level read from a `dvh1.PP.LL` / `dvhe.PP.LL` CODECS token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DvCodec {
    pub profile: u32,
    pub level: u32,
}

pub fn parse_dv_codec(token: &str) -> Option<DvCodec> {
    let t = token.to_ascii_lowercase();
    let (fourcc, rest) = t.split_once('.')?;
    if !matches!(fourcc, "dvh1" | "dvhe") {
        return None;
    }
    let mut parts = rest.split('.');
    let profile = parts.next()?.trim().parse::<u32>().ok()?;
    let level = parts.next()?.trim().parse::<u32>().ok()?;
    Some(DvCodec { profile, level })
}

/// Rough H.264 High Profile level required by resolution×fps (Apple §1.11 tables, simplified).
pub fn h264_level_required_for(width: u32, height: u32, fps: f64) -> f64 {
    let macroblocks = width.div_ceil(16) * height.div_ceil(16);
    let mbs_per_sec = macroblocks as f64 * fps;
    // Selected High-profile level ceilings (macroblocks / macroblocks-per-second).
    // Values are approximate; used only for SHOULD §1.11.
    if macroblocks <= 396 && mbs_per_sec <= 11_880.0 {
        3.0
    } else if macroblocks <= 2_448 && mbs_per_sec <= 108_000.0 {
        3.1
    } else if macroblocks <= 8_160 && mbs_per_sec <= 245_760.0 {
        4.1
    } else if macroblocks <= 22_080 && mbs_per_sec <= 522_240.0 {
        4.2
    } else if macroblocks <= 36_864 && mbs_per_sec <= 589_824.0 {
        5.0
    } else if macroblocks <= 36_864 && mbs_per_sec <= 983_040.0 {
        5.1
    } else {
        5.2
    }
}

pub fn playlist_looks_like_ts(pl: &crate::utils::validator::types::MediaPlaylist) -> bool {
    pl.segments
        .iter()
        .any(|s| s.uri.contains(".ts") || s.uri.contains(".m2ts") || s.uri.contains(".mts"))
}

pub fn playlist_has_map(pl: &crate::utils::validator::types::MediaPlaylist) -> bool {
    pl.segments.iter().any(|s| s.map_uri.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inferred_findings_are_capped_at_a_warning() {
        let heuristic =
            author_issue_with_confidence(Confidence::Heuristic, Severity::Error, "7.4", "guessed");
        assert_eq!(heuristic.severity, Severity::Warn);
        assert_eq!(heuristic.confidence, Confidence::Heuristic);

        let sampled =
            author_issue_with_confidence(Confidence::Sampled, Severity::Error, "1.13", "sampled");
        assert_eq!(sampled.severity, Severity::Warn);

        // Measured evidence is the only kind that fails a stream.
        let measured =
            author_issue_with_confidence(Confidence::Measured, Severity::Error, "8.2", "read");
        assert_eq!(measured.severity, Severity::Error);
        assert_eq!(measured.confidence, Confidence::Measured);
    }

    #[test]
    fn findings_are_measured_by_default() {
        assert_eq!(author_error("8.2", "read").confidence, Confidence::Measured);
    }

    #[test]
    fn parses_avc_hex_and_dotted_forms() {
        let hex = parse_avc_codec("avc1.640029").expect("hex form");
        assert_eq!(hex.profile_idc, 100);
        assert!((hex.level - 4.1).abs() < f64::EPSILON);

        let upper = parse_avc_codec("AVC3.4D401F").expect("uppercase hex form");
        assert_eq!(upper.profile_idc, 77);
        assert!((upper.level - 3.1).abs() < f64::EPSILON);

        let dotted = parse_avc_codec("avc1.66.30").expect("dotted form");
        assert_eq!(dotted.profile_idc, 66);
        assert!((dotted.level - 3.0).abs() < f64::EPSILON);

        assert!(parse_avc_codec("hvc1.2.4.L153.B0").is_none());
        assert!(parse_avc_codec("avc1").is_none());
    }

    #[test]
    fn parses_hevc_profile_and_tier_level() {
        let main10 = parse_hevc_codec("hvc1.2.4.L153.B0").expect("hevc token");
        assert_eq!(main10.profile_idc, Some(2));
        assert!((main10.level.unwrap() - 5.1).abs() < 0.01);

        let high_tier = parse_hevc_codec("hev1.1.6.H120").expect("high tier token");
        assert_eq!(high_tier.profile_idc, Some(1));
        assert!((high_tier.level.unwrap() - 4.0).abs() < 0.01);

        assert!(parse_hevc_codec("dvh1.05.06").is_none());
    }

    #[test]
    fn parses_dolby_vision_profile_and_level() {
        assert_eq!(
            parse_dv_codec("dvh1.05.06"),
            Some(DvCodec {
                profile: 5,
                level: 6
            })
        );
        assert_eq!(
            parse_dv_codec("dvhe.08.09"),
            Some(DvCodec {
                profile: 8,
                level: 9
            })
        );
        assert!(parse_dv_codec("dvh1").is_none());
        assert!(parse_dv_codec("hvc1.2.4.L153.B0").is_none());
    }
}
