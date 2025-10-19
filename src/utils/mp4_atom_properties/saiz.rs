use crate::utils::mp4_atom_properties::{
    array_string_from, AtomProperties, AtomPropertyValue, AtomWithProperties,
};
use mp4_atom::Saiz;

impl AtomWithProperties for Saiz {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "SampleAuxiliaryInformationSizesBox",
            properties: vec![
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
                    "default_sample_info_size",
                    AtomPropertyValue::from(self.default_sample_info_size),
                ),
                ("sample_count", AtomPropertyValue::from(self.sample_count)),
                (
                    "sample_info_size",
                    AtomPropertyValue::from(array_string_from(&self.sample_info_size)),
                ),
            ],
        }
    }
}
