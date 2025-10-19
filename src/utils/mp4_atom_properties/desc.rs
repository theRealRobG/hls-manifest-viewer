use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Desc;

impl AtomWithProperties for Desc {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "Desc MetadataItem",
            properties: vec![("desc", AtomPropertyValue::from(&self.0))],
        }
    }
}
