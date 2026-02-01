use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Covr;

impl AtomWithProperties for Covr {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "Covr MetadataItem",
            vec![("covr", AtomPropertyValue::from(&self.0))],
        )
    }
}
