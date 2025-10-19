use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Auxc;

impl AtomWithProperties for Auxc {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "AuxiliaryTypeProperty",
            properties: vec![
                ("aux_type", AtomPropertyValue::from(&self.aux_type)),
                ("aux_subtype", AtomPropertyValue::from(&self.aux_subtype)),
            ],
        }
    }
}
