use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Trex;

impl AtomWithProperties for Trex {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "TrackExtendsBox",
            properties: vec![
                ("track_id", AtomPropertyValue::from(self.track_id)),
                (
                    "default_sample_description_index",
                    AtomPropertyValue::from(self.default_sample_description_index),
                ),
                (
                    "default_sample_duration",
                    AtomPropertyValue::from(self.default_sample_duration),
                ),
                (
                    "default_sample_size",
                    AtomPropertyValue::from(self.default_sample_size),
                ),
                (
                    "default_sample_flags",
                    AtomPropertyValue::from(self.default_sample_flags),
                ),
            ],
        }
    }
}
