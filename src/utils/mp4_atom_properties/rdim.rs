use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Rdim,
};

impl AtomWithProperties for Rdim {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "CameraSystemLensReferenceDimensionsBox",
            vec![
                (
                    "reference_width",
                    AtomPropertyValue::from(self.reference_width),
                ),
                (
                    "reference_height",
                    AtomPropertyValue::from(self.reference_height),
                ),
            ],
        )
    }
}

