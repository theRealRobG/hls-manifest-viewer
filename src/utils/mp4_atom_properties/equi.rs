use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomWithProperties},
    mp4_parsing::Equi,
};

impl AtomWithProperties for Equi {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "FullEquirectangularProjectionBox",
            properties: vec![],
        }
    }
}

