use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Btrt;

impl AtomWithProperties for Btrt {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "BitRateBox",
            properties: vec![
                (
                    "buffer_size_db",
                    AtomPropertyValue::from(self.buffer_size_db),
                ),
                ("max_bitrate", AtomPropertyValue::from(self.max_bitrate)),
                ("avg_bitrate", AtomPropertyValue::from(self.avg_bitrate)),
            ],
        }
    }
}
