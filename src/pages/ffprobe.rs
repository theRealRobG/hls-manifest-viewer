use crate::utils::{
    href::{playlist_href, replace_hls_variables},
    mp4_atom_properties::{AtomPropertyValue, get_properties},
    network::{FetchError, RequestRange, fetch_array_buffer, fetch_text},
    validator::is_master_playlist,
};
use url::Url;
use leptos::prelude::*;
use mp4_atom::{Header, ReadFrom};
use quick_m3u8::{
    HlsLine, Reader,
    tag::{KnownTag, HlsPlaylistType, hls::Tag},
};
use std::{collections::{HashMap, HashSet}, io::Cursor};

// ── WASM thread-safety shim ─────────────────────────────────────────────────────────────────────────
// on_cleanup requires Send + Sync. Browser closures are !Send; WASM is single-threaded
// so wrapping via a raw pointer is safe — the cleanup only runs on this thread.
fn run_wasm_cleanup<F: FnOnce() + 'static>(f: F) -> impl FnOnce() + Send + Sync + 'static {
    let ptr = Box::into_raw(Box::new(f)) as usize; // usize is Send+Sync
    move || {
        // SAFETY: ptr was created on the same thread; WASM is single-threaded.
        let f = unsafe { Box::from_raw(ptr as *mut F) };
        f();
    }
}

// ── Constants ────────────────────────────────────────────────────────────────


// ── Data types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ProbeReport {
    pub url: String,
    pub manifest_type: String,
    pub format_name: Option<String>,
    pub major_brand: Option<String>,
    pub duration_s: Option<f64>,
    /// Highest STREAM-INF BANDWIDTH among non-I-frame variants (HLS "overall" bitrate).
    pub overall_bitrate_bps: Option<u64>,
    pub stream_count: usize,
    pub session_tags: Vec<(String, String)>,
    pub hls_version: Option<u32>,
    pub target_duration: Option<f64>,
    pub playlist_type: Option<String>,
    pub is_live: bool,
    pub total_segments: usize,
    /// True once a media playlist was read, which is what the protocol fields below are
    /// derived from. Their defaults ("not live", zero segments) are indistinguishable
    /// from a real VOD playlist with no segments, so the report has to say which it is.
    pub protocol_set: bool,
    /// EXT-X-I-FRAME-STREAM-INF variants declared by the master, which Inspect lists
    /// rather than probes — a trick-play playlist has no audio and no continuous video.
    pub iframe_variants: Vec<String>,
    pub ll_hls: Option<LlHlsInfo>,
    pub video_tracks: Vec<VideoTrackInfo>,
    pub audio_tracks: Vec<AudioTrackInfo>,
    pub subtitle_tracks: Vec<SubtitleTrackInfo>,
    pub caption_tracks: Vec<CaptionTrackInfo>,
    pub encryption_methods: Vec<String>,
    pub key_formats: Vec<String>,
    pub drm_systems: Vec<DrmInfo>,
    pub init_segment_probed: bool,
    /// True when a probed init segment held a box that would not decode. Anything the
    /// probe did not find may have been inside it, so "none found" has to be qualified.
    pub init_partially_decoded: bool,
    pub probe_notes: Vec<String>,
    /// EXT-X-DEFINE map from the probed playlist (viewer links + URI substitution).
    pub definitions: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct LlHlsInfo {
    pub part_hold_back: Option<f64>,
    pub can_skip_until: Option<f64>,
    pub can_block_reload: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VideoTrackInfo {
    pub name: String,
    pub codec: Option<String>,
    pub codec_long: Option<String>,
    pub profile: Option<String>,
    pub level: Option<String>,
    pub resolution: Option<String>,
    pub frame_rate: Option<f64>,
    pub bitrate_bps: Option<u64>,
    pub avg_bitrate_bps: Option<u64>,
    pub color_space: Option<String>,
    pub hdr_format: Option<String>,
    pub color_primaries: Option<String>,
    pub transfer_characteristics: Option<String>,
    pub matrix_coefficients: Option<String>,
    /// True when the colour fields above were read from a `colr` box. When false they
    /// are what VIDEO-RANGE implies, which is a convention rather than a measurement.
    pub color_measured: bool,
    pub full_range: Option<bool>,
    pub bit_depth: Option<u8>,
    pub pixel_format: Option<String>,
    pub sar: Option<String>,
    pub dar: Option<String>,
    /// Absolute URI of this variant's media playlist.
    pub playlist_uri: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AudioTrackInfo {
    pub name: String,
    pub group_id: String,
    pub codec: Option<String>,
    pub codec_long: Option<String>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u16>,
    pub bitrate_bps: Option<u64>,
    pub language: Option<String>,
    pub is_default: bool,
    /// Absolute URI of this rendition's media playlist (None for muxed audio)
    pub playlist_uri: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SubtitleTrackInfo {
    pub name: String,
    pub language: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CaptionTrackInfo {
    pub name: String,
    pub group_id: String,
    pub language: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrmInfo {
    pub system_name: String,
    /// Canonical system id: lowercase hex with the UUID dashes removed. A `pssh` names
    /// the system that way while an EXT-X-KEY KEYFORMAT spells the same id as a dashed
    /// UUID, so the two only match once both have been through [`canonical_system_id`].
    pub system_id: String,
}

impl DrmInfo {
    /// A DRM system as the report should carry it, whichever tag or box it came from.
    fn new(fallback_name: &str, raw_id: &str) -> Self {
        let system_id = canonical_system_id(raw_id);
        let system_name = drm_system_name(&system_id)
            .unwrap_or(fallback_name)
            .to_string();
        Self {
            system_name,
            system_id,
        }
    }

    /// The id in the dashed UUID form the specifications write it in.
    fn display_id(&self) -> String {
        let id = &self.system_id;
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return id.clone();
        }
        [&id[0..8], &id[8..12], &id[12..16], &id[16..20], &id[20..32]].join("-")
    }
}

/// How firmly a codec label is established, so that boxes arriving in any order settle on
/// the same answer. A sample-entry fourCC — including the original format an `frma`
/// restores — names the codec outright, while a configuration box only implies it: an
/// `hvcC` sits inside a Dolby Vision entry as readily as an HEVC one. An `encv` or `enca`
/// on its own is evidence of nothing but encryption.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum CodecEvidence {
    #[default]
    None,
    ConfigBox,
    SampleFormat,
}

#[derive(Clone, Default)]
struct Mp4ProbeInfo {
    major_brand: Option<String>,
    video_codec_override: Option<String>,
    video_codec_evidence: CodecEvidence,
    video_profile: Option<String>,
    video_level: Option<String>,
    video_bit_depth: Option<u8>,
    video_color_primaries: Option<String>,
    video_transfer_char: Option<String>,
    video_matrix_coefficients: Option<String>,
    video_full_range: Option<bool>,
    video_pixel_format: Option<String>,
    /// Sample aspect ratio from `pasp`, as `h:v` in lowest terms.
    video_sar: Option<String>,
    audio_codec_override: Option<String>,
    audio_codec_long: Option<String>,
    audio_codec_evidence: CodecEvidence,
    audio_sample_rate: Option<u32>,
    audio_channels: Option<u16>,
    audio_bit_depth: Option<u16>,
    audio_bitrate_bps: Option<u64>,
    drm_systems: Vec<DrmInfo>,
    /// Boxes the walk could not decode and stepped over. See [`probe_mp4`].
    decode_errors: usize,
    /// True only when the walk reached the last byte after reading at least one box.
    /// An empty body and a header that will not read both leave this false.
    walk_complete: bool,
}

impl Mp4ProbeInfo {
    fn set_video_codec(&mut self, codec: &str, evidence: CodecEvidence) {
        if evidence > self.video_codec_evidence {
            self.video_codec_override = Some(codec.to_string());
            self.video_codec_evidence = evidence;
        }
    }

    fn set_audio_codec(&mut self, short: &str, long: &str, evidence: CodecEvidence) {
        if evidence > self.audio_codec_evidence {
            self.audio_codec_override = Some(short.to_string());
            self.audio_codec_long = Some(long.to_string());
            self.audio_codec_evidence = evidence;
        }
    }
}

/// Fold a DRM system id into the one form both `pssh` and KEYFORMAT can be compared in:
/// lowercase hex with the UUID dashes and any `urn:uuid:` prefix stripped.
fn canonical_system_id(raw: &str) -> String {
    let raw = raw.trim();
    let hex = raw
        .strip_prefix("urn:uuid:")
        .or_else(|| raw.strip_prefix("URN:UUID:"))
        .unwrap_or(raw);
    hex.chars()
        .filter(|c| *c != '-')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The single name Inspect shows for a system id, so a stream signalling both a `pssh`
/// and a KEYFORMAT does not list the same DRM twice under two vendors' spellings.
fn drm_system_name(canonical_id: &str) -> Option<&'static str> {
    Some(match canonical_id {
        "edef8ba979d64acea3c827dcd51d21ed" => "Google Widevine",
        "9a04f07998404286ab92e65be0885f95" => "Microsoft PlayReady",
        "94ce86fb07ff4f43adb893d2fa968ca2" => "Apple FairPlay",
        "f239e769efa348509c16a903c6932efb" => "Adobe Primetime DRM",
        "1077efecc0b24d02ace33c1e52e2fb4b" => "W3C Common PSSH",
        "3ea8778f77424bf9b18be834b2acbd47" => "Clear Key (AES-128)",
        _ => return None,
    })
}


// ── Check category + item definitions ────────────────────────────────────────

pub struct CheckCat {
    pub id: &'static str,
    pub icon: &'static str,
    pub label: &'static str,
    pub items: &'static [CheckItem],
}

pub struct CheckItem {
    pub id: &'static str,
    pub label: &'static str,
    pub note: Option<&'static str>, // shown in parentheses
}

pub static CATEGORIES: &[CheckCat] = &[
    CheckCat {
        id: "format", icon: "📦", label: "Format & Container",
        items: &[
            CheckItem { id: "format_name",    label: "Format / Container type", note: None },
            CheckItem { id: "container_ftyp", label: "Container brand (ftyp)",  note: Some("init seg") },
            CheckItem { id: "duration",       label: "Duration",                note: None },
            CheckItem { id: "bitrate",        label: "Overall bitrate",         note: None },
            CheckItem { id: "stream_count",   label: "Number of streams",       note: None },
            CheckItem { id: "session_tags",   label: "Tags / metadata",         note: None },
        ],
    },
    CheckCat {
        id: "video", icon: "🎬", label: "Video Stream",
        items: &[
            CheckItem { id: "video_codec",        label: "Codec",                    note: None },
            CheckItem { id: "video_resolution",   label: "Resolution",               note: None },
            CheckItem { id: "video_frame_rate",   label: "Frame rate",               note: None },
            CheckItem { id: "video_bitrate",      label: "Bitrate",                  note: None },
            CheckItem { id: "video_avg_bitrate",  label: "Avg Bitrate",              note: None },
            CheckItem { id: "video_color_space",  label: "Color space / HDR format", note: None },
            CheckItem { id: "video_aspect_ratio", label: "Aspect ratio (SAR / DAR)", note: None },
            CheckItem { id: "video_profile",  label: "Profile",                  note: Some("init seg") },
            CheckItem { id: "video_level",    label: "Level",                    note: Some("init seg") },
            CheckItem { id: "video_bit_depth",label: "Bit depth",                note: Some("init seg") },
            // Filled from the init segment's colr box when there is one, and from what
            // VIDEO-RANGE implies otherwise — the table marks the latter "assumed".
            CheckItem { id: "video_primaries",label: "Color primaries",          note: Some("init seg / VIDEO-RANGE") },
            CheckItem { id: "video_transfer", label: "Transfer characteristics", note: Some("init seg / VIDEO-RANGE") },
            CheckItem { id: "video_matrix",   label: "Matrix coefficients",      note: Some("init seg / VIDEO-RANGE") },
            CheckItem { id: "video_pixel_fmt",label: "Pixel format",             note: Some("init seg") },
        ],
    },
    CheckCat {
        id: "audio", icon: "🔊", label: "Audio Stream",
        items: &[
            CheckItem { id: "audio_codec",    label: "Codec",          note: None },
            CheckItem { id: "audio_channels", label: "Channels",       note: None },
            CheckItem { id: "audio_layout",   label: "Channel layout", note: None },
            CheckItem { id: "audio_language", label: "Language",       note: None },
            CheckItem { id: "audio_bitrate", label: "Bitrate",   note: Some("init seg") },
            CheckItem { id: "audio_rate",  label: "Sample rate", note: Some("init seg") },
            CheckItem { id: "audio_depth", label: "Bit depth",   note: Some("init seg") },
        ],
    },
    CheckCat {
        id: "subtitles", icon: "📝", label: "Subtitles & Captions",
        items: &[
            CheckItem { id: "subtitle_tracks", label: "Subtitle tracks",       note: None },
            CheckItem { id: "caption_tracks",  label: "Closed caption tracks", note: None },
        ],
    },
    CheckCat {
        id: "encryption", icon: "🔒", label: "Encryption & DRM",
        items: &[
            CheckItem { id: "enc_method",  label: "Encryption method",  note: None },
            CheckItem { id: "key_format",  label: "Key format",         note: None },
            CheckItem { id: "drm_systems", label: "DRM systems", note: Some("PSSH / KEYFORMAT") },
        ],
    },
    CheckCat {
        id: "protocol", icon: "📡", label: "HLS Protocol",
        items: &[
            CheckItem { id: "hls_version",     label: "HLS version",             note: None },
            CheckItem { id: "target_duration", label: "Target segment duration", note: None },
            CheckItem { id: "playlist_type",   label: "Playlist type (VOD/Live)",note: None },
            CheckItem { id: "ll_hls",          label: "Low-Latency HLS",         note: None },
            CheckItem { id: "segment_count",   label: "Segment count",           note: None },
        ],
    },
];

fn all_check_ids() -> HashSet<String> {
    CATEGORIES.iter()
        .flat_map(|cat| cat.items.iter().map(|it| it.id.to_string()))
        .collect()
}

// ── Helper functions ─────────────────────────────────────────────────────────

fn prop_str(val: &AtomPropertyValue) -> String {
    match val {
        AtomPropertyValue::Basic(b) => String::from(b),
        AtomPropertyValue::Table(_) => String::new(),
    }
}

fn gcd(a: u32, b: u32) -> u32 { if b == 0 { a } else { gcd(b, a % b) }  }

fn parse_video_codec(codecs: &str) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    // returns (short, long, profile, level)
    for codec in codecs.split(',') {
        let t = codec.trim().to_lowercase();
        if t.starts_with("avc1") || t.starts_with("avc3") {
            let parts: Vec<&str> = t.splitn(2, '.').collect();
            if parts.len() == 2 && parts[1].len() >= 6 {
                let h = parts[1];
                if let (Ok(p), Ok(l)) = (u8::from_str_radix(&h[0..2], 16), u8::from_str_radix(&h[4..6], 16)) {
                    let prof = avc_profile_name(p);
                    let lev = format!("{:.1}", l as f32 / 10.0);
                    return (Some("H.264".into()), Some(format!("H.264 / AVC ({} Profile, Level {})", prof, lev)),
                            Some(prof.into()), Some(lev));
                }
            }
            return (Some("H.264".into()), Some("H.264 / AVC".into()), None, None);
        }
        if t.starts_with("hvc1") || t.starts_with("hev1") {
            let (profile, level) = parse_hevc_codec_token(&t);
            let long = match (&profile, &level) {
                (Some(p), Some(l)) => format!("H.265 / HEVC ({} Profile, Level {})", p, l),
                _ => "H.265 / HEVC".into(),
            };
            return (Some("H.265".into()), Some(long), profile, level);
        }
        if t.starts_with("dvhe") || t.starts_with("dvh1") {
            let (profile, level) = parse_dolby_vision_codec_token(&t);
            return (Some("H.265".into()), Some("Dolby Vision (HEVC)".into()), profile, level);
        }
        if t.starts_with("dav1") || t.starts_with("dva1") || t.starts_with("dvav") {
            let (profile, level) = parse_dolby_vision_codec_token(&t);
            return (Some("AV1".into()), Some("Dolby Vision (AV1)".into()), profile, level);
        }
        if t.starts_with("av01") { return (Some("AV1".into()), Some("AV1".into()), None, None); }
        if t.starts_with("vp09") || t.starts_with("vp9") {
            return (Some("VP9".into()), Some("VP9".into()), None, None);
        }
        if t.starts_with("vp08") { return (Some("VP8".into()), Some("VP8".into()), None, None); }
    }
    (None, None, None, None)
}

fn avc_profile_name(profile_idc: u8) -> &'static str {
    match profile_idc {
        0x42 => "Baseline", 0x4D => "Main", 0x58 => "Extended",
        0x64 => "High", 0x6E => "High 10", 0x7A => "High 4:2:2",
        0x8A | 0xF4 => "High 4:4:4", _ => "Unknown",
    }
}

fn hevc_profile_name(profile_idc: u8) -> &'static str {
    match profile_idc {
        1 => "Main", 2 => "Main 10", 3 => "Main Still Picture",
        4 => "Format Range Extensions", _ => "Unknown",
    }
}

/// Profile and level from an HEVC CODECS token, which spells them
/// `hvc1.<profile_space><profile_idc>.<compatibility>.<tier><level_idc>` (ISO/IEC
/// 14496-15 Annex E). `level_idc` is the level times 30, and the tier letter is `L` for
/// Main and `H` for High — the tier is named alongside the level because the same
/// number means a different bitrate ceiling in each.
fn parse_hevc_codec_token(token: &str) -> (Option<String>, Option<String>) {
    let mut fields = token.split('.').skip(1);
    // A profile space prefix (A, B or C) precedes the idc when there is one.
    let profile = fields.next().and_then(|f| {
        f.trim_start_matches(|c: char| c.is_ascii_alphabetic())
            .parse::<u8>()
            .ok()
            .map(|idc| hevc_profile_name(idc).to_string())
    });
    let _compatibility = fields.next();
    let level = fields.next().and_then(|f| {
        let tier = f.chars().next()?;
        let idc: u16 = f[tier.len_utf8()..].parse().ok()?;
        let level = format!("{:.1}", f64::from(idc) / 30.0);
        Some(match tier.to_ascii_uppercase() {
            'H' => format!("{level} (High tier)"),
            _ => level,
        })
    });
    (profile, level)
}

/// Profile and level from a Dolby Vision CODECS token, which spells both as two-digit
/// decimals: `dvh1.05.06` is profile 5 at level 6. Unlike HEVC the level is not scaled.
fn parse_dolby_vision_codec_token(token: &str) -> (Option<String>, Option<String>) {
    let mut fields = token.split('.').skip(1);
    let profile = fields
        .next()
        .and_then(|f| f.parse::<u8>().ok())
        .map(|p| format!("Dolby Vision Profile {p}"));
    let level = fields
        .next()
        .and_then(|f| f.parse::<u8>().ok())
        .map(|l| l.to_string());
    (profile, level)
}

fn parse_audio_codec(codecs: &str) -> (Option<String>, Option<String>) {
    for codec in codecs.split(',') {
        let t = codec.trim().to_lowercase();
        if t.starts_with("mp4a.40.29") { return (Some("HE-AAC v2".into()), Some("HE-AAC v2 (eAAC+)".into())); }
        if t.starts_with("mp4a.40.5")  { return (Some("HE-AAC".into()),    Some("HE-AAC (AAC+)".into())); }
        if t.starts_with("mp4a")       { return (Some("AAC".into()),        Some("AAC-LC".into())); }
        if t.starts_with("ec-3") || t.starts_with("ec3") {
            return (Some("E-AC-3".into()), Some("E-AC-3 (Dolby Digital Plus)".into()));
        }
        if t.starts_with("ac-4") || t.starts_with("ac4") {
            return (Some("AC-4".into()), Some("Dolby AC-4".into()));
        }
        if t.starts_with("ac-3") || t.starts_with("ac3") {
            return (Some("AC-3".into()), Some("AC-3 (Dolby Digital)".into()));
        }
        if t.starts_with("opus") { return (Some("Opus".into()), Some("Opus".into())); }
        if t.starts_with("flac") { return (Some("FLAC".into()), Some("FLAC".into())); }
        if t.starts_with("apac") { return (Some("APAC".into()), Some("Apple Positional Audio Codec".into())); }
    }
    (None, None)
}

/// Pick the audio codec token from an EXT-X-STREAM-INF CODECS list (order-independent).
fn audio_codec_token(codecs: &str) -> Option<String> {
    codecs.split(',').map(str::trim).find(|c| {
        let t = c.to_lowercase();
        t.starts_with("mp4a")
            || t.starts_with("ac-3")
            || t.starts_with("ac3")
            || t.starts_with("ec-3")
            || t.starts_with("ec3")
            || t.starts_with("ac-4")
            || t.starts_with("ac4")
            || t.starts_with("opus")
            || t.starts_with("flac")
            || t.starts_with("apac")
    }).map(str::to_string)
}

/// AC-3 / E-AC-3 `fscod` → sample rate (Hz).
fn fscod_to_hz(fscod: u8) -> Option<u32> {
    match fscod {
        0 => Some(48_000),
        1 => Some(44_100),
        2 => Some(32_000),
        _ => None,
    }
}

fn parse_sample_rate_prop(raw: &str) -> Option<u32> {
    raw.split('.').next()?.parse().ok()
}

/// Read a bitrate property, treating a declared 0 as "unknown" rather than a real rate.
fn positive_bps(get: &impl Fn(&str) -> Option<String>, key: &str) -> Option<u64> {
    get(key).and_then(|s| s.parse::<u64>().ok()).filter(|&b| b > 0)
}

fn channels_to_layout(n: u32) -> String {
    match n {
        1 => "mono", 2 => "stereo", 6 => "5.1", 8 => "7.1",
        n => return format!("{} ch", n),
    }.into()
}

fn color_primaries_name(n: u16) -> String {
    match n {
        1  => "BT.709",
        5  => "BT.470BG",
        6  => "BT.601 / SMPTE 170M",
        9  => "BT.2020",
        11 => "DCI P3",
        12 => "Display P3",
        _  => return format!("Unknown ({})", n),
    }.into()
}

fn transfer_name(n: u16) -> String {
    match n {
        1  => "BT.709",
        6  => "BT.601 / SMPTE 170M",
        8  => "Linear",
        13 => "sRGB",
        14 => "BT.2020 (10-bit)",
        15 => "BT.2020 (12-bit)",
        16 => "PQ (ST 2084 / HDR10)",
        18 => "HLG (ARIB STD-B67)",
        _  => return format!("Unknown ({})", n),
    }.into()
}

fn matrix_name(n: u16) -> String {
    match n {
        0  => "Identity",
        1  => "BT.709",
        6  => "BT.601 / SMPTE 170M",
        9  => "BT.2020 NCL",
        10 => "BT.2020 CL",
        _  => return format!("Unknown ({})", n),
    }.into()
}

fn chroma_pixel_fmt(chroma_idc: u8, bit_depth: u8) -> String {
    let sub = match chroma_idc { 0 => "400", 2 => "422", 3 => "444", _ => "420" };
    if bit_depth <= 8 { format!("yuv{}p", sub) } else { format!("yuv{}p{}le", sub, bit_depth) }
}

fn fmt_bps(bps: u64) -> String {
    if bps >= 1_000_000 { format!("{:.2} Mbps", bps as f64 / 1_000_000.0) }
    else { format!("{} kbps", bps / 1000) }
}

fn fmt_dur(secs: f64) -> String {
    let s = secs as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s/3600, (s%3600)/60, s%60) }
    else { format!("{}:{:02}", s/60, s%60) }
}

// ── Local HLS parsing types (replaces crate::utils::validator) ───────────────

/// A single variant stream from a master playlist.
struct VariantStream {
    uri: String,
    bandwidth: Option<u64>,
    average_bandwidth: Option<u64>,
    codecs: Option<String>,
    resolution: Option<String>,
    frame_rate: Option<f64>,
    video_range: Option<String>,
    audio_group: Option<String>,
    is_iframe: bool,
}

/// A media rendition from an EXT-X-MEDIA tag.
struct MediaRendition {
    media_type: String,
    name: String,
    group_id: String,
    uri: Option<String>,
    language: Option<String>,
    is_default: bool,
    /// Channel count extracted from the CHANNELS attribute.
    channels: Option<u32>,
}

/// Parsed representation of a master playlist.
struct MasterPlaylist {
    version: u32,
    variants: Vec<VariantStream>,
    media_renditions: Vec<MediaRendition>,
    definitions: HashMap<String, String>,
}

/// A single segment entry from a media playlist.
struct SegmentInfo {
    duration: f64,
    map_uri: Option<String>,
    map_byterange: Option<RequestRange>,
}

impl MediaPlaylist {
    /// The init segment location of the first segment that declares one.
    fn init_segment(&self) -> Option<(String, Option<RequestRange>)> {
        self.segments
            .iter()
            .find_map(|s| s.map_uri.clone().map(|uri| (uri, s.map_byterange)))
    }
}

/// Parsed server control tag data.
struct ServerControlInfo {
    part_hold_back: Option<f64>,
    can_skip_until: Option<f64>,
    can_block_reload: bool,
}

/// Parsed representation of a media playlist.
struct MediaPlaylist {
    version: u32,
    target_duration: f64,
    playlist_type: Option<String>,
    has_endlist: bool,
    segments: Vec<SegmentInfo>,
    server_control: Option<ServerControlInfo>,
    /// Variables in scope for this playlist's URIs: what a master handed down, plus
    /// whatever this playlist's own EXT-X-DEFINE tags added.
    definitions: HashMap<String, String>,
}

// ── HLS parsing with quick-m3u8 ──────────────────────────────────────────────

fn make_reader_opts() -> quick_m3u8::config::ParsingOptions {
    quick_m3u8::config::ParsingOptions::default()
}

fn playlist_type_str(pt: HlsPlaylistType) -> String {
    match pt {
        HlsPlaylistType::Event => "EVENT".into(),
        HlsPlaylistType::Vod => "VOD".into(),
    }
}

/// Resolve a potentially-relative URI against a base URL string.
/// Resolve a potentially-relative URI against a base URL, normalizing `..` / `.` segments.
fn resolve_uri(base: &str, uri: &str) -> String {
    if let Ok(abs) = Url::parse(uri) {
        return abs.to_string();
    }
    match Url::parse(base).and_then(|b| b.join(uri)) {
        Ok(joined) => joined.to_string(),
        Err(_) => {
            // Last-resort fallback for malformed bases — keep previous behaviour.
            let base_dir = base.rfind('/').map(|i| &base[..=i]).unwrap_or(base);
            format!("{base_dir}{uri}")
        }
    }
}

/// Substitute EXT-X-DEFINE variables, then resolve against `base`.
fn resolve_defined_uri(base: &str, uri: &str, definitions: &HashMap<String, String>) -> String {
    let substituted = replace_hls_variables(uri, definitions);
    resolve_uri(base, substituted.as_ref())
}

fn extract_query_param(url: &str, param: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    parsed
        .query_pairs()
        .find(|(k, _)| k == param)
        .map(|(_, v)| v.into_owned())
}

/// Apply one EXT-X-DEFINE to `defs`. `imported_from` is the multivariant playlist's
/// variables, which only a media playlist has — passing `None` marks a master, where
/// IMPORT has nothing to import from.
fn apply_define_tag(
    base_url: &str,
    tag: &quick_m3u8::tag::hls::Define<'_>,
    defs: &mut HashMap<String, String>,
    imported_from: Option<&HashMap<String, String>>,
) {
    match tag {
        quick_m3u8::tag::hls::Define::Name(n) => {
            defs.insert(n.name().to_string(), n.value().to_string());
        }
        quick_m3u8::tag::hls::Define::Queryparam(q) => {
            let param = q.queryparam();
            if let Some(value) = extract_query_param(base_url, param) {
                // QUERYPARAM uses the query parameter name as the variable name.
                defs.insert(param.to_string(), value);
            }
        }
        quick_m3u8::tag::hls::Define::Import(i) => {
            let name = i.import();
            if let Some(value) = imported_from.and_then(|outer| outer.get(name)) {
                defs.insert(name.to_string(), value.clone());
            }
        }
    }
}

fn audio_needs_init(at: &AudioTrackInfo) -> bool {
    at.sample_rate.is_none()
        || at.bit_depth.is_none()
        || at.codec.is_none()
        || at.bitrate_bps.is_none()
}

fn parse_master_playlist(base_url: &str, content: &str) -> MasterPlaylist {
    let mut master = MasterPlaylist {
        version: 3,
        variants: Vec::new(),
        media_renditions: Vec::new(),
        definitions: HashMap::new(),
    };
    let mut pending_stream_inf: Option<VariantStream> = None;

    // First pass: collect DEFINE so later URIs can be substituted regardless of tag order.
    {
        let mut reader = Reader::from_str(content, make_reader_opts());
        loop {
            match reader.read_line() {
                Ok(Some(HlsLine::KnownTag(KnownTag::Hls(Tag::Define(d))))) => {
                    apply_define_tag(base_url, &d, &mut master.definitions, None);
                }
                Ok(None) => break,
                Ok(Some(_)) => {}
                Err(_) => break,
            }
        }
    }

    let mut reader = Reader::from_str(content, make_reader_opts());
    loop {
        match reader.read_line() {
            Ok(Some(hls_line)) => match hls_line {
                HlsLine::KnownTag(KnownTag::Hls(Tag::Version(v))) => {
                    master.version = v.version() as u32;
                }
                HlsLine::KnownTag(KnownTag::Hls(Tag::StreamInf(si))) => {
                    let res = si.resolution().map(|r| format!("{}x{}", r.width, r.height));
                    let codecs = si.codecs().map(|c| c.to_string());
                    let video_range = si.video_range().map(|vr| vr.to_string());
                    let audio_group = si.audio().map(|a| a.to_string());
                    pending_stream_inf = Some(VariantStream {
                        uri: String::new(),
                        bandwidth: Some(si.bandwidth()),
                        average_bandwidth: si.average_bandwidth(),
                        codecs,
                        resolution: res,
                        frame_rate: si.frame_rate(),
                        video_range,
                        audio_group,
                        is_iframe: false,
                    });
                }
                HlsLine::KnownTag(KnownTag::Hls(Tag::IFrameStreamInf(isi))) => {
                    let uri = resolve_defined_uri(base_url, isi.uri(), &master.definitions);
                    master.variants.push(VariantStream {
                        uri,
                        bandwidth: Some(isi.bandwidth()),
                        average_bandwidth: isi.average_bandwidth(),
                        codecs: isi.codecs().map(|c| c.to_string()),
                        resolution: isi.resolution().map(|r| format!("{}x{}", r.width, r.height)),
                        frame_rate: None,
                        video_range: isi.video_range().map(|vr| vr.to_string()),
                        audio_group: None,
                        is_iframe: true,
                    });
                }
                HlsLine::Uri(uri) => {
                    if let Some(mut vs) = pending_stream_inf.take() {
                        vs.uri = resolve_defined_uri(base_url, &uri, &master.definitions);
                        master.variants.push(vs);
                    }
                }
                HlsLine::KnownTag(KnownTag::Hls(Tag::Media(m))) => {
                    let channels = m.channels()
                        .and_then(|c| c.valid().map(|v| v.count()));
                    master.media_renditions.push(MediaRendition {
                        media_type: m.media_type().to_string(),
                        name: m.name().to_string(),
                        group_id: m.group_id().to_string(),
                        uri: m.uri().map(|u| resolve_defined_uri(base_url, u, &master.definitions)),
                        language: m.language().map(|l| l.to_string()),
                        is_default: m.default(),
                        channels,
                    });
                }
                HlsLine::KnownTag(KnownTag::Hls(Tag::Define(_))) => {}
                _ => {}
            },
            Ok(None) => break,
            Err(_) => break,
        }
    }
    master
}

/// Parse a media playlist. `inherited` is what the multivariant playlist defined, if
/// this playlist was reached through one.
fn parse_media_playlist(
    base_url: &str,
    content: &str,
    inherited: &HashMap<String, String>,
) -> MediaPlaylist {
    let mut pl = MediaPlaylist {
        version: 3,
        target_duration: 0.0,
        playlist_type: None,
        has_endlist: false,
        segments: Vec::new(),
        server_control: None,
        // The master's variables stay in scope. RFC 8216bis §4.4.2.3 asks a media
        // playlist to IMPORT each one it uses, but a MAP URI written with a variable the
        // master defined and the media playlist never imported is common enough that
        // dropping it would leave the init segment unfetchable.
        definitions: inherited.clone(),
    };
    let opts = make_reader_opts();
    let mut pending_duration: Option<f64> = None;
    let mut current_map_uri: Option<String> = None;
    let mut current_map_byterange: Option<RequestRange> = None;

    // First pass: collect this playlist's own DEFINE tags, so a MAP or segment URI
    // written above them still substitutes. A media-only URL has no master to inherit
    // from, which is exactly when its own NAME and QUERYPARAM variables are all there is.
    {
        let mut reader = Reader::from_str(content, make_reader_opts());
        loop {
            match reader.read_line() {
                Ok(Some(HlsLine::KnownTag(KnownTag::Hls(Tag::Define(d))))) => {
                    apply_define_tag(base_url, &d, &mut pl.definitions, Some(inherited));
                }
                Ok(None) | Err(_) => break,
                Ok(Some(_)) => {}
            }
        }
    }
    let definitions = pl.definitions.clone();

    let mut reader = Reader::from_str(content, opts);
    loop {
        match reader.read_line() {
            Ok(Some(hls_line)) => match hls_line {
                HlsLine::KnownTag(KnownTag::Hls(Tag::Version(v))) => {
                    pl.version = v.version() as u32;
                }
                HlsLine::KnownTag(KnownTag::Hls(Tag::Targetduration(td))) => {
                    pl.target_duration = td.target_duration() as f64;
                }
                HlsLine::KnownTag(KnownTag::Hls(Tag::PlaylistType(pt))) => {
                    pl.playlist_type = Some(playlist_type_str(pt.playlist_type()));
                }
                HlsLine::KnownTag(KnownTag::Hls(Tag::Endlist(_))) => {
                    pl.has_endlist = true;
                }
                HlsLine::KnownTag(KnownTag::Hls(Tag::Map(m))) => {
                    current_map_uri = Some(resolve_defined_uri(base_url, m.uri(), &definitions));
                    current_map_byterange = m.byterange().map(RequestRange::from);
                }
                HlsLine::KnownTag(KnownTag::Hls(Tag::Inf(inf))) => {
                    pending_duration = Some(inf.duration());
                }
                HlsLine::Uri(_uri) => {
                    // Segment URIs are not retained for probing; MAP URI (above) is resolved
                    // via resolve_defined_uri so init fetches honour EXT-X-DEFINE.
                    if let Some(dur) = pending_duration.take() {
                        pl.segments.push(SegmentInfo {
                            duration: dur,
                            map_uri: current_map_uri.clone(),
                            map_byterange: current_map_byterange,
                        });
                    }
                }
                HlsLine::KnownTag(KnownTag::Hls(Tag::ServerControl(sc))) => {
                    pl.server_control = Some(ServerControlInfo {
                        part_hold_back: sc.part_hold_back(),
                        can_skip_until: sc.can_skip_until(),
                        can_block_reload: sc.can_block_reload(),
                    });
                }
                _ => {}
            },
            Ok(None) => break,
            Err(_) => break,
        }
    }
    pl
}

fn parse_session_data(content: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let opts = make_reader_opts();
    let mut reader = Reader::from_str(content, opts);
    loop {
        match reader.read_line() {
            Ok(Some(HlsLine::KnownTag(KnownTag::Hls(Tag::SessionData(sd))))) => {
                let k = sd.data_id().to_string();
                let v = sd.value().map(|s| s.to_string())
                    .or_else(|| sd.uri().map(|s| s.to_string()))
                    .unwrap_or_default();
                if !k.is_empty() { out.push((k, v)); }
            }
            Ok(None) | Err(_) => break,
            Ok(Some(_)) => {}
        }
    }
    out
}

/// Encryption methods and KEYFORMATs declared by a playlist. EXT-X-SESSION-KEY carries
/// the same attributes on a multivariant playlist, which is where a stream that wants a
/// player to fetch its keys before choosing a variant announces its DRM.
fn parse_key_info(content: &str) -> (Vec<String>, Vec<String>) {
    let mut methods: Vec<String> = Vec::new();
    let mut formats: Vec<String> = Vec::new();
    let mut record = |method: String, keyformat: String| {
        if !methods.contains(&method) { methods.push(method); }
        if !formats.contains(&keyformat) { formats.push(keyformat); }
    };
    let opts = make_reader_opts();
    let mut reader = Reader::from_str(content, opts);
    loop {
        match reader.read_line() {
            Ok(Some(HlsLine::KnownTag(KnownTag::Hls(Tag::Key(k))))) => {
                record(k.method().to_string(), k.keyformat().to_string());
            }
            Ok(Some(HlsLine::KnownTag(KnownTag::Hls(Tag::SessionKey(k))))) => {
                record(k.method().to_string(), k.keyformat().to_string());
            }
            Ok(None) | Err(_) => break,
            Ok(Some(_)) => {}
        }
    }
    (methods, formats)
}

/// Derive DRM system names from EXT-X-KEY / EXT-X-SESSION-KEY KEYFORMAT values.
/// FairPlay does not embed PSSH boxes in the init segment — its DRM identity
/// lives entirely in the KEYFORMAT attribute.
///
/// KEYFORMAT is a quoted string that packagers spell in whichever case they like, and
/// the UUID forms are hex, so the comparison is case-insensitive.
fn drm_from_key_formats(formats: &[String]) -> Vec<DrmInfo> {
    formats.iter().filter_map(|kf| {
        let kf = kf.trim().to_ascii_lowercase();
        match kf.as_str() {
            "com.apple.streamingkeydelivery" => {
                Some(DrmInfo::new("Apple FairPlay", "94ce86fb07ff4f43adb893d2fa968ca2"))
            }
            "com.microsoft.playready" => {
                Some(DrmInfo::new("Microsoft PlayReady", "9a04f07998404286ab92e65be0885f95"))
            }
            // Any other KEYFORMAT naming a system by UUID identifies it directly.
            _ if kf.starts_with("urn:uuid:") => {
                let info = DrmInfo::new("DRM system", &kf);
                (info.system_id.len() == 32).then_some(info)
            }
            _ => None,
        }
    }).collect()
}

// ── MP4 init-segment prober ──────────────────────────────────────────────────

/// The AudioSampleEntry fields, which sit in an `enca` exactly as they do in a plain
/// entry — encryption hides the samples, not the header that describes them.
fn read_audio_sample_entry(info: &mut Mp4ProbeInfo, get: &impl Fn(&str) -> Option<String>) {
    if let Some(ch) = get("channel_count") { info.audio_channels = ch.parse().ok(); }
    if let Some(bd) = get("sample_size")   { info.audio_bit_depth = bd.parse().ok(); }
    if let Some(sr) = get("sample_rate") {
        info.audio_sample_rate = parse_sample_rate_prop(&sr);
    }
}

/// The same fields for a Dolby entry, which writes 0 in `sample_rate` when the real rate
/// is only in the codec configuration box.
fn read_dolby_audio_sample_entry(info: &mut Mp4ProbeInfo, get: &impl Fn(&str) -> Option<String>) {
    read_audio_sample_entry(info, get);
    info.audio_sample_rate = info.audio_sample_rate.filter(|&r| r > 0);
}

/// The video codec an `frma` original format names. `frma` restores the sample entry
/// fourCC the track had before it was encrypted, so this is the codec, not a guess.
fn video_codec_from_fourcc(fourcc: &str) -> Option<&'static str> {
    Some(match fourcc.trim().to_ascii_lowercase().as_str() {
        "avc1" | "avc3" => "H.264",
        "hvc1" | "hev1" => "H.265 / HEVC",
        "dvh1" | "dvhe" => "Dolby Vision (HEVC)",
        "dav1" | "dva1" | "dvav" => "Dolby Vision (AV1)",
        "av01" => "AV1",
        "vp09" => "VP9",
        "vp08" => "VP8",
        _ => return None,
    })
}

/// The audio codec an `frma` original format names, as (short, long).
fn audio_codec_from_fourcc(fourcc: &str) -> Option<(&'static str, &'static str)> {
    Some(match fourcc.trim().to_ascii_lowercase().as_str() {
        "mp4a" => ("AAC", "AAC-LC"),
        "ac-3" => ("AC-3", "AC-3 (Dolby Digital)"),
        "ec-3" => ("E-AC-3", "E-AC-3 (Dolby Digital Plus)"),
        "ac-4" => ("AC-4", "Dolby AC-4"),
        "opus" => ("Opus", "Opus"),
        "flac" => ("FLAC", "FLAC"),
        "apac" => ("APAC", "Apple Positional Audio Codec"),
        _ => return None,
    })
}

/// Which kind of encrypted sample entry the walk is currently inside, so the `frma` that
/// follows it can be attributed to the right track. `frma` sits in the `sinf` at the end
/// of the sample entry, which a flat walk reaches after the entry itself.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EncryptedEntry {
    Video,
    Audio,
}

/// Read what an fMP4 init segment says about its tracks and protection.
///
/// A box body that will not decode is stepped over rather than ending the walk: one
/// unreadable box would otherwise hide every `pssh` and codec configuration behind it,
/// and the report would state as fact that there were none. Only a header that will not
/// read ends the walk, since without it there is no way to find the next box.
fn probe_mp4(data: Vec<u8>) -> Mp4ProbeInfo {
    let mut info = Mp4ProbeInfo::default();
    let mut reader = Cursor::new(data);
    let mut container_ends: Vec<u64> = Vec::new();
    let mut in_video = false;
    let mut in_audio = false;
    let mut encrypted_entry: Option<EncryptedEntry> = None;

    loop {
        while let Some(&end) = container_ends.last() {
            if reader.position() >= end { container_ends.pop(); } else { break; }
        }
        if reader.position() as usize >= reader.get_ref().len() {
            // Reaching the last byte is a complete walk only if something was actually
            // read and every container closed. An empty body, and a `moov` header whose
            // declared size runs past the data, both hit this arm and must not look
            // like "no DRM".
            if reader.position() > 0 && container_ends.is_empty() {
                info.walk_complete = true;
            }
            break;
        }
        let Ok(header) = Header::read_from(&mut reader) else { break; };
        let body_start = reader.position();
        let atom = match get_properties(&header, &mut reader) {
            Ok(atom) => atom,
            Err(_) => {
                info.decode_errors += 1;
                // The declared body size is the only way past a box that will not decode.
                // A box with no declared size runs to the end of the file, so there is
                // nothing left to step to.
                match header.size {
                    Some(size) => {
                        reader.set_position(body_start + size as u64);
                        continue;
                    }
                    None => break,
                }
            }
        };
        if let Some(e) = atom.new_depth_until { container_ends.push(e); }

        let props = &atom.properties;
        let get = |key: &str| -> Option<String> {
            props.properties.iter()
                .find(|(k, _)| k.as_ref() == key)
                .map(|(_, v)| prop_str(v))
                .filter(|s| !s.is_empty())
        };

        match props.box_name {
            "FileTypeBox" => {
                info.major_brand = get("major_brand");
            }
            "HandlerBox" => {
                match get("handler").as_deref() {
                    Some("vide") => { in_video = true;  in_audio = false; }
                    Some("soun") => { in_audio = true;  in_video = false; }
                    _            => { in_video = false; in_audio = false; }
                }
            }
            "AVCSampleEntryBox" if in_video || info.video_codec_override.is_none() => {
                info.set_video_codec("H.264", CodecEvidence::SampleFormat);
            }
            "HEVCSampleEntryBox" if in_video || info.video_codec_override.is_none() => {
                info.set_video_codec("H.265 / HEVC", CodecEvidence::SampleFormat);
            }
            "DolbyVisionHEVCSampleEntryBox" | "DolbyVisionHVC1SampleEntryBox" => {
                info.set_video_codec("Dolby Vision (HEVC)", CodecEvidence::SampleFormat);
            }
            "AV1SampleEntryBox"  if in_video || info.video_codec_override.is_none() => {
                info.set_video_codec("AV1", CodecEvidence::SampleFormat);
            }
            "VP09SampleEntryBox" if in_video || info.video_codec_override.is_none() => {
                info.set_video_codec("VP9", CodecEvidence::SampleFormat);
            }
            "VP08SampleEntryBox" if in_video || info.video_codec_override.is_none() => {
                info.set_video_codec("VP8", CodecEvidence::SampleFormat);
            }
            // An `encv` says the samples are encrypted and nothing about what they are.
            // The codec arrives either in the configuration box inside this entry or in
            // the `frma` of its `sinf`, both of which the walk reaches next.
            "EncryptedVisualSampleEntryBox" => {
                encrypted_entry = Some(EncryptedEntry::Video);
            }
            "MP4AudioSampleEntryBox" if in_audio => {
                info.set_audio_codec("AAC", "AAC-LC", CodecEvidence::SampleFormat);
                read_audio_sample_entry(&mut info, &get);
            }
            // Likewise `enca`: it carries the AudioSampleEntry fields, which are real,
            // but naming the codec AAC on the strength of them was a guess that a Dolby
            // or Opus track could not correct.
            "EncryptedAudioSampleEntryBox" => {
                encrypted_entry = Some(EncryptedEntry::Audio);
                read_audio_sample_entry(&mut info, &get);
            }
            "OriginalFormatBox" => {
                if let Some(fourcc) = get("data_format") {
                    match encrypted_entry {
                        Some(EncryptedEntry::Video) => {
                            if let Some(codec) = video_codec_from_fourcc(&fourcc) {
                                info.set_video_codec(codec, CodecEvidence::SampleFormat);
                            }
                        }
                        Some(EncryptedEntry::Audio) => {
                            if let Some((short, long)) = audio_codec_from_fourcc(&fourcc) {
                                info.set_audio_codec(short, long, CodecEvidence::SampleFormat);
                            }
                        }
                        None => {}
                    }
                }
            }
            "OpusSampleEntryBox" if in_audio => {
                info.set_audio_codec("Opus", "Opus", CodecEvidence::SampleFormat);
                if let Some(ch) = get("channel_count") { info.audio_channels = ch.parse().ok(); }
                if let Some(sr) = get("sample_rate") {
                    info.audio_sample_rate = parse_sample_rate_prop(&sr);
                }
            }
            "EC3SampleEntryBox" if in_audio => {
                info.set_audio_codec("E-AC-3", "E-AC-3 (Dolby Digital Plus)", CodecEvidence::SampleFormat);
                read_dolby_audio_sample_entry(&mut info, &get);
            }
            "AC3SampleEntryBox" if in_audio => {
                info.set_audio_codec("AC-3", "AC-3 (Dolby Digital)", CodecEvidence::SampleFormat);
                read_dolby_audio_sample_entry(&mut info, &get);
            }
            "AC4SampleEntryBox" if in_audio => {
                info.set_audio_codec("AC-4", "Dolby AC-4", CodecEvidence::SampleFormat);
                read_dolby_audio_sample_entry(&mut info, &get);
            }
            "ApacSampleEntryBox" if in_audio => {
                info.set_audio_codec("APAC", "Apple Positional Audio Codec", CodecEvidence::SampleFormat);
                read_audio_sample_entry(&mut info, &get);
            }
            "ElementaryStreamDescriptorBox" if in_audio => {
                // AAC declares its bitrate in the DecoderConfigDescriptor, in bits/sec.
                // avg_bitrate is 0 for VBR streams, in which case max_bitrate is the
                // only figure available.
                if info.audio_bitrate_bps.is_none() {
                    info.audio_bitrate_bps = positive_bps(&get, "decoder_config_avg_bitrate")
                        .or_else(|| positive_bps(&get, "decoder_config_max_bitrate"));
                }
            }
            "BitRateBox" if in_audio => {
                if info.audio_bitrate_bps.is_none() {
                    info.audio_bitrate_bps = positive_bps(&get, "avg_bitrate")
                        .or_else(|| positive_bps(&get, "max_bitrate"));
                }
            }
            "AC3SpecificBox" => {
                if info.audio_sample_rate.is_none()
                    && let Some(hz) = get("fscod").and_then(|s| s.parse::<u8>().ok()).and_then(fscod_to_hz)
                {
                    info.audio_sample_rate = Some(hz);
                }
                if info.audio_bitrate_bps.is_none() {
                    // AC3SpecificBox bit_rate is in kbps.
                    info.audio_bitrate_bps =
                        positive_bps(&get, "bit_rate").map(|kbps| kbps.saturating_mul(1000));
                }
                // A `dac3` only ever sits inside an AC-3 sample entry, so it names the
                // codec of an `enca` whose `frma` has not gone by yet.
                info.set_audio_codec("AC-3", "AC-3 (Dolby Digital)", CodecEvidence::ConfigBox);
            }
            "EC3SpecificBox" => {
                // fscod lives in nested independent_substream tables; data_rate is top-level.
                if info.audio_bitrate_bps.is_none() {
                    // EC3SpecificBox data_rate is in kbps.
                    info.audio_bitrate_bps =
                        positive_bps(&get, "data_rate").map(|kbps| kbps.saturating_mul(1000));
                }
                info.set_audio_codec("E-AC-3", "E-AC-3 (Dolby Digital Plus)", CodecEvidence::ConfigBox);
                // Fallback sample rate when sample entry reported 0: scan property strings for fscod.
                if info.audio_sample_rate.is_none() {
                    for (k, v) in props.properties.iter() {
                        if k.starts_with("independent_substream")
                            && let AtomPropertyValue::Table(t) = v
                        {
                            for row in &t.rows {
                                if row.len() >= 2
                                    && String::from(&row[0]) == "fscod"
                                    && let Ok(fscod) = String::from(&row[1]).parse::<u8>()
                                {
                                    info.audio_sample_rate = fscod_to_hz(fscod);
                                    break;
                                }
                            }
                        }
                        if info.audio_sample_rate.is_some() {
                            break;
                        }
                    }
                }
            }
            "AC4SpecificBox" => {
                info.set_audio_codec("AC-4", "Dolby AC-4", CodecEvidence::ConfigBox);
            }
            "PixelAspectRatioBox" if in_video => {
                if let (Some(h), Some(v)) = (
                    get("h_spacing").and_then(|s| s.parse::<u32>().ok()),
                    get("v_spacing").and_then(|s| s.parse::<u32>().ok()),
                ) && h > 0 && v > 0 {
                    let g = gcd(h, v);
                    info.video_sar = Some(format!("{}:{}", h / g, v / g));
                }
            }
            "AVCConfigurationBox" if in_video => {
                // An `avcC` is only ever inside an AVC entry, encrypted or not.
                info.set_video_codec("H.264", CodecEvidence::ConfigBox);
                if let Some(p) = get("avc_profile_indication").and_then(|s| s.parse::<u8>().ok()) {
                    info.video_profile = Some(avc_profile_name(p).into());
                }
                if let Some(l) = get("avc_level_indication").and_then(|s| s.parse::<u8>().ok()) {
                    info.video_level = Some(format!("{:.1}", l as f32 / 10.0));
                }
                // The bit depth and chroma format only appear in the record's extension,
                // which the High profiles that can vary them carry and the others omit.
                // Where they are absent the format is not 8-bit 4:2:0 by definition, it is
                // unstated — so the row stays empty rather than repeating a common default
                // back as if it had been measured.
                if let Some(bd) = get("ext_bit_depth_luma").and_then(|s| s.parse::<u8>().ok())
                    && bd > 0 { info.video_bit_depth = Some(bd); }
                if let Some(cf) = get("ext_chroma_format").and_then(|s| s.parse::<u8>().ok())
                    && let Some(bd) = info.video_bit_depth {
                    info.video_pixel_format = Some(chroma_pixel_fmt(cf, bd));
                }
            }
            "HEVCConfigurationBox" if in_video => {
                info.set_video_codec("H.265 / HEVC", CodecEvidence::ConfigBox);
                if let Some(p) = get("general_profile_idc").and_then(|s| s.parse::<u8>().ok()) {
                    info.video_profile = Some(hevc_profile_name(p).into());
                }
                if let Some(l) = get("general_level_idc").and_then(|s| s.parse::<u8>().ok()) {
                    info.video_level = Some(format!("{:.1}", l as f32 / 30.0));
                }
                if let Some(bd) = get("bit_depth_luma_minus8").and_then(|s| s.parse::<u8>().ok()) {
                    info.video_bit_depth = Some(8 + bd);
                }
                if let Some(cf) = get("chroma_format_idc").and_then(|s| s.parse::<u8>().ok()) {
                    let bd = info.video_bit_depth.unwrap_or(8);
                    info.video_pixel_format = Some(chroma_pixel_fmt(cf, bd));
                }
            }
            "ColourInformationBox" => {
                if let Some(n) = get("colour_primaries").and_then(|s| s.parse::<u16>().ok()) {
                    info.video_color_primaries = Some(color_primaries_name(n));
                }
                if let Some(n) = get("transfer_characteristics").and_then(|s| s.parse::<u16>().ok()) {
                    info.video_transfer_char = Some(transfer_name(n));
                }
                if let Some(n) = get("matrix_coefficients").and_then(|s| s.parse::<u16>().ok()) {
                    info.video_matrix_coefficients = Some(matrix_name(n));
                }
                if let Some(fr) = get("full_range_flag") { info.video_full_range = Some(fr == "true"); }
                // QuickTime nclc
                if let Some(n) = get("primaries_index").and_then(|s| s.parse::<u16>().ok()) {
                    info.video_color_primaries = Some(color_primaries_name(n));
                }
                if let Some(n) = get("transfer_function_index").and_then(|s| s.parse::<u16>().ok()) {
                    info.video_transfer_char = Some(transfer_name(n));
                }
                if let Some(n) = get("matrix_index").and_then(|s| s.parse::<u16>().ok()) {
                    info.video_matrix_coefficients = Some(matrix_name(n));
                }
            }
            "ProtectionSystemSpecificHeaderBox" => {
                let sys = get("system_ref").unwrap_or_default();
                let id  = get("system_id").unwrap_or_default();
                if !sys.is_empty() || !id.is_empty() {
                    merge_drm(&mut info.drm_systems, [DrmInfo::new(&sys, &id)]);
                }
            }
            _ => {}
        }
    }
    info
}

// ── Main probe function ───────────────────────────────────────────────────────

/// Cache key for an init-segment fetch: absolute URI plus optional byte range.
type InitCacheKey = (String, Option<(u64, u64)>);

fn init_cache_key(url: &str, range: Option<RequestRange>) -> InitCacheKey {
    (url.to_string(), range.map(|r| (r.start, r.end)))
}

/// Fetch many playlist bodies in parallel. Duplicate URIs should be deduped by the caller.
async fn fetch_texts_parallel(uris: Vec<String>) -> HashMap<String, Result<String, String>> {
    let futs = uris.into_iter().map(|uri| async move {
        let result = match fetch_text(uri.clone()).await {
            Ok(r) => Ok(r.response_text),
            Err(e) => Err(e.to_string()),
        };
        (uri, result)
    });
    futures::future::join_all(futs).await.into_iter().collect()
}

/// Fetch + probe many init segments in parallel.
async fn fetch_inits_parallel(
    targets: Vec<(String, Option<RequestRange>)>,
) -> HashMap<InitCacheKey, Result<Mp4ProbeInfo, String>> {
    let futs = targets.into_iter().map(|(url, range)| async move {
        let key = init_cache_key(&url, range);
        let result = match fetch_array_buffer(url, range).await {
            Ok(resp) => Ok(probe_mp4(resp.response_body)),
            Err(e) => Err(e.to_string()),
        };
        (key, result)
    });
    futures::future::join_all(futs).await.into_iter().collect()
}

/// Map audio-only variant URIs to their declared BANDWIDTH, so a rendition pointing at the
/// same playlist can report a bitrate without fetching an init segment. Variants carrying
/// video are excluded because their BANDWIDTH covers the muxed video too.
fn audio_only_bandwidths(master: &MasterPlaylist) -> HashMap<&str, u64> {
    master
        .variants
        .iter()
        .filter(|v| {
            !v.is_iframe
                && v.resolution.is_none()
                && v.codecs
                    .as_deref()
                    .is_none_or(|c| parse_video_codec(c).0.is_none())
        })
        .filter_map(|v| Some((v.uri.as_str(), v.bandwidth?)))
        .collect()
}

fn apply_audio_init(at: &mut AudioTrackInfo, mp4: &Mp4ProbeInfo) {
    if at.codec.is_none() {
        at.codec = mp4.audio_codec_override.clone();
        at.codec_long = mp4.audio_codec_long.clone();
    }
    if at.sample_rate.is_none() {
        at.sample_rate = mp4.audio_sample_rate;
    }
    if at.bit_depth.is_none() {
        at.bit_depth = mp4.audio_bit_depth;
    }
    if at.bitrate_bps.is_none() {
        at.bitrate_bps = mp4.audio_bitrate_bps;
    }
    if at.channels.is_none() {
        at.channels = mp4.audio_channels.map(|c| c as u32);
        if let Some(n) = at.channels {
            at.channel_layout.replace(channels_to_layout(n));
        }
    }
}

fn mp4_has_audio(mp4: &Mp4ProbeInfo) -> bool {
    mp4.audio_sample_rate.is_some()
        || mp4.audio_codec_override.is_some()
        || mp4.audio_channels.is_some()
        || mp4.audio_bitrate_bps.is_some()
}

fn apply_video_init(vt: &mut VideoTrackInfo, mp4: &Mp4ProbeInfo) {
    if vt.codec.is_none()
        && let Some(c) = mp4.video_codec_override.clone()
    {
        vt.codec = Some(c);
    }
    if vt.profile.is_none() {
        vt.profile = mp4.video_profile.clone();
    }
    if vt.level.is_none() {
        vt.level = mp4.video_level.clone();
    }
    if let Some(v) = mp4.video_bit_depth {
        vt.bit_depth = Some(v);
    }
    // A colr box is the only place these are stated rather than implied, so anything it
    // supplies both replaces the VIDEO-RANGE convention and marks the row as measured.
    if let Some(v) = mp4.video_color_primaries.clone() {
        vt.color_primaries = Some(v);
        vt.color_measured = true;
    }
    if let Some(v) = mp4.video_transfer_char.clone() {
        vt.transfer_characteristics = Some(v);
        vt.color_measured = true;
    }
    if let Some(v) = mp4.video_matrix_coefficients.clone() {
        vt.matrix_coefficients = Some(v);
        vt.color_measured = true;
    }
    if let Some(v) = mp4.video_full_range {
        vt.full_range = Some(v);
    }
    if let Some(v) = mp4.video_pixel_format.clone() {
        vt.pixel_format = Some(v);
    }
    // A pasp is what actually states the sample aspect ratio. With it the display aspect
    // ratio follows from the coded resolution; without it the resolution is all there is.
    if let Some(sar) = mp4.video_sar.clone() {
        vt.dar = display_aspect_ratio(vt.resolution.as_deref(), Some(&sar));
        vt.sar = Some(sar);
    }
}

/// Display aspect ratio in lowest terms: the coded resolution stretched by the sample
/// aspect ratio, which is 1:1 unless a `pasp` says otherwise.
fn display_aspect_ratio(resolution: Option<&str>, sar: Option<&str>) -> Option<String> {
    let (w, h) = resolution.and_then(parse_wh)?;
    let (sar_h, sar_v) = match sar {
        Some(s) => parse_ratio(s)?,
        None => (1, 1),
    };
    let (w, h) = (w.checked_mul(sar_h)?, h.checked_mul(sar_v)?);
    let g = gcd(w, h);
    (g > 0).then(|| format!("{}:{}", w / g, h / g))
}

fn parse_wh(resolution: &str) -> Option<(u32, u32)> {
    parse_pair(resolution, 'x')
}

fn parse_ratio(ratio: &str) -> Option<(u32, u32)> {
    parse_pair(ratio, ':')
}

fn parse_pair(text: &str, sep: char) -> Option<(u32, u32)> {
    let (a, b) = text.split_once(sep)?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// Take the protocol description from one media playlist. Called for the first playlist
/// that was read; the fields it fills describe the presentation as a whole.
fn apply_protocol_from_media(r: &mut ProbeReport, pl: &MediaPlaylist) {
    r.target_duration = Some(pl.target_duration);
    r.playlist_type = pl.playlist_type.clone();
    r.is_live = !pl.has_endlist;
    r.total_segments = pl.segments.len();
    r.duration_s = Some(pl.segments.iter().map(|s| s.duration).sum());
    if let Some(sc) = &pl.server_control {
        r.ll_hls = Some(LlHlsInfo {
            part_hold_back: sc.part_hold_back,
            can_skip_until: sc.can_skip_until,
            can_block_reload: sc.can_block_reload,
        });
    }
    r.protocol_set = true;
}

/// Count the elementary streams a master playlist offers. I-frame variants are trick-play
/// copies of a video stream that is already counted, and a CLOSED-CAPTIONS rendition is
/// carried inside a video stream rather than being a stream of its own, so neither adds to
/// the total.
fn stream_count(master: &MasterPlaylist) -> usize {
    master.variants.iter().filter(|v| !v.is_iframe).count()
        + master
            .media_renditions
            .iter()
            .filter(|m| m.media_type != "CLOSED-CAPTIONS")
            .count()
}

/// Add one playlist's keys to the report. Every playlist contributes: renditions are
/// routinely encrypted under different systems, and a stream that protects only its video
/// would otherwise be described by whichever playlist happened to be read first.
fn merge_key_info(r: &mut ProbeReport, body: &str) {
    let (methods, formats) = parse_key_info(body);
    merge_strings(&mut r.encryption_methods, methods);
    merge_strings(&mut r.key_formats, formats.clone());
    merge_drm(&mut r.drm_systems, drm_from_key_formats(&formats));
}

fn merge_strings(dst: &mut Vec<String>, src: impl IntoIterator<Item = String>) {
    for s in src {
        if !dst.contains(&s) {
            dst.push(s);
        }
    }
}

/// Add DRM systems not already listed. Both sides carry a [`canonical_system_id`], so a
/// `pssh`'s dashless hex and a KEYFORMAT's dashed UUID match instead of appearing twice.
fn merge_drm(dst: &mut Vec<DrmInfo>, drm: impl IntoIterator<Item = DrmInfo>) {
    for d in drm {
        if !dst.iter().any(|e| e.system_id == d.system_id) {
            dst.push(d);
        }
    }
}

/// Deduplicate strings preserving first-seen order.
fn unique_preserving(uris: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for u in uris {
        if seen.insert(u.clone()) {
            out.push(u);
        }
    }
    out
}

/// What the container is, as far as the playlists say. An EXT-X-MAP names an
/// initialisation segment, which only a fragmented MP4 needs; segments that carry no MAP
/// initialise themselves, which is MPEG-TS or a packed audio elementary stream. Where no
/// media playlist was read the transport is simply unknown.
fn format_name(saw_map: bool, saw_segments_without_map: bool) -> String {
    match (saw_map, saw_segments_without_map) {
        (true, true) => "HLS (fMP4 and self-initialising segments)",
        (true, false) => "HLS (fMP4 / CMAF)",
        (false, true) => "HLS (no EXT-X-MAP — MPEG-TS or packed audio)",
        (false, false) => "HLS",
    }
    .into()
}

/// Checks that need an init segment and describe one rendition, so every variant's
/// playlist has to be read to find every init.
fn is_per_variant_init_check(id: &str) -> bool {
    matches!(id,
        "video_codec"|"video_profile"|"video_level"|"video_bit_depth"|"video_primaries"|
        "video_transfer"|"video_matrix"|"video_pixel_fmt"|
        "audio_codec"|"audio_rate"|"audio_depth"|"audio_bitrate")
}

/// Checks that need an init segment but describe the presentation as a whole. One init
/// answers them, so there is no reason to fetch every variant playlist to find more.
fn is_stream_wide_init_check(id: &str) -> bool {
    matches!(id, "container_ftyp" | "drm_systems")
}

async fn probe_stream(url: &str, selected: &HashSet<String>) -> Result<ProbeReport, FetchError> {
    let mut r = ProbeReport { url: url.to_string(), ..Default::default() };

    let per_variant_init = selected.iter().any(|id| is_per_variant_init_check(id));
    let needs_init = per_variant_init || selected.iter().any(|id| is_stream_wide_init_check(id));

    let resp = fetch_text(url.to_string()).await?;
    let content = &resp.response_text;

    if is_master_playlist(content) {
        r.manifest_type = "HLS Multivariant Playlist".into();
        let master = parse_master_playlist(url, content);
        r.definitions = master.definitions.clone();
        r.hls_version = Some(master.version);
        r.session_tags = parse_session_data(content);
        r.stream_count = stream_count(&master);
        // EXT-X-SESSION-KEY announces the stream's DRM on the multivariant playlist, for
        // players that fetch keys before they pick a variant.
        merge_key_info(&mut r, content);

        // Collect audio codec mappings (group_id → codec token from CODECS)
        let audio_codec_map: HashMap<String, String> = master
            .variants
            .iter()
            .filter_map(|v| {
                let grp = v.audio_group.as_ref()?;
                let codecs = v.codecs.as_ref()?;
                let audio = audio_codec_token(codecs)?;
                Some((grp.clone(), audio))
            })
            .collect();

        // Video tracks — skip I-frame-only playlists (EXT-X-I-FRAME-STREAM-INF)
        for variant in master.variants.iter().filter(|v| !v.is_iframe) {
            // RESOLUTION is the coded size, so on its own it gives the display aspect
            // ratio only for square pixels. The sample aspect ratio is left unset until a
            // `pasp` in the init segment states it.
            let dar = display_aspect_ratio(variant.resolution.as_deref(), None);
            let (codec, codec_long, profile, level) = variant
                .codecs
                .as_deref()
                .map(parse_video_codec)
                .unwrap_or((None, None, None, None));
            let hdr_format = match variant.video_range.as_deref() {
                Some("PQ") => Some("HDR10 / Dolby Vision".into()),
                Some("HLG") => Some("HLG".into()),
                _ => None,
            };
            // Colour defaults inferred from VIDEO-RANGE. Init-segment probe overrides
            // these when a colr box is present; otherwise they remain correct for SDR.
            let (primaries, transfer, matrix) = match variant.video_range.as_deref() {
                Some("PQ") => ("BT.2020", "PQ (ST 2084 / HDR10)", "BT.2020 NCL"),
                Some("HLG") => ("BT.2020", "HLG (ARIB STD-B67)", "BT.2020 NCL"),
                _ => ("BT.709", "BT.709", "BT.709"),
            };
            let name = variant.resolution.clone().unwrap_or_else(|| {
                variant
                    .bandwidth
                    .map(|b| format!("{}k", b / 1000))
                    .unwrap_or_default()
            });
            r.video_tracks.push(VideoTrackInfo {
                name,
                codec,
                codec_long,
                profile,
                level,
                resolution: variant.resolution.clone(),
                frame_rate: variant.frame_rate,
                bitrate_bps: variant.bandwidth,
                avg_bitrate_bps: variant.average_bandwidth,
                color_space: variant.video_range.clone(),
                hdr_format,
                color_primaries: Some(primaries.into()),
                transfer_characteristics: Some(transfer.into()),
                matrix_coefficients: Some(matrix.into()),
                color_measured: false,
                dar,
                playlist_uri: Some(variant.uri.clone()),
                ..Default::default()
            });
        }

        // Trick-play playlists are listed rather than probed: they hold I-frames at a
        // rate no player renders continuously, and carry no audio.
        r.iframe_variants = master
            .variants
            .iter()
            .filter(|v| v.is_iframe)
            .map(|v| {
                let size = v.resolution.clone().unwrap_or_else(|| "unknown size".into());
                match v.bandwidth {
                    Some(b) => format!("{size} @ {}", fmt_bps(b)),
                    None => size,
                }
            })
            .collect();

        r.overall_bitrate_bps = master
            .variants
            .iter()
            .filter(|v| !v.is_iframe)
            .filter_map(|v| v.bandwidth)
            .max();

        let audio_only_bandwidth = audio_only_bandwidths(&master);

        // Audio tracks
        for rend in master.media_renditions.iter().filter(|r| r.media_type == "AUDIO") {
            let (codec, codec_long) = audio_codec_map
                .get(&rend.group_id)
                .map(|raw| parse_audio_codec(raw))
                .unwrap_or((None, None));
            let bitrate_bps = rend
                .uri
                .as_deref()
                .and_then(|u| audio_only_bandwidth.get(u).copied());
            r.audio_tracks.push(AudioTrackInfo {
                name: rend.name.clone(),
                group_id: rend.group_id.clone(),
                codec,
                codec_long,
                channels: rend.channels,
                channel_layout: rend.channels.map(channels_to_layout),
                bitrate_bps,
                language: rend.language.clone(),
                is_default: rend.is_default,
                playlist_uri: rend.uri.clone(),
                ..Default::default()
            });
        }

        // Subtitle tracks
        for rend in master.media_renditions.iter().filter(|r| r.media_type == "SUBTITLES") {
            r.subtitle_tracks.push(SubtitleTrackInfo {
                name: rend.name.clone(),
                language: rend.language.clone(),
                is_default: rend.is_default,
            });
        }

        // Caption tracks
        for rend in master.media_renditions.iter().filter(|r| r.media_type == "CLOSED-CAPTIONS") {
            r.caption_tracks.push(CaptionTrackInfo {
                name: rend.name.clone(),
                group_id: rend.group_id.clone(),
                language: rend.language.clone(),
                is_default: rend.is_default,
            });
        }

        // Probe media playlists.
        // - Protocol metadata comes from the first *successful* unique playlist fetch.
        // - Keys come from every playlist that was read.
        // - Every variant's playlist is only worth fetching when a per-rendition init
        //   check is selected; otherwise one playlist yields the one init that answers
        //   the stream-wide checks.
        let non_iframe_variants: Vec<_> = master.variants.iter().filter(|v| !v.is_iframe).collect();
        if non_iframe_variants.is_empty() && !r.iframe_variants.is_empty() {
            r.probe_notes.push(
                "Master playlist declares only EXT-X-I-FRAME-STREAM-INF variants — no regular STREAM-INF playlists to probe."
                    .into(),
            );
        }
        let unique_variant_uris = unique_preserving(
            non_iframe_variants.iter().map(|v| v.uri.clone()),
        );

        let mut playlist_bodies: HashMap<String, Result<String, String>> = HashMap::new();

        if per_variant_init {
            playlist_bodies = fetch_texts_parallel(unique_variant_uris.clone()).await;
        } else {
            // One successful media playlist covers duration, encryption, LL-HLS and — via
            // its EXT-X-MAP — the container brand and any PSSH.
            for uri in &unique_variant_uris {
                match fetch_text(uri.clone()).await {
                    Ok(resp) => {
                        playlist_bodies.insert(uri.clone(), Ok(resp.response_text));
                        break;
                    }
                    Err(e) => {
                        playlist_bodies.insert(uri.clone(), Err(e.to_string()));
                    }
                }
            }
        }

        // Each playlist is parsed once, and everything derived from it is taken here.
        let mut parsed_playlists: HashMap<String, MediaPlaylist> = HashMap::new();
        let mut init_targets: Vec<(String, Option<RequestRange>)> = Vec::new();
        let mut init_seen: HashSet<InitCacheKey> = HashSet::new();
        let mut saw_map = false;
        let mut saw_segments_without_map = false;

        for uri in &unique_variant_uris {
            let Some(Ok(body)) = playlist_bodies.get(uri) else {
                continue;
            };
            let pl = parse_media_playlist(uri, body, &r.definitions);
            if !r.protocol_set {
                apply_protocol_from_media(&mut r, &pl);
            }
            merge_key_info(&mut r, body);
            match pl.init_segment() {
                Some((iurl, irange)) => {
                    saw_map = true;
                    if needs_init && init_seen.insert(init_cache_key(&iurl, irange)) {
                        init_targets.push((iurl, irange));
                    }
                }
                None => saw_segments_without_map |= !pl.segments.is_empty(),
            }
            parsed_playlists.insert(uri.clone(), pl);
        }

        r.format_name = Some(format_name(saw_map, saw_segments_without_map));

        if !r.protocol_set && !unique_variant_uris.is_empty() {
            r.probe_notes.push(
                "Could not fetch any variant media playlist — duration and protocol fields unavailable."
                    .into(),
            );
        }

        let mut first_muxed_audio: Option<Mp4ProbeInfo> = None;

        if needs_init {
            let init_results = fetch_inits_parallel(init_targets).await;
            // Variants with no EXT-X-MAP are reported together: one line per rendition
            // says the same thing as many times as the master has variants.
            let mut variants_without_map: Vec<String> = Vec::new();
            // Whether the notes already account for there being no init-segment fields.
            let mut init_explained = false;

            for vt in r.video_tracks.iter_mut() {
                let Some(pl_uri) = vt.playlist_uri.clone() else {
                    continue;
                };
                let Some(pl) = parsed_playlists.get(&pl_uri) else {
                    if let Some(Err(e)) = playlist_bodies.get(&pl_uri) {
                        r.probe_notes
                            .push(format!("Variant playlist fetch failed for {}: {e}", vt.name));
                        init_explained = true;
                    }
                    continue;
                };
                let Some((iurl, irange)) = pl.init_segment() else {
                    variants_without_map.push(vt.name.clone());
                    continue;
                };
                let key = init_cache_key(&iurl, irange);
                match init_results.get(&key) {
                    Some(Ok(mp4)) => {
                        if r.major_brand.is_none() {
                            r.major_brand = mp4.major_brand.clone();
                        }
                        apply_video_init(vt, mp4);
                        if first_muxed_audio.is_none() {
                            first_muxed_audio = Some(mp4.clone());
                        }
                        merge_drm(&mut r.drm_systems, mp4.drm_systems.clone());
                        r.init_segment_probed = true;
                        r.init_partially_decoded |= init_walk_was_partial(mp4);
                    }
                    Some(Err(e)) => {
                        r.probe_notes
                            .push(format!("Init segment fetch failed for {}: {e}", vt.name));
                        init_explained = true;
                    }
                    None => {}
                }
            }

            // Muxed audio lives in the video init — fill tracks with no demuxed playlist,
            // or synthesise a track when the master declares no EXT-X-MEDIA:AUDIO at all.
            if let Some(mp4) = first_muxed_audio {
                let mut applied = false;
                for at in r.audio_tracks.iter_mut().filter(|a| a.playlist_uri.is_none()) {
                    apply_audio_init(at, &mp4);
                    applied = true;
                }
                if !applied && mp4_has_audio(&mp4) {
                    let mut at = AudioTrackInfo {
                        name: "Audio (muxed)".into(),
                        ..Default::default()
                    };
                    apply_audio_init(&mut at, &mp4);
                    r.audio_tracks.push(at);
                }
            }

            if !variants_without_map.is_empty() {
                r.probe_notes.push(format!(
                    "No EXT-X-MAP in {} of {} video rendition(s) ({}) — init-segment checks skipped for them.",
                    variants_without_map.len(),
                    r.video_tracks.len(),
                    variants_without_map.join(", "),
                ));
                init_explained = true;
            }

            // Demuxed audio: probe each rendition's own init in parallel.
            let audio_uris: Vec<String> = unique_preserving(
                r.audio_tracks
                    .iter()
                    .filter(|at| audio_needs_init(at))
                    .filter_map(|at| at.playlist_uri.clone()),
            );

            if !audio_uris.is_empty() {
                let audio_playlists = fetch_texts_parallel(audio_uris.clone()).await;
                let mut audio_init_targets: Vec<(String, Option<RequestRange>)> = Vec::new();
                let mut audio_init_seen: HashSet<InitCacheKey> = HashSet::new();
                let mut audio_parsed: HashMap<String, MediaPlaylist> = HashMap::new();

                for uri in &audio_uris {
                    match audio_playlists.get(uri) {
                        Some(Ok(body)) => {
                            let pl = parse_media_playlist(uri, body, &r.definitions);
                            if let Some((iurl, irange)) = pl.init_segment() {
                                let key = init_cache_key(&iurl, irange);
                                if audio_init_seen.insert(key) {
                                    audio_init_targets.push((iurl, irange));
                                }
                            }
                            audio_parsed.insert(uri.clone(), pl);
                        }
                        Some(Err(e)) => {
                            r.probe_notes
                                .push(format!("Audio playlist fetch failed for {uri}: {e}"));
                            init_explained = true;
                        }
                        None => {}
                    }
                }

                let audio_inits = fetch_inits_parallel(audio_init_targets).await;
                let mut audio_without_map: Vec<String> = Vec::new();

                for at in r.audio_tracks.iter_mut() {
                    let Some(apl_url) = at.playlist_uri.clone() else {
                        continue;
                    };
                    if !audio_needs_init(at) {
                        continue;
                    }
                    let Some(pl) = audio_parsed.get(&apl_url) else {
                        continue;
                    };
                    let Some((aiurl, airange)) = pl.init_segment() else {
                        audio_without_map.push(format!("{} ({})", at.name, at.group_id));
                        continue;
                    };
                    let key = init_cache_key(&aiurl, airange);
                    match audio_inits.get(&key) {
                        Some(Ok(amp4)) => {
                            // An audio-only init carries an ftyp of its own, and probing
                            // it is a probe of an init segment however the video went.
                            if r.major_brand.is_none() {
                                r.major_brand = amp4.major_brand.clone();
                            }
                            apply_audio_init(at, amp4);
                            merge_drm(&mut r.drm_systems, amp4.drm_systems.clone());
                            r.init_segment_probed = true;
                            r.init_partially_decoded |= init_walk_was_partial(amp4);
                        }
                        Some(Err(e)) => {
                            r.probe_notes.push(format!(
                                "Init segment fetch failed for audio {} ({}): {e}",
                                at.name, at.group_id
                            ));
                            init_explained = true;
                        }
                        None => {}
                    }
                }

                if !audio_without_map.is_empty() {
                    r.probe_notes.push(format!(
                        "No EXT-X-MAP in audio rendition(s) {} — init-segment checks skipped for them.",
                        audio_without_map.join(", "),
                    ));
                    init_explained = true;
                }
            }

            // Said after both the video and the audio renditions have had their turn, since
            // a demuxed audio init answers the init-segment checks as well as a video one
            // does — and only when nothing above has already accounted for the gap.
            if !r.init_segment_probed && !init_explained {
                r.probe_notes.push(
                    "No EXT-X-MAP init segment found — skipping init-segment checks.".into(),
                );
            }
        }

    } else {
        // Single media playlist
        r.manifest_type = "HLS Media Playlist".into();
        let pl = parse_media_playlist(url, content, &r.definitions);
        // A media-only URL's own EXT-X-DEFINE tags are the only variables in scope, and
        // the viewer links below are built from them.
        r.definitions = pl.definitions.clone();
        r.hls_version = Some(pl.version);
        apply_protocol_from_media(&mut r, &pl);
        merge_key_info(&mut r, content);
        r.stream_count = 1;
        r.session_tags = parse_session_data(content);
        let has_map = pl.init_segment().is_some();
        r.format_name = Some(format_name(has_map, !has_map && !pl.segments.is_empty()));
        // Init segment probe for direct media playlist URLs
        if needs_init {
            if let Some((iurl, irange)) = pl.init_segment() {
                match fetch_array_buffer(iurl, irange).await {
                    Ok(resp) => {
                        let mp4 = probe_mp4(resp.response_body);
                        r.major_brand = mp4.major_brand.clone();
                        r.init_partially_decoded |= init_walk_was_partial(&mp4);
                        // Synthesise a single video track from what the init segment tells
                        // us. Everything here was read out of a box, so the colour fields
                        // are measured rather than inferred from a VIDEO-RANGE.
                        let mut vt = VideoTrackInfo {
                            name: url.split('/').next_back().unwrap_or("stream").to_string(),
                            ..Default::default()
                        };
                        apply_video_init(&mut vt, &mp4);
                        if vt.codec.is_some() || vt.profile.is_some() {
                            r.video_tracks.push(vt);
                        }
                        // Audio from muxed init
                        if mp4_has_audio(&mp4) {
                            let mut at = AudioTrackInfo {
                                name: "Audio".into(),
                                ..Default::default()
                            };
                            apply_audio_init(&mut at, &mp4);
                            r.audio_tracks.push(at);
                        }
                        merge_drm(&mut r.drm_systems, mp4.drm_systems);
                        r.init_segment_probed = true;
                    }
                    Err(e) => { r.probe_notes.push(format!("Init segment fetch failed: {e}")); }
                }
            } else {
                r.probe_notes.push("No EXT-X-MAP init segment found — skipping init-segment checks.".into());
            }
        }
    }

    Ok(r)
}

/// An init is only fully read when the walk reached the last byte without skipping a box.
fn init_walk_was_partial(mp4: &Mp4ProbeInfo) -> bool {
    mp4.decode_errors > 0 || !mp4.walk_complete
}

/// What the DRM row may say when no system was found. Only a complete walk of a fetched
/// init can support "there are none".
fn drm_status_label(probed: bool, partial: bool) -> &'static str {
    match (probed, partial) {
        (false, _) => "Init segment not probed",
        (true, true) => "None in the part of the init segment that decoded",
        (true, false) => "None found (no PSSH or known KEYFORMAT)",
    }
}

/// Copy for the Low-Latency HLS row when there is no SERVER-CONTROL to render.
/// `None` means no media playlist was read, so the row must not claim "Not supported".
fn ll_hls_absence_label(protocol_set: bool) -> Option<&'static str> {
    protocol_set.then_some("Not signalled")
}

// ── UI ────────────────────────────────────────────────────────────────────────

#[component]
pub fn Ffprobe() -> impl IntoView {
    let (url, set_url) = signal(String::new());
    let selected = RwSignal::new(all_check_ids());
    let (report, set_report) = signal(None::<ProbeReport>);
    let (error_msg, set_error_msg) = signal(None::<String>);
    let (loading, set_loading) = signal(false);
    let probe_gen = RwSignal::new(0u64);

    let toggle_check = move |id: String| {
        selected.update(|s| {
            if s.contains(&id) { s.remove(&id); } else { s.insert(id); }
        });
    };

    let toggle_category = move |cat_id: &'static str| {
        let cat_items: Vec<&'static str> = CATEGORIES.iter()
            .find(|c| c.id == cat_id)
            .map(|c| c.items.iter().map(|i| i.id).collect())
            .unwrap_or_default();
        let all_selected = cat_items.iter().all(|id| selected.get_untracked().contains(*id));
        selected.update(|s| {
            for id in &cat_items {
                if all_selected { s.remove(*id); } else { s.insert(id.to_string()); }
            }
        });
    };

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        // A pasted URL routinely arrives with a trailing newline or space, which the fetch
        // would send verbatim and the origin would reject.
        let u = url.get().trim().to_string();
        if u.is_empty() { return; }
        let sel = selected.get();
        let generation = probe_gen.get_untracked() + 1;
        probe_gen.set(generation);
        set_loading.set(true);
        set_error_msg.set(None);
        set_report.set(None);
        leptos::task::spawn_local(async move {
            match probe_stream(&u, &sel).await {
                Ok(r) => {
                    if probe_gen.get_untracked() == generation {
                        set_report.set(Some(r));
                        set_loading.set(false);
                    }
                }
                Err(e) => {
                    if probe_gen.get_untracked() == generation {
                        set_error_msg.set(Some(format!("Probe failed: {e}")));
                        set_loading.set(false);
                    }
                }
            }
        });
    };

    view! {
        <div class="body-content" style="max-width: min(96vw, 1440px); margin-bottom: 2em;">
            <div>
                <div class="body-content">
                    <h1 class="body-content">
                        "Stream Inspector"
                    </h1>
                    <p class="body-content body-text">
                        "Enter an HLS stream URL. Choose the checks you want, then click Probe. \
                         Checks marked \u{201c}(init seg)\u{201d} will fetch the first init segment \u{2014} \
                         these work even on encrypted streams because init segments carry \
                         unencrypted codec and DRM metadata. Where a value is inferred from a \
                         playlist attribute rather than read from the init segment, the table \
                         marks it \u{201c}assumed\u{201d}."
                    </p>
                </div>
            </div>

            // ── Input card ──────────────────────────────────────────────────
            <div style="background: var(--color-white); border: 1px solid var(--color-sky-200); border-radius: 12px; \
                        padding: calc(var(--spacing) * 6) calc(var(--spacing) * 7); \
                        box-shadow: 0 2px 12px rgba(0,0,0,.06); \
                        margin-top: calc(var(--spacing) * 6); margin-bottom: calc(var(--spacing) * 6);">
                <form on:submit=on_submit>
                    <div style="display: flex; gap: calc(var(--spacing) * 2.5); flex-wrap: wrap; margin-bottom: calc(var(--spacing) * 5);">
                        <input
                            type="text"
                            placeholder="https://example.com/stream/master.m3u8"
                            style="flex: 1; min-width: 260px; background: var(--color-sky-50); \
                                   border: 1.5px solid var(--color-sky-200); border-radius: 8px; \
                                   color: var(--color-sky-950); font-size: 1rem; \
                                   padding: calc(var(--spacing) * 3) calc(var(--spacing) * 4); outline: none;"
                            prop:value=move || url.get()
                            on:input=move |ev| set_url.set(event_target_value(&ev))
                        />
                        <button
                            type="submit"
                            style="background: linear-gradient(135deg, var(--color-sky-300), var(--color-sky-500)); \
                                   color: var(--color-white); border: none; border-radius: 8px; \
                                   padding: calc(var(--spacing) * 3) calc(var(--spacing) * 7); \
                                   font-size: 1rem; font-weight: 700; cursor: pointer; white-space: nowrap;"
                            disabled=move || loading.get()
                        >
                            {move || if loading.get() { "⏳ Probing…" } else { "🔍 Probe" }}
                        </button>
                    </div>

                    // ── Check category boxes ────────────────────────────────
                    <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); \
                                gap: calc(var(--spacing) * 3);">
                        {CATEGORIES.iter().map(|cat| {
                            let cat_id = cat.id;
                            view! {
                                <div style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); \
                                            border-radius: 8px; padding: calc(var(--spacing) * 3);">
                                    <div style="display: flex; align-items: center; gap: calc(var(--spacing) * 1.5); \
                                                margin-bottom: calc(var(--spacing) * 2);">
                                        <span style="font-size: .85rem;">{cat.icon}</span>
                                        <span style="font-size: .78rem; font-weight: 700; \
                                                     color: var(--color-sky-950); text-transform: uppercase; \
                                                     letter-spacing: .06em;">
                                            {cat.label}
                                        </span>
                                        <button
                                            type="button"
                                            // sky-700 rather than sky-300: this sits on the
                                            // sky-50 panel, where the lighter token is barely
                                            // distinguishable from the background.
                                            style="margin-left: auto; font-size: .65rem; font-weight: 600; \
                                                   color: var(--color-sky-700); background: none; border: none; \
                                                   cursor: pointer; padding: 0;"
                                            on:click=move |_| toggle_category(cat_id)
                                        >
                                            {move || {
                                                let cat_items: Vec<&str> = CATEGORIES.iter()
                                                    .find(|c| c.id == cat_id)
                                                    .map(|c| c.items.iter().map(|i| i.id).collect())
                                                    .unwrap_or_default();
                                                let all = cat_items.iter().all(|id| selected.get().contains(*id));
                                                if all { "Deselect all" } else { "Select all" }
                                            }}
                                        </button>
                                    </div>
                                    {cat.items.iter().map(|item| {
                                        let id = item.id.to_string();
                                        let id2 = id.clone();
                                        let label = item.label;
                                        let note = item.note;
                                        view! {
                                            <label style="display: flex; align-items: center; gap: calc(var(--spacing) * 1.5); \
                                                          font-size: .78rem; color: var(--color-sky-800); \
                                                          margin-bottom: var(--spacing); cursor: pointer;">
                                                <input
                                                    type="checkbox"
                                                    prop:checked=move || selected.get().contains(&id)
                                                    on:change=move |_| toggle_check(id2.clone())
                                                />
                                                {label}
                                                {note.map(|n| view! {
                                                    // Same reasoning as the Select-all button:
                                                    // sky-200 on sky-50 is unreadable.
                                                    <span style="font-size: .68rem; color: var(--color-sky-700); \
                                                                 font-style: italic;">
                                                        {format!("({})", n)}
                                                    </span>
                                                })}
                                            </label>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                </form>
            </div>

            // ── Loading bar ─────────────────────────────────────────────────
            {move || loading.get().then(|| view! {
                <div style="margin-bottom: calc(var(--spacing) * 4);">
                    <div style="height: 3px; background: var(--color-sky-200); border-radius: 2px; overflow: hidden;">
                        <div style="width: 40%; height: 100%; \
                                    background: linear-gradient(90deg, var(--color-sky-300), var(--color-sky-500)); \
                                    animation: progress 1.5s ease-in-out infinite; border-radius: 2px;">
                        </div>
                    </div>
                    <div style="font-size: .85rem; color: var(--color-sky-700); margin-top: calc(var(--spacing) * 1.5);">
                        "Fetching manifest and probing stream…"
                    </div>
                </div>
            })}

            // ── Error ───────────────────────────────────────────────────────
            {move || error_msg.get().map(|e| view! {
                <div style="padding: calc(var(--spacing) * 3.5) calc(var(--spacing) * 4.5); \
                            background: rgba(239,68,68,.12); \
                            border: 1px solid var(--color-red-400); border-radius: 8px; \
                            color: var(--color-red-400); font-size: .9rem; \
                            margin-bottom: calc(var(--spacing) * 4);">
                    {format!("⚠ {}", e)}
                </div>
            })}

            // ── Results ─────────────────────────────────────────────────────
            {move || report.get().map(|r| view! {
                <ProbeResults report=r selected=selected.get() />
            })}
        </div>
    }
}

// ── Results components ────────────────────────────────────────────────────────

#[component]
fn ProbeResults(report: ProbeReport, selected: HashSet<String>) -> impl IntoView {
    let url = report.url.clone();
    let manifest_type = report.manifest_type.clone();
    let definitions = report.definitions.clone();
    let probed_init = report.init_segment_probed;
    let partial_init = report.init_partially_decoded;
    let protocol_set = report.protocol_set;
    let mut notes = report.probe_notes.clone();
    if partial_init {
        notes.push(
            "The init segment was not fully decoded — a box was skipped or the walk ended \
             before the last byte — so init-segment fields may be incomplete."
                .into(),
        );
    }
    if !report.iframe_variants.is_empty() {
        notes.push(format!(
            "{} EXT-X-I-FRAME-STREAM-INF trick-play variant(s) declared ({}) — listed, not probed.",
            report.iframe_variants.len(),
            report.iframe_variants.join(", "),
        ));
    }
    let playlist_kind_label = if manifest_type.contains("Media") {
        "Media Playlist"
    } else {
        "Multivariant Playlist"
    };

    // Pre-compute all selection booleans before view! to avoid move issues
    let s_format_name   = selected.contains("format_name");
    let s_container     = selected.contains("container_ftyp");
    let s_duration      = selected.contains("duration");
    let s_overall_br    = selected.contains("bitrate");
    let s_stream_count  = selected.contains("stream_count");
    let s_session_tags  = selected.contains("session_tags");
    let s_hls_version   = selected.contains("hls_version");
    let s_target_dur    = selected.contains("target_duration");
    let s_playlist_type = selected.contains("playlist_type");
    let s_segment_count = selected.contains("segment_count");
    let s_ll_hls        = selected.contains("ll_hls");
    let s_enc_method    = selected.contains("enc_method");
    let s_key_format    = selected.contains("key_format");
    let s_drm_systems   = selected.contains("drm_systems");
    let s_sub_tracks    = selected.contains("subtitle_tracks");
    let s_cap_tracks    = selected.contains("caption_tracks");

    let show_format  = s_format_name || s_container || s_duration || s_overall_br || s_stream_count || s_session_tags;
    let show_hls     = s_hls_version || s_target_dur || s_playlist_type || s_ll_hls || s_segment_count;
    let show_drm     = s_enc_method || s_key_format || s_drm_systems;
    let show_subs    = s_sub_tracks || s_cap_tracks;
    let show_video_table = VIDEO_COL_DEFS.iter().any(|(id, _)| selected.contains(*id));
    let show_audio_table = AUDIO_COL_DEFS.iter().any(|(id, _)| selected.contains(*id));

    let tags = report.session_tags.clone();
    let ll   = report.ll_hls.clone();
    let enc  = report.encryption_methods.clone();
    let kf   = report.key_formats.clone();
    let drm  = report.drm_systems.clone();
    let subs = report.subtitle_tracks.clone();
    let caps = report.caption_tracks.clone();
    let video_tracks = report.video_tracks.clone();
    let audio_tracks = report.audio_tracks.clone();
    let video_sel = selected.clone();
    let audio_sel = selected;
    let defs_for_master = definitions.clone();
    let defs_for_video = definitions.clone();
    let defs_for_audio = definitions;

    view! {
        <div>
            // Header badge
            <div style="display: flex; align-items: flex-start; gap: calc(var(--spacing) * 3); \
                        padding: calc(var(--spacing) * 4) calc(var(--spacing) * 5); \
                        margin-bottom: calc(var(--spacing) * 5); \
                        background: rgba(56,189,248,.08); border: 1px solid rgba(56,189,248,.3); \
                        border-radius: 10px; flex-wrap: wrap;">
                <span style="font-size: 1.4rem; margin-top: 2px;">{"🔍"}</span>
                <div style="flex: 1; min-width: 0;">
                    <div style="font-size: .9rem; font-weight: 700; color: var(--color-sky-500); margin-bottom: calc(var(--spacing) * 2);">{manifest_type}</div>
                    // Primary playlist link
                    <div style="margin-bottom: 6px;">
                        <span style="font-size: .72rem; font-weight: 600; color: var(--color-sky-700); text-transform: uppercase; \
                                     letter-spacing: .05em; margin-right: calc(var(--spacing) * 2);">{playlist_kind_label}</span>
                        <a href={Url::parse(&url).ok().and_then(|b| playlist_href(b, "", &defs_for_master)).map(|h| format!("/hls-manifest-viewer/{}", h)).unwrap_or_default()}
                           target="_blank"
                           style="font-size: .75rem; color: var(--color-sky-500); word-break: break-all; text-decoration: none; \
                                  border-bottom: 1px dotted var(--color-sky-500);"
                        >{url.clone()}</a>
                    </div>
                    // Video media playlist links (one per variant, ascending by bitrate → pixel count)
                    {(!video_tracks.is_empty()).then(|| {
                        let mut links: Vec<(u64, String, String)> = video_tracks.iter()
                            .filter_map(|vt| vt.playlist_uri.clone().map(|uri| {
                                // Primary sort key: bitrate; fallback: pixel count from resolution
                                let sort_key = vt.bitrate_bps.unwrap_or_else(|| {
                                    vt.resolution.as_deref()
                                        .and_then(|r| {
                                            let mut pts = r.splitn(2, 'x');
                                            let w = pts.next()?.parse::<u64>().ok()?;
                                            let h = pts.next()?.parse::<u64>().ok()?;
                                            Some(w * h)
                                        })
                                        .unwrap_or(0)
                                });
                                (sort_key, vt.name.clone(), uri)
                            }))
                            .collect();
                        links.sort_by_key(|(k, _, _)| *k);
                        let links: Vec<(String, String)> = links.into_iter().map(|(_, l, u)| (l, u)).collect();
                        let defs = defs_for_video.clone();
                        (!links.is_empty()).then(|| view! {
                            <div style="margin-bottom: 4px;">
                                <span style="font-size: .72rem; font-weight: 600; color: var(--color-sky-700); text-transform: uppercase; \
                                             letter-spacing: .05em;">"Variant Playlists"</span>
                                <div style="display: flex; flex-wrap: wrap; gap: 6px; margin-top: 4px;">
                                    {links.into_iter().map(|(label, uri)| {
                                        let href = Url::parse(&uri).ok().and_then(|b| playlist_href(b, "", &defs)).map(|h| format!("/hls-manifest-viewer/{}", h)).unwrap_or_default();
                                        view! {
                                            <a href={href} target="_blank"
                                               style="font-size: .72rem; background: rgba(56,189,248,.12); \
                                                      border: 1px solid rgba(56,189,248,.35); border-radius: 4px; \
                                                      padding: calc(var(--spacing) * 0.5) calc(var(--spacing) * 2); \
                                                      color: var(--color-sky-500); text-decoration: none; white-space: nowrap;">
                                                {label}
                                            </a>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        })
                    })}
                    // Audio media playlist links (one per rendition that has a URI)
                    {(!audio_tracks.is_empty()).then(|| {
                        let links = audio_tracks.iter()
                            .filter_map(|at| at.playlist_uri.clone().map(|uri| (at.name.clone(), uri)))
                            .collect::<Vec<_>>();
                        let defs = defs_for_audio.clone();
                        (!links.is_empty()).then(|| view! {
                            <div>
                                <span style="font-size: .72rem; font-weight: 600; color: var(--color-sky-700); text-transform: uppercase; \
                                             letter-spacing: .05em;">"Audio Playlists"</span>
                                <div style="display: flex; flex-wrap: wrap; gap: 6px; margin-top: 4px;">
                                    {links.into_iter().map(|(label, uri)| {
                                        let href = Url::parse(&uri).ok().and_then(|b| playlist_href(b, "", &defs)).map(|h| format!("/hls-manifest-viewer/{}", h)).unwrap_or_default();
                                        view! {
                                            <a href={href} target="_blank"
                                               style="font-size: .72rem; background: rgba(16,185,129,.1); \
                                                      border: 1px solid rgba(16,185,129,.35); border-radius: 4px; \
                                                      padding: calc(var(--spacing) * 0.5) calc(var(--spacing) * 2); \
                                                      color: var(--color-green-600); text-decoration: none; white-space: nowrap;">
                                                {label}
                                            </a>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        })
                    })}
                </div>
                <div style="font-size: .75rem; color: var(--color-sky-700); white-space: nowrap;">
                    {if probed_init { "✓ Init segment fetched" } else { "" }}
                </div>
            </div>

            {(!notes.is_empty()).then(|| {
                let notes2 = notes.clone();
                view! {
                    <div style="margin-bottom: calc(var(--spacing) * 4); \
                                padding: calc(var(--spacing) * 3) calc(var(--spacing) * 4); \
                                background: rgba(245,158,11,.1); border: 1px solid rgba(245,158,11,.35); \
                                border-radius: 8px; font-size: .8rem; color: #d97706;">
                        {notes2.iter().map(|n| view! { <div>{format!("ⓘ {}", n)}</div> }).collect::<Vec<_>>()}
                    </div>
                }
            })}

            // ── Format & Container  +  HLS Protocol  (side by side)
            <div style="display: flex; gap: 20px; align-items: flex-start; flex-wrap: wrap;">
                <div style="flex: 1; min-width: 280px;">
                    <ProbeSection title="📦 Format & Container" show=show_format>
                        <ProbeRow label="Format" value=report.format_name.clone() show=s_format_name />
                        <ProbeRow label="Container (ftyp)" value=report.major_brand.clone() show=s_container />
                        <ProbeRow label="Duration" value=report.duration_s.map(fmt_dur) show=s_duration />
                        <ProbeRow label="Overall bitrate" value=report.overall_bitrate_bps.map(fmt_bps) show=s_overall_br />
                        <ProbeRow label="Streams" value=Some(report.stream_count.to_string()) show=s_stream_count />
                        {s_session_tags.then(|| {
                            view! {
                                <InfoRow label="Tags / metadata">
                                    {if tags.is_empty() {
                                        // An em dash, as every other empty row shows —
                                        // dropping the row made a selected check look
                                        // like it had not run.
                                        view! { <span>{"—"}</span> }.into_any()
                                    } else {
                                        view! {
                                            <div>
                                                {tags.iter().map(|(k, v)| view! {
                                                    <div style="font-size: .78rem;">
                                                        <b>{k.clone()}</b>{format!(": {}", v)}
                                                    </div>
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    }}
                                </InfoRow>
                            }
                        })}
                    </ProbeSection>
                </div>
                <div style="flex: 1; min-width: 280px;">
                    <ProbeSection title="📡 HLS Protocol" show=show_hls>
                        <ProbeRow label="HLS version" value=report.hls_version.map(|v| format!("{}", v)) show=s_hls_version />
                        <ProbeRow label="Target duration" value=report.target_duration.map(|d| format!("{} s", d)) show=s_target_dur />
                        // Both rows describe a media playlist, so they stay empty until one
                        // was read. "VOD" and a segment count of 0 are what the defaults
                        // look like, and printing them turned a failed fetch into a claim.
                        <ProbeRow label="Playlist type" value=protocol_set.then(|| if report.is_live { "Live (no EXT-X-ENDLIST)".into() } else { report.playlist_type.clone().unwrap_or_else(|| "VOD".into()) }) show=s_playlist_type />
                        <ProbeRow label="Segment count" value=protocol_set.then(|| report.total_segments.to_string()) show=s_segment_count />
                        {s_ll_hls.then(|| view! {
                            <div>
                                {if let Some(ll) = ll {
                                    view! {
                                        <InfoRow label="Low-Latency HLS">
                                            <div style="font-size: .78rem;">
                                                {ll.part_hold_back.map(|v| format!("PART-HOLD-BACK={:.3}s ", v)).unwrap_or_default()}
                                                {ll.can_skip_until.map(|v| format!("CAN-SKIP-UNTIL={:.1}s ", v)).unwrap_or_default()}
                                                {if ll.can_block_reload { "CAN-BLOCK-RELOAD=YES" } else { "" }}
                                            </div>
                                        </InfoRow>
                                    }.into_any()
                                } else {
                                    let status = ll_hls_absence_label(protocol_set).unwrap_or("—");
                                    view! { <InfoRow label="Low-Latency HLS"><span>{status}</span></InfoRow> }.into_any()
                                }}
                            </div>
                        })}
                    </ProbeSection>
                </div>
            </div>

            // ── Subtitles & Captions  +  Encryption & DRM  (side by side)
            {(show_subs || show_drm).then(|| view! {
                <div style="display: flex; gap: 20px; align-items: flex-start; flex-wrap: wrap;">
                    // Subtitles & Captions
                    <div style="flex: 1; min-width: 280px;">
                        {show_subs.then(|| view! {
                            <div>
                                <SectionTitle label="📝 Subtitles & Captions" />
                                <div style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 10px; padding: calc(var(--spacing) * 4) calc(var(--spacing) * 4.5); margin-bottom: calc(var(--spacing) * 7);">
                                    {if subs.is_empty() && caps.is_empty() {
                                        view! {
                                            <div style="font-size: .82rem; color: var(--color-sky-700); font-style: italic;">
                                                "No subtitle or caption tracks found in this stream."
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div>
                                                {(s_sub_tracks && !subs.is_empty()).then(|| view! {
                                                    <div style="margin-bottom: 10px;">
                                                        <div style="font-size: .75rem; font-weight: 700; color: var(--color-sky-700); text-transform: uppercase; margin-bottom: calc(var(--spacing) * 1.5);">"Subtitles"</div>
                                                        {subs.iter().map(|s| view! {
                                                            <div style="font-size: .82rem; color: var(--color-sky-950); margin-bottom: calc(var(--spacing) * 0.75);">
                                                                {format!("{} — lang: {} {}", s.name, s.language.as_deref().unwrap_or("—"), if s.is_default { "(default)" } else { "" })}
                                                            </div>
                                                        }).collect::<Vec<_>>()}
                                                    </div>
                                                })}
                                                {(s_cap_tracks && !caps.is_empty()).then(|| view! {
                                                    <div>
                                                        <div style="font-size: .75rem; font-weight: 700; color: var(--color-sky-700); text-transform: uppercase; margin-bottom: calc(var(--spacing) * 1.5);">"Closed Captions"</div>
                                                        {caps.iter().map(|c| view! {
                                                            <div style="font-size: .82rem; color: var(--color-sky-950); margin-bottom: calc(var(--spacing) * 0.75);">
                                                                {format!("{} (group: {}) — lang: {} {}", c.name, c.group_id, c.language.as_deref().unwrap_or("—"), if c.is_default { "(default)" } else { "" })}
                                                            </div>
                                                        }).collect::<Vec<_>>()}
                                                    </div>
                                                })}
                                            </div>
                                        }.into_any()
                                    }}
                                </div>
                            </div>
                        })}
                    </div>
                    // Encryption & DRM
                    <div style="flex: 1; min-width: 280px;">
                        {show_drm.then(|| view! {
                            <ProbeSection title="🔒 Encryption & DRM" show=true>
                                <ProbeRow label="Encryption methods" value=(!enc.is_empty()).then(|| enc.join(", ")) show=s_enc_method />
                                <ProbeRow label="Key formats" value=(!kf.is_empty()).then(|| kf.join(", ")) show=s_key_format />
                                {s_drm_systems.then(|| view! {
                                    <InfoRow label="DRM systems (PSSH / KEYFORMAT)">
                                        {if drm.is_empty() {
                                            let status = drm_status_label(probed_init, partial_init);
                                            view! {
                                                <span style="color: var(--color-sky-700); font-style: italic;">{status}</span>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <div>
                                                    {drm.iter().map(|d| view! {
                                                        <div style="font-size: .8rem; margin-bottom: calc(var(--spacing) * 0.75);">
                                                            <b>{d.system_name.clone()}</b>
                                                            <span style="color: var(--color-sky-700); font-family: monospace; font-size: .72rem;">
                                                                {format!(" — {}", d.display_id())}
                                                            </span>
                                                        </div>
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                            }.into_any()
                                        }}
                                    </InfoRow>
                                })}
                            </ProbeSection>
                        })}
                    </div>
                </div>
            })}

            // ── Video tracks (table)
            {(!video_tracks.is_empty() && show_video_table).then(|| view! {
                <div>
                    <SectionTitle label="🎬 Video Streams" />
                    <VideoTable tracks=video_tracks selected=video_sel />
                </div>
            })}

            // ── Audio tracks (table)
            {(!audio_tracks.is_empty() && show_audio_table).then(|| view! {
                <div>
                    <SectionTitle label="🔊 Audio Streams" />
                    <AudioTable tracks=audio_tracks selected=audio_sel />
                </div>
            })}
        </div>
    }
}

// ── Sub-components ────────────────────────────────────────────────────────────

#[component]
fn SectionTitle(label: &'static str) -> impl IntoView {
    view! {
        <div style="font-size: 1rem; font-weight: 700; color: var(--color-sky-700); text-transform: uppercase; \
                    letter-spacing: .08em; margin-bottom: calc(var(--spacing) * 3); display: flex; align-items: center; gap: calc(var(--spacing) * 2);">
            {label}
            <span style="flex: 1; height: 1px; background: var(--color-sky-200);"></span>
        </div>
    }
}

#[component]
fn ProbeSection(title: &'static str, show: bool, children: Children) -> impl IntoView {
    show.then(|| view! {
        <div style="margin-bottom: calc(var(--spacing) * 7);">
            <SectionTitle label=title />
            <div style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 10px; padding: calc(var(--spacing) * 4) calc(var(--spacing) * 4.5);">
                {children()}
            </div>
        </div>
    })
}

#[component]
fn ProbeRow(label: &'static str, value: Option<String>, show: bool) -> impl IntoView {
    show.then(|| {
        let display = value.filter(|v| !v.is_empty()).unwrap_or_else(|| "—".into());
        view! {
            <div style="display: flex; gap: calc(var(--spacing) * 3); padding: calc(var(--spacing) * 1.25) 0; \
                        border-bottom: 1px solid var(--color-sky-100); align-items: baseline; flex-wrap: wrap;">
                <span style="min-width: 200px; font-size: .78rem; font-weight: 600; \
                             color: var(--color-sky-700); flex-shrink: 0;">
                    {label}
                </span>
                <span style="font-size: .82rem; color: var(--color-sky-950); font-family: ui-monospace, monospace;">
                    {display}
                </span>
            </div>
        }
    })
}

#[component]
fn InfoRow(label: &'static str, children: Children) -> impl IntoView {
    view! {
        <div style="display: flex; gap: calc(var(--spacing) * 3); padding: calc(var(--spacing) * 1.25) 0; \
                    border-bottom: 1px solid var(--color-sky-100); align-items: baseline; flex-wrap: wrap;">
            <span style="min-width: 200px; font-size: .78rem; font-weight: 600; \
                         color: var(--color-sky-700); flex-shrink: 0;">
                {label}
            </span>
            <div style="font-size: .82rem; color: var(--color-sky-950);">{children()}</div>
        </div>
    }
}

/// Track whether a horizontally scrollable element still has content past its right edge,
/// so the table can show a hint that there is more to scroll to. Both track tables want
/// the same behaviour, and the closure plumbing that keeps the listeners alive long
/// enough to be cleaned up is the bulk of it.
fn scroll_hint_signal(scroll_ref: NodeRef<leptos::html::Div>) -> RwSignal<bool> {
    let show_hint = RwSignal::new(false);

    Effect::new(move |_| {
        use wasm_bindgen::JsCast;
        let Some(el) = scroll_ref.get() else {
            return;
        };
        // Shared check: has unscrolled overflow remaining?
        let check = {
            let el = el.clone();
            move || {
                let remaining = el.scroll_width() - el.scroll_left() - el.client_width();
                show_hint.set(remaining > 4);
            }
        };

        // Scroll listener — stored so on_cleanup can drop it properly.
        let check_scroll = check.clone();
        let scroll_cb = wasm_bindgen::closure::Closure::<dyn Fn()>::new(check_scroll);
        el.add_event_listener_with_callback("scroll", scroll_cb.as_ref().unchecked_ref())
            .ok();

        // ResizeObserver fires on mount and on every size change (window resize,
        // column toggle, etc.) so the hint stays in sync without a separate
        // window "resize" listener.
        let check_resize = check.clone();
        let resize_cb = wasm_bindgen::closure::Closure::<dyn Fn()>::new(check_resize);
        if let Ok(observer) = web_sys::ResizeObserver::new(resize_cb.as_ref().unchecked_ref()) {
            observer.observe(&el);
            // Register cleanup: disconnect the observer and drop closures when
            // the component is removed, preventing memory leaks.
            let el_clone = el.clone();
            // run_wasm_cleanup satisfies on_cleanup's Send+Sync bound safely in WASM.
            on_cleanup(run_wasm_cleanup(move || {
                observer.disconnect();
                el_clone
                    .remove_event_listener_with_callback(
                        "scroll",
                        scroll_cb.as_ref().unchecked_ref(),
                    )
                    .ok();
                drop(resize_cb);
            }));
        } else {
            // ResizeObserver unavailable — remove the scroll listener we already
            // registered and run the check once immediately as a fallback.
            el.remove_event_listener_with_callback("scroll", scroll_cb.as_ref().unchecked_ref())
                .ok();
            drop(scroll_cb);
            drop(resize_cb);
            check();
        }
    });

    show_hint
}

// Column definitions: (check_id, column header) — order matches video_cells() return
const VIDEO_COL_DEFS: &[(&str, &str)] = &[
    ("video_codec",        "Codec"),
    ("video_resolution",   "Resolution"),
    ("video_frame_rate",   "Frame Rate"),
    ("video_bitrate",      "Bitrate"),
    ("video_avg_bitrate",  "Avg Bitrate"),
    ("video_color_space",  "Color / HDR"),
    ("video_aspect_ratio", "DAR"),
    ("video_profile",      "Profile"),
    ("video_level",        "Level"),
    ("video_bit_depth",    "Bit depth"),
    ("video_primaries",    "Color primaries"),
    ("video_transfer",     "Transfer char."),
    ("video_matrix",       "Matrix coeff."),
    ("video_pixel_fmt",    "Pixel format"),
];

fn video_cells(t: &VideoTrackInfo) -> [Option<String>; 14] {
    // A colour value that came from VIDEO-RANGE rather than a colr box is a convention
    // the row would otherwise present as a reading off the init segment.
    let colour = |value: &Option<String>| match (value, t.color_measured) {
        (Some(v), false) => Some(format!("{v} (assumed)")),
        (v, _) => v.clone(),
    };
    [
        t.codec_long.clone().or(t.codec.clone()),
        t.resolution.clone(),
        t.frame_rate.map(|f| format!("{:.3} fps", f)),
        t.bitrate_bps.map(fmt_bps),
        t.avg_bitrate_bps.map(fmt_bps),
        match (&t.color_space, &t.hdr_format) {
            (Some(cs), Some(hdr)) => Some(format!("{} / {}", cs, hdr)),
            (Some(cs), None) => Some(cs.clone()),
            (None, Some(hdr)) => Some(hdr.clone()),
            _ => None,
        },
        match (&t.dar, &t.sar) {
            (Some(dar), Some(sar)) => Some(format!("{} (SAR {})", dar, sar)),
            (Some(dar), None) => Some(dar.clone()),
            _ => None,
        },
        t.profile.clone(),
        t.level.clone(),
        t.bit_depth.map(|b| format!("{}-bit", b)),
        colour(&t.color_primaries),
        colour(&t.transfer_characteristics),
        {
            let mc = colour(&t.matrix_coefficients);
            let fr = t.full_range.map(|b| if b { " (full range)" } else { " (limited)" });
            match (mc, fr) {
                (Some(m), Some(f)) => Some(format!("{}{}", m, f)),
                (Some(m), None) => Some(m),
                _ => None,
            }
        },
        t.pixel_format.clone(),
    ]
}

#[component]
fn VideoTable(mut tracks: Vec<VideoTrackInfo>, selected: HashSet<String>) -> impl IntoView {
    // Sort renditions ascending by bitrate
    tracks.sort_by_key(|t| t.bitrate_bps.unwrap_or(0));

    // Determine which columns are visible based on selection
    let vis: Vec<usize> = (0..VIDEO_COL_DEFS.len())
        .filter(|&i| selected.contains(VIDEO_COL_DEFS[i].0))
        .collect();

    let headers: Vec<&'static str> = vis.iter().map(|&i| VIDEO_COL_DEFS[i].1).collect();

    // Pre-compute all cell values: Vec<row> where row = Vec<display_string>
    let table_rows: Vec<Vec<String>> = tracks.iter()
        .map(|t| {
            let cells = video_cells(t);
            vis.iter()
               .map(|&i| cells[i].clone().unwrap_or_else(|| "—".into()))
               .collect()
        })
        .collect();

    let scroll_ref = NodeRef::<leptos::html::Div>::new();
    let show_hint = scroll_hint_signal(scroll_ref);

    view! {
        // Outer wrapper: relative so the scroll-hint overlay is clipped to the table bounds
        <div style="position: relative; margin-bottom: calc(var(--spacing) * 7);">
            <div node_ref=scroll_ref style="overflow-x: auto; border: 1px solid var(--color-sky-200); border-radius: 10px;">
                <table style="width: max-content; min-width: 100%; border-collapse: collapse; font-size: .82rem;">
                    <thead>
                        <tr>
                            {headers.into_iter().map(|h| view! {
                                <th style="text-align: left; padding: calc(var(--spacing) * 2.25) calc(var(--spacing) * 3.5); \
                                           background: var(--color-sky-100); \
                                           border-bottom: 2px solid var(--color-sky-200); \
                                           font-size: .7rem; font-weight: 700; color: var(--color-sky-700); \
                                           text-transform: uppercase; letter-spacing: .07em; \
                                           white-space: nowrap;">
                                    {h}
                                </th>
                            }).collect::<Vec<_>>()}
                        </tr>
                    </thead>
                    <tbody>
                        {table_rows.into_iter().enumerate().map(|(ri, row)| {
                            let bg = if ri % 2 == 0 { "var(--color-white)" } else { "var(--color-sky-50)" };
                            view! {
                                <tr style=format!("background: {}; transition: background .1s;", bg)>
                                    {row.into_iter().map(|val| view! {
                                        <td style="padding: calc(var(--spacing) * 2) calc(var(--spacing) * 3.5); \
                                                   border-bottom: 1px solid var(--color-sky-100); \
                                                   color: var(--color-sky-950); \
                                                   font-family: ui-monospace, monospace; \
                                                   white-space: nowrap; vertical-align: middle;">
                                            {val}
                                        </td>
                                    }).collect::<Vec<_>>()}
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>
            // Scroll hint: right-edge gradient + animated chevron anchored to the header row.
            // Pinning to the header keeps it visible regardless of how far the user has scrolled.
            // pointer-events: none so it never blocks scroll or click interactions.
            <div style=move || format!("position: absolute; right: 1px; top: 1px; height: 38px; width: 52px;                         border-radius: 0 10px 0 0; pointer-events: none;                         background: linear-gradient(to right, transparent, color-mix(in srgb, var(--color-sky-100) 96%, transparent));                         display: flex; align-items: center; justify-content: flex-end;                         padding-right: calc(var(--spacing) * 1.75);                         opacity: {}; transition: opacity .2s;", if show_hint.get() {{ 1 }} else {{ 0 }})>
                <style>
                    "@keyframes hls-scroll-bounce {
                        0%, 100% { transform: translateX(0); opacity: .55; }
                        50%       { transform: translateX(4px); opacity: 1; }
                    }"
                </style>
                <span style="font-size: 1.25rem; color: var(--color-sky-700); line-height: 1; \
                             user-select: none; \
                             animation: hls-scroll-bounce 1.4s ease-in-out infinite;">
                    {"›"}
                </span>
            </div>
        </div>
    }
}

// Column definitions for audio table — order matches audio_cells() return
const AUDIO_COL_DEFS: &[(&str, &str)] = &[
    ("audio_codec",    "Codec"),
    ("audio_language", "Language"),
    ("audio_channels", "Channels"),
    ("audio_layout",   "Channel Layout"),
    ("audio_bitrate",  "Bitrate"),
    ("audio_rate",     "Sample Rate"),
    ("audio_depth",    "Bit Depth"),
];

fn audio_cells(t: &AudioTrackInfo) -> [Option<String>; 7] {
    [
        t.codec_long.clone().or(t.codec.clone()),
        t.language.clone(),
        t.channels.map(|c| c.to_string()),
        t.channel_layout.clone(),
        t.bitrate_bps.map(fmt_bps),
        t.sample_rate.map(|r| format!("{} Hz", r)),
        t.bit_depth.map(|b| format!("{}-bit", b)),
    ]
}

#[component]
fn AudioTable(tracks: Vec<AudioTrackInfo>, selected: HashSet<String>) -> impl IntoView {
    let vis: Vec<usize> = (0..AUDIO_COL_DEFS.len())
        .filter(|&i| selected.contains(AUDIO_COL_DEFS[i].0))
        .collect();

    let headers: Vec<&'static str> = vis.iter().map(|&i| AUDIO_COL_DEFS[i].1).collect();

    let table_rows: Vec<(String, bool, Vec<String>)> = tracks.iter()
        .map(|t| {
            let cells = audio_cells(t);
            let row_cells = vis.iter()
                .map(|&i| cells[i].clone().unwrap_or_else(|| "—".into()))
                .collect();
            let track_label = if t.group_id.is_empty() {
                t.name.clone()
            } else {
                format!("{} ({})", t.name, t.group_id)
            };
            (track_label, t.is_default, row_cells)
        })
        .collect();

    let scroll_ref = NodeRef::<leptos::html::Div>::new();
    let show_hint = scroll_hint_signal(scroll_ref);

    view! {
        // Outer wrapper: relative so the scroll-hint overlay is clipped to the table bounds
        <div style="position: relative; margin-bottom: calc(var(--spacing) * 7);">
            <div node_ref=scroll_ref style="overflow-x: auto; border: 1px solid var(--color-sky-200); border-radius: 10px;">
                <table style="width: max-content; min-width: 100%; border-collapse: collapse; font-size: .82rem;">
                    <thead>
                        <tr>
                            <th style="text-align: left; padding: calc(var(--spacing) * 2.25) calc(var(--spacing) * 3.5); \
                                       background: var(--color-sky-100); border-bottom: 2px solid var(--color-sky-200); \
                                       font-size: .7rem; font-weight: 700; color: var(--color-sky-700); \
                                       text-transform: uppercase; letter-spacing: .07em; \
                                       white-space: nowrap;">
                                "Track"
                            </th>
                            {headers.into_iter().map(|h| view! {
                                <th style="text-align: left; padding: calc(var(--spacing) * 2.25) calc(var(--spacing) * 3.5); \
                                           background: var(--color-sky-100); border-bottom: 2px solid var(--color-sky-200); \
                                           font-size: .7rem; font-weight: 700; color: var(--color-sky-700); \
                                           text-transform: uppercase; letter-spacing: .07em; \
                                           white-space: nowrap;">
                                    {h}
                                </th>
                            }).collect::<Vec<_>>()}
                        </tr>
                    </thead>
                    <tbody>
                        {table_rows.into_iter().enumerate().map(|(ri, (name, is_default, row))| {
                            let bg = if ri % 2 == 0 { "var(--color-white)" } else { "var(--color-sky-50)" };
                            view! {
                                <tr style=format!("background: {}; transition: background .1s;", bg)>
                                    <td style="padding: calc(var(--spacing) * 2) calc(var(--spacing) * 3.5); border-bottom: 1px solid var(--color-sky-100); \
                                               white-space: nowrap; vertical-align: middle;">
                                        <div style="display: flex; align-items: center; gap: 6px;">
                                            <span style="font-size: .82rem; font-weight: 600; color: var(--color-sky-950);">
                                                {name}
                                            </span>
                                            {is_default.then(|| view! {
                                                <span style="font-size: .65rem; color: var(--color-green-600); font-weight: 700; \
                                                             background: rgba(34,197,94,.1); \
                                                             border: 1px solid rgba(34,197,94,.3); \
                                                             border-radius: 4px; padding: var(--spacing) calc(var(--spacing) * 1.25);">
                                                    "DEFAULT"
                                                </span>
                                            })}
                                        </div>
                                    </td>
                                    {row.into_iter().map(|val| view! {
                                        <td style="padding: calc(var(--spacing) * 2) calc(var(--spacing) * 3.5); border-bottom: 1px solid var(--color-sky-100); \
                                                   color: var(--color-sky-950); font-family: ui-monospace, monospace; \
                                                   white-space: nowrap; vertical-align: middle;">
                                            {val}
                                        </td>
                                    }).collect::<Vec<_>>()}
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>
            // Scroll hint: right-edge gradient + animated chevron anchored to the header row.
            <div style=move || format!("position: absolute; right: 1px; top: 1px; height: 38px; width: 52px;                         border-radius: 0 10px 0 0; pointer-events: none;                         background: linear-gradient(to right, transparent, color-mix(in srgb, var(--color-sky-100) 96%, transparent));                         display: flex; align-items: center; justify-content: flex-end;                         padding-right: calc(var(--spacing) * 1.75);                         opacity: {}; transition: opacity .2s;", if show_hint.get() {{ 1 }} else {{ 0 }})>
                <style>
                    "@keyframes hls-scroll-bounce {
                        0%, 100% { transform: translateX(0); opacity: .55; }
                        50%       { transform: translateX(4px); opacity: 1; }
                    }"
                </style>
                <span style="font-size: 1.25rem; color: var(--color-sky-700); line-height: 1; \
                             user-select: none; \
                             animation: hls-scroll-bounce 1.4s ease-in-out infinite;">
                    {"›"}
                </span>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPLE_MASTER: &str = "#EXTM3U\n\
        \n\
        #EXT-X-VERSION:6\n\
        \n\
        #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aache-44-64\",NAME=\"English\",LANGUAGE=\"en-US\",AUTOSELECT=YES,DEFAULT=YES,CHANNELS=\"2\",URI=\"audio/prog_index.m3u8\"\n\
        #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"ec3-48-768\",NAME=\"English\",LANGUAGE=\"en-US\",AUTOSELECT=YES,DEFAULT=YES,CHANNELS=\"16/JOC\",URI=\"atmos/prog_index.m3u8\"\n\
        \n\
        #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subtitles\",NAME=\"English\",LANGUAGE=\"en-US\",AUTOSELECT=YES,FORCED=NO,DEFAULT=YES,URI=\"subs/prog_index.m3u8\"\n\
        \n\
        #EXT-X-MEDIA:TYPE=CLOSED-CAPTIONS,GROUP-ID=\"cc\",LANGUAGE=\"en\",NAME=\"English\",DEFAULT=YES,AUTOSELECT=YES,INSTREAM-ID=\"CC1\"\n\
        \n\
        #EXT-X-STREAM-INF:AVERAGE-BANDWIDTH=2193519,BANDWIDTH=3307941,VIDEO-RANGE=SDR,CODECS=\"avc1.64001f,mp4a.40.5\",RESOLUTION=1024x576,FRAME-RATE=23.976,CLOSED-CAPTIONS=\"cc\",AUDIO=\"aache-44-64\",SUBTITLES=\"subtitles\"\n\
        sdr/prog_index.m3u8\n\
        \n\
        #EXT-X-STREAM-INF:AVERAGE-BANDWIDTH=25097073,BANDWIDTH=37409767,VIDEO-RANGE=PQ,CODECS=\"dvh1.05.06,ec-3\",RESOLUTION=3840x2160,FRAME-RATE=23.976,CLOSED-CAPTIONS=\"cc\",AUDIO=\"ec3-48-768\",SUBTITLES=\"subtitles\"\n\
        dv/prog_index.m3u8\n";

    const APPLE_MEDIA: &str = "#EXTM3U\n\
        #EXT-X-TARGETDURATION:6\n\
        #EXT-X-VERSION:7\n\
        #EXT-X-MEDIA-SEQUENCE:0\n\
        #EXT-X-PLAYLIST-TYPE:VOD\n\
        #EXT-X-INDEPENDENT-SEGMENTS\n\
        \n\
        #EXT-X-PROGRAM-DATE-TIME:2025-01-01T05:00:00.000Z\n\
        #EXT-X-DATERANGE:ID=\"ad1\",CLASS=\"com.apple.hls.interstitial\",START-DATE=\"2025-01-01T05:00:10.000Z\",DURATION=6,X-ASSET-URI=\"https://example.com/ad.m3u8\",CUE=\"PRE\",X-RESUME-OFFSET=0,X-TIMELINE-STYLE=\"HIGHLIGHT\",X-TIMELINE-OCCUPIES=\"POINT\",X-RESTRICT=\"JUMP\",X-PLAYOUT-LIMIT=5.0\n\
        #EXT-X-DATERANGE:ID=\"ad2\",CLASS=\"com.apple.hls.interstitial\",START-DATE=\"2025-01-01T05:00:25.000Z\",DURATION=25,X-ASSET-LIST=\"https://example.com/ads.json\",X-RESUME-OFFSET=0\n\
        \n\
        #EXT-X-MAP:URI=\"fileSequence0.mp4\"\n\
        #EXTINF:5.46379,\t\n\
        fileSequence1.m4s\n\
        #EXTINF:5.21354,\t\n\
        fileSequence2.m4s\n\
        #EXT-X-ENDLIST\n";

    // The `diagnose_*` tests that used to sit here only asserted that quick-m3u8 reads
    // CHANNELS="16/JOC", EXT-X-DATERANGE and a STREAM-INF/URI pair the way this page
    // expects. `master_parses_variants_and_renditions` and
    // `media_playlist_parsed_past_daterange` below exercise the same playlist text
    // through Inspect's own parsers, so the direct-to-library assertions protected
    // nothing that these do not.

    #[test]
    fn master_parses_variants_and_renditions() {
        let master = parse_master_playlist("https://example.com/master.m3u8", APPLE_MASTER);
        assert_eq!(master.variants.len(), 2, "should parse 2 EXT-X-STREAM-INF variants");
        assert_eq!(master.version, 6);
        // DVH1 / Dolby Vision variant
        let dv = master.variants.iter().find(|v| v.uri.contains("dv/")).unwrap();
        assert!(dv.codecs.as_deref().unwrap().starts_with("dvh1"), "dvh1 codec must be preserved");
        // Audio renditions: 2 (one JOC, one stereo)
        let audio: Vec<_> = master.media_renditions.iter().filter(|r| r.media_type == "AUDIO").collect();
        assert_eq!(audio.len(), 2);
        // CHANNELS="16/JOC" should not cause a parse break
        let joc = audio.iter().find(|r| r.name.contains("English") && r.group_id == "ec3-48-768");
        assert!(joc.is_some(), "JOC audio rendition should be present");
        // Subtitles
        let subs: Vec<_> = master.media_renditions.iter().filter(|r| r.media_type == "SUBTITLES").collect();
        assert_eq!(subs.len(), 1);
        // Closed captions (no URI)
        let cc: Vec<_> = master.media_renditions.iter().filter(|r| r.media_type == "CLOSED-CAPTIONS").collect();
        assert_eq!(cc.len(), 1);
    }

    #[test]
    fn media_playlist_parsed_past_daterange() {
        let pl = parse_media_playlist("https://example.com/sdr/prog_index.m3u8", APPLE_MEDIA, &HashMap::new());
        assert_eq!(pl.target_duration, 6.0, "EXT-X-TARGETDURATION should be 6");
        assert_eq!(pl.playlist_type.as_deref(), Some("VOD"), "playlist type should be VOD");
        assert!(pl.has_endlist, "EXT-X-ENDLIST should be detected");
        assert_eq!(pl.segments.len(), 2, "should parse 2 segments (EXT-X-DATERANGE must not break the loop)");
        // EXT-X-MAP URI must be propagated to each segment
        assert!(
            pl.segments.iter().all(|s| s.map_uri.as_deref() == Some("https://example.com/sdr/fileSequence0.mp4")),
            "all segments must carry the resolved map_uri"
        );
    }

    #[test]
    fn audio_only_variant_supplies_rendition_bitrate() {
        const MASTER: &str = "#EXTM3U\n\
            #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud1\",NAME=\"English\",DEFAULT=YES,URI=\"a1/prog_index.m3u8\"\n\
            #EXT-X-STREAM-INF:BANDWIDTH=160000,CODECS=\"mp4a.40.2\"\n\
            a1/prog_index.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=3000000,CODECS=\"avc1.64002a,mp4a.40.2\",RESOLUTION=1280x720,AUDIO=\"aud1\"\n\
            v1/prog_index.m3u8\n";
        let master = parse_master_playlist("https://example.com/master.m3u8", MASTER);
        let map = audio_only_bandwidths(&master);
        assert_eq!(
            map.get("https://example.com/a1/prog_index.m3u8").copied(),
            Some(160_000),
            "audio-only variant BANDWIDTH should be available to the matching rendition"
        );
        assert!(
            !map.contains_key("https://example.com/v1/prog_index.m3u8"),
            "a variant carrying video must not be treated as an audio bitrate source"
        );
    }

    #[test]
    fn audio_columns_and_cells_stay_aligned() {
        assert_eq!(
            AUDIO_COL_DEFS.len(),
            audio_cells(&AudioTrackInfo::default()).len(),
            "every audio column needs a matching cell"
        );
    }

    #[test]
    fn audio_codec_token_is_order_independent() {
        assert_eq!(
            audio_codec_token("avc1.64002a,mp4a.40.2").as_deref(),
            Some("mp4a.40.2")
        );
        assert_eq!(
            audio_codec_token("mp4a.40.2,avc1.64002a").as_deref(),
            Some("mp4a.40.2")
        );
        assert_eq!(
            audio_codec_token("avc1.64002a,ac-3").as_deref(),
            Some("ac-3")
        );
        assert_eq!(audio_codec_token("avc1.64002a").as_deref(), None);
    }

    #[test]
    fn fscod_maps_to_ac3_sample_rates() {
        assert_eq!(fscod_to_hz(0), Some(48_000));
        assert_eq!(fscod_to_hz(1), Some(44_100));
        assert_eq!(fscod_to_hz(2), Some(32_000));
        assert_eq!(fscod_to_hz(3), None);
    }

    #[test]
    fn map_byterange_is_captured() {
        let pl = parse_media_playlist(
            "https://example.com/v5/prog_index.m3u8",
            "#EXTM3U\n#EXT-X-TARGETDURATION:6\n\
             #EXT-X-MAP:URI=\"main.mp4\",BYTERANGE=\"719@0\"\n\
             #EXTINF:6.0,\n\
             main.mp4\n",
            &HashMap::new(),
        );
        let (uri, range) = pl.init_segment().expect("init segment");
        assert_eq!(uri, "https://example.com/v5/main.mp4");
        let range = range.expect("byterange");
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 718);
    }

    #[test]
    fn resolve_defined_uri_substitutes_then_joins() {
        let defs = HashMap::from([("PATH".into(), "v1".into())]);
        assert_eq!(
            resolve_defined_uri("https://ex.com/hls/master.m3u8", "{$PATH}/prog.m3u8", &defs),
            "https://ex.com/hls/v1/prog.m3u8"
        );
    }

    #[test]
    fn define_name_value_resolves_variant_uri() {
        let content = "#EXTM3U\n\
            #EXT-X-DEFINE:NAME=\"HOST\",VALUE=\"https://cdn.example\"\n\
            #EXT-X-STREAM-INF:BANDWIDTH=1000\n\
            {$HOST}/v.m3u8\n";
        let master = parse_master_playlist("https://origin.example/master.m3u8", content);
        assert_eq!(master.definitions.get("HOST").map(String::as_str), Some("https://cdn.example"));
        assert_eq!(master.variants[0].uri, "https://cdn.example/v.m3u8");
    }

    #[test]
    fn is_master_with_iframe_only_stream_inf() {
        let content = "#EXTM3U\n#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=1000,URI=\"iframe.m3u8\"\n";
        assert!(is_master_playlist(content));
    }

    #[test]
    fn resolve_uri_normalizes_parent_and_root_relative() {
        let base = "https://ex.com/hls/v1/master.m3u8";
        assert_eq!(
            resolve_uri(base, "../a1/prog_index.m3u8"),
            "https://ex.com/hls/a1/prog_index.m3u8"
        );
        assert_eq!(
            resolve_uri(base, "./seg/init.mp4"),
            "https://ex.com/hls/v1/seg/init.mp4"
        );
        assert_eq!(
            resolve_uri(base, "/root/a.m3u8"),
            "https://ex.com/root/a.m3u8"
        );
        assert_eq!(
            resolve_uri(base, "https://cdn.example/x.m3u8"),
            "https://cdn.example/x.m3u8"
        );
    }

    #[test]
    fn mp4_has_audio_detects_codec_or_channels() {
        assert!(!mp4_has_audio(&Mp4ProbeInfo::default()));
        assert!(mp4_has_audio(&Mp4ProbeInfo {
            audio_codec_override: Some("AAC".into()),
            ..Default::default()
        }));
        assert!(mp4_has_audio(&Mp4ProbeInfo {
            audio_channels: Some(2),
            ..Default::default()
        }));
    }

    #[test]
    fn apply_audio_init_fills_missing_fields_only() {
        let mp4 = Mp4ProbeInfo {
            audio_codec_override: Some("AAC".into()),
            audio_codec_long: Some("AAC-LC".into()),
            audio_sample_rate: Some(48_000),
            audio_channels: Some(2),
            audio_bit_depth: Some(16),
            audio_bitrate_bps: Some(160_000),
            ..Default::default()
        };
        let mut at = AudioTrackInfo {
            codec: Some("HE-AAC".into()),
            sample_rate: Some(44_100),
            ..Default::default()
        };
        apply_audio_init(&mut at, &mp4);
        assert_eq!(at.codec.as_deref(), Some("HE-AAC"), "existing codec must win");
        assert_eq!(at.sample_rate, Some(44_100), "existing sample rate must win");
        assert_eq!(at.bit_depth, Some(16));
        assert_eq!(at.bitrate_bps, Some(160_000));
        assert_eq!(at.channels, Some(2));
        assert_eq!(at.channel_layout.as_deref(), Some("stereo"));
    }

    #[test]
    fn video_columns_and_cells_stay_aligned() {
        assert_eq!(
            VIDEO_COL_DEFS.len(),
            video_cells(&VideoTrackInfo::default()).len(),
            "every video column needs a matching cell"
        );
    }

    // ── ISOBMFF fixtures ─────────────────────────────────────────────────────
    //
    // Init segments assembled box by box, so a test can state exactly which boxes the
    // walk is given and in which order. Order is the point of several of them: an `frma`
    // sits at the end of the sample entry it describes, so a flat walk meets it after the
    // configuration box it has to be able to correct.

    /// Box of `kind` around `body`.
    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    fn encoded<T: mp4_atom::Encode>(atom: &T) -> Vec<u8> {
        let mut buf = Vec::new();
        atom.encode(&mut buf).expect("fixture atom encodes");
        buf
    }

    /// The `ftyp` a packager writes ahead of an HLS fMP4 init.
    fn ftyp() -> Vec<u8> {
        use mp4_atom::{FourCC, Ftyp};
        encoded(&Ftyp {
            major_brand: FourCC::new(b"iso5"),
            minor_version: 0,
            compatible_brands: vec![FourCC::new(b"isom"), FourCC::new(b"hlsf")],
        })
    }

    /// A `trak` whose `hdlr` names what the sample entry below it is, which is how the
    /// walk knows whether a codec box belongs to the video or the audio track.
    fn trak(handler: &[u8; 4], sample_entry: &[u8]) -> Vec<u8> {
        use mp4_atom::{FourCC, Hdlr};
        let mut mdia = encoded(&Hdlr {
            handler: FourCC::new(handler),
            name: String::new(),
        });
        let mut stsd = vec![0u8; 4]; // version + flags
        stsd.extend_from_slice(&1u32.to_be_bytes()); // entry_count
        stsd.extend_from_slice(sample_entry);
        mdia.extend(boxed(b"minf", &boxed(b"stbl", &boxed(b"stsd", &stsd))));
        boxed(b"trak", &boxed(b"mdia", &mdia))
    }

    /// The fixed part of a `VisualSampleEntry`, which every codec configuration box
    /// inside a video sample entry sits behind.
    fn visual() -> Vec<u8> {
        use mp4_atom::{Encode, Visual};
        let mut out = Vec::new();
        Visual {
            width: 1920,
            height: 1080,
            ..Default::default()
        }
        .encode(&mut out)
        .expect("visual sample entry encodes");
        out
    }

    /// The fixed part of an `AudioSampleEntry`. These fields are real even in an `enca`,
    /// which is why the entry is read for them while saying nothing about the codec.
    fn audio(channel_count: u16, sample_rate: u16) -> Vec<u8> {
        use mp4_atom::{Audio, Encode, FixedPoint};
        let mut out = Vec::new();
        Audio {
            data_reference_index: 1,
            channel_count,
            sample_size: 16,
            sample_rate: FixedPoint::new(sample_rate, 0),
        }
        .encode(&mut out)
        .expect("audio sample entry encodes");
        out
    }

    /// An `hvcC` for Main 10 at Level 5.1: `general_level_idc` is level × 30, and the
    /// bit depth fields are `minus8`, so 2 is 10-bit.
    fn hvcc() -> Vec<u8> {
        use mp4_atom::Hvcc;
        encoded(&Hvcc {
            general_profile_idc: 2,
            general_level_idc: 153,
            general_profile_compatibility_flags: [0x60, 0, 0, 0],
            chroma_format_idc: 1,
            bit_depth_luma_minus8: 2,
            bit_depth_chroma_minus8: 2,
            num_temporal_layers: 1,
            temporal_id_nested: true,
            length_size_minus_one: 3,
            ..Hvcc::new()
        })
    }

    /// A `dec3` for 5.1 E-AC-3 at 768 kbps and 48 kHz: `data_rate` is 13 bits of kbps,
    /// `fscod` 0 is 48 kHz, and `acmod` 7 with `lfeon` set is 3/2.1.
    fn dec3() -> Vec<u8> {
        boxed(b"dec3", &[0x18, 0x00, 0x20, 0x0F, 0x00])
    }

    /// The `sinf` of an encrypted sample entry, whose `frma` restores the fourCC the
    /// samples had before they were encrypted.
    fn sinf(original_format: &[u8; 4]) -> Vec<u8> {
        let mut schm = 0u32.to_be_bytes().to_vec(); // version 0, no flags
        schm.extend_from_slice(b"cenc");
        schm.extend_from_slice(&0u32.to_be_bytes()); // scheme_version

        let mut body = boxed(b"frma", original_format);
        body.extend(boxed(b"schm", &schm));
        boxed(b"sinf", &body)
    }

    /// A version 0 `pssh` for `system_id`, carrying no system-specific data — enough to
    /// name the DRM system, which is all the report reads out of it.
    fn pssh(system_id: [u8; 16]) -> Vec<u8> {
        let mut body = 0u32.to_be_bytes().to_vec(); // version 0, no flags
        body.extend_from_slice(&system_id);
        body.extend_from_slice(&0u32.to_be_bytes()); // data_size
        boxed(b"pssh", &body)
    }

    const WIDEVINE_SYSTEM_ID: [u8; 16] = [
        0xED, 0xEF, 0x8B, 0xA9, 0x79, 0xD6, 0x4A, 0xCE, 0xA3, 0xC8, 0x27, 0xDC, 0xD5, 0x1D, 0x21,
        0xED,
    ];

    fn init(boxes: &[Vec<u8>]) -> Vec<u8> {
        let mut moov = Vec::new();
        for b in boxes {
            moov.extend_from_slice(b);
        }
        let mut out = ftyp();
        out.extend(boxed(b"moov", &moov));
        out
    }

    /// A box body that will not decode used to end the walk, so a `pssh` behind it read
    /// as absent and the report said there was no DRM. An encrypted init writes its
    /// `pssh` inside the `moov` alongside the tracks, so anything unreadable in the
    /// `moov` was enough to lose it.
    #[test]
    fn a_box_that_will_not_decode_does_not_hide_the_boxes_behind_it() {
        let mut entry = visual();
        entry.extend(hvcc());
        // 4 bytes is version + flags and nothing else, which no `mdhd` can be.
        let data = init(&[
            boxed(b"mdhd", &[0u8; 4]),
            pssh(WIDEVINE_SYSTEM_ID),
            trak(b"vide", &boxed(b"hvc1", &entry)),
        ]);

        let mp4 = probe_mp4(data);
        assert_eq!(mp4.decode_errors, 1, "only the truncated mdhd should be skipped");
        assert_eq!(mp4.major_brand.as_deref(), Some("iso5"));
        assert_eq!(
            mp4.drm_systems.len(),
            1,
            "the pssh behind the unreadable box must still be found"
        );
        assert_eq!(mp4.video_codec_override.as_deref(), Some("H.265 / HEVC"));
        assert_eq!(mp4.video_profile.as_deref(), Some("Main 10"));
        assert_eq!(mp4.video_level.as_deref(), Some("5.1"));
    }

    #[test]
    fn a_well_formed_init_reports_no_decode_errors() {
        let mut entry = visual();
        entry.extend(hvcc());
        let mp4 = probe_mp4(init(&[trak(b"vide", &boxed(b"hvc1", &entry))]));
        assert_eq!(mp4.decode_errors, 0);
        assert!(mp4.walk_complete, "a well-formed init must be a complete walk");
    }

    #[test]
    fn an_empty_init_body_is_not_a_complete_walk() {
        let mp4 = probe_mp4(Vec::new());
        assert!(!mp4.walk_complete);
        assert_eq!(mp4.decode_errors, 0);
    }

    /// A header that will not read used to leave `decode_errors` at 0 and look like a
    /// complete walk, so the DRM row claimed "None found".
    #[test]
    fn a_truncated_box_header_ends_the_walk_and_says_so() {
        let mut data = ftyp();
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x04, 0x00]);
        let mp4 = probe_mp4(data);
        assert!(!mp4.walk_complete);
        assert_eq!(mp4.decode_errors, 0);
        assert_eq!(mp4.major_brand.as_deref(), Some("iso5"));
    }

    /// A `moov` header whose declared size runs past the buffer used to look like a
    /// complete walk: the header itself reads, the container never closes, and the DRM
    /// row claimed "None found" about a `moov` the walk never entered.
    #[test]
    fn an_unclosed_container_is_not_a_complete_walk() {
        let mut data = ftyp();
        data.extend_from_slice(&1000u32.to_be_bytes());
        data.extend_from_slice(b"moov");
        let mp4 = probe_mp4(data);
        assert!(!mp4.walk_complete);
        assert_eq!(mp4.major_brand.as_deref(), Some("iso5"));
    }

    #[test]
    fn a_truncated_walk_never_claims_no_drm_was_found() {
        assert_ne!(
            drm_status_label(true, true),
            "None found (no PSSH or known KEYFORMAT)",
        );
        assert_eq!(
            drm_status_label(true, false),
            "None found (no PSSH or known KEYFORMAT)",
        );
        let empty = probe_mp4(Vec::new());
        assert_eq!(
            drm_status_label(true, init_walk_was_partial(&empty)),
            "None in the part of the init segment that decoded",
        );
    }

    #[test]
    fn low_latency_is_only_reported_by_a_playlist_that_was_read() {
        assert_eq!(ll_hls_absence_label(false), None);
        assert_eq!(ll_hls_absence_label(true), Some("Not signalled"));
    }

    /// An `enca` says the samples are encrypted and nothing about what they are, so the
    /// codec has to come from the `dec3` inside the entry or the `frma` at the end of it
    /// — both of which the walk reaches after the entry itself.
    #[test]
    fn an_encrypted_audio_entry_is_named_by_its_dec3_and_frma_not_aac() {
        let mut entry = audio(6, 48_000);
        entry.extend(dec3());
        entry.extend(sinf(b"ec-3"));
        let mp4 = probe_mp4(init(&[trak(b"soun", &boxed(b"enca", &entry))]));

        assert_eq!(
            mp4.audio_codec_override.as_deref(),
            Some("E-AC-3"),
            "an enca must not be labelled AAC"
        );
        assert_eq!(
            mp4.audio_codec_long.as_deref(),
            Some("E-AC-3 (Dolby Digital Plus)")
        );
        assert_eq!(mp4.audio_channels, Some(6));
        assert_eq!(mp4.audio_sample_rate, Some(48_000));
        assert_eq!(mp4.audio_bitrate_bps, Some(768_000), "dec3 data_rate is kbps");
    }

    /// The `frma` of an encrypted entry outranks the configuration box inside it: an
    /// `hvcC` sits in a Dolby Vision entry as readily as an HEVC one, so `dvh1` is the
    /// answer even though the `hvcC` went by first.
    #[test]
    fn frma_original_format_outranks_the_configuration_box() {
        let mut entry = visual();
        entry.extend(hvcc());
        entry.extend(sinf(b"dvh1"));
        let mp4 = probe_mp4(init(&[trak(b"vide", &boxed(b"encv", &entry))]));

        assert_eq!(
            mp4.video_codec_override.as_deref(),
            Some("Dolby Vision (HEVC)"),
            "an encv arm that reads nothing leaves the codec to the hvcC alone"
        );
        assert_eq!(mp4.video_profile.as_deref(), Some("Main 10"));
        assert_eq!(mp4.video_bit_depth, Some(10));
    }

    #[test]
    fn pasp_supplies_the_sample_aspect_ratio() {
        let mut entry = visual();
        entry.extend(hvcc());
        entry.extend(encoded(
            &mp4_atom::Pasp::new(4, 3).expect("pasp fixture is valid"),
        ));
        let mp4 = probe_mp4(init(&[trak(b"vide", &boxed(b"hvc1", &entry))]));
        assert_eq!(mp4.video_sar.as_deref(), Some("4:3"));

        let mut vt = VideoTrackInfo {
            resolution: Some("1440x1080".into()),
            ..Default::default()
        };
        apply_video_init(&mut vt, &mp4);
        assert_eq!(vt.sar.as_deref(), Some("4:3"));
        assert_eq!(vt.dar.as_deref(), Some("16:9"));
    }

    /// An init with no `pasp` states no sample aspect ratio, so there is no display
    /// aspect ratio to report either — 1:1 is the common case, not a measurement.
    #[test]
    fn no_pasp_means_no_sample_or_display_aspect_ratio() {
        let mut entry = visual();
        entry.extend(hvcc());
        let mp4 = probe_mp4(init(&[trak(b"vide", &boxed(b"hvc1", &entry))]));
        assert!(mp4.video_sar.is_none());

        let mut vt = VideoTrackInfo {
            resolution: Some("1920x1080".into()),
            ..Default::default()
        };
        apply_video_init(&mut vt, &mp4);
        assert!(vt.sar.is_none() && vt.dar.is_none());
    }

    /// An `hvcC` without the bit depth and chroma fields states no pixel format, and
    /// nothing about HEVC makes 8-bit 4:2:0 the answer by default.
    #[test]
    fn an_hvcc_reports_the_pixel_format_it_states() {
        let mut entry = visual();
        entry.extend(hvcc());
        let mp4 = probe_mp4(init(&[trak(b"vide", &boxed(b"hvc1", &entry))]));
        assert_eq!(mp4.video_bit_depth, Some(10));
        assert_eq!(mp4.video_pixel_format.as_deref(), Some("yuv420p10le"));
    }

    /// An audio-only init is what a demuxed audio rendition points its EXT-X-MAP at, and
    /// it carries the same `ftyp` a muxed one does. Reading it is what lets the report
    /// name a container brand for an audio-only presentation.
    #[test]
    fn an_audio_only_init_still_names_a_container_brand() {
        let mut entry = audio(2, 48_000);
        entry.extend(boxed(b"btrt", &{
            let mut b = 0u32.to_be_bytes().to_vec(); // buffer_size_db
            b.extend_from_slice(&160_000u32.to_be_bytes()); // max_bitrate
            b.extend_from_slice(&128_000u32.to_be_bytes()); // avg_bitrate
            b
        }));
        let mp4 = probe_mp4(init(&[trak(b"soun", &boxed(b"mp4a", &entry))]));

        assert_eq!(mp4.major_brand.as_deref(), Some("iso5"));
        assert!(mp4_has_audio(&mp4), "an audio init has to read as having audio");
        assert_eq!(mp4.audio_codec_override.as_deref(), Some("AAC"));
        assert_eq!(mp4.audio_channels, Some(2));
        assert_eq!(mp4.audio_bitrate_bps, Some(128_000));
    }

    /// A `pssh` writes the system id as dashless hex and a KEYFORMAT writes the same id
    /// as a dashed UUID, so a stream signalling both used to list Widevine twice.
    #[test]
    fn a_pssh_and_a_keyformat_for_one_system_are_one_row() {
        let mut mp4 = probe_mp4(init(&[
            pssh(WIDEVINE_SYSTEM_ID),
            trak(b"vide", &boxed(b"hvc1", &visual())),
        ]));
        assert_eq!(mp4.drm_systems.len(), 1);

        let from_key = drm_from_key_formats(&[
            "urn:uuid:EDEF8BA9-79D6-4ACE-A3C8-27DCD51D21ED".to_string(),
        ]);
        merge_drm(&mut mp4.drm_systems, from_key);

        assert_eq!(mp4.drm_systems.len(), 1, "one system is one row");
        assert_eq!(mp4.drm_systems[0].system_name, "Google Widevine");
        assert_eq!(
            mp4.drm_systems[0].display_id(),
            "edef8ba9-79d6-4ace-a3c8-27dcd51d21ed"
        );
    }

    #[test]
    fn canonical_system_id_folds_both_spellings_together() {
        assert_eq!(
            canonical_system_id("urn:uuid:EDEF8BA9-79D6-4ACE-A3C8-27DCD51D21ED"),
            canonical_system_id("edef8ba979d64acea3c827dcd51d21ed")
        );
        assert_eq!(
            canonical_system_id(" 9A04F079-9840-4286-AB92-E65BE0885F95 "),
            "9a04f07998404286ab92e65be0885f95"
        );
    }

    /// FairPlay's KEYFORMAT is a reverse-DNS string rather than a UUID, and packagers
    /// disagree about its case.
    #[test]
    fn fairplay_keyformat_is_recognised_whatever_its_case() {
        for format in [
            "com.apple.streamingkeydelivery",
            "com.apple.streamingKeyDelivery",
        ] {
            let drm = drm_from_key_formats(&[format.to_string()]);
            assert_eq!(drm.len(), 1, "{format} should name a DRM system");
            assert_eq!(drm[0].system_name, "Apple FairPlay");
        }
    }

    /// Every playlist's keys count. Renditions are routinely protected under different
    /// systems, and a stream whose video alone is encrypted would otherwise be described
    /// by whichever playlist happened to be read first.
    #[test]
    fn keys_are_merged_across_playlists_and_from_session_keys() {
        let mut r = ProbeReport::default();
        merge_key_info(
            &mut r,
            "#EXTM3U\n#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,\
             KEYFORMAT=\"urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed\",URI=\"skd://a\"\n",
        );
        merge_key_info(
            &mut r,
            "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,\
             KEYFORMAT=\"com.apple.streamingkeydelivery\",URI=\"skd://b\"\n",
        );
        merge_key_info(
            &mut r,
            "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,\
             KEYFORMAT=\"com.apple.streamingkeydelivery\",URI=\"skd://c\"\n",
        );

        assert_eq!(r.encryption_methods, vec!["SAMPLE-AES".to_string()]);
        assert_eq!(r.key_formats.len(), 2, "both key formats, each once");
        let names: Vec<&str> = r.drm_systems.iter().map(|d| d.system_name.as_str()).collect();
        assert_eq!(names, vec!["Google Widevine", "Apple FairPlay"]);
    }

    /// The variant fetch can fail for every playlist, in which case nothing is known
    /// about the playlist type or the segment count. `VOD` and `0` are what the struct
    /// defaults to, not what was read.
    #[test]
    fn protocol_fields_are_only_set_by_a_playlist_that_was_read() {
        let mut r = ProbeReport::default();
        assert!(!r.protocol_set, "an unread presentation states no protocol");

        let pl = parse_media_playlist(
            "https://example.com/v1/prog_index.m3u8",
            APPLE_MEDIA,
            &HashMap::new(),
        );
        apply_protocol_from_media(&mut r, &pl);
        assert!(r.protocol_set);
        assert_eq!(r.playlist_type.as_deref(), Some("VOD"));
        assert_eq!(r.total_segments, 2);
    }

    /// A CLOSED-CAPTIONS rendition is carried inside a video stream rather than being one,
    /// and an I-frame variant is a trick-play copy of a stream already counted. The Apple
    /// master offers 2 video variants, 2 audio renditions and 1 subtitle rendition.
    #[test]
    fn stream_count_excludes_captions_and_iframe_variants() {
        let master = parse_master_playlist("https://example.com/master.m3u8", APPLE_MASTER);
        assert_eq!(stream_count(&master), 5);
    }

    #[test]
    fn iframe_variants_are_not_counted_as_streams() {
        const MASTER: &str = "#EXTM3U\n\
            #EXT-X-STREAM-INF:BANDWIDTH=3000000,CODECS=\"avc1.64002a\",RESOLUTION=1280x720\n\
            v1/prog_index.m3u8\n\
            #EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=100000,CODECS=\"avc1.64002a\",RESOLUTION=1280x720,URI=\"v1/iframe_index.m3u8\"\n";
        let master = parse_master_playlist("https://example.com/master.m3u8", MASTER);
        assert_eq!(master.variants.len(), 2, "the I-frame variant is still parsed");
        assert_eq!(stream_count(&master), 1);
    }

    /// A media playlist can define its own variables, and a media-only URL is the one
    /// case where there is no master to have defined them.
    #[test]
    fn a_media_playlist_resolves_its_own_defines() {
        let content = "#EXTM3U\n\
            #EXT-X-TARGETDURATION:6\n\
            #EXT-X-DEFINE:NAME=\"DIR\",VALUE=\"seg\"\n\
            #EXT-X-MAP:URI=\"{$DIR}/init.mp4\"\n\
            #EXTINF:6.0,\n\
            {$DIR}/1.m4s\n";
        let pl = parse_media_playlist("https://example.com/v1/prog.m3u8", content, &HashMap::new());
        let (uri, _) = pl.init_segment().expect("init segment");
        assert_eq!(uri, "https://example.com/v1/seg/init.mp4");
        assert_eq!(pl.segments.len(), 1);
    }

    /// An IMPORT takes its value from the multivariant playlist that referred to this one,
    /// so a variant playlist read on its own has nothing to import from.
    #[test]
    fn a_media_playlist_imports_a_definition_from_the_master() {
        let content = "#EXTM3U\n\
            #EXT-X-TARGETDURATION:6\n\
            #EXT-X-DEFINE:IMPORT=\"HOST\"\n\
            #EXT-X-MAP:URI=\"{$HOST}/init.mp4\"\n\
            #EXTINF:6.0,\n\
            1.m4s\n";
        let master_defs = HashMap::from([("HOST".to_string(), "https://cdn.example".to_string())]);
        let pl = parse_media_playlist("https://example.com/v1/prog.m3u8", content, &master_defs);
        let (uri, _) = pl.init_segment().expect("init segment");
        assert_eq!(uri, "https://cdn.example/init.mp4");
    }

    #[test]
    fn hevc_and_dolby_vision_codecs_report_profile_and_level() {
        let (short, _, profile, level) = parse_video_codec("hvc1.2.4.L153.B0");
        assert_eq!(short.as_deref(), Some("H.265"));
        assert_eq!(profile.as_deref(), Some("Main 10"));
        assert_eq!(level.as_deref(), Some("5.1"));

        let (short, long, profile, level) = parse_video_codec("dvh1.05.06");
        assert_eq!(short.as_deref(), Some("H.265"));
        assert_eq!(long.as_deref(), Some("Dolby Vision (HEVC)"));
        assert_eq!(profile.as_deref(), Some("Dolby Vision Profile 5"));
        assert_eq!(level.as_deref(), Some("6"));
    }
}
