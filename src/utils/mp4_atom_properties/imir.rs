use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Imir;

impl AtomWithProperties for Imir {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ImageMirror",
            properties: vec![("axis", AtomPropertyValue::from(self.axis))],
        }
    }
}
