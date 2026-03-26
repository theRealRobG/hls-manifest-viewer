use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// CameraSystemLensIntrinsicsBox, QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta).
#[derive(Debug, Clone, PartialEq)]
pub struct Lnin {
    pub denominator_shift_operand: i16,
    pub skew_denominator_shift_operand: i16,
    pub focal_length_x: i32,
    pub principal_point_x: i32,
    pub principal_point_y: i32,
    /// Present when flags & 1
    pub focal_length_y: Option<i32>,
    /// Present when flags & 1
    pub skew_factor: Option<i32>,
    /// Present when flags & 2, BEFloat32
    pub projection_offset: Option<f32>,
}

impl Atom for Lnin {
    const KIND: FourCC = FourCC::new(b"lnin");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let ext = u32::decode(buf)?;
        let flags = ext & 0x00FF_FFFF;
        let denominator_shift_operand = i16::decode(buf)?;
        let skew_denominator_shift_operand = i16::decode(buf)?;
        let focal_length_x = i32::decode(buf)?;
        let principal_point_x = i32::decode(buf)?;
        let principal_point_y = i32::decode(buf)?;
        let (focal_length_y, skew_factor) = if flags & 1 == 1 {
            (
                Some(i32::decode(buf)?),
                Some(i32::decode(buf)?),
            )
        } else {
            (None, None)
        };
        let projection_offset = if flags & 2 == 2 {
            Some(f32::from_bits(u32::decode(buf)?))
        } else {
            None
        };
        Ok(Self {
            denominator_shift_operand,
            skew_denominator_shift_operand,
            focal_length_x,
            principal_point_x,
            principal_point_y,
            focal_length_y,
            skew_factor,
            projection_offset,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

