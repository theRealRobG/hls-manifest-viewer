use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// RequiredBoxTypesBox, ISO/IEC 23001-18.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Must {
    pub required_box_types: Vec<FourCC>,
}

impl Atom for Must {
    const KIND: FourCC = FourCC::new(b"must");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let mut required_box_types = Vec::new();
        while buf.remaining() >= 4 {
            required_box_types.push(FourCC::decode(buf)?);
        }
        Ok(Self { required_box_types })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

