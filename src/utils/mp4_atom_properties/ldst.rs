use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Ldst,
};

impl AtomWithProperties for Ldst {
    fn properties(&self) -> AtomProperties {
        let mut props: Vec<(&'static str, AtomPropertyValue)> = vec![
            ("k1", AtomPropertyValue::from(format!("{}", self.k1))),
            ("k2", AtomPropertyValue::from(format!("{}", self.k2))),
            ("p1", AtomPropertyValue::from(format!("{}", self.p1))),
            ("p2", AtomPropertyValue::from(format!("{}", self.p2))),
        ];
        if let Some(v) = self.calibration_limit_radial_angle {
            props.push((
                "calibration_limit_radial_angle",
                AtomPropertyValue::from(format!("{v}")),
            ));
        }
        AtomProperties::from_static_keys("CameraSystemLensDistortionsBox", props)
    }
}

