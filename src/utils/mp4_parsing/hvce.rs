use mp4_atom::{Atom, Buf, BufMut, FourCC, Hvcc, Result};

/// DolbyVisionELHEVCConfigurationBox, Dolby Vision Streams Within the ISO Base Media File Format,
/// Section 2.3.
///
/// Contains an HEVCDecoderConfigurationRecord, identical in structure to hvcC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hvce(pub Hvcc);

impl Atom for Hvce {
    const KIND: FourCC = FourCC::new(b"hvcE");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let hvcc = Hvcc::decode_body(buf)?;
        Ok(Self(hvcc))
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
