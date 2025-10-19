use crate::utils::mp4_atom_properties::{
    array_string_from, AtomProperties, AtomPropertyValue, AtomWithProperties,
};
use mp4_atom::Co64;

impl AtomWithProperties for Co64 {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ChunkLargeOffsetBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::from(array_string_from(&self.entries)),
            )],
        }
    }
}
