use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Must,
};

impl AtomWithProperties for Must {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "RequiredBoxTypesBox",
            vec![(
                "required_box_types",
                AtomPropertyValue::from(&self.required_box_types),
            )],
        )
    }
}

