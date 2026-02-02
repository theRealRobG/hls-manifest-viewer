use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// AC3SpecificBox, ETSI TS 102 366 V1.4.1 (2017-09) Sect F.4
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dac3 {
    pub fscod: u8,
    pub bsid: u8,
    pub bsmod: u8,
    pub acmod: u8,
    pub lfeon: u8,
    pub bit_rate_code: u8,
}
impl Atom for Dac3 {
    const KIND: FourCC = FourCC::new(b"dac3");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let first_16 = u16::decode(buf)?;
        let last_8 = u8::decode(buf)?;
        let bits = ((first_16 as u32) << 8) | (last_8 as u32);
        let fscod = fscod_from(bits);
        let bsid = bsid_from(bits);
        let bsmod = bsmod_from(bits);
        let acmod = acmod_from(bits);
        let lfeon = lfeon_from(bits);
        let bit_rate_code = bit_rate_code_from(bits);
        Ok(Self {
            fscod,
            bsid,
            bsmod,
            acmod,
            lfeon,
            bit_rate_code,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
impl Dac3 {
    pub fn bit_rate(&self) -> u16 {
        match self.bit_rate_code {
            0b00000 => 32,
            0b00001 => 40,
            0b00010 => 48,
            0b00011 => 56,
            0b00100 => 64,
            0b00101 => 80,
            0b00110 => 96,
            0b00111 => 112,
            0b01000 => 128,
            0b01001 => 160,
            0b01010 => 192,
            0b01011 => 224,
            0b01100 => 256,
            0b01101 => 320,
            0b01110 => 384,
            0b01111 => 448,
            0b10000 => 512,
            0b10001 => 576,
            0b10010 => 640,
            _ => 0,
        }
    }
}
fn fscod_from(bits: u32) -> u8 {
    ((bits >> 22) & 0x03) as u8
}
fn bsid_from(bits: u32) -> u8 {
    ((bits >> 17) & 0x1F) as u8
}
fn bsmod_from(bits: u32) -> u8 {
    ((bits >> 14) & 0x07) as u8
}
fn acmod_from(bits: u32) -> u8 {
    ((bits >> 11) & 0x07) as u8
}
fn lfeon_from(bits: u32) -> u8 {
    ((bits >> 10) & 0x01) as u8
}
fn bit_rate_code_from(bits: u32) -> u8 {
    ((bits >> 5) & 0x1F) as u8
}
