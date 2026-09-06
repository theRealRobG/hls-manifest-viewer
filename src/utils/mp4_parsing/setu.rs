use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// MetadataSetupBox, ISO/IEC 14496-12:2024 Sect 12.9.4.6
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setu {
    pub namespace_defined_data: Vec<u8>,
}

impl Atom for Setu {
    const KIND: FourCC = FourCC::new(b"setu");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let namespace_defined_data = Vec::decode(buf)?;
        Ok(Self {
            namespace_defined_data,
        })
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

    const SETU: &[u8] = &[
        0x00, 0x00, 0x00, 0x15, 0x73, 0x65, 0x74, 0x75, 0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x2C, 0x20,
        0x57, 0x6F, 0x72, 0x6C, 0x64, 0x21,
    ];

    #[test]
    fn parses_correctly() {
        let mut buf = Cursor::new(SETU);
        assert_eq!(
            Setu {
                namespace_defined_data: b"Hello, World!".to_vec(),
            },
            Setu::decode(&mut buf).expect("keyd should decode successfully")
        );
    }
}
