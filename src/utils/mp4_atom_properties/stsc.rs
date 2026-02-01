use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Stsc;

impl AtomWithProperties for Stsc {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "SampleToChunkBox",
            vec![(
                "entries",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec![
                        "first_chunk",
                        "samples_per_chunk",
                        "sample_description_index",
                    ]),
                    rows: self
                        .entries
                        .iter()
                        .map(|entry| {
                            vec![
                                BasicPropertyValue::from(entry.first_chunk),
                                BasicPropertyValue::from(entry.samples_per_chunk),
                                BasicPropertyValue::from(entry.sample_description_index),
                            ]
                        })
                        .collect(),
                }),
            )],
        )
    }
}
