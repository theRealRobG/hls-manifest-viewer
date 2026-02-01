use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Pasp;

impl AtomWithProperties for Pasp {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "PixelAspectRatioBox",
            vec![
                ("h_spacing", AtomPropertyValue::from(self.h_spacing)),
                ("v_spacing", AtomPropertyValue::from(self.v_spacing)),
            ],
        )
    }
}
