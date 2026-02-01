use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// TrackEncryptionBox, ISO/IEC 23001-7:2016 Sect 8.2.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tenc {
    pub default_is_protected: u8,
    pub default_per_sample_iv_size: u8,
    pub default_key_id: [u8; 16],
    pub default_constant_iv: Option<Vec<u8>>,
    pub default_crypt_byte_block: Option<u8>,
    pub default_skip_byte_block: Option<u8>,
}
// Field semantics, ISO/IEC 23001-7:2016 Sect 9.1
impl Tenc {
    pub fn is_protected(&self) -> &'static str {
        match self.default_is_protected {
            0 => "Not protected",
            1 => "Protected",
            _ => "Reserved",
        }
    }

    pub fn per_sample_iv_size(&self) -> &'static str {
        match self.default_per_sample_iv_size {
            0 if self.default_is_protected == 0 => "Not protected",
            0 => "Constant IV",
            8 => "64-bit",
            16 => "128-bit",
            _ => "Undocumented in ISO/IEC 23001-7:2016",
        }
    }

    pub fn constant_iv_size(&self) -> &'static str {
        let Some(ref constant_iv) = self.default_constant_iv else {
            return "None";
        };
        match constant_iv.len() {
            8 => "64-bit",
            16 => "128-bit",
            _ => "Undocumented size in ISO/IEC 23001-7:2016",
        }
    }
}
impl Atom for Tenc {
    const KIND: FourCC = FourCC::new(b"tenc");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let ext = u32::decode(buf)?;
        let version = ext >> 24;
        u8::decode(buf)?;
        let (default_crypt_byte_block, default_skip_byte_block) = if version == 0 {
            u8::decode(buf)?;
            (None, None)
        } else {
            let byte_block_info = u8::decode(buf)?;
            let crypt = byte_block_info >> 4;
            let skip = byte_block_info & 0b1111;
            (Some(crypt), Some(skip))
        };
        let default_is_protected = u8::decode(buf)?;
        let default_per_sample_iv_size = u8::decode(buf)?;
        let default_key_id = <[u8; 16]>::decode(buf)?;
        let default_constant_iv = if default_is_protected == 1 && default_per_sample_iv_size == 0 {
            let iv_size = u8::decode(buf)?;
            let mut iv = Vec::with_capacity(iv_size.into());
            for _ in 0..iv_size {
                iv.push(u8::decode(buf)?);
            }
            Some(iv)
        } else {
            None
        };
        Ok(Self {
            default_is_protected,
            default_per_sample_iv_size,
            default_key_id,
            default_constant_iv,
            default_crypt_byte_block,
            default_skip_byte_block,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
