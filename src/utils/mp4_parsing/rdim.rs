use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// CameraSystemLensReferenceDimensionsBox, ISO/IEC 23001-18.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rdim {
    pub reference_width: u32,
    pub reference_height: u32,
}

impl Atom for Rdim {
    const KIND: FourCC = FourCC::new(b"rdim");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let reference_width = u32::decode(buf)?;
        let reference_height = u32::decode(buf)?;
        Ok(Self {
            reference_width,
            reference_height,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

