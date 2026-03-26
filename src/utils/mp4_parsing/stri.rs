use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// StereoViewInformationBox, QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stri {
    pub eye_views_reversed: bool,
    pub has_additional_views: bool,
    pub has_right_eye_view: bool,
    pub has_left_eye_view: bool,
}

impl Atom for Stri {
    const KIND: FourCC = FourCC::new(b"stri");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags
        let byte = u8::decode(buf)?;
        let eye_views_reversed = (byte & 0b0000_1000) != 0;
        let has_additional_views = (byte & 0b0000_0100) != 0;
        let has_right_eye_view = (byte & 0b0000_0010) != 0;
        let has_left_eye_view = (byte & 0b0000_0001) != 0;
        Ok(Self {
            eye_views_reversed,
            has_additional_views,
            has_right_eye_view,
            has_left_eye_view,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

