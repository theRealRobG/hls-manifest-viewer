use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Iinf;

impl AtomWithProperties for Iinf {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ItemInfoBox",
            properties: vec![(
                "item_infos",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec![
                        "item_id",
                        "item_protection_index",
                        "item_type",
                        "item_name",
                        "content_type",
                        "content_encoding",
                        "item_not_in_presentation",
                    ]),
                    rows: self
                        .item_infos
                        .iter()
                        .map(|iinf| {
                            vec![
                                BasicPropertyValue::from(iinf.item_id),
                                BasicPropertyValue::from(iinf.item_protection_index),
                                BasicPropertyValue::from(iinf.item_type),
                                BasicPropertyValue::from(&iinf.item_name),
                                BasicPropertyValue::from(iinf.content_type.as_ref()),
                                BasicPropertyValue::from(iinf.content_encoding.as_ref()),
                                BasicPropertyValue::from(iinf.item_not_in_presentation),
                            ]
                        })
                        .collect(),
                }),
            )],
        }
    }
}
