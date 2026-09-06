use crate::utils::{
    mp4_atom_properties::{
        AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
    },
    mp4_parsing::Keyd,
};

impl AtomWithProperties for Keyd {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "MetadataKeyDeclarationBox",
            vec![
                ("key_namespace", AtomPropertyValue::from(self.key_namespace)),
                if let Ok(s) = std::str::from_utf8(&self.key_value) {
                    ("key_value", AtomPropertyValue::from(s))
                } else {
                    (
                        "key_value",
                        AtomPropertyValue::Basic(BasicPropertyValue::Hex(self.key_value.clone())),
                    )
                },
            ],
        )
    }
}
