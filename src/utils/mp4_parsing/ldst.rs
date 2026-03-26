use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// CameraSystemLensDistortionsBox, ISO/IEC 23001-18.
#[derive(Debug, Clone, PartialEq)]
pub struct Ldst {
    pub k1: f32,
    pub k2: f32,
    pub p1: f32,
    pub p2: f32,
    /// Present when flags & 1, BEFloat32
    pub calibration_limit_radial_angle: Option<f32>,
}

impl Atom for Ldst {
    const KIND: FourCC = FourCC::new(b"ldst");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let ext = u32::decode(buf)?;
        let flags = ext & 0x00FF_FFFF;
        let k1 = f32::from_bits(u32::decode(buf)?);
        let k2 = f32::from_bits(u32::decode(buf)?);
        let p1 = f32::from_bits(u32::decode(buf)?);
        let p2 = f32::from_bits(u32::decode(buf)?);
        let calibration_limit_radial_angle = if flags & 1 == 1 {
            Some(f32::from_bits(u32::decode(buf)?))
        } else {
            None
        };
        Ok(Self {
            k1,
            k2,
            p1,
            p2,
            calibration_limit_radial_angle,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

