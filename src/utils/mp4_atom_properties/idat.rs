use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Idat;

impl AtomWithProperties for Idat {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "ItemDataBox",
            vec![("data", AtomPropertyValue::from(&self.data))],
        )
    }
}
