use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, array_string_from,
};
use mp4_atom::Saio;

impl AtomWithProperties for Saio {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "SampleAuxiliaryInformationOffsetsBox",
            vec![
                (
                    "aux_info_type",
                    AtomPropertyValue::from(self.aux_info.as_ref().map(|a| a.aux_info_type)),
                ),
                (
                    "aux_info_type_parameter",
                    AtomPropertyValue::from(
                        self.aux_info.as_ref().map(|a| a.aux_info_type_parameter),
                    ),
                ),
                (
                    "offsets",
                    AtomPropertyValue::from(array_string_from(&self.offsets)),
                ),
            ],
        )
    }
}
