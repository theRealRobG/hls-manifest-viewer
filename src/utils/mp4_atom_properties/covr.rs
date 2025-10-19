use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Covr;

impl AtomWithProperties for Covr {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "Covr MetadataItem",
            properties: vec![("covr", AtomPropertyValue::from(&self.0))],
        }
    }
}
