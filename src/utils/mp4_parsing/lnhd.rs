use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// CameraSystemLensHeaderBox, ISO/IEC 23001-18.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lnhd {
    pub lens_identifier: u32,
    pub lens_algorithm_kind: FourCC,
    pub lens_domain: FourCC,
    pub lens_role: FourCC,
}

impl Atom for Lnhd {
    const KIND: FourCC = FourCC::new(b"lnhd");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let lens_identifier = u32::decode(buf)?;
        let lens_algorithm_kind = FourCC::decode(buf)?;
        let lens_domain = FourCC::decode(buf)?;
        let lens_role = FourCC::decode(buf)?;
        Ok(Self {
            lens_identifier,
            lens_algorithm_kind,
            lens_domain,
            lens_role,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

