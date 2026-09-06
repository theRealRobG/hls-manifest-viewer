use crate::utils::{
    mp4_atom_properties::{
        AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
    },
    mp4_parsing::{dtyp::DtypArrayValue, Dtyp},
};

impl AtomWithProperties for Dtyp {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "MetadataDatatypeDefinitionAtom",
            match DtypArrayValue::from(self) {
                DtypArrayValue::WellKnownType(well_known_type) => vec![
                    (
                        "namespace",
                        AtomPropertyValue::from(format!(
                            "{} (QuickTime well-known type)",
                            self.data_namespace,
                        )),
                    ),
                    ("code", AtomPropertyValue::from(well_known_type.code())),
                    ("type", AtomPropertyValue::from(well_known_type.name())),
                    (
                        "comment",
                        AtomPropertyValue::from(well_known_type.comment()),
                    ),
                ],
                DtypArrayValue::ReverseAddress(s) => vec![
                    (
                        "namespace",
                        AtomPropertyValue::from(format!(
                            "{} (reverse-address style UTF-8 string indicating extended data type)",
                            self.data_namespace,
                        )),
                    ),
                    ("value", AtomPropertyValue::from(s)),
                ],
                DtypArrayValue::Unknown(items) => vec![
                    (
                        "namespace",
                        AtomPropertyValue::from(
                            format!("{} (unknown value)", self.data_namespace,),
                        ),
                    ),
                    (
                        "value",
                        AtomPropertyValue::Basic(BasicPropertyValue::Hex(items.to_vec())),
                    ),
                ],
            },
        )
    }
}
