use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Dac3,
};

impl AtomWithProperties for Dac3 {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "AC3SpecificBox",
            vec![
                ("fscod", AtomPropertyValue::from(self.fscod)),
                ("bsid", AtomPropertyValue::from(self.bsid)),
                ("bsmod", AtomPropertyValue::from(self.bsmod)),
                ("acmod", AtomPropertyValue::from(self.acmod)),
                ("lfeon", AtomPropertyValue::from(self.lfeon)),
                ("bit_rate", AtomPropertyValue::from(self.bit_rate())),
            ],
        )
    }
}
