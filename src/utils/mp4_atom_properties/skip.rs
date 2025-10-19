use crate::utils::mp4_atom_properties::{AtomProperties, AtomWithProperties};
use mp4_atom::Skip;

impl AtomWithProperties for Skip {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "FreeSpaceBox",
            properties: vec![],
        }
    }
}
