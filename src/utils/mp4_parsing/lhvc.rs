use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, HvcCArray, Result};

/// LayeredHEVCConfigurationBox, ISO/IEC 14496-15 Section 8.4.1.1.
///
/// Contains an LHEVCDecoderConfigurationRecord with a 6-byte fixed header:
///   - configurationVersion (1 byte)
///   - min_spatial_segmentation_idc (upper 4 bits reserved, lower 12 bits) (2 bytes)
///   - parallelism_type (upper 6 bits reserved, lower 2 bits) (1 byte)
///   - num_temporal_layers (3 bits) | temporal_id_nested (1 bit) | length_size_minus_one (2 bits)
///     (upper 2 bits reserved) (1 byte)
///   - numOfArrays (1 byte)
///   - arrays (same format as hvcC)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lhvc {
    pub configuration_version: u8,
    pub min_spatial_segmentation_idc: u16,
    pub parallelism_type: u8,
    pub num_temporal_layers: u8,
    pub temporal_id_nested: bool,
    pub length_size_minus_one: u8,
    pub arrays: Vec<HvcCArray>,
}

impl Atom for Lhvc {
    const KIND: FourCC = FourCC::new(b"lhvC");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let configuration_version = u8::decode(buf)?;
        let min_spatial_segmentation_idc = u16::decode(buf)? & 0x0FFF;
        let parallelism_type = u8::decode(buf)? & 0b11;
        let temp = u8::decode(buf)?;
        let length_size_minus_one = temp & 0b11;
        let temporal_id_nested = (temp & 0b0000_0100) != 0;
        let num_temporal_layers = (temp & 0b0011_1000) >> 3;
        let num_of_arrays = u8::decode(buf)?;

        let mut arrays = Vec::with_capacity(num_of_arrays.min(8) as _);
        for _ in 0..num_of_arrays {
            let params = u8::decode(buf)?;
            let num_nalus = u16::decode(buf)?;
            let mut nalus = Vec::with_capacity(num_nalus.min(8) as usize);

            for _ in 0..num_nalus {
                let size = u16::decode(buf)? as usize;
                let data = Vec::decode_exact(buf, size)?;
                nalus.push(data);
            }

            arrays.push(HvcCArray {
                completeness: (params & 0b10000000) > 0,
                nal_unit_type: params & 0b111111,
                nalus,
            });
        }

        Ok(Lhvc {
            configuration_version,
            min_spatial_segmentation_idc,
            parallelism_type,
            num_temporal_layers,
            temporal_id_nested,
            length_size_minus_one,
            arrays,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
