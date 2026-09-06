use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// MetadataLocaleBox, ISO/IEC 14496-12:2024 Sect 12.9.4.5
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loca {
    pub locale: String,
}

impl Atom for Loca {
    const KIND: FourCC = FourCC::new(b"loca");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let locale = String::decode(buf)?;
        Ok(Self { locale })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::io::Cursor;

    const LOCA: &[u8] = &[
        0x00, 0x00, 0x00, 0x0D, 0x6C, 0x6F, 0x63, 0x61, 0x65, 0x6E, 0x2D, 0x55, 0x53,
    ];

    #[test]
    fn parses_correctly() {
        let mut buf = Cursor::new(LOCA);
        assert_eq!(
            Loca {
                locale: String::from("en-US")
            },
            Loca::decode(&mut buf).expect("keyd should decode successfully")
        );
    }
}
