use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomWithProperties},
    mp4_parsing::Prim,
};

impl AtomWithProperties for Prim {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ParametricImmersiveMediaProjectionBox",
            properties: vec![],
        }
    }
}

