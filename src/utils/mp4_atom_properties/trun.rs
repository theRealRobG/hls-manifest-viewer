use crate::utils::mp4_atom_properties::{
    byte_array_string_from, AtomProperties, AtomPropertyValue, AtomWithProperties,
    BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Trun;

impl AtomWithProperties for Trun {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "TrackRunBox",
            vec![
                ("data_offset", AtomPropertyValue::from(self.data_offset)),
                ("sample_count", AtomPropertyValue::from(self.entries.len())),
                (
                    "entries",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec!["#", "duration", "size", "flags", "cts"]),
                        rows: self
                            .entries
                            .iter()
                            .enumerate()
                            .map(|(index, entry)| {
                                vec![
                                    BasicPropertyValue::from(index + 1),
                                    BasicPropertyValue::from(entry.duration),
                                    BasicPropertyValue::from(entry.size),
                                    if let Some(flags) = entry.flags {
                                        byte_array_string_from(&flags.to_be_bytes())
                                    } else {
                                        BasicPropertyValue::from(entry.flags)
                                    },
                                    BasicPropertyValue::from(entry.cts),
                                ]
                            })
                            .collect(),
                    }),
                ),
            ],
        )
    }
}
