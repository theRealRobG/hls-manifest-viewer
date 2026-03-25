use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// StereoComfortDisparityAdjustmentBox, ISO/IEC 23001-18.
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

