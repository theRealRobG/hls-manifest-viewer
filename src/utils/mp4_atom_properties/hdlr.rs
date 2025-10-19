use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Hdlr;

impl AtomWithProperties for Hdlr {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "HandlerBox",
            properties: vec![
                ("handler", AtomPropertyValue::from(self.handler)),
                ("name", AtomPropertyValue::from(&self.name)),
            ],
        }
    }
}
