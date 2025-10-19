use crate::utils::mp4_atom_properties::{AtomProperties, AtomWithProperties};
use mp4_atom::Free;

impl AtomWithProperties for Free {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "FreeSpaceBox",
            properties: vec![],
        }
    }
}
