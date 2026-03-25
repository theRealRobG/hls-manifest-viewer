use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Pkin,
};

impl AtomWithProperties for Pkin {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "ViewPackingInformationBox",
            vec![(
                "view_packing_kind",
                AtomPropertyValue::from(self.view_packing_kind),
            )],
        )
    }
}

