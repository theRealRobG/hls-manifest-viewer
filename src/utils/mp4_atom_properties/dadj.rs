use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Dadj,
};

impl AtomWithProperties for Dadj {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "StereoComfortDisparityAdjustmentBox",
            vec![(
                "disparity_adjustment",
                AtomPropertyValue::from(self.disparity_adjustment),
            )],
        )
    }
}

