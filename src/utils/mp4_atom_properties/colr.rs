use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Colr;

impl AtomWithProperties for Colr {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ColourInformationBox",
            properties: match self {
                mp4_atom::Colr::Nclx {
                    colour_primaries,
                    transfer_characteristics,
                    matrix_coefficients,
                    full_range_flag,
                } => vec![
                    ("colour_type", AtomPropertyValue::from("nclx")),
                    (
                        "colour_primaries",
                        AtomPropertyValue::from(*colour_primaries),
                    ),
                    (
                        "transfer_characteristics",
                        AtomPropertyValue::from(*transfer_characteristics),
                    ),
                    (
                        "matrix_coefficients",
                        AtomPropertyValue::from(*matrix_coefficients),
                    ),
                    ("full_range_flag", AtomPropertyValue::from(*full_range_flag)),
                ],
                mp4_atom::Colr::Ricc { profile } => vec![
                    ("colour_type", AtomPropertyValue::from("ricc")),
                    ("profile", AtomPropertyValue::from(profile)),
                ],
                mp4_atom::Colr::Prof { profile } => vec![
                    ("colour_type", AtomPropertyValue::from("prof")),
                    ("profile", AtomPropertyValue::from(profile)),
                ],
            },
        }
    }
}
