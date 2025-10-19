use crate::utils::{
    mp4::Schm,
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
};

impl AtomWithProperties for Schm {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "SchemeTypeBox",
            properties: vec![
                ("scheme_type", AtomPropertyValue::from(self.scheme_type)),
                (
                    "scheme_version",
                    AtomPropertyValue::from(self.scheme_version),
                ),
                (
                    "scheme_uri",
                    AtomPropertyValue::from(self.scheme_uri.as_ref()),
                ),
            ],
        }
    }
}
