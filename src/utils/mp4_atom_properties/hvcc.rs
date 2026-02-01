use crate::utils::mp4_atom_properties::{
    array_string_from, byte_array_from, AtomProperties, AtomPropertyValue, AtomWithProperties,
    BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Hvcc;

impl AtomWithProperties for Hvcc {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "HEVCConfigurationBox",
            vec![
                (
                    "configuration_version",
                    AtomPropertyValue::from(self.configuration_version),
                ),
                (
                    "general_profile_space",
                    AtomPropertyValue::from(self.general_profile_space),
                ),
                (
                    "general_tier_flag",
                    AtomPropertyValue::from(self.general_tier_flag),
                ),
                (
                    "general_profile_idc",
                    AtomPropertyValue::from(self.general_profile_idc),
                ),
                (
                    "general_profile_compatibility_flags",
                    AtomPropertyValue::from(array_string_from(
                        &self.general_profile_compatibility_flags,
                    )),
                ),
                (
                    "general_constraint_indicator_flags",
                    AtomPropertyValue::from(array_string_from(
                        &self.general_constraint_indicator_flags,
                    )),
                ),
                (
                    "general_level_idc",
                    AtomPropertyValue::from(self.general_level_idc),
                ),
                (
                    "min_spatial_segmentation_idc",
                    AtomPropertyValue::from(self.min_spatial_segmentation_idc),
                ),
                (
                    "parallelism_type",
                    AtomPropertyValue::from(self.parallelism_type),
                ),
                (
                    "chroma_format_idc",
                    AtomPropertyValue::from(self.chroma_format_idc),
                ),
                (
                    "bit_depth_luma_minus8",
                    AtomPropertyValue::from(self.bit_depth_luma_minus8),
                ),
                (
                    "bit_depth_chroma_minus8",
                    AtomPropertyValue::from(self.bit_depth_chroma_minus8),
                ),
                (
                    "avg_frame_rate",
                    AtomPropertyValue::from(self.avg_frame_rate),
                ),
                (
                    "constant_frame_rate",
                    AtomPropertyValue::from(self.constant_frame_rate),
                ),
                (
                    "num_temporal_layers",
                    AtomPropertyValue::from(self.num_temporal_layers),
                ),
                (
                    "temporal_id_nested",
                    AtomPropertyValue::from(self.temporal_id_nested),
                ),
                (
                    "length_size_minus_one",
                    AtomPropertyValue::from(self.length_size_minus_one),
                ),
                (
                    "arrays",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec!["completeness", "nal_unit_type", "nalus"]),
                        rows: self
                            .arrays
                            .iter()
                            .map(|array| {
                                vec![
                                    BasicPropertyValue::from(array.completeness),
                                    BasicPropertyValue::from(array.nal_unit_type),
                                    byte_array_from(
                                        &array.nalus.iter().flatten().copied().collect::<Vec<u8>>(),
                                    ),
                                ]
                            })
                            .collect(),
                    }),
                ),
            ],
        )
    }
}
