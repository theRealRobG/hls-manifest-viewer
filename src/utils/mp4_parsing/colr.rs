use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// ColourInformationBox, ISO/IEC 14496-12:2024 Sect 12.1.5
///
/// With additional `nclc` case from QuickTime File Format:
/// https://developer.apple.com/documentation/quicktime-file-format/color_parameter_atom/color_parameter_type
///
/// This implementation copy+pastes and slightly changes the implementation in mp4-atom, providing
/// the following changes:
/// * Addition of `nclc` option defined in QuickTime File Format
/// * Loosens error case for unknown type by creating unknown case and storing vector of bytes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Colr {
    Nclx {
        colour_primaries: u16,
        transfer_characteristics: u16,
        matrix_coefficients: u16,
        full_range_flag: bool,
    },
    Ricc {
        profile: Vec<u8>,
    },
    Prof {
        profile: Vec<u8>,
    },
    Nclc {
        primaries_index: u16,
        transfer_function_index: u16,
        matrix_index: u16,
    },
    Unknown {
        colour_type: FourCC,
        bytes: Vec<u8>,
    },
}
impl Atom for Colr {
    const KIND: FourCC = FourCC::new(b"colr");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let colour_type = FourCC::decode(buf)?;
        if colour_type == FourCC::new(b"nclx") {
            let colour_primaries = u16::decode(buf)?;
            let transfer_characteristics = u16::decode(buf)?;
            let matrix_coefficients = u16::decode(buf)?;
            let full_range_flag = u8::decode(buf)? == 0x80;
            Ok(Colr::Nclx {
                colour_primaries,
                transfer_characteristics,
                matrix_coefficients,
                full_range_flag,
            })
        } else if colour_type == FourCC::new(b"prof") {
            let profile_len = buf.remaining();
            let profile = buf.slice(profile_len).to_vec();
            Ok(Colr::Prof { profile })
        } else if colour_type == FourCC::new(b"rICC") {
            let profile_len = buf.remaining();
            let profile = buf.slice(profile_len).to_vec();
            Ok(Colr::Ricc { profile })
        } else if colour_type == FourCC::new(b"nclc") {
            let primaries_index = u16::decode(buf)?;
            let transfer_function_index = u16::decode(buf)?;
            let matrix_index = u16::decode(buf)?;
            Ok(Colr::Nclc {
                primaries_index,
                transfer_function_index,
                matrix_index,
            })
        } else {
            let remaining_len = buf.remaining();
            let bytes = buf.slice(remaining_len).to_vec();
            Ok(Colr::Unknown { colour_type, bytes })
        }
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!();
    }
}
