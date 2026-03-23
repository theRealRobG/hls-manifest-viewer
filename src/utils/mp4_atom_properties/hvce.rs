use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomWithProperties},
    mp4_parsing::Hvce,
};

impl AtomWithProperties for Hvce {
    fn properties(&self) -> AtomProperties {
        let mut properties = self.0.properties();
        properties.box_name = "DolbyVisionELHEVCConfigurationBox";
        properties
    }
}
