use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Mdhd;

impl AtomWithProperties for Mdhd {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "MediaHeaderBox",
            properties: vec![
                ("creation_time", AtomPropertyValue::from(self.creation_time)),
                (
                    "modification_time",
                    AtomPropertyValue::from(self.modification_time),
                ),
                ("timescale", AtomPropertyValue::from(self.timescale)),
                ("duration", AtomPropertyValue::from(self.duration)),
                ("language", AtomPropertyValue::from(&self.language)),
            ],
        }
    }
}
