use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// ParametricImmersiveMediaProjectionBox, ISO/IEC 23001-18.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prim;

impl Atom for Prim {
    const KIND: FourCC = FourCC::new(b"prim");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        Ok(Self)
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

