use crate::utils::{
    mp4_atom_properties::{
        AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
    },
    mp4_parsing::Setu,
};

impl AtomWithProperties for Setu {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "MetadataSetupBox",
            vec![(
                "namespace_defined_data",
                AtomPropertyValue::Basic(BasicPropertyValue::Hex(
                    self.namespace_defined_data.clone(),
                )),
            )],
        )
    }
}
