use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// CameraSystemLensFrameAdjustmentBox, QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta).
#[derive(Debug, Clone, PartialEq)]
pub struct Lfad {
    pub polynomial_parameters_x: [f32; 3],
    pub polynomial_parameters_y: [f32; 3],
}

impl Atom for Lfad {
    const KIND: FourCC = FourCC::new(b"lfad");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let polynomial_parameters_x = [
            f32::from_bits(u32::decode(buf)?),
            f32::from_bits(u32::decode(buf)?),
            f32::from_bits(u32::decode(buf)?),
        ];
        let polynomial_parameters_y = [
            f32::from_bits(u32::decode(buf)?),
            f32::from_bits(u32::decode(buf)?),
            f32::from_bits(u32::decode(buf)?),
        ];
        Ok(Self {
            polynomial_parameters_x,
            polynomial_parameters_y,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

