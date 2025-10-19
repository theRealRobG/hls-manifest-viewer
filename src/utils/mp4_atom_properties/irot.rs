use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Irot;

impl AtomWithProperties for Irot {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ImageRotation",
            properties: vec![("angle", AtomPropertyValue::from(self.angle))],
        }
    }
}
