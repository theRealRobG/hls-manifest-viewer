use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Name;

impl AtomWithProperties for Name {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "Name MetadataItem",
            vec![("name", AtomPropertyValue::from(&self.0))],
        )
    }
}
