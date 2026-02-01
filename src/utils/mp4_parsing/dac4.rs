use crate::utils::bitter::ByteAlign;
use bitter::{BigEndianReader, BitReader};
use mp4_atom::{Atom, Buf, BufMut, FourCC, Result};

/// AC4SpecificBox, ETSI TS 103 190-2 V1.2.1 (2018-02) Sect E.5.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dac4 {
    pub ac4_dsi_version: u8,
    pub bitstream_version: u8,
    pub fs_index: bool,
    pub frame_rate_index: u8,
    pub short_program_id: Option<u16>,
    pub program_uuid: Option<[u8; 16]>,
    pub bit_rate_mode: Ac4BitrateMode,
    pub bit_rate: u32,
    pub bit_rate_precision: u32,
    pub presentations: Vec<Ac4Presentation>,
}
const READ_ERR: mp4_atom::Error = mp4_atom::Error::OutOfBounds;
impl Atom for Dac4 {
    const KIND: FourCC = FourCC::new(b"dac4");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut reader = BigEndianReader::new(buf.slice(buf.remaining()));
        let starting_bytes_remaining = reader.bytes_remaining();
        let ac4_dsi_version = reader.read_bits(3).ok_or(READ_ERR)? as u8;
        let bitstream_version = reader.read_bits(7).ok_or(READ_ERR)? as u8;
        let fs_index = reader.read_bit().ok_or(READ_ERR)?;
        let frame_rate_index = reader.read_bits(4).ok_or(READ_ERR)? as u8;
        let n_presentations = reader.read_bits(9).ok_or(READ_ERR)?;
        let (short_program_id, program_uuid) = if bitstream_version > 1 {
            let b_program_id = reader.read_bit().ok_or(READ_ERR)?;
            if b_program_id {
                let short_program_id = reader.read_u16().ok_or(READ_ERR)?;
                let b_uuid = reader.read_bit().ok_or(READ_ERR)?;
                if b_uuid {
                    let mut program_uuid = [0u8; 16];
                    reader
                        .read_bytes(&mut program_uuid)
                        .then_some(0)
                        .ok_or(READ_ERR)?;
                    (Some(short_program_id), Some(program_uuid))
                } else {
                    (Some(short_program_id), None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        let bit_rate_mode = Ac4BitrateMode::from(reader.read_bits(2).ok_or(READ_ERR)? as u8);
        let bit_rate = reader.read_u32().ok_or(READ_ERR)?;
        let bit_rate_precision = reader.read_u32().ok_or(READ_ERR)?;
        reader.align().map_err(|_| READ_ERR)?;
        let mut presentations = Vec::with_capacity(n_presentations as usize);
        for _ in 0..n_presentations {
            let presentation_version = reader.read_u8().ok_or(READ_ERR)?;
            let mut pres_bytes = reader.read_u8().ok_or(READ_ERR)? as usize;
            if pres_bytes == 255 {
                let add_pres_bytes = reader.read_u16().ok_or(READ_ERR)? as usize;
                pres_bytes += add_pres_bytes;
            }
            let remaining_before_reading_presentation = reader.bytes_remaining();
            if presentation_version == 0 {
                presentations.push(Ac4Presentation::V0(ac4_presentation_v0_dsi(&mut reader)?));
            } else if presentation_version == 1 {
                presentations.push(Ac4Presentation::V1(ac4_presentation_v1_dsi(
                    &mut reader,
                    pres_bytes,
                )?));
            } else if presentation_version == 2 {
                // V2 extension provided by:
                // https://media.developer.dolby.com/AC4/AC4_DASH_for_BROADCAST_SPEC.pdf
                presentations.push(Ac4Presentation::V2(ac4_presentation_v1_dsi(
                    &mut reader,
                    pres_bytes,
                )?));
            } else {
                presentations.push(Ac4Presentation::UnknownVersion(presentation_version));
            }
            let presentation_bytes =
                remaining_before_reading_presentation - reader.bytes_remaining();
            if pres_bytes < presentation_bytes {
                return Err(mp4_atom::Error::Unsupported(
                    "dac4 pres_bytes < presentation_bytes",
                ));
            }
            let mut skip_bytes = vec![0u8; pres_bytes - presentation_bytes];
            reader
                .read_bytes(&mut skip_bytes)
                .then_some(0)
                .ok_or(READ_ERR)?;
        }
        let ending_bytes_remaining = reader.bytes_remaining();
        buf.advance(starting_bytes_remaining - ending_bytes_remaining);
        Ok(Dac4 {
            ac4_dsi_version,
            bitstream_version,
            fs_index,
            frame_rate_index,
            short_program_id,
            program_uuid,
            bit_rate_mode,
            bit_rate,
            bit_rate_precision,
            presentations,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ac4BitrateMode {
    NotSpecified,
    Constant,
    Average,
    Variable,
}
impl From<u8> for Ac4BitrateMode {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Constant,
            2 => Self::Average,
            3 => Self::Variable,
            _ => Self::NotSpecified,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ac4Presentation {
    V0(Ac4PresentationV0),
    V1(Ac4PresentationV1),
    // V2 extension provided by:
    // https://media.developer.dolby.com/AC4/AC4_DASH_for_BROADCAST_SPEC.pdf
    V2(Ac4PresentationV1),
    UnknownVersion(u8),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4PresentationV0 {
    pub presentation_config: u8,
    pub md_compat: Option<u8>,
    pub presentation_id: Option<u8>,
    pub dsi_frame_rate_multiply_info: Option<u8>,
    pub presentation_emdf_version: Option<u8>,
    pub presentation_key_id: Option<u16>,
    pub presentation_channel_mask: Option<[u8; 3]>,
    pub b_hsf_ext: Option<bool>,
    pub substream_groups: Option<Vec<Ac4SubstreamGroup>>,
    pub b_pre_virtualized: Option<bool>,
    pub emdf_substreams: Vec<EmdfSubstream>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4PresentationV1 {
    pub presentation_config_v1: u8,
    pub md_compat: Option<u8>,
    pub presentation_id: Option<u8>,
    pub dsi_frame_rate_multiply_info: Option<u8>,
    pub dsi_frame_rate_fraction_info: Option<u8>,
    pub presentation_emdf_version: Option<u8>,
    pub presentation_key_id: Option<u16>,
    pub b_presentation_channel_coded: Option<bool>,
    pub dsi_presentation_ch_mode: Option<u8>,
    pub pres_b_4_back_channels_present: Option<bool>,
    pub pres_top_channel_pairs: Option<u8>,
    pub presentation_channel_mask_v1: Option<[u8; 3]>,
    pub b_presentation_core_differs: Option<bool>,
    pub b_presentation_core_channel_coded: Option<bool>,
    pub dsi_presentation_channel_mode_core: Option<u8>,
    pub b_presentation_filter: Option<bool>,
    pub b_enable_presentation: Option<bool>,
    pub filter_data: Option<Vec<u8>>,
    pub b_multi_pid: Option<bool>,
    pub substream_groups: Option<Vec<Ac4SubstreamGroup>>,
    pub b_pre_virtualized: Option<bool>,
    pub emdf_substreams: Vec<EmdfSubstream>,
    pub bit_rate_mode: Option<Ac4BitrateMode>,
    pub bit_rate: Option<u32>,
    pub bit_rate_precision: Option<u32>,
    pub alternative_info: Option<Ac4AlternativeInfo>,
    pub de_indicator: Option<bool>,
    pub immersive_audio_indicator: Option<bool>,
    pub extended_presentation_id: Option<u16>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4SubstreamGroup {
    pub b_substreams_present: bool,
    pub b_hsf_ext: bool,
    pub b_channel_coded: bool,
    pub substreams: Vec<Ac4PresentationSubstream>,
    pub content_classifier: Option<Ac4ContentClassifier>,
    pub language_tag: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmdfSubstream {
    pub emdf_version: u8,
    pub key_id: u16,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4PresentationSubstream {
    pub dsi_sf_multiplier: u8,
    pub bitrate_indicator: Option<u8>,
    pub channel_mask: Option<[u8; 3]>,
    pub n_dmx_objects_minus1: Option<u8>,
    pub n_umx_objects_minus1: Option<u8>,
    pub contains_bed_objects: Option<bool>,
    pub contains_dynamic_objects: Option<bool>,
    pub contains_isf_objects: Option<bool>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ac4ContentClassifier {
    CompleteMain,     // 000
    MusicAndEffects,  // 001
    VisuallyImpaired, // 010
    HearingImpaired,  // 011
    Dialogue,         // 100
    Commentary,       // 101
    Emergency,        // 110
    VoiceOver,        // 111
}
impl From<u8> for Ac4ContentClassifier {
    fn from(value: u8) -> Self {
        match value {
            0b001 => Self::MusicAndEffects,
            0b010 => Self::VisuallyImpaired,
            0b011 => Self::HearingImpaired,
            0b100 => Self::Dialogue,
            0b101 => Self::Commentary,
            0b110 => Self::Emergency,
            0b111 => Self::VoiceOver,
            _ => Self::CompleteMain,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4AlternativeInfo {
    pub presentation_name: String,
    pub targets: Vec<Ac4AlternativeInfoTarget>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4AlternativeInfoTarget {
    pub md_compat: u8,
    pub device_category: u8,
}
fn ac4_presentation_v0_dsi(reader: &mut BigEndianReader) -> Result<Ac4PresentationV0> {
    let presentation_config = reader.read_bits(5).ok_or(READ_ERR)? as u8;
    let (
        md_compat,
        presentation_id,
        dsi_frame_rate_multiply_info,
        presentation_emdf_version,
        presentation_key_id,
        presentation_channel_mask,
        b_hsf_ext,
        substream_groups,
        b_pre_virtualized,
        b_add_emdf_substreams,
    ) = if presentation_config == 0x06 {
        (None, None, None, None, None, None, None, None, None, true)
    } else {
        let md_compat = reader.read_bits(3).ok_or(READ_ERR)? as u8;
        let b_presentation_id = reader.read_bit().ok_or(READ_ERR)?;
        let presentation_id = if b_presentation_id {
            Some(reader.read_bits(5).ok_or(READ_ERR)? as u8)
        } else {
            None
        };
        let dsi_frame_rate_multiply_info = reader.read_bits(2).ok_or(READ_ERR)? as u8;
        let presentation_emdf_version = reader.read_bits(5).ok_or(READ_ERR)? as u8;
        let presentation_key_id = reader.read_bits(10).ok_or(READ_ERR)? as u16;
        let mut presentation_channel_mask = [0u8; 3];
        reader
            .read_bytes(&mut presentation_channel_mask)
            .then_some(0)
            .ok_or(READ_ERR)?;
        // ETSI TS 103 190-1 v1.4.1 Table E.5a
        // > This virtual variable shall be considered to be `true` if presentation_config is set to
        // > 0x1F, otherwise it shall be considered as being `false`.
        let b_single_substream = presentation_config == 0x1F;
        let (b_hsf_ext, substream_groups) = if b_single_substream {
            (None, Some(vec![ac4_substream_group_dsi(reader)?]))
        } else {
            let b_hsf_ext = reader.read_bit().ok_or(READ_ERR)?;
            if [0u8, 1, 2].contains(&presentation_config) {
                (
                    Some(b_hsf_ext),
                    Some(vec![
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                    ]),
                )
            } else if [3u8, 4].contains(&presentation_config) {
                (
                    Some(b_hsf_ext),
                    Some(vec![
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                    ]),
                )
            } else if presentation_config == 5 {
                let n_substream_groups_minus2 = reader.read_bits(3).ok_or(READ_ERR)? as usize;
                let n_substream_groups = n_substream_groups_minus2 + 2;
                let mut substream_groups = Vec::with_capacity(n_substream_groups);
                for _ in 0..n_substream_groups {
                    substream_groups.push(ac4_substream_group_dsi(reader)?);
                }
                (Some(b_hsf_ext), Some(substream_groups))
            } else {
                let n_skip_bytes = reader.read_bits(7).ok_or(READ_ERR)?;
                for _ in 0..n_skip_bytes {
                    _ = reader.read_u8().ok_or(READ_ERR)?;
                }
                (Some(b_hsf_ext), None)
            }
        };
        let b_pre_virtualized = reader.read_bit().ok_or(READ_ERR)?;
        let b_add_emdf_substreams = reader.read_bit().ok_or(READ_ERR)?;
        (
            Some(md_compat),
            presentation_id,
            Some(dsi_frame_rate_multiply_info),
            Some(presentation_emdf_version),
            Some(presentation_key_id),
            Some(presentation_channel_mask),
            b_hsf_ext,
            substream_groups,
            Some(b_pre_virtualized),
            b_add_emdf_substreams,
        )
    };
    let emdf_substreams = if b_add_emdf_substreams {
        let n_add_emdf_substreams = reader.read_bits(7).ok_or(READ_ERR)? as usize;
        let mut emdf_substreams = Vec::with_capacity(n_add_emdf_substreams);
        for _ in 0..n_add_emdf_substreams {
            let emdf_version = reader.read_bits(5).ok_or(READ_ERR)? as u8;
            let key_id = reader.read_bits(10).ok_or(READ_ERR)? as u16;
            emdf_substreams.push(EmdfSubstream {
                emdf_version,
                key_id,
            });
        }
        emdf_substreams
    } else {
        Vec::new()
    };
    reader.align().map_err(|_| READ_ERR)?;

    Ok(Ac4PresentationV0 {
        presentation_config,
        md_compat,
        presentation_id,
        dsi_frame_rate_multiply_info,
        presentation_emdf_version,
        presentation_key_id,
        presentation_channel_mask,
        b_hsf_ext,
        substream_groups,
        b_pre_virtualized,
        emdf_substreams,
    })
}
fn ac4_presentation_v1_dsi(
    reader: &mut BigEndianReader,
    pres_bytes: usize,
) -> Result<Ac4PresentationV1> {
    let starting_bytes = reader.bytes_remaining();
    let presentation_config_v1 = reader.read_bits(5).ok_or(READ_ERR)? as u8;
    let (
        md_compat,
        presentation_id,
        dsi_frame_rate_multiply_info,
        dsi_frame_rate_fraction_info,
        presentation_emdf_version,
        presentation_key_id,
        b_presentation_channel_coded,
        dsi_presentation_ch_mode,
        pres_b_4_back_channels_present,
        pres_top_channel_pairs,
        presentation_channel_mask_v1,
        b_presentation_core_differs,
        b_presentation_core_channel_coded,
        dsi_presentation_channel_mode_core,
        b_presentation_filter,
        b_enable_presentation,
        filter_data,
        b_multi_pid,
        substream_groups,
        b_pre_virtualized,
        b_add_emdf_substreams,
    ) = if presentation_config_v1 == 0x06 {
        (
            None, None, None, None, None, None, None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, true,
        )
    } else {
        let md_compat = reader.read_bits(3).ok_or(READ_ERR)? as u8;
        let b_presentation_id = reader.read_bit().ok_or(READ_ERR)?;
        let presentation_id = if b_presentation_id {
            Some(reader.read_bits(5).ok_or(READ_ERR)? as u8)
        } else {
            None
        };
        let dsi_frame_rate_multiply_info = reader.read_bits(2).ok_or(READ_ERR)? as u8;
        let dsi_frame_rate_fraction_info = reader.read_bits(2).ok_or(READ_ERR)? as u8;
        let presentation_emdf_version = reader.read_bits(5).ok_or(READ_ERR)? as u8;
        let presentation_key_id = reader.read_bits(10).ok_or(READ_ERR)? as u16;
        let b_presentation_channel_coded = reader.read_bit().ok_or(READ_ERR)?;
        let (
            dsi_presentation_ch_mode,
            pres_b_4_back_channels_present,
            pres_top_channel_pairs,
            presentation_channel_mask_v1,
        ) = if b_presentation_channel_coded {
            let dsi_presentation_ch_mode = reader.read_bits(5).ok_or(READ_ERR)? as u8;
            let (pres_b_4_back_channels_present, pres_top_channel_pairs) =
                if [11u8, 12, 13, 14].contains(&dsi_presentation_ch_mode) {
                    let pres_b_4_back_channels_present = reader.read_bit().ok_or(READ_ERR)?;
                    let pres_top_channel_pairs = reader.read_bits(2).ok_or(READ_ERR)? as u8;
                    (
                        Some(pres_b_4_back_channels_present),
                        Some(pres_top_channel_pairs),
                    )
                } else {
                    (None, None)
                };
            let mut presentation_channel_mask_v1 = [0u8; 3];
            reader
                .read_bytes(&mut presentation_channel_mask_v1)
                .then_some(0)
                .ok_or(READ_ERR)?;
            (
                Some(dsi_presentation_ch_mode),
                pres_b_4_back_channels_present,
                pres_top_channel_pairs,
                Some(presentation_channel_mask_v1),
            )
        } else {
            (None, None, None, None)
        };
        let b_presentation_core_differs = reader.read_bit().ok_or(READ_ERR)?;
        let (b_presentation_core_channel_coded, dsi_presentation_channel_mode_core) =
            if b_presentation_core_differs {
                let b_presentation_core_channel_coded = reader.read_bit().ok_or(READ_ERR)?;
                if b_presentation_core_channel_coded {
                    let dsi_presentation_channel_mode_core =
                        reader.read_bits(2).ok_or(READ_ERR)? as u8;
                    (
                        Some(b_presentation_core_channel_coded),
                        Some(dsi_presentation_channel_mode_core),
                    )
                } else {
                    (Some(b_presentation_core_channel_coded), None)
                }
            } else {
                (None, None)
            };
        let b_presentation_filter = reader.read_bit().ok_or(READ_ERR)?;
        let (b_enable_presentation, filter_data) = if b_presentation_filter {
            let b_enable_presentation = reader.read_bit().ok_or(READ_ERR)?;
            let n_filter_bytes = reader.read_u8().ok_or(READ_ERR)? as usize;
            let mut filter_data = Vec::with_capacity(n_filter_bytes);
            for _ in 0..n_filter_bytes {
                filter_data.push(reader.read_u8().ok_or(READ_ERR)?);
            }
            (Some(b_enable_presentation), Some(filter_data))
        } else {
            (None, None)
        };
        let (b_multi_pid, substream_groups) = if presentation_config_v1 == 0x1F {
            (None, Some(vec![ac4_substream_group_dsi(reader)?]))
        } else {
            let b_multi_pid = reader.read_bit().ok_or(READ_ERR)?;
            if [0u8, 1, 2].contains(&presentation_config_v1) {
                (
                    Some(b_multi_pid),
                    Some(vec![
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                    ]),
                )
            } else if [3u8, 4].contains(&presentation_config_v1) {
                (
                    Some(b_multi_pid),
                    Some(vec![
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                    ]),
                )
            } else if presentation_config_v1 == 5 {
                let n_substream_groups_minus2 = reader.read_bits(3).ok_or(READ_ERR)? as usize;
                let n_substream_groups = n_substream_groups_minus2 + 2;
                let mut substream_groups = Vec::with_capacity(n_substream_groups);
                for _ in 0..n_substream_groups {
                    substream_groups.push(ac4_substream_group_dsi(reader)?);
                }
                (Some(b_multi_pid), Some(substream_groups))
            } else {
                let n_skip_bytes = reader.read_bits(7).ok_or(READ_ERR)?;
                for _ in 0..n_skip_bytes {
                    _ = reader.read_u8().ok_or(READ_ERR)?;
                }
                (Some(b_multi_pid), None)
            }
        };
        let b_pre_virtualized = reader.read_bit().ok_or(READ_ERR)?;
        let b_add_emdf_substreams = reader.read_bit().ok_or(READ_ERR)?;
        (
            Some(md_compat),
            presentation_id,
            Some(dsi_frame_rate_multiply_info),
            Some(dsi_frame_rate_fraction_info),
            Some(presentation_emdf_version),
            Some(presentation_key_id),
            Some(b_presentation_channel_coded),
            dsi_presentation_ch_mode,
            pres_b_4_back_channels_present,
            pres_top_channel_pairs,
            presentation_channel_mask_v1,
            Some(b_presentation_core_differs),
            b_presentation_core_channel_coded,
            dsi_presentation_channel_mode_core,
            Some(b_presentation_filter),
            b_enable_presentation,
            filter_data,
            b_multi_pid,
            substream_groups,
            Some(b_pre_virtualized),
            b_add_emdf_substreams,
        )
    };
    let emdf_substreams = if b_add_emdf_substreams {
        let n_add_emdf_substreams = reader.read_bits(7).ok_or(READ_ERR)? as usize;
        let mut emdf_substreams = Vec::with_capacity(n_add_emdf_substreams);
        for _ in 0..n_add_emdf_substreams {
            let emdf_version = reader.read_bits(5).ok_or(READ_ERR)? as u8;
            let key_id = reader.read_bits(10).ok_or(READ_ERR)? as u16;
            emdf_substreams.push(EmdfSubstream {
                emdf_version,
                key_id,
            });
        }
        emdf_substreams
    } else {
        Vec::new()
    };
    let b_presentation_bitrate_info = reader.read_bit().ok_or(READ_ERR)?;
    let (bit_rate_mode, bit_rate, bit_rate_precision) = if b_presentation_bitrate_info {
        let bit_rate_mode = Ac4BitrateMode::from(reader.read_bits(2).ok_or(READ_ERR)? as u8);
        let bit_rate = reader.read_u32().ok_or(READ_ERR)?;
        let bit_rate_precision = reader.read_u32().ok_or(READ_ERR)?;
        (
            Some(bit_rate_mode),
            Some(bit_rate),
            Some(bit_rate_precision),
        )
    } else {
        (None, None, None)
    };
    let b_alternative = reader.read_bit().ok_or(READ_ERR)?;
    let alternative_info = if b_alternative {
        reader.align().map_err(|_| READ_ERR)?;
        let name_len = reader.read_u16().ok_or(READ_ERR)? as usize;
        let mut presentation_name_vec = vec![0; name_len];
        reader
            .read_bytes(&mut presentation_name_vec)
            .then_some(0)
            .ok_or(READ_ERR)?;
        let presentation_name = String::from_utf8_lossy(&presentation_name_vec).to_string();
        let n_targets = reader.read_bits(5).ok_or(READ_ERR)? as usize;
        let mut targets = Vec::with_capacity(n_targets);
        for _ in 0..n_targets {
            let md_compat = reader.read_bits(3).ok_or(READ_ERR)? as u8;
            let device_category = reader.read_u8().ok_or(READ_ERR)?;
            targets.push(Ac4AlternativeInfoTarget {
                md_compat,
                device_category,
            });
        }
        Some(Ac4AlternativeInfo {
            presentation_name,
            targets,
        })
    } else {
        None
    };
    reader.align().map_err(|_| READ_ERR)?;
    let bits_read = (starting_bytes - reader.bytes_remaining()) * 8;
    let (de_indicator, immersive_audio_indicator, extended_presentation_id) =
        if bits_read <= (pres_bytes - 1) * 8 {
            let de_indicator = reader.read_bit().ok_or(READ_ERR)?;
            let immersive_audio_indicator = reader.read_bit().ok_or(READ_ERR)?;
            _ = reader.read_bits(4).ok_or(READ_ERR)?;
            let b_extended_presentation_id = reader.read_bit().ok_or(READ_ERR)?;
            if b_extended_presentation_id {
                let extended_presentation_id = reader.read_bits(9).ok_or(READ_ERR)? as u16;
                (
                    Some(de_indicator),
                    Some(immersive_audio_indicator),
                    Some(extended_presentation_id),
                )
            } else {
                _ = reader.read_bit().ok_or(READ_ERR)?;
                (Some(de_indicator), Some(immersive_audio_indicator), None)
            }
        } else {
            (None, None, None)
        };

    Ok(Ac4PresentationV1 {
        presentation_config_v1,
        md_compat,
        presentation_id,
        dsi_frame_rate_multiply_info,
        dsi_frame_rate_fraction_info,
        presentation_emdf_version,
        presentation_key_id,
        b_presentation_channel_coded,
        dsi_presentation_ch_mode,
        pres_b_4_back_channels_present,
        pres_top_channel_pairs,
        presentation_channel_mask_v1,
        b_presentation_core_differs,
        b_presentation_core_channel_coded,
        dsi_presentation_channel_mode_core,
        b_presentation_filter,
        b_enable_presentation,
        filter_data,
        b_multi_pid,
        substream_groups,
        b_pre_virtualized,
        emdf_substreams,
        bit_rate_mode,
        bit_rate,
        bit_rate_precision,
        alternative_info,
        de_indicator,
        immersive_audio_indicator,
        extended_presentation_id,
    })
}
fn ac4_substream_group_dsi(reader: &mut BigEndianReader) -> Result<Ac4SubstreamGroup> {
    let b_substreams_present = reader.read_bit().ok_or(READ_ERR)?;
    let b_hsf_ext = reader.read_bit().ok_or(READ_ERR)?;
    let b_channel_coded = reader.read_bit().ok_or(READ_ERR)?;
    let n_substreams = reader.read_u8().ok_or(READ_ERR)? as usize;
    let mut substreams = Vec::with_capacity(n_substreams);
    for _ in 0..n_substreams {
        let dsi_sf_multiplier = reader.read_bits(2).ok_or(READ_ERR)? as u8;
        let b_substream_bitrate_indicator = reader.read_bit().ok_or(READ_ERR)?;
        let bitrate_indicator = if b_substream_bitrate_indicator {
            Some(reader.read_bits(5).ok_or(READ_ERR)? as u8)
        } else {
            None
        };
        let (
            channel_mask,
            n_dmx_objects_minus1,
            n_umx_objects_minus1,
            contains_bed_objects,
            contains_dynamic_objects,
            contains_isf_objects,
        ) = if b_channel_coded {
            let mut channel_mask = [0u8; 3];
            reader
                .read_bytes(&mut channel_mask)
                .then_some(0)
                .ok_or(READ_ERR)?;
            (Some(channel_mask), None, None, None, None, None)
        } else {
            let b_ajoc = reader.read_bit().ok_or(READ_ERR)?;
            let (dmx, umx) = if b_ajoc {
                let b_static_dmx = reader.read_bit().ok_or(READ_ERR)?;
                let dmx = if !b_static_dmx {
                    Some(reader.read_bits(4).ok_or(READ_ERR)? as u8)
                } else {
                    None
                };
                let umx = reader.read_bits(6).ok_or(READ_ERR)? as u8;
                (dmx, Some(umx))
            } else {
                (None, None)
            };
            let contains_bed_objects = reader.read_bit().ok_or(READ_ERR)?;
            let contains_dynamic_objects = reader.read_bit().ok_or(READ_ERR)?;
            let contains_isf_objects = reader.read_bit().ok_or(READ_ERR)?;
            _ = reader.read_bit().ok_or(READ_ERR)?;
            (
                None,
                dmx,
                umx,
                Some(contains_bed_objects),
                Some(contains_dynamic_objects),
                Some(contains_isf_objects),
            )
        };
        substreams.push(Ac4PresentationSubstream {
            dsi_sf_multiplier,
            bitrate_indicator,
            channel_mask,
            n_dmx_objects_minus1,
            n_umx_objects_minus1,
            contains_bed_objects,
            contains_dynamic_objects,
            contains_isf_objects,
        });
    }
    let b_content_type = reader.read_bit().ok_or(READ_ERR)?;
    let (content_classifier, language_tag) = if b_content_type {
        let content_classifier =
            Ac4ContentClassifier::from(reader.read_bits(3).ok_or(READ_ERR)? as u8);
        let b_language_indicator = reader.read_bit().ok_or(READ_ERR)?;
        if b_language_indicator {
            let n_language_tag_bytes = reader.read_bits(6).ok_or(READ_ERR)? as usize;
            let mut language_tag_bytes = vec![0; n_language_tag_bytes];
            reader
                .read_bytes(&mut language_tag_bytes)
                .then_some(0)
                .ok_or(READ_ERR)?;
            (
                Some(content_classifier),
                Some(String::from_utf8_lossy(&language_tag_bytes).to_string()),
            )
        } else {
            (Some(content_classifier), None)
        }
    } else {
        (None, None)
    };

    Ok(Ac4SubstreamGroup {
        b_substreams_present,
        b_hsf_ext,
        b_channel_coded,
        substreams,
        content_classifier,
        language_tag,
    })
}

#[cfg(test)]
mod tests {
    // Test dac4 atoms found here:
    // https://ott.dolby.com/OnDelKits/AC-4/Dolby_AC-4_Online_Delivery_Kit_1.5/help_files/topics/kit_wrapper_MP4_multiplexed_streams.html
    use super::*;
    use mp4_atom::Decode;
    use pretty_assertions::assert_eq;
    use std::io::Cursor;

    #[test]
    fn dac4_test() {
        const DAC4: &[u8] = &[
            0x00, 0x00, 0x00, 0x25, 0x64, 0x61, 0x63, 0x34, 0x20, 0xA6, 0x01, 0x40, 0x00, 0x00,
            0x00, 0x1F, 0xFF, 0xFF, 0xFF, 0xE0, 0x01, 0x0F, 0xF9, 0x80, 0x00, 0x00, 0x48, 0x00,
            0x00, 0x8E, 0x50, 0x10, 0x00, 0x00, 0x8F, 0x00, 0x80,
        ];
        let mut buf = Cursor::new(DAC4);
        // Validated with https://ott.dolby.com/OnDel_tools/mp4_inspector/index.html
        assert_eq!(
            Dac4 {
                ac4_dsi_version: 1,
                bitstream_version: 2,
                fs_index: true,
                frame_rate_index: 3,
                short_program_id: None,
                program_uuid: None,
                bit_rate_mode: Ac4BitrateMode::Average,
                bit_rate: 0,
                bit_rate_precision: 4294967295,
                presentations: vec![Ac4Presentation::V1(Ac4PresentationV1 {
                    presentation_config_v1: 31,
                    md_compat: Some(1),
                    presentation_id: Some(0),
                    dsi_frame_rate_multiply_info: Some(0),
                    dsi_frame_rate_fraction_info: Some(0),
                    presentation_emdf_version: Some(0),
                    presentation_key_id: Some(0),
                    b_presentation_channel_coded: Some(true),
                    dsi_presentation_ch_mode: Some(4),
                    pres_b_4_back_channels_present: None,
                    pres_top_channel_pairs: None,
                    presentation_channel_mask_v1: Some([0, 0, 0b01000111]),
                    b_presentation_core_differs: Some(false),
                    b_presentation_core_channel_coded: None,
                    dsi_presentation_channel_mode_core: None,
                    b_presentation_filter: Some(false),
                    b_enable_presentation: None,
                    filter_data: None,
                    b_multi_pid: None,
                    substream_groups: Some(vec![Ac4SubstreamGroup {
                        b_substreams_present: true,
                        b_hsf_ext: false,
                        b_channel_coded: true,
                        substreams: vec![Ac4PresentationSubstream {
                            dsi_sf_multiplier: 0,
                            bitrate_indicator: None,
                            channel_mask: Some([0, 0, 0b01000111]),
                            n_dmx_objects_minus1: None,
                            n_umx_objects_minus1: None,
                            contains_bed_objects: None,
                            contains_dynamic_objects: None,
                            contains_isf_objects: None,
                        }],
                        content_classifier: Some(Ac4ContentClassifier::CompleteMain),
                        language_tag: None,
                    }]),
                    b_pre_virtualized: Some(false),
                    emdf_substreams: Vec::new(),
                    bit_rate_mode: None,
                    bit_rate: None,
                    bit_rate_precision: None,
                    alternative_info: None,
                    de_indicator: Some(true),
                    immersive_audio_indicator: Some(false),
                    extended_presentation_id: None,
                })]
            },
            Dac4::decode(&mut buf).expect("dac4 should decode successfully"),
        )
    }

    #[test]
    fn dac4_multi_presentation_including_v2() {
        const DAC4: &[u8] = &[
            0x00, 0x00, 0x00, 0x36, 0x64, 0x61, 0x63, 0x34, 0x20, 0xA6, 0x02, 0x40, 0x00, 0x00,
            0x00, 0x1F, 0xFF, 0xFF, 0xFF, 0xE0, 0x02, 0x0F, 0xF8, 0x80, 0x00, 0x00, 0x42, 0x00,
            0x00, 0x02, 0x50, 0x10, 0x00, 0x00, 0x03, 0x08, 0xC0, 0x01, 0x0F, 0xF8, 0x80, 0x00,
            0x00, 0x42, 0x00, 0x00, 0x02, 0x50, 0x10, 0x00, 0x00, 0x03, 0x00, 0x80,
        ];
        let mut buf = Cursor::new(DAC4);
        // Validated with https://ott.dolby.com/OnDel_tools/mp4_inspector/index.html
        assert_eq!(
            Dac4 {
                ac4_dsi_version: 1,
                bitstream_version: 2,
                fs_index: true,
                frame_rate_index: 3,
                short_program_id: None,
                program_uuid: None,
                bit_rate_mode: Ac4BitrateMode::Average,
                bit_rate: 0,
                bit_rate_precision: 4294967295,
                presentations: vec![
                    Ac4Presentation::V2(Ac4PresentationV1 {
                        presentation_config_v1: 31,
                        md_compat: Some(0),
                        presentation_id: Some(0),
                        dsi_frame_rate_multiply_info: Some(0),
                        dsi_frame_rate_fraction_info: Some(0),
                        presentation_emdf_version: Some(0),
                        presentation_key_id: Some(0),
                        b_presentation_channel_coded: Some(true),
                        dsi_presentation_ch_mode: Some(1),
                        pres_b_4_back_channels_present: None,
                        pres_top_channel_pairs: None,
                        presentation_channel_mask_v1: Some([0, 0, 1]),
                        b_presentation_core_differs: Some(false),
                        b_presentation_core_channel_coded: None,
                        dsi_presentation_channel_mode_core: None,
                        b_presentation_filter: Some(false),
                        b_enable_presentation: None,
                        filter_data: None,
                        b_multi_pid: None,
                        substream_groups: Some(vec![Ac4SubstreamGroup {
                            b_substreams_present: true,
                            b_hsf_ext: false,
                            b_channel_coded: true,
                            substreams: vec![Ac4PresentationSubstream {
                                dsi_sf_multiplier: 0,
                                bitrate_indicator: None,
                                channel_mask: Some([0, 0, 1]),
                                n_dmx_objects_minus1: None,
                                n_umx_objects_minus1: None,
                                contains_bed_objects: None,
                                contains_dynamic_objects: None,
                                contains_isf_objects: None,
                            }],
                            content_classifier: Some(Ac4ContentClassifier::CompleteMain),
                            language_tag: None,
                        }]),
                        b_pre_virtualized: Some(true),
                        emdf_substreams: Vec::new(),
                        bit_rate_mode: None,
                        bit_rate: None,
                        bit_rate_precision: None,
                        alternative_info: None,
                        de_indicator: Some(true),
                        immersive_audio_indicator: Some(true),
                        extended_presentation_id: None,
                    }),
                    Ac4Presentation::V1(Ac4PresentationV1 {
                        presentation_config_v1: 31,
                        md_compat: Some(0),
                        presentation_id: Some(0),
                        dsi_frame_rate_multiply_info: Some(0),
                        dsi_frame_rate_fraction_info: Some(0),
                        presentation_emdf_version: Some(0),
                        presentation_key_id: Some(0),
                        b_presentation_channel_coded: Some(true),
                        dsi_presentation_ch_mode: Some(1),
                        pres_b_4_back_channels_present: None,
                        pres_top_channel_pairs: None,
                        presentation_channel_mask_v1: Some([0, 0, 1]),
                        b_presentation_core_differs: Some(false),
                        b_presentation_core_channel_coded: None,
                        dsi_presentation_channel_mode_core: None,
                        b_presentation_filter: Some(false),
                        b_enable_presentation: None,
                        filter_data: None,
                        b_multi_pid: None,
                        substream_groups: Some(vec![Ac4SubstreamGroup {
                            b_substreams_present: true,
                            b_hsf_ext: false,
                            b_channel_coded: true,
                            substreams: vec![Ac4PresentationSubstream {
                                dsi_sf_multiplier: 0,
                                bitrate_indicator: None,
                                channel_mask: Some([0, 0, 1]),
                                n_dmx_objects_minus1: None,
                                n_umx_objects_minus1: None,
                                contains_bed_objects: None,
                                contains_dynamic_objects: None,
                                contains_isf_objects: None,
                            }],
                            content_classifier: Some(Ac4ContentClassifier::CompleteMain),
                            language_tag: None,
                        }]),
                        b_pre_virtualized: Some(false),
                        emdf_substreams: Vec::new(),
                        bit_rate_mode: None,
                        bit_rate: None,
                        bit_rate_precision: None,
                        alternative_info: None,
                        de_indicator: Some(true),
                        immersive_audio_indicator: Some(false),
                        extended_presentation_id: None,
                    })
                ]
            },
            Dac4::decode(&mut buf).expect("dac4 should decode successfully"),
        )
    }
}
