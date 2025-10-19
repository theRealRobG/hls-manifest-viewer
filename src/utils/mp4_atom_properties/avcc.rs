use crate::utils::mp4_atom_properties::{
    byte_array_from, AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
    TablePropertyValue,
};
use mp4_atom::Avcc;

impl AtomWithProperties for Avcc {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "AVCConfigurationBox",
            properties: vec![
                (
                    "configuration_version",
                    AtomPropertyValue::from(self.configuration_version),
                ),
                (
                    "avc_profile_indication",
                    AtomPropertyValue::from(self.avc_profile_indication),
                ),
                (
                    "profile_compatibility",
                    AtomPropertyValue::from(self.profile_compatibility),
                ),
                (
                    "avc_level_indication",
                    AtomPropertyValue::from(self.avc_level_indication),
                ),
                ("length_size", AtomPropertyValue::from(self.length_size)),
                (
                    "sequence_parameter_sets",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: None,
                        rows: self
                            .sequence_parameter_sets
                            .iter()
                            .map(|bytes| vec![byte_array_from(bytes)])
                            .collect::<Vec<Vec<BasicPropertyValue>>>(),
                    }),
                ),
                (
                    "picture_parameter_sets",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: None,
                        rows: self
                            .picture_parameter_sets
                            .iter()
                            .map(|bytes| vec![byte_array_from(bytes)])
                            .collect::<Vec<Vec<BasicPropertyValue>>>(),
                    }),
                ),
                (
                    "ext_chroma_format",
                    AtomPropertyValue::from(self.ext.as_ref().map(|ext| ext.chroma_format)),
                ),
                (
                    "ext_bit_depth_luma",
                    AtomPropertyValue::from(self.ext.as_ref().map(|ext| ext.bit_depth_luma)),
                ),
                (
                    "ext_bit_depth_chroma",
                    AtomPropertyValue::from(self.ext.as_ref().map(|ext| ext.bit_depth_chroma)),
                ),
                (
                    "ext_sequence_parameter_sets",
                    self.ext
                        .as_ref()
                        .map(|ext| {
                            AtomPropertyValue::Table(TablePropertyValue {
                                headers: None,
                                rows: ext
                                    .sequence_parameter_sets_ext
                                    .iter()
                                    .map(|bytes| vec![byte_array_from(bytes)])
                                    .collect::<Vec<Vec<BasicPropertyValue>>>(),
                            })
                        })
                        .unwrap_or(AtomPropertyValue::Basic(BasicPropertyValue::String(
                            "".to_string(),
                        ))),
                ),
            ],
        }
    }
}
