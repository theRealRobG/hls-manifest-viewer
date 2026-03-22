use crate::utils::mp4_parsing::Dvvc;
use mp4_atom::{Atom, Buf, BufMut, FourCC, Result};

/// DOVIDecoderConfigurationRecord, Dolby Vision Streams Within the ISO Base Media File Format,
/// Section 2.2
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct Dvcc {
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

impl Atom for Dvcc {
    const KIND: FourCC = FourCC::new(b"dvcC");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let dvvc = Dvvc::decode_body(buf)?;
        Ok(Self::from(dvvc))
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

impl From<Dvvc> for Dvcc {
    fn from(dvvc: Dvvc) -> Self {
        Self {
            dv_version_major: dvvc.dv_version_major,
            dv_version_minor: dvvc.dv_version_minor,
            dv_profile: dvvc.dv_profile,
            dv_level: dvvc.dv_level,
            rpu_present: dvvc.rpu_present,
            el_present: dvvc.el_present,
            bl_present: dvvc.bl_present,
            dv_bl_signal_compatibility_id: dvvc.dv_bl_signal_compatibility_id,
            dv_md_compression: dvvc.dv_md_compression,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mp4_atom::Decode;
    use pretty_assertions::assert_eq;
    use std::io::Cursor;

    const DVCC: &[u8] = &[
        0x00, 0x00, 0x00, 0x20, 0x64, 0x76, 0x63, 0x43, 0x01, 0x00, 0x0A, 0x0D, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];

    #[test]
    fn decode_dvvc() {
        let mut buf = Cursor::new(DVCC);
        assert_eq!(
            Dvcc {
                dv_version_major: 1,
                dv_version_minor: 0,
                dv_profile: 5,
                dv_level: 1,
                rpu_present: true,
                el_present: false,
                bl_present: true,
                dv_bl_signal_compatibility_id: 0,
                dv_md_compression: 0,
            },
            Dvcc::decode(&mut buf).unwrap()
        );
    }
}
