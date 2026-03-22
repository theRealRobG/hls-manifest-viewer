use crate::utils::mp4_parsing::dvcc::Dvcc;
use mp4_atom::{u24, Atom, Buf, BufMut, Decode, FourCC, Result};

/// DOVIDecoderConfigurationRecord, Dolby Vision Streams Within the ISO Base Media File Format,
/// Section 2.2
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct Dvvc {
    pub dv_version_major: u8,
    pub dv_version_minor: u8,
    pub dv_profile: u8,
    pub dv_level: u8,
    pub rpu_present: bool,
    pub el_present: bool,
    pub bl_present: bool,
    pub dv_bl_signal_compatibility_id: u8,
    pub dv_md_compression: u8,
}

impl Atom for Dvvc {
    const KIND: FourCC = FourCC::new(b"dvvC");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let dv_version_major = u8::decode(buf)?;
        let dv_version_minor = u8::decode(buf)?;

        // Next 16 bits:
        // * dv_profile(7)
        // * dv_level(6)
        // * rpu_present(1)
        // * el_present(1)
        // * bl_present(1)
        let word = u16::decode(buf)?;
        let dv_profile = ((word >> 9) & 0x7f) as u8;
        let dv_level = ((word >> 3) & 0x3f) as u8;
        let rpu_present = ((word >> 2) & 0x01) == 1;
        let el_present = ((word >> 1) & 0x01) == 1;
        let bl_present = (word & 0x01) == 1;

        // 5th byte:
        // * dv_bl_signal_compatibility_id(4)
        // * dv_md_compression(2)
        // * reserved(2) — first 2 bits of the 26 reserved bits that follow
        let byte = u8::decode(buf)?;
        let dv_bl_signal_compatibility_id = (byte >> 4) & 0x0f;
        let dv_md_compression = (byte >> 2) & 0x03;

        // 26 reserved bits: 2 consumed above, skip the remaining 24 bits (3 bytes)
        let _ = u24::decode(buf)?;

        // 4 reserved 32-bit words
        let _ = u64::decode(buf)?;
        let _ = u64::decode(buf)?;

        Ok(Self {
            dv_version_major,
            dv_version_minor,
            dv_profile,
            dv_level,
            rpu_present,
            el_present,
            bl_present,
            dv_bl_signal_compatibility_id,
            dv_md_compression,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

impl From<Dvcc> for Dvvc {
    fn from(dvcc: Dvcc) -> Self {
        Self {
            dv_version_major: dvcc.dv_version_major,
            dv_version_minor: dvcc.dv_version_minor,
            dv_profile: dvcc.dv_profile,
            dv_level: dvcc.dv_level,
            rpu_present: dvcc.rpu_present,
            el_present: dvcc.el_present,
            bl_present: dvcc.bl_present,
            dv_bl_signal_compatibility_id: dvcc.dv_bl_signal_compatibility_id,
            dv_md_compression: dvcc.dv_md_compression,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::io::Cursor;

    const DVVC: &[u8] = &[
        0x00, 0x00, 0x00, 0x20, 0x64, 0x76, 0x76, 0x43, 0x01, 0x00, 0x10, 0x0D, 0x10, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];

    #[test]
    fn decode_dvvc() {
        let mut buf = Cursor::new(DVVC);
        assert_eq!(
            Dvvc {
                dv_version_major: 1,
                dv_version_minor: 0,
                dv_profile: 8,
                dv_level: 1,
                rpu_present: true,
                el_present: false,
                bl_present: true,
                dv_bl_signal_compatibility_id: 1,
                dv_md_compression: 0,
            },
            Dvvc::decode(&mut buf).unwrap()
        );
    }
}
