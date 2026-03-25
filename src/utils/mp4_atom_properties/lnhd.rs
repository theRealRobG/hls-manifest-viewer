use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Lnhd,
};

impl AtomWithProperties for Lnhd {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "CameraSystemLensHeaderBox",
            vec![
                (
                    "lens_identifier",
                    AtomPropertyValue::from(self.lens_identifier),
                ),
                (
                    "lens_algorithm_kind",
                    AtomPropertyValue::from(self.lens_algorithm_kind),
                ),
                (
                    "lens_domain",
                    AtomPropertyValue::from(self.lens_domain),
                ),
                (
                    "lens_role",
                    AtomPropertyValue::from(self.lens_role),
                ),
            ],
        )
    }
}

