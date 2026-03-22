use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomWithProperties},
    mp4_parsing::{dvcc::Dvcc, Dvvc},
};

impl AtomWithProperties for Dvcc {
    fn properties(&self) -> AtomProperties {
        let dvvc = Dvvc::from(*self);
        dvvc.properties()
    }
}
