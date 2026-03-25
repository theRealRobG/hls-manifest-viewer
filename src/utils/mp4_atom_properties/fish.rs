use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomWithProperties},
    mp4_parsing::Fish,
};

impl AtomWithProperties for Fish {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "FisheyeProjectionBox",
            properties: vec![],
        }
    }
}

