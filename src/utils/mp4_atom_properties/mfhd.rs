use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Mfhd;

impl AtomWithProperties for Mfhd {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "MovieFragmentHeaderBox",
            vec![(
                "sequence_number",
                AtomPropertyValue::from(self.sequence_number),
            )],
        )
    }
}
