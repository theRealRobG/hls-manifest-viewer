//! CEA-608/708 caption extraction from H.264/H.265 SEI NAL units in mdat boxes.
//!
//! This module parses NAL unit streams (using the length-prefixed format from ISO BMFF),
//! finds SEI messages with `user_data_registered_itu_t_t35` payloads containing `cc_data`,
//! and decodes CEA-608 character pairs into displayable text.

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
    let mut pos = header_len;
    let payload = nal_data;

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
