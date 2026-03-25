use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// HalfEquirectangularProjectionBox, ISO/IEC 23001-18.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hequ;

impl Atom for Hequ {
    const KIND: FourCC = FourCC::new(b"hequ");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        Ok(Self)
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

