use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Pasp;

impl AtomWithProperties for Pasp {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "PixelAspectRatioBox",
            properties: vec![
                ("h_spacing", AtomPropertyValue::from(self.h_spacing)),
                ("v_spacing", AtomPropertyValue::from(self.v_spacing)),
            ],
        }
    }
}
