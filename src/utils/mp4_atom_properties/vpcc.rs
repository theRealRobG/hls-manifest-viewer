use crate::utils::mp4_atom_properties::{
    byte_array_from, AtomProperties, AtomPropertyValue, AtomWithProperties,
};
use mp4_atom::VpcC;

impl AtomWithProperties for VpcC {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "VPCodecConfigurationBox",
            properties: vec![
                ("profile", AtomPropertyValue::from(self.profile)),
                ("level", AtomPropertyValue::from(self.level)),
                ("bit_depth", AtomPropertyValue::from(self.bit_depth)),
                (
                    "chroma_subsampling",
                    AtomPropertyValue::from(self.chroma_subsampling),
                ),
                (
                    "video_full_range_flag",
                    AtomPropertyValue::from(self.video_full_range_flag),
                ),
                (
                    "color_primaries",
                    AtomPropertyValue::from(self.color_primaries),
                ),
                (
                    "transfer_characteristics",
                    AtomPropertyValue::from(self.transfer_characteristics),
                ),
                (
                    "matrix_coefficients",
                    AtomPropertyValue::from(self.matrix_coefficients),
                ),
                (
                    "codec_initialization_data",
                    AtomPropertyValue::from(byte_array_from(&self.codec_initialization_data)),
                ),
            ],
        }
    }
}
