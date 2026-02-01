use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Ipma;

impl AtomWithProperties for Ipma {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "ItemPropertyAssociationBox",
            vec![(
                "item_properties",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec!["item_id", "associations"]),
                    rows: self
                        .item_properties
                        .iter()
                        .map(|ipma| {
                            vec![
                                BasicPropertyValue::from(ipma.item_id),
                                BasicPropertyValue::from(
                                    ipma.associations
                                        .iter()
                                        .map(|assoc| {
                                            format!(
                                                "(essential: {}, property_index: {})",
                                                assoc.essential, assoc.property_index
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
        )
    }
}
