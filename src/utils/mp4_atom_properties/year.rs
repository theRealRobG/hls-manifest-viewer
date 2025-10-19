use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Year;

impl AtomWithProperties for Year {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "Year MetadataItem",
            properties: vec![("year", AtomPropertyValue::from(&self.0))],
        }
    }
}
