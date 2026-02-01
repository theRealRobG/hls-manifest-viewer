use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Ispe;

impl AtomWithProperties for Ispe {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "ImageSpatialExtentProperty",
            vec![
                ("width", AtomPropertyValue::from(self.width)),
                ("height", AtomPropertyValue::from(self.height)),
            ],
        )
    }
}
