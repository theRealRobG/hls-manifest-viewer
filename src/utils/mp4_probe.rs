//! Shared init-segment probe for Inspect and Author validation.

use std::io::Cursor;

use mp4_atom::{Header, ReadFrom};

use crate::utils::mp4_atom_properties::{AtomPropertyValue, get_properties};

/// One `trak` of an init segment's `moov`.
///
/// An init can carry more than a video and an audio track — timed metadata (`mebx`)
/// tracks are common, and they are often written first. Every per-track value is
/// therefore kept against its own track rather than flattened onto the probe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackProbe {
    /// `tkhd` track_ID, which is what a media segment's `tfhd` refers to.
    pub track_id: Option<u32>,
    /// `hdlr` handler type — "vide", "soun", "meta", …
    pub handler: Option<String>,
    /// `mdhd` timescale for this track.
    pub timescale: Option<u32>,
    /// Raw `stsd` sample-entry fourCC for this track.
    pub sample_fourcc: Option<String>,
}

impl TrackProbe {
    fn is_handler(&self, handler: &str) -> bool {
        self.handler.as_deref() == Some(handler)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InitSegmentProbe {
    pub major_brand: Option<String>,
    pub compatible_brands: Vec<String>,
    /// Raw stsd sample-entry fourCC (e.g. "avc1", "hvc1", "encv").
    pub video_sample_fourcc: Option<String>,
    pub audio_sample_fourcc: Option<String>,
    pub video_profile: Option<String>,
    pub video_level: Option<String>,
    /// HEVC general_tier_flag as the atom reports it: "true" for High tier, "false" for Main.
    pub video_tier: Option<String>,
    pub has_ludt: bool,
    pub has_senc: bool,
    pub has_saiz: bool,
    pub has_saio: bool,
    pub has_vexu: bool,
    /// `lhvC` layered-HEVC configuration, which is how MV-HEVC is signaled (§1.36).
    pub has_lhvc: bool,
    pub has_tenc: bool,
    /// True when a `tenc` was parsed inside a video track (`vide` handler).
    pub has_video_tenc: bool,
    /// CENC scheme_type from `schm` (e.g. "cenc", "cbcs").
    pub scheme_type: Option<String>,
    /// From `tenc` version ≥1 pattern encryption fields (last track seen).
    pub crypt_byte_block: Option<u8>,
    pub skip_byte_block: Option<u8>,
    /// Same fields, but only from the video track — the 1:9 `cbcs` pattern
    /// requirement applies to video, while audio is commonly encrypted without one.
    pub video_crypt_byte_block: Option<u8>,
    pub video_skip_byte_block: Option<u8>,
    /// HDR10 static metadata boxes / SEI containers often signaled near hvcC.
    pub has_mdcv: bool,
    pub has_clli: bool,
    /// True when an encrypted sample entry (`encv`/`enca`) was observed before `frma`.
    pub had_encrypted_sample_entry: bool,
    /// Media timescale from `mdhd`, preferring the video track then the audio track.
    /// Rules that know which track they are reasoning about should ask for that
    /// track instead — see [`InitSegmentProbe::video_timescale`].
    pub timescale: Option<u32>,
    /// Movie timescale from `mvhd`.
    pub movie_timescale: Option<u32>,
    /// Every `trak` in the `moov`, in the order they appear.
    pub tracks: Vec<TrackProbe>,
}

fn prop_str(val: &AtomPropertyValue) -> String {
    match val {
        AtomPropertyValue::Basic(b) => String::from(b),
        AtomPropertyValue::Table(_) => String::new(),
    }
}

fn fourcc_str(header: &Header) -> String {
    header.kind.to_string()
}

const VIDEO_SAMPLE_ENTRIES: &[&str] = &[
    "avc1", "avc3", "hvc1", "hev1", "dvh1", "dvhe", "av01", "vp09", "vp08", "encv", "mjpg",
];
const AUDIO_SAMPLE_ENTRIES: &[&str] = &[
    "mp4a", "ac-3", "ec-3", "ac-4", "Opus", "enca", "apac", "fLaC",
];

/// Parse init-segment bytes (ftyp + moov). Stops on first decode error.
pub fn probe_init_segment(data: &[u8]) -> InitSegmentProbe {
    let mut info = InitSegmentProbe::default();
    let mut reader = Cursor::new(data.to_vec());
    let mut container_ends: Vec<u64> = Vec::new();
    let mut in_video = false;
    let mut in_audio = false;
    // The current `trak`, filled in as its boxes go by and committed when the next
    // `trak` starts. Buffering it means `mdhd` preceding `hdlr` — the usual order —
    // still lands the timescale on the right track.
    let mut track: Option<TrackProbe> = None;

    loop {
        while let Some(&end) = container_ends.last() {
            if reader.position() >= end {
                container_ends.pop();
            } else {
                break;
            }
        }
        if reader.position() as usize >= reader.get_ref().len() {
            break;
        }
        let Ok(header) = Header::read_from(&mut reader) else {
            break;
        };
        let kind = fourcc_str(&header);
        match kind.as_str() {
            "ludt" => info.has_ludt = true,
            "senc" => info.has_senc = true,
            "saiz" => info.has_saiz = true,
            "saio" => info.has_saio = true,
            "vexu" => info.has_vexu = true,
            "lhvC" => info.has_lhvc = true,
            "tenc" => info.has_tenc = true,
            "mdcv" => info.has_mdcv = true,
            "clli" => info.has_clli = true,
            _ => {}
        }

        let Ok(atom) = get_properties(&header, &mut reader) else {
            break;
        };
        if let Some(e) = atom.new_depth_until {
            container_ends.push(e);
        }

        let props = &atom.properties;
        let get = |key: &str| -> Option<String> {
            props
                .properties
                .iter()
                .find(|(k, _)| k.as_ref() == key)
                .map(|(_, v)| prop_str(v))
                .filter(|s| !s.is_empty())
        };

        match props.box_name {
            "TrackBox" => {
                info.tracks.extend(track.replace(TrackProbe::default()));
            }
            "TrackHeaderBox" => {
                if let Some(t) = track.as_mut() {
                    t.track_id = get("track_id").and_then(|s| s.parse().ok());
                }
            }
            "FileTypeBox" => {
                info.major_brand = get("major_brand");
                if let Some(cb) = get("compatible_brands") {
                    info.compatible_brands = cb
                        .split(|c: char| c == ',' || c.is_whitespace())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
            "HandlerBox" => {
                let handler = get("handler");
                if let Some(t) = track.as_mut() {
                    t.handler = handler.clone();
                }
                match handler.as_deref() {
                    Some("vide") => {
                        in_video = true;
                        in_audio = false;
                    }
                    Some("soun") => {
                        in_audio = true;
                        in_video = false;
                    }
                    _ => {
                        in_video = false;
                        in_audio = false;
                    }
                }
            }
            "OriginalFormatBox" => {
                if let Some(fmt) = get("data_format") {
                    if in_video && info.video_sample_fourcc.as_deref() == Some("encv") {
                        info.video_sample_fourcc = Some(fmt);
                    } else if in_audio && info.audio_sample_fourcc.as_deref() == Some("enca") {
                        info.audio_sample_fourcc = Some(fmt);
                    }
                }
            }
            "MediaHeaderBox" => {
                if let Some(t) = track.as_mut() {
                    t.timescale = get("timescale").and_then(|s| s.parse().ok());
                }
            }
            "MovieHeaderBox" if info.movie_timescale.is_none() => {
                info.movie_timescale = get("timescale").and_then(|s| s.parse().ok());
            }
            "AVCConfigurationBox" if info.video_profile.is_none() => {
                info.video_profile = get("avc_profile_indication");
                info.video_level = get("avc_level_indication");
            }
            "HEVCConfigurationBox" if info.video_profile.is_none() => {
                info.video_profile = get("general_profile_idc").or_else(|| get("profile"));
                info.video_level = get("general_level_idc").or_else(|| get("level"));
                info.video_tier = get("general_tier_flag").or_else(|| get("tier"));
            }
            "TrackEncryptionBox" => {
                info.has_tenc = true;
                if let Some(c) = get("default_crypt_byte_block") {
                    info.crypt_byte_block = c.split_whitespace().next().and_then(|s| s.parse().ok());
                }
                if let Some(s) = get("default_skip_byte_block") {
                    info.skip_byte_block = s.split_whitespace().next().and_then(|x| x.parse().ok());
                }
                if in_video {
                    info.has_video_tenc = true;
                    info.video_crypt_byte_block = info.crypt_byte_block;
                    info.video_skip_byte_block = info.skip_byte_block;
                }
            }
            "SchemeTypeBox" if info.scheme_type.is_none() => {
                info.scheme_type = get("scheme_type");
            }
            _ => {}
        }

        // Capture raw sample-entry fourCC from the header kind
        let is_video_entry = VIDEO_SAMPLE_ENTRIES
            .iter()
            .any(|e| e.eq_ignore_ascii_case(&kind));
        let is_audio_entry = AUDIO_SAMPLE_ENTRIES
            .iter()
            .any(|e| e.eq_ignore_ascii_case(&kind));
        if is_video_entry && (in_video || info.video_sample_fourcc.is_none()) {
            if kind.eq_ignore_ascii_case("encv") {
                info.had_encrypted_sample_entry = true;
            }
            info.video_sample_fourcc = Some(kind.clone());
        }
        if is_audio_entry && (in_audio || info.audio_sample_fourcc.is_none()) {
            if kind.eq_ignore_ascii_case("enca") {
                info.had_encrypted_sample_entry = true;
            }
            info.audio_sample_fourcc = Some(kind.clone());
        }
        if (is_video_entry || is_audio_entry)
            && let Some(t) = track.as_mut()
            && t.sample_fourcc.is_none()
        {
            t.sample_fourcc = Some(kind.clone());
        }
    }
    info.tracks.extend(track);
    info.timescale = info
        .video_timescale()
        .or_else(|| info.audio_timescale())
        .or_else(|| info.tracks.iter().find_map(|t| t.timescale));

    // Byte scan fallback for boxes our property walker may skip.
    for pos in 4..data.len().saturating_sub(3) {
        let typ = &data[pos..pos + 4];
        if !matches!(typ, b"mdcv" | b"clli" | b"ludt" | b"vexu" | b"tenc")
            || !is_plausible_box_at(data, pos)
        {
            continue;
        }
        match typ {
            b"mdcv" => info.has_mdcv = true,
            b"clli" => info.has_clli = true,
            b"ludt" => info.has_ludt = true,
            b"vexu" => info.has_vexu = true,
            b"tenc" => info.has_tenc = true,
            _ => {}
        }
    }

    info
}

/// Whether the fourCC at `pos` is preceded by a box size that fits the buffer.
/// Without that check any ASCII run in a payload — a `vexu` inside a URL, or in
/// compressed sample data — would register as a box and set a probe flag.
fn is_plausible_box_at(data: &[u8], pos: usize) -> bool {
    if pos < 4 || pos + 4 > data.len() {
        return false;
    }
    let start = pos - 4;
    let size = u32::from_be_bytes([data[start], data[start + 1], data[start + 2], data[start + 3]]);
    match size {
        // Size 0 means the box runs to the end of the file.
        0 => true,
        // Size 1 moves the real size into a 64-bit largesize after the type.
        1 => {
            let Some(raw) = data.get(pos + 4..pos + 12) else {
                return false;
            };
            let mut large = [0u8; 8];
            large.copy_from_slice(raw);
            let large = u64::from_be_bytes(large);
            large >= 16 && start as u64 + large <= data.len() as u64
        }
        _ => size >= 8 && start as u64 + size as u64 <= data.len() as u64,
    }
}

impl InitSegmentProbe {
    /// Brands declared on this init (major + compatible).
    pub fn all_brands(&self) -> Vec<&str> {
        let mut out = Vec::new();
        if let Some(m) = &self.major_brand {
            out.push(m.as_str());
        }
        for b in &self.compatible_brands {
            out.push(b.as_str());
        }
        out
    }

    pub fn looks_like_fmp4_init(&self) -> bool {
        self.major_brand.is_some()
            || self.video_sample_fourcc.is_some()
            || self.audio_sample_fourcc.is_some()
            || !self.compatible_brands.is_empty()
    }

    /// Media timescale of the first track with the given `hdlr` handler type.
    pub fn track_timescale(&self, handler: &str) -> Option<u32> {
        self.tracks
            .iter()
            .filter(|t| t.is_handler(handler))
            .find_map(|t| t.timescale)
    }

    /// Timescale of the `vide` track. Durations compared against a video playlist's
    /// EXTINF have to use this rather than the probe's flattened `timescale`, which
    /// falls back to whichever track the init happens to carry.
    pub fn video_timescale(&self) -> Option<u32> {
        self.track_timescale("vide")
    }

    pub fn audio_timescale(&self) -> Option<u32> {
        self.track_timescale("soun")
    }

    fn track_id(&self, handler: &str) -> Option<u32> {
        self.tracks
            .iter()
            .filter(|t| t.is_handler(handler))
            .find_map(|t| t.track_id)
    }

    pub fn video_track_id(&self) -> Option<u32> {
        self.track_id("vide")
    }

    pub fn audio_track_id(&self) -> Option<u32> {
        self.track_id("soun")
    }

    /// What a media segment scan should be told about this init's tracks.
    pub fn scan_hints(&self, codec: VideoCodecHint) -> SegmentScanHints {
        SegmentScanHints {
            codec,
            video_track_id: self.video_track_id(),
            audio_track_id: self.audio_track_id(),
        }
    }
}

/// Which video codec's NAL syntax should be trusted while scanning a segment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VideoCodecHint {
    /// Codec unknown — both H.264 and HEVC NAL types are checked, which can
    /// over-count (an H.264 SPS header reads as an HEVC IDR NAL type).
    #[default]
    Unknown,
    Avc,
    Hevc,
}

impl VideoCodecHint {
    /// Derive from a CODECS token or an fMP4 sample-entry fourCC.
    pub fn from_codec_str(s: &str) -> Self {
        let s = s.trim().to_ascii_lowercase();
        if s.starts_with("avc1") || s.starts_with("avc3") {
            Self::Avc
        } else if s.starts_with("hvc1")
            || s.starts_with("hev1")
            || s.starts_with("dvh1")
            || s.starts_with("dvhe")
        {
            Self::Hevc
        } else {
            Self::Unknown
        }
    }

    fn checks_avc(self) -> bool {
        matches!(self, Self::Avc | Self::Unknown)
    }

    fn checks_hevc(self) -> bool {
        matches!(self, Self::Hevc | Self::Unknown)
    }
}

/// What the matching init segment says about the tracks inside a media segment.
/// Without it a scan cannot tell a video `traf` from a metadata one, and reads
/// whichever fragment happens to come first.
#[derive(Debug, Clone, Copy, Default)]
pub struct SegmentScanHints {
    pub codec: VideoCodecHint,
    pub video_track_id: Option<u32>,
    pub audio_track_id: Option<u32>,
}

impl SegmentScanHints {
    /// Hints for a segment whose init could not be probed.
    pub fn for_codec(codec: VideoCodecHint) -> Self {
        Self {
            codec,
            ..Default::default()
        }
    }
}

/// Best-effort scan of a media segment for Author Phase B/C flags.
pub fn scan_segment_bytes(data: &[u8], hints: SegmentScanHints) -> SegmentScan {
    use std::collections::HashMap;

    let codec = hints.codec;
    let mut scan = SegmentScan::default();

    // MPEG-TS path
    if data.len() >= 188 && data[0] == 0x47 {
        scan.looks_like_ts = true;
        let mut last_cc: HashMap<u16, u8> = HashMap::new();
        let mut cc_ok = true;
        let mut offset = 0usize;
        while offset + 188 <= data.len() {
            if data[offset] != 0x47 {
                offset += 1;
                continue;
            }
            let pid = (((data[offset + 1] as u16) & 0x1f) << 8) | data[offset + 2] as u16;
            let adaptation = (data[offset + 3] >> 4) & 0x3;
            let cc = data[offset + 3] & 0x0f;
            // Continuity only increments when payload is present (adaptation != 0b10)
            if adaptation != 0b10 && pid != 0x1fff {
                if let Some(prev) = last_cc.get(&pid).copied() {
                    let expect = (prev + 1) & 0x0f;
                    if cc != expect {
                        cc_ok = false;
                    }
                }
                last_cc.insert(pid, cc);
            }
            offset += 188;
        }
        scan.ts_continuity_ok = Some(cc_ok);
        scan_nal_hints(data, &mut scan, codec);
        return scan;
    }

    // ISOBMFF: size-based box walk with recursion into containers that hold traf/mdat.
    let mut walk = FragmentWalk {
        data,
        trafs: Vec::new(),
        mdats: Vec::new(),
    };
    walk.walk_boxes(0, data.len(), &mut scan, 0);
    let FragmentWalk { trafs, mdats, .. } = walk;

    scan.video_tfdt = traf_tfdt(&trafs, hints.video_track_id);
    scan.audio_tfdt = traf_tfdt(&trafs, hints.audio_track_id);
    // Without hints the first fragment is all there is to go on, which is what this
    // scan did before it could tell the tracks apart.
    scan.tfdt_base_media_decode_time = scan
        .video_tfdt
        .or(scan.audio_tfdt)
        .or_else(|| trafs.iter().find_map(|t| t.tfdt));

    let video_runs: Vec<(usize, usize)> = match hints.video_track_id {
        Some(id) => trafs
            .iter()
            .filter(|t| t.track_id == Some(id))
            .flat_map(|t| t.runs.iter().copied())
            .collect(),
        None => Vec::new(),
    };
    if !video_runs.is_empty() {
        scan.nal_scan_scoped_to_video = true;
        for (start, end) in video_runs {
            scan_nal_hints(&data[start..end], &mut scan, codec);
        }
    } else if !mdats.is_empty() {
        for &(start, end) in &mdats {
            scan_nal_hints(&data[start..end], &mut scan, codec);
        }
    } else if scan.looks_like_fmp4 {
        // No `mdat` was found — the media may sit in a box shape this walk does not
        // follow, so the whole payload is the only thing left to look at.
        scan_nal_hints(data, &mut scan, codec);
    }

    scan
}

/// Decode time of the fragment belonging to `track_id`, when that track is known.
fn traf_tfdt(trafs: &[TrafInfo], track_id: Option<u32>) -> Option<u64> {
    let track_id = track_id?;
    trafs
        .iter()
        .find(|t| t.track_id == Some(track_id))
        .and_then(|t| t.tfdt)
}

/// One `traf`, with the byte ranges its `trun`s cover so a NAL scan can be limited
/// to a single track's samples.
#[derive(Debug, Clone, Default)]
struct TrafInfo {
    track_id: Option<u32>,
    tfdt: Option<u64>,
    runs: Vec<(usize, usize)>,
}

struct FragmentWalk<'a> {
    data: &'a [u8],
    trafs: Vec<TrafInfo>,
    mdats: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, Default)]
struct TrackFragmentHeader {
    track_id: Option<u32>,
    base_data_offset: Option<u64>,
    default_base_is_moof: bool,
    default_sample_size: Option<u32>,
}

/// Header length and total size of the box starting at `i`, or `None` when no box
/// of a plausible size fits in `[i, end)`.
fn box_bounds(data: &[u8], i: usize, end: usize) -> Option<(usize, usize)> {
    if i + 8 > end {
        return None;
    }
    let mut size = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
    let mut header = 8usize;
    if size == 1 {
        // 64-bit largesize
        if i + 16 > end {
            return None;
        }
        size = u64::from_be_bytes([
            data[i + 8],
            data[i + 9],
            data[i + 10],
            data[i + 11],
            data[i + 12],
            data[i + 13],
            data[i + 14],
            data[i + 15],
        ]) as usize;
        header = 16;
    } else if size == 0 {
        size = end.saturating_sub(i);
    }
    if size < header || i + size > end {
        return None;
    }
    Some((header, size))
}

fn read_u32(data: &[u8], at: usize) -> Option<u32> {
    let raw: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(raw))
}

fn read_u64(data: &[u8], at: usize) -> Option<u64> {
    let raw: [u8; 8] = data.get(at..at + 8)?.try_into().ok()?;
    Some(u64::from_be_bytes(raw))
}

/// Version byte and 24-bit flags of a FullBox body starting at `at`.
fn full_box_header(data: &[u8], at: usize) -> Option<(u8, u32)> {
    let raw = data.get(at..at + 4)?;
    Some((raw[0], u32::from_be_bytes([0, raw[1], raw[2], raw[3]])))
}

impl FragmentWalk<'_> {
    fn walk_boxes(&mut self, start: usize, end: usize, scan: &mut SegmentScan, depth: usize) {
        if depth > 12 {
            return;
        }
        let mut i = start;
        while let Some((header, size)) = box_bounds(self.data, i, end) {
            let typ = &self.data[i + 4..i + 8];
            let body_start = i + header;
            let body_end = i + size;

            match typ {
                b"ftyp" | b"moov" | b"sidx" => scan.looks_like_fmp4 = true,
                b"moof" => {
                    scan.looks_like_fmp4 = true;
                    scan.has_moof = true;
                    self.walk_moof(i, body_start, body_end, scan);
                }
                b"trak" | b"mdia" | b"minf" | b"stbl" => {
                    self.walk_boxes(body_start, body_end, scan, depth + 1);
                }
                b"mdat" => {
                    scan.looks_like_fmp4 = true;
                    self.mdats.push((body_start, body_end));
                }
                b"senc" => scan.has_senc = true,
                b"saiz" => scan.has_saiz = true,
                b"saio" => scan.has_saio = true,
                _ => {}
            }

            i += size;
        }
    }

    /// Walk a `moof`, collecting one [`TrafInfo`] per `traf`. `moof_start` is the
    /// offset of the `moof` box itself, which is where `trun` data offsets are
    /// measured from unless a `traf` says otherwise.
    fn walk_moof(&mut self, moof_start: usize, start: usize, end: usize, scan: &mut SegmentScan) {
        let mut i = start;
        // A `traf` that states no base offset of its own starts where the previous
        // one's data ended, and the first one starts at the `moof` (ISO/IEC 14496-12
        // §8.8.7). CMAF sets default-base-is-moof instead, which skips the chain.
        let mut chained_base = moof_start;
        while let Some((header, size)) = box_bounds(self.data, i, end) {
            if &self.data[i + 4..i + 8] == b"traf" {
                let traf = self.parse_traf(moof_start, chained_base, i + header, i + size, scan);
                if let Some(data_end) = traf.runs.iter().map(|&(_, end)| end).max() {
                    chained_base = data_end;
                }
                self.trafs.push(traf);
            }
            i += size;
        }
    }

    fn parse_traf(
        &self,
        moof_start: usize,
        chained_base: usize,
        start: usize,
        end: usize,
        scan: &mut SegmentScan,
    ) -> TrafInfo {
        let mut info = TrafInfo::default();
        // ISO/IEC 14496-12 requires `tfhd` first in a `traf`, so its defaults are
        // known by the time a `trun` needs them.
        let mut base_offset = chained_base as i64;
        let mut default_sample_size: Option<u32> = None;
        let mut i = start;
        while let Some((header, size)) = box_bounds(self.data, i, end) {
            let typ = &self.data[i + 4..i + 8];
            let body_start = i + header;
            let body_end = i + size;
            match typ {
                b"tfhd" => {
                    let tfhd = self.parse_tfhd(body_start, body_end);
                    info.track_id = tfhd.track_id;
                    base_offset = match (tfhd.base_data_offset, tfhd.default_base_is_moof) {
                        (Some(base), _) => base as i64,
                        (None, true) => moof_start as i64,
                        (None, false) => chained_base as i64,
                    };
                    default_sample_size = tfhd.default_sample_size;
                }
                b"tfdt" => {
                    scan.has_tfdt = true;
                    scan.looks_like_fmp4 = true;
                    if info.tfdt.is_none() {
                        info.tfdt = self.parse_tfdt(body_start, body_end);
                    }
                }
                b"trun" => {
                    if let Some(run) =
                        self.parse_trun(body_start, body_end, base_offset, default_sample_size)
                    {
                        info.runs.push(run);
                    }
                }
                b"senc" => scan.has_senc = true,
                b"saiz" => scan.has_saiz = true,
                b"saio" => scan.has_saio = true,
                _ => {}
            }
            i += size;
        }
        info
    }

    /// Where a `traf`'s samples live and how big they are, before its `trun`s
    /// refine it.
    fn parse_tfhd(&self, body: usize, end: usize) -> TrackFragmentHeader {
        let Some((_, flags)) = full_box_header(self.data, body) else {
            return TrackFragmentHeader::default();
        };
        let mut header = TrackFragmentHeader {
            default_base_is_moof: flags & 0x020000 != 0,
            ..Default::default()
        };
        let mut at = body + 4;
        header.track_id = read_u32(self.data, at);
        at += 4;
        if flags & 0x000001 != 0 {
            header.base_data_offset = read_u64(self.data, at);
            at += 8;
        }
        for flag in [0x000002, 0x000008] {
            if flags & flag != 0 {
                at += 4;
            }
        }
        if flags & 0x000010 != 0 {
            header.default_sample_size = read_u32(self.data, at);
            at += 4;
        }
        if at > end {
            return TrackFragmentHeader {
                track_id: header.track_id,
                ..Default::default()
            };
        }
        header
    }

    fn parse_tfdt(&self, body: usize, end: usize) -> Option<u64> {
        let (version, _) = full_box_header(self.data, body)?;
        if body + 8 > end {
            return None;
        }
        if version == 1 {
            if body + 12 > end {
                return None;
            }
            read_u64(self.data, body + 4)
        } else {
            read_u32(self.data, body + 4).map(u64::from)
        }
    }

    /// Byte range in the file covered by a `trun`'s samples, when their sizes are
    /// known. A run whose sizes are only in `trex` defaults cannot be measured, and
    /// is left out rather than guessed at.
    fn parse_trun(
        &self,
        body: usize,
        end: usize,
        base_offset: i64,
        default_sample_size: Option<u32>,
    ) -> Option<(usize, usize)> {
        let (_, flags) = full_box_header(self.data, body)?;
        let mut at = body + 4;
        let sample_count = read_u32(self.data, at)? as usize;
        at += 4;
        let mut data_offset = 0i64;
        if flags & 0x000001 != 0 {
            data_offset = read_u32(self.data, at)? as i32 as i64;
            at += 4;
        }
        if flags & 0x000004 != 0 {
            at += 4;
        }

        let has_sample_size = flags & 0x000200 != 0;
        // Bytes this run carries per sample. A `sample_count` larger than the box can
        // hold describes a run that is not there, and walking it to find that out
        // would take as long as the count says.
        let per_sample: usize = [0x000100, 0x000200, 0x000400, 0x000800]
            .into_iter()
            .filter(|flag| flags & flag != 0)
            .count()
            * 4;
        if sample_count.checked_mul(per_sample)? > end.saturating_sub(at) {
            return None;
        }

        let total: u64 = if has_sample_size {
            let mut total = 0u64;
            for _ in 0..sample_count {
                if flags & 0x000100 != 0 {
                    at += 4;
                }
                total += u64::from(read_u32(self.data, at)?);
                at += 4;
                for flag in [0x000400, 0x000800] {
                    if flags & flag != 0 {
                        at += 4;
                    }
                }
            }
            total
        } else {
            sample_count as u64 * u64::from(default_sample_size?)
        };

        let start = base_offset.checked_add(data_offset)?;
        if start < 0 {
            return None;
        }
        let start = start as usize;
        let run_end = start.checked_add(usize::try_from(total).ok()?)?;
        if start >= self.data.len() || run_end > self.data.len() {
            return None;
        }
        Some((start, run_end))
    }
}

/// Soft cap on bytes scanned per payload; only pathological segments are truncated,
/// in which case the IDR count under-reports (§1.13 estimates read as conservative).
const MAX_NAL_SCAN_BYTES: usize = 8 * 1024 * 1024;

fn scan_nal_hints(data: &[u8], scan: &mut SegmentScan, codec: VideoCodecHint) {
    let buf = &data[..data.len().min(MAX_NAL_SCAN_BYTES)];

    // Offsets of the NAL header byte, so the Annex-B and length-prefixed passes
    // agree on a position for the same NAL and can be deduped.
    let mut nals = NalOffsets::default();

    // Annex-B start codes (MPEG-TS, and some fMP4 payloads).
    let mut i = 0usize;
    while i + 4 < buf.len() {
        let (sc_len, nal_off) = if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1 {
            (3usize, i + 3)
        } else if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 0 && buf[i + 3] == 1 {
            (4usize, i + 4)
        } else {
            i += 1;
            continue;
        };
        classify_nal(buf, nal_off, codec, &mut nals, scan);
        i += sc_len;
    }

    // Length-prefixed NALs (common in fMP4 mdat). The walk must stay aligned from
    // offset 0: a bad length means the payload is not length-prefixed, so we stop
    // instead of resynchronising, which would invent NAL headers in Annex-B/TS data.
    let mut off = 0usize;
    while off + 4 < buf.len() {
        let nalu_len =
            u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as usize;
        if nalu_len == 0 || nalu_len > buf.len() - (off + 4) {
            break;
        }
        classify_nal(buf, off + 4, codec, &mut nals, scan);
        off += 4 + nalu_len;
    }

    nals.commit(scan);
}

/// Offsets of the random-access NAL headers one payload holds. An IDR is also an IRAP,
/// so the two sets overlap; they are kept apart because §7.4 is written about IDRs while
/// §1.13 counts key frames, which a CRA also provides.
#[derive(Debug, Default)]
struct NalOffsets {
    irap: Vec<usize>,
    idr: Vec<usize>,
}

/// A random-access NAL this close to the payload start opens the segment.
const SEGMENT_START_WINDOW: usize = 8 * 1024;

impl NalOffsets {
    fn commit(mut self, scan: &mut SegmentScan) {
        for (offsets, count, at_start, hint) in [
            (
                &mut self.irap,
                &mut scan.irap_count,
                &mut scan.irap_at_start,
                &mut scan.has_irap_nal_hint,
            ),
            (
                &mut self.idr,
                &mut scan.idr_count,
                &mut scan.idr_at_start,
                &mut scan.has_idr_nal_hint,
            ),
        ] {
            offsets.sort_unstable();
            offsets.dedup();
            let Some(&first) = offsets.first() else {
                continue;
            };
            *count += offsets.len();
            *hint = true;
            if !*at_start {
                *at_start = first < SEGMENT_START_WINDOW;
            }
        }
    }
}

/// Inspect the NAL header at `nal_off`, recording random-access offsets and CEA-608/708
/// SEI hints.
fn classify_nal(
    buf: &[u8],
    nal_off: usize,
    codec: VideoCodecHint,
    nals: &mut NalOffsets,
    scan: &mut SegmentScan,
) {
    let Some(&b0) = buf.get(nal_off) else {
        return;
    };
    if codec.checks_avc() {
        match b0 & 0x1f {
            // H.264 has no CRA, so its only random-access picture is the IDR.
            5 => {
                nals.irap.push(nal_off);
                nals.idr.push(nal_off);
            }
            6 if has_ga94_payload(buf, nal_off) => scan.has_cc_sei_hint = true,
            _ => {}
        }
    }
    if codec.checks_hevc() {
        match (b0 >> 1) & 0x3f {
            // HEVC IRAP types (ISO/IEC 23008-2 Table 7-1): BLA 16–18, IDR 19–20, CRA 21.
            // A CRA opens a segment just as well as an IDR does, so all of them count as
            // random access; only 19 and 20 are IDRs, which is what §7.4 asks for.
            t @ 16..=21 => {
                nals.irap.push(nal_off);
                if matches!(t, 19 | 20) {
                    nals.idr.push(nal_off);
                }
            }
            39 if has_ga94_payload(buf, nal_off) => scan.has_cc_sei_hint = true,
            _ => {}
        }
    }
}

fn has_ga94_payload(buf: &[u8], nal_off: usize) -> bool {
    let end = (nal_off + 64).min(buf.len());
    buf[nal_off..end].windows(4).any(|w| w == b"GA94")
}

#[derive(Debug, Clone, Default)]
pub struct SegmentScan {
    pub looks_like_ts: bool,
    pub looks_like_fmp4: bool,
    pub has_moof: bool,
    /// An IRAP that is also an IDR was seen (HEVC NAL 19–20, H.264 NAL 5).
    pub has_idr_nal_hint: bool,
    /// IDR found near the start of the segment payload.
    pub idr_at_start: bool,
    pub idr_count: usize,
    /// Any IRAP was seen, including the CRA and BLA pictures that open an HEVC segment
    /// without being an IDR.
    pub has_irap_nal_hint: bool,
    /// IRAP found near the start of the segment payload.
    pub irap_at_start: bool,
    pub irap_count: usize,
    pub has_tfdt: bool,
    /// Decode time of the video fragment, when the init named a video track.
    pub video_tfdt: Option<u64>,
    /// Decode time of the audio fragment, when the init named an audio track.
    pub audio_tfdt: Option<u64>,
    /// Decode time of the video fragment, falling back to audio and then to whichever
    /// `traf` came first when the tracks could not be told apart.
    pub tfdt_base_media_decode_time: Option<u64>,
    /// True when NAL hints came from the video track's `trun` byte ranges. When false
    /// the whole payload was read, so IDR counts include any other track's samples.
    pub nal_scan_scoped_to_video: bool,
    pub has_senc: bool,
    pub has_saiz: bool,
    pub has_saio: bool,
    /// `Some(false)` when a continuity counter discontinuity was observed in-sample.
    pub ts_continuity_ok: Option<bool>,
    /// Best-effort CEA-608/708 SEI / GA94 hint.
    pub has_cc_sei_hint: bool,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_probe() {
        let p = probe_init_segment(&[]);
        assert!(p.major_brand.is_none());
    }

    #[test]
    fn ascii_fourcc_without_a_box_size_is_not_a_box() {
        // The four bytes ahead of "vexu" read as a ~1.7 GB size, so nothing here
        // looks like a box even though the fourCC is present verbatim.
        let p = probe_init_segment(b"free-text-with-vexu-inside-and-tenc-too");
        assert!(!p.has_vexu);
        assert!(!p.has_tenc);
    }

    #[test]
    fn fallback_scan_accepts_a_sized_box() {
        let mut data = vec![0u8; 16];
        data.extend_from_slice(&8u32.to_be_bytes());
        data.extend_from_slice(b"vexu");
        assert!(probe_init_segment(&data).has_vexu);
    }

    #[test]
    fn box_size_must_fit_the_buffer() {
        let mut data = 64u32.to_be_bytes().to_vec();
        data.extend_from_slice(b"vexu");
        // A 64-byte box in a 12-byte buffer is a truncated read at best.
        data.extend_from_slice(&[0u8; 4]);
        assert!(!is_plausible_box_at(&data, 4));
    }

    #[test]
    fn scan_ts_sync() {
        let mut data = vec![0u8; 188];
        data[0] = 0x47;
        let s = scan_segment_bytes(&data, SegmentScanHints::for_codec(VideoCodecHint::Unknown));
        assert!(s.looks_like_ts);
    }

    /// Length-prefixed NAL of `nal_header` padded to `payload_len` bytes.
    fn length_prefixed_nal(nal_header: u8, payload_len: usize) -> Vec<u8> {
        let mut out = (payload_len as u32).to_be_bytes().to_vec();
        out.push(nal_header);
        out.resize(4 + payload_len, 0);
        out
    }

    fn mdat(payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(b"mdat");
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn idr_count_accumulates_across_mdat_boxes() {
        let mut data = mdat(&length_prefixed_nal(0x65, 32));
        data.extend(mdat(&length_prefixed_nal(0x65, 32)));
        let s = scan_segment_bytes(&data, SegmentScanHints::for_codec(VideoCodecHint::Avc));
        assert!(s.has_idr_nal_hint);
        assert_eq!(s.idr_count, 2);
    }

    #[test]
    fn avc_sps_is_not_counted_as_idr() {
        // 0x27 is an H.264 SPS, but reads as HEVC NAL type 19 (IDR_W_RADL).
        let data = mdat(&length_prefixed_nal(0x27, 32));
        let avc = scan_segment_bytes(&data, SegmentScanHints::for_codec(VideoCodecHint::Avc));
        assert!(!avc.has_idr_nal_hint);
        assert_eq!(avc.idr_count, 0);

        let unknown = scan_segment_bytes(&data, SegmentScanHints::for_codec(VideoCodecHint::Unknown));
        assert!(unknown.has_idr_nal_hint);
    }

    #[test]
    fn annex_b_and_length_prefixed_idr_deduped() {
        // A 4-byte start code also parses as a length prefix of 1, so both passes
        // see the same NAL header and must count it once.
        let mut payload = vec![0u8, 0, 0, 1, 0x65];
        payload.resize(64, 0);
        let s = scan_segment_bytes(&mdat(&payload), SegmentScanHints::for_codec(VideoCodecHint::Avc));
        assert_eq!(s.idr_count, 1);
    }

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

    /// A `trak` in the order a real init writes it: `mdhd` ahead of `hdlr`, so the
    /// timescale is known before the handler that gives it meaning.
    fn trak(track_id: u32, handler: &[u8; 4], timescale: u32, sample_entry: &[u8]) -> Vec<u8> {
        use mp4_atom::{FourCC, Hdlr, Mdhd, Tkhd};
        let mut mdia = encoded(&Mdhd {
            timescale,
            ..Default::default()
        });
        mdia.extend(encoded(&Hdlr {
            handler: FourCC::new(handler),
            name: String::new(),
        }));
        let mut stsd = vec![0u8; 4]; // version + flags
        stsd.extend_from_slice(&1u32.to_be_bytes()); // entry_count
        stsd.extend_from_slice(sample_entry);
        let stbl = boxed(b"stbl", &boxed(b"stsd", &stsd));
        mdia.extend(boxed(b"minf", &stbl));

        let mut trak = encoded(&Tkhd {
            track_id,
            ..Default::default()
        });
        trak.extend(boxed(b"mdia", &mdia));
        boxed(b"trak", &trak)
    }

    /// Init carrying a timed-metadata track at 90 kHz ahead of a 600 Hz video track,
    /// which is the layout that used to hand every rule the metadata timescale.
    fn init_with_metadata_track_first() -> Vec<u8> {
        use mp4_atom::{Encode, FourCC, Ftyp, Visual};
        let mut out = encoded(&Ftyp {
            major_brand: FourCC::new(b"iso5"),
            minor_version: 0,
            compatible_brands: vec![
                FourCC::new(b"isom"),
                FourCC::new(b"iso5"),
                FourCC::new(b"hlsf"),
            ],
        });
        let mut visual = Vec::new();
        Visual {
            width: 1280,
            height: 720,
            ..Default::default()
        }
        .encode(&mut visual)
        .expect("visual sample entry encodes");
        let mut moov = trak(1, b"meta", 90_000, &boxed(b"mebx", &[0u8; 8]));
        moov.extend(trak(2, b"vide", 600, &boxed(b"hvc1", &visual)));
        out.extend(boxed(b"moov", &moov));
        out
    }

    /// One `traf` plus the samples it points at, for [`media_segment`].
    struct Fragment {
        track_id: u32,
        tfdt: u64,
        payload: Vec<u8>,
        /// Carry the sample size in the `tfhd` defaults rather than in the `trun`.
        size_in_tfhd_default: bool,
        /// Set default-base-is-moof, as CMAF does. Without it the run offsets chain
        /// on from the previous fragment's data instead.
        base_is_moof: bool,
    }

    impl Fragment {
        /// A fragment whose single sample reads as an HEVC IDR_W_RADL NAL (type 19).
        fn new(track_id: u32, tfdt: u64) -> Self {
            Self {
                track_id,
                tfdt,
                payload: length_prefixed_nal(0x26, 40),
                size_in_tfhd_default: false,
                base_is_moof: true,
            }
        }

        /// The same fragment opened by `nal_header` instead of an IDR.
        fn opened_by(mut self, nal_header: u8) -> Self {
            self.payload = length_prefixed_nal(nal_header, 40);
            self
        }
    }

    fn traf(fragment: &Fragment, data_offset: i32) -> Vec<u8> {
        let size = fragment.payload.len() as u32;
        let mut tfhd = vec![0x00, 0x00, 0x00, 0x00]; // version 0, no flags yet
        if fragment.base_is_moof {
            tfhd[1] |= 0x02;
        }
        if fragment.size_in_tfhd_default {
            tfhd[3] |= 0x10;
        }
        tfhd.extend_from_slice(&fragment.track_id.to_be_bytes());
        if fragment.size_in_tfhd_default {
            tfhd.extend_from_slice(&size.to_be_bytes());
        }

        let mut tfdt = vec![1, 0, 0, 0]; // version 1
        tfdt.extend_from_slice(&fragment.tfdt.to_be_bytes());

        // flags: data-offset-present (+ sample-size-present)
        let mut trun = vec![0x00, 0x00, 0x00, 0x01];
        if !fragment.size_in_tfhd_default {
            trun[2] |= 0x02;
        }
        trun.extend_from_slice(&1u32.to_be_bytes()); // sample_count
        trun.extend_from_slice(&data_offset.to_be_bytes());
        if !fragment.size_in_tfhd_default {
            trun.extend_from_slice(&size.to_be_bytes());
        }

        let mut body = boxed(b"tfhd", &tfhd);
        body.extend(boxed(b"tfdt", &tfdt));
        body.extend(boxed(b"trun", &trun));
        boxed(b"traf", &body)
    }

    /// `moof` + `mdat` holding each fragment's payload back to back. Run offsets are
    /// relative to the `moof`, so the box is laid out twice: once to learn its size,
    /// then again with the offsets that size implies.
    fn media_segment(fragments: &[Fragment]) -> Vec<u8> {
        let build = |offsets: &[i32]| {
            let mut body = boxed(b"mfhd", &[0, 0, 0, 0, 0, 0, 0, 1]);
            for (fragment, &offset) in fragments.iter().zip(offsets) {
                body.extend(traf(fragment, offset));
            }
            boxed(b"moof", &body)
        };
        let placeholder = vec![0i32; fragments.len()];
        // The segment starts at the `moof`, so a file offset is also a moof-relative one.
        let mut sample_start = build(&placeholder).len() as i32 + 8; // past the mdat header
        let mut chained_base = 0i32;
        let mut offsets = Vec::new();
        let mut payload = Vec::new();
        for fragment in fragments {
            let base = if fragment.base_is_moof { 0 } else { chained_base };
            offsets.push(sample_start - base);
            sample_start += fragment.payload.len() as i32;
            chained_base = sample_start;
            payload.extend_from_slice(&fragment.payload);
        }
        let mut out = build(&offsets);
        out.extend(mdat(&payload));
        out
    }

    /// A timed-metadata sample ahead of a video one, both of which read as an HEVC
    /// IRAP NAL — arbitrary `mebx` bytes are not NAL syntax, but nothing stops them
    /// looking like it, which is why a scan has to know where the video samples are.
    fn metadata_and_video_fragments() -> Vec<Fragment> {
        vec![Fragment::new(1, 900_000), Fragment::new(2, 6_000)]
    }

    #[test]
    fn init_probe_keeps_a_timescale_per_track() {
        let probe = probe_init_segment(&init_with_metadata_track_first());
        assert_eq!(probe.video_timescale(), Some(600));
        assert_eq!(probe.track_timescale("meta"), Some(90_000));
        assert_eq!(probe.audio_timescale(), None);
        // The flat field is what most rules still read, so it has to name the video
        // track rather than whichever `mdhd` the walk happened to reach first.
        assert_eq!(probe.timescale, Some(600));
        assert_eq!(probe.video_track_id(), Some(2));
        assert_eq!(probe.video_sample_fourcc.as_deref(), Some("hvc1"));
        assert_eq!(probe.tracks.len(), 2);
        assert_eq!(probe.tracks[0].handler.as_deref(), Some("meta"));
        assert_eq!(probe.tracks[0].sample_fourcc, None);
    }

    #[test]
    fn video_tfdt_comes_from_the_video_track_fragment() {
        let probe = probe_init_segment(&init_with_metadata_track_first());
        let segment = media_segment(&metadata_and_video_fragments());

        let scan = scan_segment_bytes(&segment, probe.scan_hints(VideoCodecHint::Hevc));
        assert!(scan.has_moof && scan.has_tfdt);
        assert_eq!(scan.video_tfdt, Some(6_000));
        assert_eq!(scan.tfdt_base_media_decode_time, Some(6_000));

        // Without the init's track IDs the first fragment is all there is to go on.
        let blind =
            scan_segment_bytes(&segment, SegmentScanHints::for_codec(VideoCodecHint::Hevc));
        assert_eq!(blind.video_tfdt, None);
        assert_eq!(blind.tfdt_base_media_decode_time, Some(900_000));
    }

    #[test]
    fn metadata_samples_are_not_counted_as_irap_when_runs_are_known() {
        let probe = probe_init_segment(&init_with_metadata_track_first());
        let segment = media_segment(&metadata_and_video_fragments());

        let scan = scan_segment_bytes(&segment, probe.scan_hints(VideoCodecHint::Hevc));
        assert!(scan.nal_scan_scoped_to_video);
        assert_eq!(scan.idr_count, 1);
        assert!(scan.idr_at_start);

        let blind =
            scan_segment_bytes(&segment, SegmentScanHints::for_codec(VideoCodecHint::Hevc));
        assert!(!blind.nal_scan_scoped_to_video);
        assert_eq!(blind.idr_count, 2);
    }

    /// An open-GOP encoder starts a segment on a CRA, which gives random access without
    /// being an IDR. Counting only IDRs reads such a segment as having no key frames.
    #[test]
    fn cra_and_bla_opened_runs_are_irap_without_being_idr() {
        let probe = probe_init_segment(&init_with_metadata_track_first());
        // HEVC NAL type 21 (CRA_NUT) and type 16 (BLA_W_LP) in the NAL header's high bits.
        for nal_header in [0x2a, 0x20] {
            let segment = media_segment(&[
                Fragment::new(1, 900_000),
                Fragment::new(2, 6_000).opened_by(nal_header),
            ]);
            let scan = scan_segment_bytes(&segment, probe.scan_hints(VideoCodecHint::Hevc));
            assert!(scan.nal_scan_scoped_to_video);
            assert_eq!(scan.irap_count, 1, "NAL header {nal_header:#x}");
            assert!(scan.irap_at_start && scan.has_irap_nal_hint);
            assert_eq!(scan.idr_count, 0);
            assert!(!scan.has_idr_nal_hint && !scan.idr_at_start);
        }
    }

    /// An IDR is an IRAP as well, so a segment that opens on one satisfies both counts.
    #[test]
    fn idr_opened_run_counts_as_irap_too() {
        let probe = probe_init_segment(&init_with_metadata_track_first());
        let segment = media_segment(&metadata_and_video_fragments());
        let scan = scan_segment_bytes(&segment, probe.scan_hints(VideoCodecHint::Hevc));
        assert_eq!((scan.idr_count, scan.irap_count), (1, 1));
        assert!(scan.idr_at_start && scan.irap_at_start);
    }

    #[test]
    fn run_ranges_fall_back_to_the_tfhd_default_sample_size() {
        let probe = probe_init_segment(&init_with_metadata_track_first());
        let mut fragments = metadata_and_video_fragments();
        for fragment in &mut fragments {
            fragment.size_in_tfhd_default = true;
        }
        let scan = scan_segment_bytes(
            &media_segment(&fragments),
            probe.scan_hints(VideoCodecHint::Hevc),
        );
        assert!(scan.nal_scan_scoped_to_video);
        assert_eq!(scan.idr_count, 1);
        assert_eq!(scan.video_tfdt, Some(6_000));
    }

    /// A fragment that sets no base offset of its own starts where the previous
    /// fragment's data ended, which a scan has to follow to stay on the right track.
    #[test]
    fn run_ranges_chain_on_from_the_previous_fragment() {
        let probe = probe_init_segment(&init_with_metadata_track_first());
        let mut fragments = metadata_and_video_fragments();
        for fragment in &mut fragments {
            fragment.base_is_moof = false;
        }
        let scan = scan_segment_bytes(
            &media_segment(&fragments),
            probe.scan_hints(VideoCodecHint::Hevc),
        );
        assert!(scan.nal_scan_scoped_to_video);
        assert_eq!(scan.idr_count, 1);
        assert_eq!(scan.video_tfdt, Some(6_000));
    }

    #[test]
    fn codec_hint_from_codec_str() {
        assert_eq!(
            VideoCodecHint::from_codec_str("avc1.640028"),
            VideoCodecHint::Avc
        );
        assert_eq!(
            VideoCodecHint::from_codec_str("hvc1.2.4.L153.B0"),
            VideoCodecHint::Hevc
        );
        assert_eq!(
            VideoCodecHint::from_codec_str("mp4a.40.2"),
            VideoCodecHint::Unknown
        );
    }
}
