use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// HeroStereoEyeDescriptionBox, ISO/IEC 23001-18.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hero {
    pub hero_eye_indicator: u8,
}

impl Atom for Hero {
    const KIND: FourCC = FourCC::new(b"hero");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let hero_eye_indicator = u8::decode(buf)?;
        Ok(Self { hero_eye_indicator })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

