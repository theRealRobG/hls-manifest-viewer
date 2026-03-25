use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomWithProperties},
    mp4_parsing::Hequ,
};

impl AtomWithProperties for Hequ {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "HalfEquirectangularProjectionBox",
            properties: vec![],
        }
    }
}

