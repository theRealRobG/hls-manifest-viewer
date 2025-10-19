use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Name;

impl AtomWithProperties for Name {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "Name MetadataItem",
            properties: vec![("name", AtomPropertyValue::from(&self.0))],
        }
    }
}
