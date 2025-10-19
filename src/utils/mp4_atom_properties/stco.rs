use crate::utils::mp4_atom_properties::{
    array_string_from, AtomProperties, AtomPropertyValue, AtomWithProperties,
};
use mp4_atom::Stco;

impl AtomWithProperties for Stco {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ChunkOffsetBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::from(array_string_from(&self.entries)),
            )],
        }
    }
}
