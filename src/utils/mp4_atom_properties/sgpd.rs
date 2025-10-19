use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::{AnySampleGroupEntry, Sgpd};

impl AtomWithProperties for Sgpd {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "SampleGroupDescriptionBox",
            properties: vec![
                ("grouping_type", AtomPropertyValue::from(self.grouping_type)),
                (
                    "default_length",
                    AtomPropertyValue::from(self.default_length),
                ),
                (
                    "default_group_description_index",
                    AtomPropertyValue::from(self.default_group_description_index),
                ),
                (
                    "static_group_description",
                    AtomPropertyValue::from(self.static_group_description),
                ),
                (
                    "static_mapping",
                    AtomPropertyValue::from(self.static_mapping),
                ),
                ("essential", AtomPropertyValue::from(self.essential)),
                (
                    "entries",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec!["description_length", "4CC", "data"]),
                        rows: self
                            .entries
                            .iter()
                            .map(|entry| match &entry.entry {
                                AnySampleGroupEntry::UnknownGroupingType(four_cc, items) => vec![
                                    BasicPropertyValue::from(entry.description_length),
                                    BasicPropertyValue::from(*four_cc),
                                    BasicPropertyValue::Hex(items.clone()),
                                ],
                            })
                            .collect(),
                    }),
                ),
            ],
        }
    }
}
