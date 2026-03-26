use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// CameraSystemUnitQuaternionTransformBox, QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta).
#[derive(Debug, Clone, PartialEq)]
pub struct Uqua {
    pub xyz: [f32; 3],
}

impl Atom for Uqua {
    const KIND: FourCC = FourCC::new(b"uqua");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let xyz = [
            f32::from_bits(u32::decode(buf)?),
            f32::from_bits(u32::decode(buf)?),
            f32::from_bits(u32::decode(buf)?),
        ];
        Ok(Self { xyz })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

