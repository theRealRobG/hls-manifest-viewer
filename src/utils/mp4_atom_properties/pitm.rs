use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Pitm;

impl AtomWithProperties for Pitm {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "PrimaryItemBox",
            vec![("item_id", AtomPropertyValue::from(self.item_id))],
        )
    }
}
