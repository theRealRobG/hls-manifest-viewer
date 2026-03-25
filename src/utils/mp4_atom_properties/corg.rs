use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Corg,
};

impl AtomWithProperties for Corg {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "CameraSystemOriginSourceBox",
            vec![(
                "source_of_origin",
                AtomPropertyValue::from(self.source_of_origin),
            )],
        )
    }
}

