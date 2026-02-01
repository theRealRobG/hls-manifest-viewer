use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Year;

impl AtomWithProperties for Year {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "Year MetadataItem",
            vec![("year", AtomPropertyValue::from(&self.0))],
        )
    }
}
