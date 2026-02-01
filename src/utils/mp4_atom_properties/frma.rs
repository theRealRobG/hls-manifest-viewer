use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Frma,
};

impl AtomWithProperties for Frma {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "OriginalFormatBox",
            vec![("data_format", AtomPropertyValue::from(self.data_format))],
        )
    }
}
