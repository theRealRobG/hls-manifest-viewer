//! CEA-608/708 caption extraction from MP4 segments.
//!
//! Supports two formats:
//! - **SEI-based** (H.264/H.265): Parses NAL unit streams from mdat, finds SEI messages
//!   with `user_data_registered_itu_t_t35` payloads containing `cc_data`.
//! - **Apple `c608` format**: Raw CEA-608 data in a separate track. Samples contain
//!   `cdat` (field 1) and/or `cdt2` (field 2) boxes with cc_data byte pairs.

use mp4_atom::{Atom, Buf, DecodeAtom, Header, Moof, ReadFrom, Tfhd, Traf, Trun};
use std::io::Cursor;

/// Codec type to select correct NAL unit type parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecType {
    H264,
    H265,
}

/// A single caption data unit extracted from SEI.
#[derive(Debug, Clone)]
pub struct CaptionEntry {
    /// 0-based index of the NAL unit in the mdat that contained this caption data.
    pub nal_index: usize,
    /// CEA-608 field (1 or 2), or 0 for CEA-708 DTVCC.
    pub field: u8,
    /// cc_type value (0=field1, 1=field2, 2=DTVCC start, 3=DTVCC continuation).
    pub cc_type: u8,
    /// The two data bytes.
    pub cc_data1: u8,
    pub cc_data2: u8,
    /// Decoded text representation (if CEA-608).
    pub text: Option<String>,
}

/// Parse all caption entries from mdat bytes.
///
/// `nal_length_size` is typically `length_size_minus_one + 1` from `avcC`/`hvcC`.
pub fn extract_captions(
    mdat_body: &[u8],
    nal_length_size: u8,
    codec: CodecType,
) -> Vec<CaptionEntry> {
    let mut entries = Vec::new();
    let mut offset = 0usize;
    let mut nal_index = 0usize;
    let len_size = nal_length_size as usize;

    while offset + len_size <= mdat_body.len() {
        let nal_len = read_nal_length(&mdat_body[offset..], len_size);
        offset += len_size;
        if nal_len == 0 || offset + nal_len > mdat_body.len() {
            break;
        }
        let nal_data = &mdat_body[offset..offset + nal_len];
        offset += nal_len;

        if is_sei_nal(nal_data, codec) {
            parse_sei_for_captions(nal_data, codec, nal_index, &mut entries);
        }
        nal_index += 1;
    }
    entries
}

fn read_nal_length(data: &[u8], len_size: usize) -> usize {
    let mut value = 0usize;
    for &b in &data[..len_size] {
        value = (value << 8) | b as usize;
    }
    value
}

fn is_sei_nal(nal_data: &[u8], codec: CodecType) -> bool {
    if nal_data.is_empty() {
        return false;
    }
    match codec {
        CodecType::H264 => {
            let nal_type = nal_data[0] & 0x1f;
            nal_type == 6 // SEI
        }
        CodecType::H265 => {
            if nal_data.len() < 2 {
                return false;
            }
            let nal_type = (nal_data[0] >> 1) & 0x3f;
            nal_type == 39 || nal_type == 40 // PREFIX_SEI or SUFFIX_SEI
        }
    }
}

/// Remove H.264/H.265 emulation prevention bytes (0x00 0x00 0x03 → 0x00 0x00)
/// from the RBSP to recover the raw SEI payload.
fn remove_emulation_prevention_bytes(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    let mut zero_count = 0u32;
    for &b in data {
        if zero_count == 2 && b == 0x03 {
            // Skip the emulation prevention byte
            zero_count = 0;
            continue;
        }
        if b == 0x00 {
            zero_count += 1;
        } else {
            zero_count = 0;
        }
        result.push(b);
    }
    result
}

/// Parse SEI NAL unit payload for cc_data in user_data_registered_itu_t_t35 messages.
fn parse_sei_for_captions(
    nal_data: &[u8],
    codec: CodecType,
    nal_index: usize,
    entries: &mut Vec<CaptionEntry>,
) {
    // Skip NAL header
    let header_len = match codec {
        CodecType::H264 => 1,
        CodecType::H265 => 2,
    };
    if nal_data.len() <= header_len {
        return;
    }
    // Remove emulation prevention bytes from RBSP before parsing
    let payload = remove_emulation_prevention_bytes(&nal_data[header_len..]);
    let mut pos = 0;

    // SEI can contain multiple messages
    while pos < payload.len() {
        // Read payload_type
        let mut payload_type: u32 = 0;
        while pos < payload.len() && payload[pos] == 0xff {
            payload_type += 255;
            pos += 1;
        }
        if pos >= payload.len() {
            break;
        }
        payload_type += payload[pos] as u32;
        pos += 1;

        // Read payload_size
        let mut payload_size: u32 = 0;
        while pos < payload.len() && payload[pos] == 0xff {
            payload_size += 255;
            pos += 1;
        }
        if pos >= payload.len() {
            break;
        }
        payload_size += payload[pos] as u32;
        pos += 1;

        let sei_end = pos + payload_size as usize;
        if sei_end > payload.len() {
            break;
        }

        // payload_type 4 = user_data_registered_itu_t_t35
        if payload_type == 4 {
            parse_itu_t_t35_for_captions(&payload[pos..sei_end], nal_index, entries);
        }

        pos = sei_end;
    }
}

/// Parse ITU-T T.35 payload for ATSC `cc_data` (A/53 Part 4).
///
/// Expected structure:
/// - country_code: 0xB5 (United States)
/// - provider_code: 0x0031 (ATSC)
/// - user_identifier: 0x47413934 ("GA94")
/// - user_data_type_code: 0x03 (cc_data)
fn parse_itu_t_t35_for_captions(data: &[u8], nal_index: usize, entries: &mut Vec<CaptionEntry>) {
    // Minimum: country_code(1) + provider_code(2) + user_identifier(4) + type_code(1) + flags(1) + count_byte(1) = 10
    if data.len() < 10 {
        return;
    }
    if data[0] != 0xB5 {
        return;
    }
    let provider_code = u16::from_be_bytes([data[1], data[2]]);
    if provider_code != 0x0031 {
        return;
    }
    if &data[3..7] != b"GA94" {
        return;
    }
    if data[7] != 0x03 {
        return;
    }

    // cc_data(): process_em_data(1) + process_cc_data(1) + additional_data(1) + cc_count(5)
    let flags_byte = data[8];
    if (flags_byte >> 6) & 1 == 0 {
        return;
    }
    let cc_count = (flags_byte & 0x1f) as usize;

    let cc_data_start = 10; // skip em_data byte at data[9]
    let cc_data_end = cc_data_start + cc_count * 3;
    if cc_data_end > data.len() {
        return;
    }

    for i in 0..cc_count {
        let off = cc_data_start + i * 3;
        let marker_and_type = data[off];
        let cc_valid = (marker_and_type >> 2) & 1;
        let cc_type = marker_and_type & 0x03;
        let cc_data1 = data[off + 1];
        let cc_data2 = data[off + 2];

        if cc_valid == 0 {
            continue;
        }

        let field = match cc_type {
            0 => 1,
            1 => 2,
            _ => 0,
        };
        let text = if cc_type <= 1 {
            decode_cea608_pair(cc_data1, cc_data2)
        } else {
            None
        };

        entries.push(CaptionEntry {
            nal_index,
            field,
            cc_type,
            cc_data1,
            cc_data2,
            text,
        });
    }
}

/// Extract raw CEA-608 captions from the Apple `c608` track format.
///
/// Parses the full segment data to find `moof`/`traf` boxes for the given `track_id`,
/// then reads sample data from the `mdat` using `trun` offsets and sizes. Each sample
/// contains `cdat` (field 1) and/or `cdt2` (field 2) boxes with raw cc_data byte pairs.
pub fn extract_c608_captions(segment_data: &[u8], track_id: u32) -> Vec<CaptionEntry> {
    let mut entries = Vec::new();
    let mut reader = Cursor::new(segment_data.to_vec());
    let mut sample_index = 0usize;

    while reader.has_remaining() {
        let Ok(header) = Header::read_from(&mut reader) else {
            break;
        };
        let body_size = header.size.unwrap_or(reader.remaining());
        let box_end = reader.position() + body_size as u64;

        if header.kind == Moof::KIND {
            let moof_start = reader.position() as usize - 8; // account for header
            // Find traf boxes matching our track_id and collect sample info
            let samples =
                parse_moof_for_c608_samples(&mut reader, body_size, moof_start, track_id);
            for (data_start, sample_size) in samples {
                if data_start + sample_size <= segment_data.len() {
                    let sample_data = &segment_data[data_start..data_start + sample_size];
                    parse_c608_sample(sample_data, sample_index, &mut entries);
                }
                sample_index += 1;
            }
        }

        reader.set_position(box_end);
    }
    entries
}

/// Parse a `moof` box to find `traf` entries for the given c608 track.
/// Returns a list of (absolute_data_offset, sample_size) for each sample.
///
/// Advances the cursor past the moof body.
fn parse_moof_for_c608_samples(
    reader: &mut Cursor<Vec<u8>>,
    moof_body_size: usize,
    moof_start: usize,
    track_id: u32,
) -> Vec<(usize, usize)> {
    let moof_end = reader.position() + moof_body_size as u64;

    while reader.position() < moof_end {
        let Ok(header) = Header::read_from(reader) else {
            break;
        };
        let body_size = header.size.unwrap_or(reader.remaining());
        let box_end = reader.position() + body_size as u64;

        if header.kind == Traf::KIND
            && let Some(samples) =
                parse_traf_for_c608_samples(reader, body_size, moof_start, track_id)
        {
            return samples;
        }

        reader.set_position(box_end);
    }
    Vec::new()
}

/// Parse a `traf` box: check `tfhd` for track_id match, then extract sample offsets from `trun`.
///
/// Advances the cursor past the traf body.
fn parse_traf_for_c608_samples(
    reader: &mut Cursor<Vec<u8>>,
    traf_body_size: usize,
    moof_start: usize,
    target_track_id: u32,
) -> Option<Vec<(usize, usize)>> {
    let traf_end = reader.position() + traf_body_size as u64;
    let mut tfhd: Option<Tfhd> = None;
    let mut truns: Vec<Trun> = Vec::new();

    // Single pass: decode tfhd and collect trun boxes
    while reader.position() < traf_end {
        let Ok(header) = Header::read_from(reader) else {
            break;
        };
        let body_size = header.size.unwrap_or(reader.remaining());
        let box_end = reader.position() + body_size as u64;

        match header.kind {
            Tfhd::KIND => {
                if let Ok(atom) = Tfhd::decode_atom(&header, reader)
                    && atom.track_id == target_track_id
                {
                    tfhd = Some(atom);
                }
            }
            Trun::KIND => {
                if let Ok(trun) = Trun::decode_atom(&header, reader) {
                    truns.push(trun);
                }
            }
            _ => {}
        }

        reader.set_position(box_end);
    }

    let tfhd = tfhd?;
    let default_sample_size = tfhd.default_sample_size.unwrap_or(0) as usize;

    let mut samples = Vec::new();
    for trun in truns {
        let data_offset = trun.data_offset.unwrap_or(0);
        let abs_data_start = (moof_start as i64 + data_offset as i64) as usize;
        let mut current_offset = abs_data_start;

        for entry in &trun.entries {
            let sample_size = entry.size.map_or(default_sample_size, |s| s as usize);
            samples.push((current_offset, sample_size));
            current_offset += sample_size;
        }
    }

    Some(samples)
}

/// Parse a single c608 sample for `cdat` (field 1) and `cdt2` (field 2) boxes.
fn parse_c608_sample(sample_data: &[u8], sample_index: usize, entries: &mut Vec<CaptionEntry>) {
    let mut offset = 0;
    while offset + 8 <= sample_data.len() {
        let box_size = read_u32_be(&sample_data[offset..]) as usize;
        let box_type = &sample_data[offset + 4..offset + 8];
        if box_size < 8 || offset + box_size > sample_data.len() {
            break;
        }
        let body = &sample_data[offset + 8..offset + box_size];

        let field = if box_type == b"cdat" {
            1u8
        } else if box_type == b"cdt2" {
            2u8
        } else {
            offset += box_size;
            continue;
        };

        // Body contains raw cc_data byte pairs
        let cc_type = if field == 1 { 0 } else { 1 };
        let mut i = 0;
        while i + 1 < body.len() {
            let cc_data1 = body[i];
            let cc_data2 = body[i + 1];
            i += 2;

            // Skip padding/empty pairs
            if cc_data1 == 0x80 && cc_data2 == 0x80 {
                continue;
            }

            let text = decode_cea608_pair(cc_data1, cc_data2);

            entries.push(CaptionEntry {
                nal_index: sample_index,
                field,
                cc_type,
                cc_data1,
                cc_data2,
                text,
            });
        }

        offset += box_size;
    }
}

fn read_u32_be(data: &[u8]) -> u32 {
    u32::from_be_bytes([data[0], data[1], data[2], data[3]])
}

/// Decode a CEA-608 character pair into displayable text.
fn decode_cea608_pair(cc1: u8, cc2: u8) -> Option<String> {
    let c1 = cc1 & 0x7f;
    let c2 = cc2 & 0x7f;

    if c1 == 0 && c2 == 0 {
        return None;
    }
    // Control codes
    if (0x01..=0x1f).contains(&c1) {
        return None;
    }

    let mut text = String::new();
    if c1 >= 0x20 {
        text.push(c1 as char);
    }
    if c2 >= 0x20 {
        text.push(c2 as char);
    }

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
