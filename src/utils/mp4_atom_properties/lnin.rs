use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Lnin,
};

impl AtomWithProperties for Lnin {
    fn properties(&self) -> AtomProperties {
        let mut props: Vec<(&'static str, AtomPropertyValue)> = vec![
            (
                "denominator_shift_operand",
                AtomPropertyValue::from(self.denominator_shift_operand),
            ),
            (
                "skew_denominator_shift_operand",
                AtomPropertyValue::from(self.skew_denominator_shift_operand),
            ),
            (
                "focal_length_x",
                AtomPropertyValue::from(self.focal_length_x),
            ),
            (
                "principal_point_x",
                AtomPropertyValue::from(self.principal_point_x),
            ),
            (
                "principal_point_y",
                AtomPropertyValue::from(self.principal_point_y),
            ),
        ];
        if let Some(v) = self.focal_length_y {
            props.push(("focal_length_y", AtomPropertyValue::from(v)));
        }
        if let Some(v) = self.skew_factor {
            props.push(("skew_factor", AtomPropertyValue::from(v)));
        }
        if let Some(v) = self.projection_offset {
            props.push((
                "projection_offset",
                AtomPropertyValue::from(format!("{v}")),
            ));
        }
        AtomProperties::from_static_keys("CameraSystemLensIntrinsicsBox", props)
    }
}

