use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Stri,
};

impl AtomWithProperties for Stri {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "StereoViewInformationBox",
            vec![
                (
                    "eye_views_reversed",
                    AtomPropertyValue::from(self.eye_views_reversed),
                ),
                (
                    "has_additional_views",
                    AtomPropertyValue::from(self.has_additional_views),
                ),
                (
                    "has_right_eye_view",
                    AtomPropertyValue::from(self.has_right_eye_view),
                ),
                (
                    "has_left_eye_view",
                    AtomPropertyValue::from(self.has_left_eye_view),
                ),
            ],
        )
    }
}

