use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Prji,
};

impl AtomWithProperties for Prji {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "ProjectionInformationBox",
            vec![(
                "projection_kind",
                AtomPropertyValue::from(self.projection_kind),
            )],
        )
    }
}

