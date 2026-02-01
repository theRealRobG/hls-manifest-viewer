use crate::utils::mp4_atom_properties::{
    array_string_from, AtomProperties, AtomPropertyValue, AtomWithProperties,
};
use mp4_atom::Stsz;

impl AtomWithProperties for Stsz {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "SampleSizeBox",
            vec![
                (
                    "sample_count",
                    match &self.samples {
                        mp4_atom::StszSamples::Identical { count, size: _ } => {
                            AtomPropertyValue::from(*count)
                        }
                        mp4_atom::StszSamples::Different { sizes } => {
                            AtomPropertyValue::from(sizes.len())
                        }
                    },
                ),
                match &self.samples {
                    mp4_atom::StszSamples::Identical { count: _, size } => {
                        ("sample_size", AtomPropertyValue::from(*size))
                    }
                    mp4_atom::StszSamples::Different { sizes } => (
                        "sample_sizes",
                        AtomPropertyValue::from(array_string_from(sizes)),
                    ),
                },
            ],
        )
    }
}
