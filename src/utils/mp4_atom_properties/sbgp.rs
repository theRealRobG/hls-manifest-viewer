use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Sbgp;

impl AtomWithProperties for Sbgp {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "SampleToGroupBox",
            vec![
                ("grouping_type", AtomPropertyValue::from(self.grouping_type)),
                (
                    "grouping_type_parameter",
                    AtomPropertyValue::from(self.grouping_type_parameter),
                ),
                (
                    "entries",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec!["sample_count", "group_description_index"]),
                        rows: self
                            .entries
                            .iter()
                            .map(|entry| {
                                vec![
                                    BasicPropertyValue::from(entry.sample_count),
                                    BasicPropertyValue::from(entry.group_description_index),
                                ]
                            })
                            .collect(),
                    }),
                ),
            ],
        )
    }
}
