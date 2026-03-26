use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// FisheyeProjectionBox, QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fish;

impl Atom for Fish {
    const KIND: FourCC = FourCC::new(b"fish");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        Ok(Self)
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

