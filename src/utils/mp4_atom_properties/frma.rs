use crate::utils::{
    mp4::Frma,
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
};

impl AtomWithProperties for Frma {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "OriginalFormatBox",
            properties: vec![("data_format", AtomPropertyValue::from(self.data_format))],
        }
    }
}
