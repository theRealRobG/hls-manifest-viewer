use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// StereoComfortDisparityAdjustmentBox, QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dadj {
    pub disparity_adjustment: i32,
}

impl Atom for Dadj {
    const KIND: FourCC = FourCC::new(b"dadj");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let disparity_adjustment = u32::decode(buf)? as i32;
        Ok(Self {
            disparity_adjustment,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

