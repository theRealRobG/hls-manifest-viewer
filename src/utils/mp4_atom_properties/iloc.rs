use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Iloc;

impl AtomWithProperties for Iloc {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ItemLocationBox",
            properties: vec![(
                "item_locations",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec![
                        "item_id",
                        "construction_method",
                        "data_reference_index",
                        "base_offset",
                        "extents",
                    ]),
                    rows: self
                        .item_locations
                        .iter()
                        .map(|iloc| {
                            vec![
                                BasicPropertyValue::from(iloc.item_id),
                                BasicPropertyValue::from(iloc.construction_method),
                                BasicPropertyValue::from(iloc.data_reference_index),
                                BasicPropertyValue::from(iloc.base_offset),
                                BasicPropertyValue::from(
                                    iloc.extents
                                        .iter()
                                        .map(|ext| {
                                            format!(
                                                "({},{},{})",
                                                ext.item_reference_index, ext.offset, ext.length
                                            )
                                        })
                                        .collect::<Vec<String>>()
                                        .join(", "),
                                ),
                            ]
                        })
                        .collect(),
                }),
            )],
        }
    }
}
