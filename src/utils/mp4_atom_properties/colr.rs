use crate::utils::{
    mp4::Colr,
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
};

impl AtomWithProperties for Colr {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ColourInformationBox",
            properties: match self {
                Colr::Nclx {
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
                Colr::Ricc { profile } => vec![
                    ("colour_type", AtomPropertyValue::from("ricc")),
                    ("profile", AtomPropertyValue::from(profile)),
                ],
                Colr::Prof { profile } => vec![
                    ("colour_type", AtomPropertyValue::from("prof")),
                    ("profile", AtomPropertyValue::from(profile)),
                ],
                Colr::Nclc {
                    primaries_index,
                    transfer_function_index,
                    matrix_index,
                } => vec![
                    ("colour_type", AtomPropertyValue::from("nclc")),
                    ("primaries_index", AtomPropertyValue::from(*primaries_index)),
                    (
                        "transfer_function_index",
                        AtomPropertyValue::from(*transfer_function_index),
                    ),
                    ("matrix_index", AtomPropertyValue::from(*matrix_index)),
                ],
                Colr::Unknown { colour_type, bytes } => vec![
                    ("colour_type", AtomPropertyValue::from(*colour_type)),
                    ("bytes", AtomPropertyValue::from(bytes)),
                ],
            },
        }
    }
}
