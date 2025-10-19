use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Styp;

impl AtomWithProperties for Styp {
    fn properties(&self) -> super::AtomProperties {
        AtomProperties {
            box_name: "SegmentTypeBox",
            properties: vec![
                ("major_brand", AtomPropertyValue::from(self.major_brand)),
                ("minor_version", AtomPropertyValue::from(self.minor_version)),
                (
                    "compatible_brands",
                    AtomPropertyValue::from(&self.compatible_brands),
                ),
            ],
        }
    }
}
