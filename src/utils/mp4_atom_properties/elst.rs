use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Elst;

impl AtomWithProperties for Elst {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "EditListBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec![
                        "segment_duration",
                        "media_time",
                        "media_rate",
                        "media_rate_fraction",
                    ]),
                    rows: self
                        .entries
                        .iter()
                        .map(|entry| {
                            vec![
                                BasicPropertyValue::from(entry.segment_duration),
                                BasicPropertyValue::from(entry.media_time),
                                BasicPropertyValue::from(entry.media_rate),
                                BasicPropertyValue::from(entry.media_rate_fraction),
                            ]
                        })
                        .collect(),
                }),
            )],
        }
    }
}
