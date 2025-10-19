use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Cmpd;

impl AtomWithProperties for Cmpd {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ComponentDefinitionBox",
            properties: vec![(
                "components",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec!["type", "type_uri"]),
                    rows: self
                        .components
                        .iter()
                        .map(|c| {
                            vec![
                                BasicPropertyValue::from(c.component_type),
                                BasicPropertyValue::from(c.component_type_uri.as_ref()),
                            ]
                        })
                        .collect(),
                }),
            )],
        }
    }
}
