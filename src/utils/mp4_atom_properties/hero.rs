use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Hero,
};

impl AtomWithProperties for Hero {
    fn properties(&self) -> AtomProperties {
        let description = match self.hero_eye_indicator {
            0 => "none",
            1 => "left",
            2 => "right",
            _ => "reserved",
        };
        AtomProperties::from_static_keys(
            "HeroStereoEyeDescriptionBox",
            vec![
                (
                    "hero_eye_indicator",
                    AtomPropertyValue::from(self.hero_eye_indicator),
                ),
                ("description", AtomPropertyValue::from(description)),
            ],
        )
    }
}

