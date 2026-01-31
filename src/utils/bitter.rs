use bitter::{BigEndianReader, BitReader};

pub trait ByteAlign {
    fn align(&mut self) -> Result<(), &'static str>;
}

impl ByteAlign for BigEndianReader<'_> {
    fn align(&mut self) -> Result<(), &'static str> {
        let bits_to_byte_align = self.remainder().partial_bits();
        if bits_to_byte_align > 0 {
            self.read_bits(bits_to_byte_align.into()).ok_or("ERROR")?;
        }
        Ok(())
    }
}
