use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Subs;

impl AtomWithProperties for Subs {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "SubSampleInformationBox",
            vec![
                (
                    "flags",
                    AtomPropertyValue::from(BasicPropertyValue::Hex(self.flags.to_vec())),
                ),
                (
                    "entries",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec![
                            "sample_delta",
                            "size",
                            "priority",
                            "discardable",
                            "params",
                        ]),
                        rows: self
                            .entries
                            .iter()
                            .map(|entry| {
                                entry
                                    .subsamples
                                    .iter()
                                    .flat_map(|subsample| {
                                        vec![
                                            BasicPropertyValue::from(entry.sample_delta),
                                            BasicPropertyValue::from(subsample.size.value()),
                                            BasicPropertyValue::from(subsample.priority),
                                            BasicPropertyValue::from(subsample.discardable),
                                            BasicPropertyValue::Hex(
                                                subsample.codec_specific_parameters.clone(),
                                            ),
                                        ]
                                    })
                                    .collect()
                            })
                            .collect(),
                    }),
                ),
            ],
        )
    }
}
