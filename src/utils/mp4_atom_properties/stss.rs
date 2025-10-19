use crate::utils::mp4_atom_properties::{
    array_string_from, AtomProperties, AtomPropertyValue, AtomWithProperties,
};
use mp4_atom::Stss;

impl AtomWithProperties for Stss {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "SyncSampleBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::from(array_string_from(&self.entries)),
            )],
        }
    }
}
