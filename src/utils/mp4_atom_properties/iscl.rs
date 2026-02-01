use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Iscl;

impl AtomWithProperties for Iscl {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "ImageScaling",
            vec![
                (
                    "target_width_numerator",
                    AtomPropertyValue::from(self.target_width_numerator),
                ),
                (
                    "target_width_denominator",
                    AtomPropertyValue::from(self.target_width_denominator),
                ),
                (
                    "target_height_numerator",
                    AtomPropertyValue::from(self.target_height_numerator),
                ),
                (
                    "target_height_denominator",
                    AtomPropertyValue::from(self.target_height_denominator),
                ),
            ],
        )
    }
}
