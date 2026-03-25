use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// CameraSystemOriginSourceBox, ISO/IEC 23001-18.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corg {
    pub source_of_origin: FourCC,
}

impl Atom for Corg {
    const KIND: FourCC = FourCC::new(b"corg");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let source_of_origin = FourCC::decode(buf)?;
        Ok(Self { source_of_origin })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

