use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomWithProperties},
    mp4_parsing::Rect,
};

impl AtomWithProperties for Rect {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "RectilinearProjectionBox",
            properties: vec![],
        }
    }
}

