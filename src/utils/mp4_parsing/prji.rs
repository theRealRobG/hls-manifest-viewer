use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// ProjectionInformationBox, QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prji {
    pub projection_kind: FourCC,
}

impl Atom for Prji {
    const KIND: FourCC = FourCC::new(b"prji");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let projection_kind = FourCC::decode(buf)?;
        Ok(Self { projection_kind })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

