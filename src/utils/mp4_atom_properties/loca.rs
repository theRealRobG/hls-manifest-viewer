use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Loca,
};

impl AtomWithProperties for Loca {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "MetadataLocaleBox",
            vec![("locale", AtomPropertyValue::from(&self.locale))],
        )
    }
}
