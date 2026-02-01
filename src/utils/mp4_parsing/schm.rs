use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// SchemeTypeBox, ISO/IEC 14496-12:2024 Sect 13.4.6
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schm {
    pub scheme_type: FourCC,
    pub scheme_version: u32,
    pub scheme_uri: Option<String>,
}
impl Atom for Schm {
    const KIND: FourCC = FourCC::new(b"schm");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let ext = u32::decode(buf)?;
        let has_browser_uri = ext & 1 == 1;
        let scheme_type = FourCC::decode(buf)?;
        let scheme_version = u32::decode(buf)?;
        let scheme_uri = if has_browser_uri {
            Some(String::decode(buf)?)
        } else {
            None
        };
        Ok(Self {
            scheme_type,
            scheme_version,
            scheme_uri,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
