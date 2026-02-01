use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Rref;

impl AtomWithProperties for Rref {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "RequiredReferenceTypesProperty",
            vec![(
                "reference_types",
                AtomPropertyValue::from(&self.reference_types),
            )],
        )
    }
}
