use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Iref;

impl AtomWithProperties for Iref {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "ItemReferenceBox",
            vec![(
                "references",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec!["reference_type", "from_item_id", "to_item_ids"]),
                    rows: self
                        .references
                        .iter()
                        .map(|iref| {
                            vec![
                                BasicPropertyValue::from(iref.reference_type),
                                BasicPropertyValue::from(iref.from_item_id),
                                BasicPropertyValue::from(
                                    iref.to_item_ids
                                        .iter()
                                        .map(|id| format!("{id}"))
                                        .collect::<Vec<String>>()
                                        .join(", "),
                                ),
                            ]
                        })
                        .collect(),
                }),
            )],
        )
    }
}
