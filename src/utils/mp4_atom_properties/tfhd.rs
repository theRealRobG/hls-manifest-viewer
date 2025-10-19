use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Tfhd;

impl AtomWithProperties for Tfhd {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "TrackFragmentHeaderBox",
            properties: vec![
                ("track_id", AtomPropertyValue::from(self.track_id)),
                (
                    "base_data_offset",
                    AtomPropertyValue::from(self.base_data_offset),
                ),
                (
                    "sample_description_index",
                    AtomPropertyValue::from(self.sample_description_index),
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
