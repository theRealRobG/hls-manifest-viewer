use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// HorizontalFieldOfViewBox, QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hfov {
    pub field_of_view: u32,
}

impl Atom for Hfov {
    const KIND: FourCC = FourCC::new(b"hfov");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let field_of_view = u32::decode(buf)?;
        Ok(Self { field_of_view })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

