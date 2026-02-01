use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Ctts;

impl AtomWithProperties for Ctts {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "CompositionOffsetBox",
            vec![(
                "entries",
                AtomPropertyValue::from(
                    self.entries
                        .iter()
                        .map(|entry| {
                            format!(
                                "(count: {}, offset: {})",
                                entry.sample_count, entry.sample_offset
                            )
                        })
                        .collect::<Vec<String>>()
                        .join(", "),
                ),
            )],
        )
    }
}
