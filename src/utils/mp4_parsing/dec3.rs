use bitter::{BigEndianReader, BitReader};
use mp4_atom::{Atom, Buf, BufMut, FourCC, Result};

/// EC3SpecificBox, ETSI TS 102 366 V1.4.1 (2017-09) Sect F.6
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dec3 {
    pub data_rate: u16,
    pub independent_substreams: Vec<IndependentSubstream>,
}
const READ_ERR: mp4_atom::Error = mp4_atom::Error::OutOfBounds;
impl Atom for Dec3 {
    const KIND: FourCC = FourCC::new(b"dec3");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut reader = BigEndianReader::new(buf.slice(buf.remaining()));
        let data_rate = reader.read_bits(13).ok_or(READ_ERR)? as u16;
        let num_ind_sub = reader.read_bits(3).ok_or(READ_ERR)? as usize;
        let mut independent_substreams = Vec::with_capacity(num_ind_sub);
        for _ in 0..(num_ind_sub + 1) {
            let fscod = reader.read_bits(2).ok_or(READ_ERR)? as u8;
            let bsid = reader.read_bits(5).ok_or(READ_ERR)? as u8;
            _ = reader.read_bit().ok_or(READ_ERR)?;
            let asvc = reader.read_bit().ok_or(READ_ERR)? as u8;
            let bsmod = reader.read_bits(3).ok_or(READ_ERR)? as u8;
            let acmod = reader.read_bits(3).ok_or(READ_ERR)? as u8;
            let lfeon = reader.read_bit().ok_or(READ_ERR)? as u8;
            _ = reader.read_bits(3).ok_or(READ_ERR)?;
            let num_dep_sub = reader.read_bits(4).ok_or(READ_ERR)? as u8;
            let chan_loc = if num_dep_sub > 0 {
                Some(reader.read_bits(9).ok_or(READ_ERR)? as u16)
            } else {
                _ = reader.read_bit().ok_or(READ_ERR)?;
                None
            };
            independent_substreams.push(IndependentSubstream {
                fscod,
                bsid,
                asvc,
                bsmod,
                acmod,
                lfeon,
                num_dep_sub,
                chan_loc,
            });
        }
        // Advance everything once we're here. If we over-decoded we would've failed when reading
        // using the BigEndianReader. There is no under-decode for this box, as the remaining bits
        // are all reserved for this case:
        //
        //   F.6.2.14 reserved - variable
        //
        //   Additional reserved bytes may follow at the end of the EC3SpecificBox. The number of
        //   reserved bytes present is determined by subtracting the number of bytes used by the
        //   EC3SpecificBox fields specified above from the total length of the EC3SpecificBox as
        //   specified by the value of the BoxHeader.Size field of the EC3SpecificBox. Decoders
        //   conformant with this annex should ignore any reserved bytes that are present at the end
        //   of the EC3SpecificBox.
        //
        buf.advance(buf.remaining());
        Ok(Self {
            data_rate,
            independent_substreams,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndependentSubstream {
    pub fscod: u8,
    pub bsid: u8,
    pub asvc: u8,
    pub bsmod: u8,
    pub acmod: u8,
    pub lfeon: u8,
    pub num_dep_sub: u8,
    pub chan_loc: Option<u16>,
}
impl IndependentSubstream {
    pub fn contains(&self, chan_loc: ChanLoc) -> bool {
        let Some(self_chan_loc) = self.chan_loc else {
            return false;
        };
        ((self_chan_loc & chan_loc.mask()) >> chan_loc.shift()) == 1
    }

    pub fn descriptive_chan_loc(&self) -> Vec<ChanLoc> {
        if self.chan_loc.is_none() {
            return Vec::new();
        }
        let mut v = Vec::new();
        if self.contains(ChanLoc::LcRc) {
            v.push(ChanLoc::LcRc);
        }
        if self.contains(ChanLoc::LrsRrs) {
            v.push(ChanLoc::LrsRrs);
        }
        if self.contains(ChanLoc::Cs) {
            v.push(ChanLoc::Cs);
        }
        if self.contains(ChanLoc::Ts) {
            v.push(ChanLoc::Ts);
        }
        if self.contains(ChanLoc::LsdRsd) {
            v.push(ChanLoc::LsdRsd);
        }
        if self.contains(ChanLoc::LwRw) {
            v.push(ChanLoc::LwRw);
        }
        if self.contains(ChanLoc::LvhRvh) {
            v.push(ChanLoc::LvhRvh);
        }
        if self.contains(ChanLoc::Cvh) {
            v.push(ChanLoc::Cvh);
        }
        if self.contains(ChanLoc::LFE2) {
            v.push(ChanLoc::LFE2);
        }
        v
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChanLoc {
    LcRc,
    LrsRrs,
    Cs,
    Ts,
    LsdRsd,
    LwRw,
    LvhRvh,
    Cvh,
    LFE2,
}
impl ChanLoc {
    fn mask(&self) -> u16 {
        match self {
            ChanLoc::LcRc => 0b000000001,
            ChanLoc::LrsRrs => 0b000000010,
            ChanLoc::Cs => 0b000000100,
            ChanLoc::Ts => 0b000001000,
            ChanLoc::LsdRsd => 0b000010000,
            ChanLoc::LwRw => 0b000100000,
            ChanLoc::LvhRvh => 0b001000000,
            ChanLoc::Cvh => 0b010000000,
            ChanLoc::LFE2 => 0b100000000,
        }
    }

    fn shift(&self) -> u8 {
        match self {
            ChanLoc::LcRc => 0,
            ChanLoc::LrsRrs => 1,
            ChanLoc::Cs => 2,
            ChanLoc::Ts => 3,
            ChanLoc::LsdRsd => 4,
            ChanLoc::LwRw => 5,
            ChanLoc::LvhRvh => 6,
            ChanLoc::Cvh => 7,
            ChanLoc::LFE2 => 8,
        }
    }
}
