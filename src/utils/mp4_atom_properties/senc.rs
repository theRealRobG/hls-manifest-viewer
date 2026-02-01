use crate::utils::{
    mp4_atom_properties::{
        AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
        TablePropertyValue,
    },
    mp4_parsing::Senc,
};

impl AtomWithProperties for Senc {
    fn properties(&self) -> AtomProperties {
        let box_name = "SampleEncryptionBox";
        if self.entries.is_empty() {
            return AtomProperties::from_static_keys(
                box_name,
                vec![("IV", AtomPropertyValue::from("Constant"))],
            );
        }
        AtomProperties::from_static_keys(
            box_name,
            self.entries
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
        )
    }
}
