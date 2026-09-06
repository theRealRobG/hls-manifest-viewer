use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// MetadataKeyDeclarationBox, ISO/IEC 14496-12:2024 Sect 12.9.4.4
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyd {
    pub key_namespace: FourCC,
    pub key_value: Vec<u8>,
}

impl Atom for Keyd {
    const KIND: FourCC = FourCC::new(b"keyd");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let key_namespace = FourCC::decode(buf)?;
        let key_value = Vec::decode(buf)?;
        Ok(Self {
            key_namespace,
            key_value,
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

    // Example taken from:
    // https://devstreaming-cdn.apple.com/videos/streaming/examples/immersive-media/apple-immersive-video/primary.m3u8
    const KEYD: &[u8] = &[
        0x00, 0x00, 0x00, 0x42, 0x6B, 0x65, 0x79, 0x64, 0x6D, 0x64, 0x74, 0x61, 0x63, 0x6F, 0x6D,
        0x2E, 0x61, 0x70, 0x70, 0x6C, 0x65, 0x2E, 0x71, 0x75, 0x69, 0x63, 0x6B, 0x74, 0x69, 0x6D,
        0x65, 0x2E, 0x76, 0x69, 0x64, 0x65, 0x6F, 0x2E, 0x70, 0x72, 0x65, 0x73, 0x65, 0x6E, 0x74,
        0x61, 0x74, 0x69, 0x6F, 0x6E, 0x2E, 0x69, 0x6D, 0x6D, 0x65, 0x72, 0x73, 0x69, 0x76, 0x65,
        0x2D, 0x6D, 0x65, 0x64, 0x69, 0x61,
    ];

    #[test]
    fn parses_correctly() {
        let mut buf = Cursor::new(KEYD);
        assert_eq!(
            Keyd {
                key_namespace: FourCC::from(b"mdta"),
                key_value: b"com.apple.quicktime.video.presentation.immersive-media".to_vec(),
            },
            Keyd::decode(&mut buf).expect("keyd should decode successfully")
        );
    }
}
