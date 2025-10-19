use crate::utils::mp4_atom_properties::{
    byte_array_from, AtomProperties, AtomPropertyValue, AtomWithProperties,
};
use mp4_atom::Av1c;

impl AtomWithProperties for Av1c {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "AV1CodecConfigurationBox",
            properties: vec![
                ("seq_profile", AtomPropertyValue::from(self.seq_profile)),
                (
                    "seq_level_idx_0",
                    AtomPropertyValue::from(self.seq_level_idx_0),
                ),
                ("seq_tier_0", AtomPropertyValue::from(self.seq_tier_0)),
                ("high_bitdepth", AtomPropertyValue::from(self.high_bitdepth)),
                ("twelve_bit", AtomPropertyValue::from(self.twelve_bit)),
                ("monochrome", AtomPropertyValue::from(self.monochrome)),
                (
                    "chroma_subsampling_x",
                    AtomPropertyValue::from(self.chroma_subsampling_x),
                ),
                (
                    "chroma_subsampling_y",
                    AtomPropertyValue::from(self.chroma_subsampling_y),
                ),
                (
                    "chroma_sample_position",
                    AtomPropertyValue::from(self.chroma_sample_position),
                ),
                (
                    "initial_presentation_delay",
                    AtomPropertyValue::from(self.initial_presentation_delay),
                ),
                (
                    "config_obus",
                    AtomPropertyValue::from(byte_array_from(&self.config_obus)),
                ),
            ],
        }
    }
}
