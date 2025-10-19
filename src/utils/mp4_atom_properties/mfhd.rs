use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Mfhd;

impl AtomWithProperties for Mfhd {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "MovieFragmentHeaderBox",
            properties: vec![(
                "sequence_number",
                AtomPropertyValue::from(self.sequence_number),
            )],
        }
    }
}
