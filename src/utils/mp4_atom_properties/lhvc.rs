use crate::utils::{
    mp4_atom_properties::{
        AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
        TablePropertyValue, byte_array_from,
    },
    mp4_parsing::Lhvc,
};

impl AtomWithProperties for Lhvc {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "LayeredHEVCConfigurationBox",
            vec![
                (
                    "configuration_version",
                    AtomPropertyValue::from(self.configuration_version),
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
