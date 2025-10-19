use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::UncC;

impl AtomWithProperties for UncC {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "UncompressedFrameConfigBox",
            properties: match self {
                mp4_atom::UncC::V1 { profile } => {
                    vec![("profile", AtomPropertyValue::from(*profile))]
                }
                mp4_atom::UncC::V0 {
                    profile,
                    components,
                    sampling_type,
                    interleave_type,
                    block_size,
                    components_little_endian,
                    block_pad_lsb,
                    block_little_endian,
                    block_reversed,
                    pad_unknown,
                    pixel_size,
                    row_align_size,
                    tile_align_size,
                    num_tile_cols_minus_one,
                    num_tile_rows_minus_one,
                } => vec![
                    ("profile", AtomPropertyValue::from(*profile)),
                    (
                        "components",
                        AtomPropertyValue::Table(TablePropertyValue {
                            headers: Some(vec![
                                "index",
                                "bit_depth_minus_one",
                                "format",
                                "align_size",
                            ]),
                            rows: components
                                .iter()
                                .map(|c| {
                                    vec![
                                        BasicPropertyValue::from(c.component_index),
                                        BasicPropertyValue::from(c.component_bit_depth_minus_one),
                                        BasicPropertyValue::from(c.component_format),
                                        BasicPropertyValue::from(c.component_align_size),
                                    ]
                                })
                                .collect(),
                        }),
                    ),
                    ("sampling_type", AtomPropertyValue::from(*sampling_type)),
                    ("interleave_type", AtomPropertyValue::from(*interleave_type)),
                    ("block_size", AtomPropertyValue::from(*block_size)),
                    (
                        "components_little_endian",
                        AtomPropertyValue::from(*components_little_endian),
                    ),
                    ("block_pad_lsb", AtomPropertyValue::from(*block_pad_lsb)),
                    (
                        "block_little_endian",
                        AtomPropertyValue::from(*block_little_endian),
                    ),
                    ("block_reversed", AtomPropertyValue::from(*block_reversed)),
                    ("pad_unknown", AtomPropertyValue::from(*pad_unknown)),
                    ("pixel_size", AtomPropertyValue::from(*pixel_size)),
                    ("row_align_size", AtomPropertyValue::from(*row_align_size)),
                    ("tile_align_size", AtomPropertyValue::from(*tile_align_size)),
                    (
                        "num_tile_cols_minus_one",
                        AtomPropertyValue::from(*num_tile_cols_minus_one),
                    ),
                    (
                        "num_tile_rows_minus_one",
                        AtomPropertyValue::from(*num_tile_rows_minus_one),
                    ),
                ],
            },
        }
    }
}
