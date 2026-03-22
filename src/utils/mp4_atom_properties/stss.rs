use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, array_string_from,
};
use mp4_atom::Stss;

impl AtomWithProperties for Stss {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "SyncSampleBox",
            vec![(
                "entries",
                AtomPropertyValue::from(array_string_from(&self.entries)),
            )],
        )
    }
}
