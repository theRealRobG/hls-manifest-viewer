use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Smhd;

impl AtomWithProperties for Smhd {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "SoundMediaHeaderBox",
            vec![(
                "balance",
                AtomPropertyValue::from(format!("{:?}", self.balance)),
            )],
        )
    }
}
