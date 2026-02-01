use crate::utils::{
    mp4_atom_properties::{
        AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
        TablePropertyValue,
    },
    mp4_parsing::Lac4,
};

impl AtomWithProperties for Lac4 {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "AC4PresentationLabelBox",
            properties: vec![
                (
                    "language_tag",
                    AtomPropertyValue::from(self.language_tag.clone()),
                ),
                (
                    "labels",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec!["id", "label"]),
                        rows: self
                            .labels
                            .iter()
                            .map(|entry| {
                                vec![
                                    BasicPropertyValue::from(entry.id),
                                    BasicPropertyValue::from(entry.label.clone()),
                                ]
                            })
                            .collect(),
                    }),
                ),
            ],
        }
    }
}
