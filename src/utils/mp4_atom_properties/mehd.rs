use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Mehd;

impl AtomWithProperties for Mehd {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "MovieExtendsHeaderBox",
            vec![(
                "fragment_duration",
                AtomPropertyValue::from(self.fragment_duration),
            )],
        )
    }
}
