use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Lfad,
};

fn format_float_array(arr: &[f32; 3]) -> String {
    format!("[{}, {}, {}]", arr[0], arr[1], arr[2])
}

impl AtomWithProperties for Lfad {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "CameraSystemLensFrameAdjustmentBox",
            vec![
                (
                    "polynomial_parameters_x",
                    AtomPropertyValue::from(format_float_array(&self.polynomial_parameters_x)),
                ),
                (
                    "polynomial_parameters_y",
                    AtomPropertyValue::from(format_float_array(&self.polynomial_parameters_y)),
                ),
            ],
        )
    }
}

