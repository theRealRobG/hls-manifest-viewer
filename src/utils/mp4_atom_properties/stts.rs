use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Stts;

impl AtomWithProperties for Stts {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "TimeToSampleBox",
            vec![(
                "entries",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec!["count", "delta"]),
                    rows: self
                        .entries
                        .iter()
                        .map(|entry| {
                            vec![
                                BasicPropertyValue::from(entry.sample_count),
                                BasicPropertyValue::from(entry.sample_delta),
                            ]
                        })
                        .collect(),
                }),
            )],
        )
    }
}
