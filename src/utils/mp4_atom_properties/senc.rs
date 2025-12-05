use crate::utils::{
    mp4::Senc,
    mp4_atom_properties::{
        AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
        TablePropertyValue,
    },
};

impl AtomWithProperties for Senc {
    fn properties(&self) -> AtomProperties {
        let box_name = "SampleEncryptionBox";
        if self.entries.is_empty() {
            return AtomProperties {
                box_name,
                properties: vec![("IV", AtomPropertyValue::from("Constant"))],
            };
        }
        AtomProperties {
            box_name,
            properties: self
                .entries
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    (
                        "",
                        AtomPropertyValue::Table(TablePropertyValue {
                            headers: None,
                            rows: entry.subsample_encryption.iter().fold(
                                vec![vec![
                                    BasicPropertyValue::from(format!("sample {} IV", i + 1)),
                                    BasicPropertyValue::from(&entry.initialization_vector),
                                ]],
                                |acc, subsample| {
                                    let row = vec![
                                        BasicPropertyValue::from("subsample 🔓/🔒"),
                                        BasicPropertyValue::from(format!(
                                            "{}/{}",
                                            subsample.bytes_of_clear_data,
                                            subsample.bytes_of_protected_data
                                        )),
                                    ];
                                    acc.into_iter().chain(vec![row]).collect()
                                },
                            ),
                        }),
                    )
                })
                .collect(),
        }
    }
}
