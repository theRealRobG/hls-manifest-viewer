use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Mehd;

impl AtomWithProperties for Mehd {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "MovieExtendsHeaderBox",
            properties: vec![(
                "fragment_duration",
                AtomPropertyValue::from(self.fragment_duration),
            )],
        }
    }
}
