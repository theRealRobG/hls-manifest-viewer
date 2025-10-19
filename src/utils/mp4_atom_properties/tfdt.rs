use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Tfdt;

impl AtomWithProperties for Tfdt {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "TrackFragmentBaseMediaDecodeTimeBox",
            properties: vec![(
                "base_media_decode_time",
                AtomPropertyValue::from(self.base_media_decode_time),
            )],
        }
    }
}
