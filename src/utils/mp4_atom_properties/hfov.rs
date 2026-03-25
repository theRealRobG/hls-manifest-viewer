use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Hfov,
};

impl AtomWithProperties for Hfov {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "HorizontalFieldOfViewBox",
            vec![(
                "field_of_view",
                AtomPropertyValue::from(self.field_of_view),
            )],
        )
    }
}

