use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// OriginalFormatBox, ISO/IEC 14496-12:2024 Sect 13.4.3
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frma {
    pub data_format: FourCC,
}
impl Atom for Frma {
    const KIND: FourCC = FourCC::new(b"frma");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let data_format = FourCC::decode(buf)?;
        Ok(Self { data_format })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
