use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Dref;

impl AtomWithProperties for Dref {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "DataReferenceBox",
            properties: vec![(
                "urls",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: None,
                    rows: self
                        .urls
                        .iter()
                        .map(|url| vec![BasicPropertyValue::from(&url.location)])
                        .collect(),
                }),
            )],
        }
    }
}
