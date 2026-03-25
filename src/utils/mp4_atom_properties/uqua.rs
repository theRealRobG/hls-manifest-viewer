use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Uqua,
};

impl AtomWithProperties for Uqua {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "CameraSystemUnitQuaternionTransformBox",
            vec![(
                "xyz",
                AtomPropertyValue::from(format!(
                    "[{}, {}, {}]",
                    self.xyz[0], self.xyz[1], self.xyz[2]
                )),
            )],
        )
    }
}

