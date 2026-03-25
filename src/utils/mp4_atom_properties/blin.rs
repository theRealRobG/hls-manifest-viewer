use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Blin,
};

impl AtomWithProperties for Blin {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "StereoCameraSystemBaselineBox",
            vec![(
                "baseline_value",
                AtomPropertyValue::from(self.baseline_value),
            )],
        )
    }
}

