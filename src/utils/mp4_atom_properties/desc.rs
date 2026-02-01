use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Desc;

impl AtomWithProperties for Desc {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "Desc MetadataItem",
            vec![("desc", AtomPropertyValue::from(&self.0))],
        )
    }
}
