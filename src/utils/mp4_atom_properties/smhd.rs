use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Smhd;

impl AtomWithProperties for Smhd {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "SoundMediaHeaderBox",
            properties: vec![(
                "balance",
                AtomPropertyValue::from(format!("{:?}", self.balance)),
            )],
        }
    }
}
