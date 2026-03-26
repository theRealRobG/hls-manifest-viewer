use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// ViewPackingInformationBox, QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkin {
    pub view_packing_kind: FourCC,
}

impl Atom for Pkin {
    const KIND: FourCC = FourCC::new(b"pkin");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let view_packing_kind = FourCC::decode(buf)?;
        Ok(Self { view_packing_kind })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

