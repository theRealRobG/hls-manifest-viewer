use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Dops;

impl AtomWithProperties for Dops {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "OpusSpecificBox",
            properties: vec![
                (
                    "output_channel_count",
                    AtomPropertyValue::from(self.output_channel_count),
                ),
                ("pre_skip", AtomPropertyValue::from(self.pre_skip)),
                (
                    "input_sample_rate",
                    AtomPropertyValue::from(self.input_sample_rate),
                ),
                ("output_gain", AtomPropertyValue::from(self.output_gain)),
            ],
        }
    }
}
