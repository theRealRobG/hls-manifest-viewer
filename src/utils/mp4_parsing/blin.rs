use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// StereoCameraSystemBaselineBox, QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blin {
    pub baseline_value: u32,
}

impl Atom for Blin {
    const KIND: FourCC = FourCC::new(b"blin");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let baseline_value = u32::decode(buf)?;
        Ok(Self { baseline_value })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

