use mp4_atom::{
    Any, AnySampleGroupEntry, Atom, Audio, Buf, Decode, DecodeAtom, FourCC, Header, Visual,
};
use std::{fmt::Display, io::Cursor};

use crate::utils::mp4::Prft;

#[derive(Debug, Clone, PartialEq)]
pub struct AtomProperties {
    pub box_name: &'static str,
    pub properties: Vec<(&'static str, AtomPropertyValue)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AtomPropertyValue {
    Basic(BasicPropertyValue),
    Table(TablePropertyValue),
}
impl<T> From<T> for AtomPropertyValue
where
    BasicPropertyValue: From<T>,
{
    fn from(value: T) -> Self {
        Self::Basic(BasicPropertyValue::from(value))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BasicPropertyValue {
    String(String),
    U64(u64),
    U32(u32),
    U16(u16),
    U8(u8),
    I32(i32),
    I16(i16),
    I8(i8),
    Usize(usize),
    Bool(bool),
    Hex(Vec<u8>),
}
impl BasicPropertyValue {
    pub fn is_hex(&self) -> bool {
        matches!(self, Self::Hex(_))
    }
}
impl From<&BasicPropertyValue> for String {
    fn from(value: &BasicPropertyValue) -> Self {
        match value {
            BasicPropertyValue::String(s) => s.to_string(),
            BasicPropertyValue::U64(u) => format!("{u}"),
            BasicPropertyValue::U32(u) => format!("{u}"),
            BasicPropertyValue::U16(u) => format!("{u}"),
            BasicPropertyValue::U8(u) => format!("{u}"),
            BasicPropertyValue::I32(i) => format!("{i}"),
            BasicPropertyValue::I16(i) => format!("{i}"),
            BasicPropertyValue::I8(i) => format!("{i}"),
            BasicPropertyValue::Usize(u) => format!("{u}"),
            BasicPropertyValue::Bool(b) => format!("{b}"),
            BasicPropertyValue::Hex(bytes) => {
                // Rows of hex - 16 columns to a row
                let mut rows = Vec::new();
                // Columns of hex - 4 sections to a column
                let mut columns = Vec::new();
                // Sections of hex - 4 bytes to a section
                let mut sections = Vec::new();
                for byte in bytes {
                    sections.push(format!("{byte:02X}"));
                    if sections.len() == 4 {
                        columns.push(sections.join(" "));
                        sections.clear();
                        if columns.len() == 4 {
                            rows.push(columns.join("  "));
                            columns.clear();
                        }
                    }
                }
                if !sections.is_empty() {
                    columns.push(sections.join(" "));
                }
                if !columns.is_empty() {
                    rows.push(columns.join("  "));
                }
                rows.join("\n")
            }
        }
    }
}
impl From<FourCC> for BasicPropertyValue {
    fn from(value: FourCC) -> Self {
        Self::String(format!("{value}"))
    }
}
impl From<u64> for BasicPropertyValue {
    fn from(value: u64) -> Self {
        Self::U64(value)
    }
}
impl From<u32> for BasicPropertyValue {
    fn from(value: u32) -> Self {
        Self::U32(value)
    }
}
impl From<u16> for BasicPropertyValue {
    fn from(value: u16) -> Self {
        Self::U16(value)
    }
}
impl From<u8> for BasicPropertyValue {
    fn from(value: u8) -> Self {
        Self::U8(value)
    }
}
impl From<usize> for BasicPropertyValue {
    fn from(value: usize) -> Self {
        Self::Usize(value)
    }
}
impl From<i32> for BasicPropertyValue {
    fn from(value: i32) -> Self {
        Self::I32(value)
    }
}
impl From<i16> for BasicPropertyValue {
    fn from(value: i16) -> Self {
        Self::I16(value)
    }
}
impl From<i8> for BasicPropertyValue {
    fn from(value: i8) -> Self {
        Self::I8(value)
    }
}
impl From<String> for BasicPropertyValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}
impl From<&str> for BasicPropertyValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}
impl From<&String> for BasicPropertyValue {
    fn from(value: &String) -> Self {
        Self::String(value.to_string())
    }
}
impl From<Vec<u8>> for BasicPropertyValue {
    fn from(value: Vec<u8>) -> Self {
        Self::from(&value)
    }
}
impl From<&Vec<u8>> for BasicPropertyValue {
    fn from(value: &Vec<u8>) -> Self {
        Self::String(format!("Data<{}>", value.len()))
    }
}
impl From<bool> for BasicPropertyValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}
impl From<Vec<FourCC>> for BasicPropertyValue {
    fn from(value: Vec<FourCC>) -> Self {
        Self::from(&value)
    }
}
impl From<&Vec<FourCC>> for BasicPropertyValue {
    fn from(value: &Vec<FourCC>) -> Self {
        Self::from(
            value
                .iter()
                .map(|v| format!("{v}"))
                .collect::<Vec<String>>()
                .join(", "),
        )
    }
}
impl<T> From<Option<T>> for BasicPropertyValue
where
    BasicPropertyValue: From<T>,
{
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => BasicPropertyValue::from(value),
            None => BasicPropertyValue::String(String::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TablePropertyValue {
    pub headers: Option<Vec<&'static str>>,
    pub rows: Vec<Vec<BasicPropertyValue>>,
}

pub fn get_properties_from_atom(atom: &Any) -> AtomProperties {
    #[deny(clippy::wildcard_enum_match_arm)]
    match atom {
        Any::Ftyp(ftyp) => AtomProperties {
            box_name: "FileTypeBox",
            properties: vec![
                ("major_brand", AtomPropertyValue::from(ftyp.major_brand)),
                ("minor_version", AtomPropertyValue::from(ftyp.minor_version)),
                (
                    "compatible_brands",
                    AtomPropertyValue::from(&ftyp.compatible_brands),
                ),
            ],
        },
        Any::Styp(styp) => AtomProperties {
            box_name: "SegmentTypeBox",
            properties: vec![
                ("major_brand", AtomPropertyValue::from(styp.major_brand)),
                ("minor_version", AtomPropertyValue::from(styp.minor_version)),
                (
                    "compatible_brands",
                    AtomPropertyValue::from(&styp.compatible_brands),
                ),
            ],
        },
        Any::Hdlr(hdlr) => AtomProperties {
            box_name: "HandlerBox",
            properties: vec![
                ("handler", AtomPropertyValue::from(hdlr.handler)),
                ("name", AtomPropertyValue::from(&hdlr.name)),
            ],
        },
        Any::Pitm(pitm) => AtomProperties {
            box_name: "PrimaryItemBox",
            properties: vec![("item_id", AtomPropertyValue::from(pitm.item_id))],
        },
        Any::Iloc(iloc) => AtomProperties {
            box_name: "ItemLocationBox",
            properties: vec![(
                "item_locations",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec![
                        "item_id",
                        "construction_method",
                        "data_reference_index",
                        "base_offset",
                        "extents",
                    ]),
                    rows: iloc
                        .item_locations
                        .iter()
                        .map(|iloc| {
                            vec![
                                BasicPropertyValue::from(iloc.item_id),
                                BasicPropertyValue::from(iloc.construction_method),
                                BasicPropertyValue::from(iloc.data_reference_index),
                                BasicPropertyValue::from(iloc.base_offset),
                                BasicPropertyValue::from(
                                    iloc.extents
                                        .iter()
                                        .map(|ext| {
                                            format!(
                                                "({},{},{})",
                                                ext.item_reference_index, ext.offset, ext.length
                                            )
                                        })
                                        .collect::<Vec<String>>()
                                        .join(", "),
                                ),
                            ]
                        })
                        .collect(),
                }),
            )],
        },
        Any::Iinf(iinf) => AtomProperties {
            box_name: "ItemInfoBox",
            properties: vec![(
                "item_infos",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec![
                        "item_id",
                        "item_protection_index",
                        "item_type",
                        "item_name",
                        "content_type",
                        "content_encoding",
                        "item_not_in_presentation",
                    ]),
                    rows: iinf
                        .item_infos
                        .iter()
                        .map(|iinf| {
                            vec![
                                BasicPropertyValue::from(iinf.item_id),
                                BasicPropertyValue::from(iinf.item_protection_index),
                                BasicPropertyValue::from(iinf.item_type),
                                BasicPropertyValue::from(&iinf.item_name),
                                BasicPropertyValue::from(iinf.content_type.as_ref()),
                                BasicPropertyValue::from(iinf.content_encoding.as_ref()),
                                BasicPropertyValue::from(iinf.item_not_in_presentation),
                            ]
                        })
                        .collect(),
                }),
            )],
        },
        Any::Auxc(auxc) => AtomProperties {
            box_name: "AuxiliaryTypeProperty",
            properties: vec![
                ("aux_type", AtomPropertyValue::from(&auxc.aux_type)),
                ("aux_subtype", AtomPropertyValue::from(&auxc.aux_subtype)),
            ],
        },
        Any::Clap(clap) => AtomProperties {
            box_name: "CleanApertureBox",
            properties: vec![
                (
                    "clean_aperture_width_n",
                    AtomPropertyValue::from(clap.clean_aperture_width_n),
                ),
                (
                    "clean_aperture_width_d",
                    AtomPropertyValue::from(clap.clean_aperture_width_d),
                ),
                (
                    "clean_aperture_height_n",
                    AtomPropertyValue::from(clap.clean_aperture_height_n),
                ),
                (
                    "clean_aperture_height_d",
                    AtomPropertyValue::from(clap.clean_aperture_height_d),
                ),
                ("horiz_off_n", AtomPropertyValue::from(clap.horiz_off_n)),
                ("horiz_off_d", AtomPropertyValue::from(clap.horiz_off_d)),
                ("vert_off_n", AtomPropertyValue::from(clap.vert_off_n)),
                ("vert_off_d", AtomPropertyValue::from(clap.vert_off_d)),
            ],
        },
        Any::Imir(imir) => AtomProperties {
            box_name: "ImageMirror",
            properties: vec![("axis", AtomPropertyValue::from(imir.axis))],
        },
        Any::Irot(irot) => AtomProperties {
            box_name: "ImageRotation",
            properties: vec![("angle", AtomPropertyValue::from(irot.angle))],
        },
        Any::Iscl(iscl) => AtomProperties {
            box_name: "ImageScaling",
            properties: vec![
                (
                    "target_width_numerator",
                    AtomPropertyValue::from(iscl.target_width_numerator),
                ),
                (
                    "target_width_denominator",
                    AtomPropertyValue::from(iscl.target_width_denominator),
                ),
                (
                    "target_height_numerator",
                    AtomPropertyValue::from(iscl.target_height_numerator),
                ),
                (
                    "target_height_denominator",
                    AtomPropertyValue::from(iscl.target_height_denominator),
                ),
            ],
        },
        Any::Ispe(ispe) => AtomProperties {
            box_name: "ImageSpatialExtentProperty",
            properties: vec![
                ("width", AtomPropertyValue::from(ispe.width)),
                ("height", AtomPropertyValue::from(ispe.height)),
            ],
        },
        Any::Pixi(pixi) => AtomProperties {
            box_name: "PixelInformationProperty",
            properties: vec![(
                "bits_per_channel",
                AtomPropertyValue::from(
                    pixi.bits_per_channel
                        .iter()
                        .map(|bits| format!("{bits}"))
                        .collect::<Vec<String>>()
                        .join(", "),
                ),
            )],
        },
        Any::Rref(rref) => AtomProperties {
            box_name: "RequiredReferenceTypesProperty",
            properties: vec![(
                "reference_types",
                AtomPropertyValue::from(&rref.reference_types),
            )],
        },
        Any::Ipma(ipma) => AtomProperties {
            box_name: "ItemPropertyAssociationBox",
            properties: vec![(
                "item_properties",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec!["item_id", "associations"]),
                    rows: ipma
                        .item_properties
                        .iter()
                        .map(|ipma| {
                            vec![
                                BasicPropertyValue::from(ipma.item_id),
                                BasicPropertyValue::from(
                                    ipma.associations
                                        .iter()
                                        .map(|assoc| {
                                            format!(
                                                "(essential: {}, property_index: {})",
                                                assoc.essential, assoc.property_index
                                            )
                                        })
                                        .collect::<Vec<String>>()
                                        .join(", "),
                                ),
                            ]
                        })
                        .collect(),
                }),
            )],
        },
        Any::Iref(iref) => AtomProperties {
            box_name: "ItemReferenceBox",
            properties: vec![(
                "references",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec!["reference_type", "from_item_id", "to_item_ids"]),
                    rows: iref
                        .references
                        .iter()
                        .map(|iref| {
                            vec![
                                BasicPropertyValue::from(iref.reference_type),
                                BasicPropertyValue::from(iref.from_item_id),
                                BasicPropertyValue::from(
                                    iref.to_item_ids
                                        .iter()
                                        .map(|id| format!("{id}"))
                                        .collect::<Vec<String>>()
                                        .join(", "),
                                ),
                            ]
                        })
                        .collect(),
                }),
            )],
        },
        Any::Idat(idat) => AtomProperties {
            box_name: "ItemDataBox",
            properties: vec![("data", AtomPropertyValue::from(&idat.data))],
        },
        Any::Covr(covr) => AtomProperties {
            box_name: "Covr MetadataItem",
            properties: vec![("covr", AtomPropertyValue::from(&covr.0))],
        },
        Any::Desc(desc) => AtomProperties {
            box_name: "Desc MetadataItem",
            properties: vec![("desc", AtomPropertyValue::from(&desc.0))],
        },
        Any::Name(name) => AtomProperties {
            box_name: "Name MetadataItem",
            properties: vec![("name", AtomPropertyValue::from(&name.0))],
        },
        Any::Year(year) => AtomProperties {
            box_name: "Year MetadataItem",
            properties: vec![("year", AtomPropertyValue::from(&year.0))],
        },
        Any::Mvhd(mvhd) => AtomProperties {
            box_name: "MovieHeaderBox",
            properties: vec![
                ("creation_time", AtomPropertyValue::from(mvhd.creation_time)),
                (
                    "modification_time",
                    AtomPropertyValue::from(mvhd.modification_time),
                ),
                ("timescale", AtomPropertyValue::from(mvhd.timescale)),
                ("duration", AtomPropertyValue::from(mvhd.duration)),
                ("rate", AtomPropertyValue::from(format!("{:?}", mvhd.rate))),
                (
                    "volume",
                    AtomPropertyValue::from(format!("{:?}", mvhd.volume)),
                ),
                (
                    "matrix",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: None,
                        rows: vec![
                            vec![
                                BasicPropertyValue::from(mvhd.matrix.a),
                                BasicPropertyValue::from(mvhd.matrix.b),
                                BasicPropertyValue::from(mvhd.matrix.u),
                            ],
                            vec![
                                BasicPropertyValue::from(mvhd.matrix.c),
                                BasicPropertyValue::from(mvhd.matrix.d),
                                BasicPropertyValue::from(mvhd.matrix.v),
                            ],
                            vec![
                                BasicPropertyValue::from(mvhd.matrix.x),
                                BasicPropertyValue::from(mvhd.matrix.y),
                                BasicPropertyValue::from(mvhd.matrix.w),
                            ],
                        ],
                    }),
                ),
                ("next_track_id", AtomPropertyValue::from(mvhd.next_track_id)),
            ],
        },
        Any::Tkhd(tkhd) => AtomProperties {
            box_name: "TrackHeaderBox",
            properties: vec![
                ("creation_time", AtomPropertyValue::from(tkhd.creation_time)),
                (
                    "modification_time",
                    AtomPropertyValue::from(tkhd.modification_time),
                ),
                ("track_id", AtomPropertyValue::from(tkhd.track_id)),
                ("duration", AtomPropertyValue::from(tkhd.duration)),
                ("layer", AtomPropertyValue::from(tkhd.layer)),
                (
                    "alternate_group",
                    AtomPropertyValue::from(tkhd.alternate_group),
                ),
                ("enabled", AtomPropertyValue::from(tkhd.enabled)),
                (
                    "volume",
                    AtomPropertyValue::from(format!("{:?}", tkhd.volume)),
                ),
                (
                    "matrix",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: None,
                        rows: vec![
                            vec![
                                BasicPropertyValue::from(tkhd.matrix.a),
                                BasicPropertyValue::from(tkhd.matrix.b),
                                BasicPropertyValue::from(tkhd.matrix.u),
                            ],
                            vec![
                                BasicPropertyValue::from(tkhd.matrix.c),
                                BasicPropertyValue::from(tkhd.matrix.d),
                                BasicPropertyValue::from(tkhd.matrix.v),
                            ],
                            vec![
                                BasicPropertyValue::from(tkhd.matrix.x),
                                BasicPropertyValue::from(tkhd.matrix.y),
                                BasicPropertyValue::from(tkhd.matrix.w),
                            ],
                        ],
                    }),
                ),
                (
                    "width",
                    AtomPropertyValue::from(format!("{:?}", tkhd.width)),
                ),
                (
                    "height",
                    AtomPropertyValue::from(format!("{:?}", tkhd.height)),
                ),
            ],
        },
        Any::Mdhd(mdhd) => AtomProperties {
            box_name: "MediaHeaderBox",
            properties: vec![
                ("creation_time", AtomPropertyValue::from(mdhd.creation_time)),
                (
                    "modification_time",
                    AtomPropertyValue::from(mdhd.modification_time),
                ),
                ("timescale", AtomPropertyValue::from(mdhd.timescale)),
                ("duration", AtomPropertyValue::from(mdhd.duration)),
                ("language", AtomPropertyValue::from(&mdhd.language)),
            ],
        },
        Any::Avcc(avcc) => AtomProperties {
            box_name: "AVCConfigurationBox",
            properties: vec![
                (
                    "configuration_version",
                    AtomPropertyValue::from(avcc.configuration_version),
                ),
                (
                    "avc_profile_indication",
                    AtomPropertyValue::from(avcc.avc_profile_indication),
                ),
                (
                    "profile_compatibility",
                    AtomPropertyValue::from(avcc.profile_compatibility),
                ),
                (
                    "avc_level_indication",
                    AtomPropertyValue::from(avcc.avc_level_indication),
                ),
                ("length_size", AtomPropertyValue::from(avcc.length_size)),
                (
                    "sequence_parameter_sets",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: None,
                        rows: avcc
                            .sequence_parameter_sets
                            .iter()
                            .map(|bytes| vec![byte_array_from(bytes)])
                            .collect::<Vec<Vec<BasicPropertyValue>>>(),
                    }),
                ),
                (
                    "picture_parameter_sets",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: None,
                        rows: avcc
                            .picture_parameter_sets
                            .iter()
                            .map(|bytes| vec![byte_array_from(bytes)])
                            .collect::<Vec<Vec<BasicPropertyValue>>>(),
                    }),
                ),
                (
                    "ext_chroma_format",
                    AtomPropertyValue::from(avcc.ext.as_ref().map(|ext| ext.chroma_format)),
                ),
                (
                    "ext_bit_depth_luma",
                    AtomPropertyValue::from(avcc.ext.as_ref().map(|ext| ext.bit_depth_luma)),
                ),
                (
                    "ext_bit_depth_chroma",
                    AtomPropertyValue::from(avcc.ext.as_ref().map(|ext| ext.bit_depth_chroma)),
                ),
                (
                    "ext_sequence_parameter_sets",
                    avcc.ext
                        .as_ref()
                        .map(|ext| {
                            AtomPropertyValue::Table(TablePropertyValue {
                                headers: None,
                                rows: ext
                                    .sequence_parameter_sets_ext
                                    .iter()
                                    .map(|bytes| vec![byte_array_from(bytes)])
                                    .collect::<Vec<Vec<BasicPropertyValue>>>(),
                            })
                        })
                        .unwrap_or(AtomPropertyValue::Basic(BasicPropertyValue::String(
                            "".to_string(),
                        ))),
                ),
            ],
        },
        Any::Btrt(btrt) => AtomProperties {
            box_name: "BitRateBox",
            properties: vec![
                (
                    "buffer_size_db",
                    AtomPropertyValue::from(btrt.buffer_size_db),
                ),
                ("max_bitrate", AtomPropertyValue::from(btrt.max_bitrate)),
                ("avg_bitrate", AtomPropertyValue::from(btrt.avg_bitrate)),
            ],
        },
        Any::Ccst(ccst) => AtomProperties {
            box_name: "CodingConstraintsBox",
            properties: vec![
                (
                    "all_ref_pics_intra",
                    AtomPropertyValue::from(ccst.all_ref_pics_intra),
                ),
                (
                    "intra_pred_used",
                    AtomPropertyValue::from(ccst.intra_pred_used),
                ),
                (
                    "max_ref_per_pic",
                    AtomPropertyValue::from(ccst.max_ref_per_pic),
                ),
            ],
        },
        Any::Colr(colr) => AtomProperties {
            box_name: "ColourInformationBox",
            properties: match colr {
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
        },
        Any::Pasp(pasp) => AtomProperties {
            box_name: "PixelAspectRatioBox",
            properties: vec![
                ("h_spacing", AtomPropertyValue::from(pasp.h_spacing)),
                ("v_spacing", AtomPropertyValue::from(pasp.v_spacing)),
            ],
        },
        Any::Taic(taic) => AtomProperties {
            box_name: "TAIClockInfoBox",
            properties: vec![
                (
                    "time_uncertainty",
                    AtomPropertyValue::from(taic.time_uncertainty),
                ),
                (
                    "clock_resolution",
                    AtomPropertyValue::from(taic.clock_resolution),
                ),
                (
                    "clock_drift_rate",
                    AtomPropertyValue::from(taic.clock_drift_rate),
                ),
                (
                    "clock_type",
                    AtomPropertyValue::from(match taic.clock_type {
                        mp4_atom::ClockType::Unknown => "Unknown",
                        mp4_atom::ClockType::DoesNotSync => "DoesNotSync",
                        mp4_atom::ClockType::CanSync => "CanSync",
                        mp4_atom::ClockType::Reserved => "Reserved",
                    }),
                ),
            ],
        },
        Any::Hvcc(hvcc) => AtomProperties {
            box_name: "HEVCConfigurationBox",
            properties: vec![
                (
                    "configuration_version",
                    AtomPropertyValue::from(hvcc.configuration_version),
                ),
                (
                    "general_profile_space",
                    AtomPropertyValue::from(hvcc.general_profile_space),
                ),
                (
                    "general_tier_flag",
                    AtomPropertyValue::from(hvcc.general_tier_flag),
                ),
                (
                    "general_profile_idc",
                    AtomPropertyValue::from(hvcc.general_profile_idc),
                ),
                (
                    "general_profile_compatibility_flags",
                    AtomPropertyValue::from(array_string_from(
                        &hvcc.general_profile_compatibility_flags,
                    )),
                ),
                (
                    "general_constraint_indicator_flags",
                    AtomPropertyValue::from(array_string_from(
                        &hvcc.general_constraint_indicator_flags,
                    )),
                ),
                (
                    "general_level_idc",
                    AtomPropertyValue::from(hvcc.general_level_idc),
                ),
                (
                    "min_spatial_segmentation_idc",
                    AtomPropertyValue::from(hvcc.min_spatial_segmentation_idc),
                ),
                (
                    "parallelism_type",
                    AtomPropertyValue::from(hvcc.parallelism_type),
                ),
                (
                    "chroma_format_idc",
                    AtomPropertyValue::from(hvcc.chroma_format_idc),
                ),
                (
                    "bit_depth_luma_minus8",
                    AtomPropertyValue::from(hvcc.bit_depth_luma_minus8),
                ),
                (
                    "bit_depth_chroma_minus8",
                    AtomPropertyValue::from(hvcc.bit_depth_chroma_minus8),
                ),
                (
                    "avg_frame_rate",
                    AtomPropertyValue::from(hvcc.avg_frame_rate),
                ),
                (
                    "constant_frame_rate",
                    AtomPropertyValue::from(hvcc.constant_frame_rate),
                ),
                (
                    "num_temporal_layers",
                    AtomPropertyValue::from(hvcc.num_temporal_layers),
                ),
                (
                    "temporal_id_nested",
                    AtomPropertyValue::from(hvcc.temporal_id_nested),
                ),
                (
                    "length_size_minus_one",
                    AtomPropertyValue::from(hvcc.length_size_minus_one),
                ),
                (
                    "arrays",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec!["completeness", "nal_unit_type", "nalus"]),
                        rows: hvcc
                            .arrays
                            .iter()
                            .map(|array| {
                                vec![
                                    BasicPropertyValue::from(array.completeness),
                                    BasicPropertyValue::from(array.nal_unit_type),
                                    byte_array_from(
                                        &array.nalus.iter().flatten().copied().collect::<Vec<u8>>(),
                                    ),
                                ]
                            })
                            .collect(),
                    }),
                ),
            ],
        },
        Any::Esds(esds) => AtomProperties {
            box_name: "ElementaryStreamDescriptorBox",
            properties: vec![
                ("es_id", AtomPropertyValue::from(esds.es_desc.es_id)),
                (
                    "decoder_config_object_type_indication",
                    AtomPropertyValue::from(esds.es_desc.dec_config.object_type_indication),
                ),
                (
                    "decoder_config_stream_type",
                    AtomPropertyValue::from(esds.es_desc.dec_config.stream_type),
                ),
                (
                    "decoder_config_up_stream",
                    AtomPropertyValue::from(esds.es_desc.dec_config.up_stream),
                ),
                (
                    "decoder_config_buffer_size_db",
                    AtomPropertyValue::from(u32::from(esds.es_desc.dec_config.buffer_size_db)),
                ),
                (
                    "decoder_config_max_bitrate",
                    AtomPropertyValue::from(esds.es_desc.dec_config.max_bitrate),
                ),
                (
                    "decoder_config_avg_bitrate",
                    AtomPropertyValue::from(esds.es_desc.dec_config.avg_bitrate),
                ),
                (
                    "decoder_specific_profile",
                    AtomPropertyValue::from(esds.es_desc.dec_config.dec_specific.profile),
                ),
                (
                    "decoder_specific_freq_index",
                    AtomPropertyValue::from(esds.es_desc.dec_config.dec_specific.freq_index),
                ),
                (
                    "decoder_specific_chan_conf",
                    AtomPropertyValue::from(esds.es_desc.dec_config.dec_specific.chan_conf),
                ),
            ],
        },
        Any::Tx3g(tx3g) => AtomProperties {
            box_name: "3GPP Timed Text",
            properties: vec![
                (
                    "data_reference_index",
                    AtomPropertyValue::from(tx3g.data_reference_index),
                ),
                ("display_flags", AtomPropertyValue::from(tx3g.display_flags)),
                (
                    "horizontal_justification",
                    AtomPropertyValue::from(tx3g.horizontal_justification),
                ),
                (
                    "vertical_justification",
                    AtomPropertyValue::from(tx3g.vertical_justification),
                ),
                (
                    "bg_color_rgba",
                    AtomPropertyValue::from(format!(
                        "r:{},g:{},b:{},a:{}",
                        tx3g.bg_color_rgba.red,
                        tx3g.bg_color_rgba.green,
                        tx3g.bg_color_rgba.blue,
                        tx3g.bg_color_rgba.alpha
                    )),
                ),
                (
                    "box_record",
                    AtomPropertyValue::from(format!(
                        "{}, {}, {}, {}",
                        tx3g.box_record[0],
                        tx3g.box_record[1],
                        tx3g.box_record[2],
                        tx3g.box_record[3]
                    )),
                ),
                (
                    "style_record",
                    AtomPropertyValue::from(array_string_from(&tx3g.style_record)),
                ),
            ],
        },
        Any::VpcC(vpc_c) => AtomProperties {
            box_name: "VPCodecConfigurationBox",
            properties: vec![
                ("profile", AtomPropertyValue::from(vpc_c.profile)),
                ("level", AtomPropertyValue::from(vpc_c.level)),
                ("bit_depth", AtomPropertyValue::from(vpc_c.bit_depth)),
                (
                    "chroma_subsampling",
                    AtomPropertyValue::from(vpc_c.chroma_subsampling),
                ),
                (
                    "video_full_range_flag",
                    AtomPropertyValue::from(vpc_c.video_full_range_flag),
                ),
                (
                    "color_primaries",
                    AtomPropertyValue::from(vpc_c.color_primaries),
                ),
                (
                    "transfer_characteristics",
                    AtomPropertyValue::from(vpc_c.transfer_characteristics),
                ),
                (
                    "matrix_coefficients",
                    AtomPropertyValue::from(vpc_c.matrix_coefficients),
                ),
                (
                    "codec_initialization_data",
                    AtomPropertyValue::from(byte_array_from(&vpc_c.codec_initialization_data)),
                ),
            ],
        },
        Any::Av1c(av1c) => AtomProperties {
            box_name: "AV1CodecConfigurationBox",
            properties: vec![
                ("seq_profile", AtomPropertyValue::from(av1c.seq_profile)),
                (
                    "seq_level_idx_0",
                    AtomPropertyValue::from(av1c.seq_level_idx_0),
                ),
                ("seq_tier_0", AtomPropertyValue::from(av1c.seq_tier_0)),
                ("high_bitdepth", AtomPropertyValue::from(av1c.high_bitdepth)),
                ("twelve_bit", AtomPropertyValue::from(av1c.twelve_bit)),
                ("monochrome", AtomPropertyValue::from(av1c.monochrome)),
                (
                    "chroma_subsampling_x",
                    AtomPropertyValue::from(av1c.chroma_subsampling_x),
                ),
                (
                    "chroma_subsampling_y",
                    AtomPropertyValue::from(av1c.chroma_subsampling_y),
                ),
                (
                    "chroma_sample_position",
                    AtomPropertyValue::from(av1c.chroma_sample_position),
                ),
                (
                    "initial_presentation_delay",
                    AtomPropertyValue::from(av1c.initial_presentation_delay),
                ),
                (
                    "config_obus",
                    AtomPropertyValue::from(byte_array_from(&av1c.config_obus)),
                ),
            ],
        },
        Any::Dops(dops) => AtomProperties {
            box_name: "OpusSpecificBox",
            properties: vec![
                (
                    "output_channel_count",
                    AtomPropertyValue::from(dops.output_channel_count),
                ),
                ("pre_skip", AtomPropertyValue::from(dops.pre_skip)),
                (
                    "input_sample_rate",
                    AtomPropertyValue::from(dops.input_sample_rate),
                ),
                ("output_gain", AtomPropertyValue::from(dops.output_gain)),
            ],
        },
        Any::Cmpd(cmpd) => AtomProperties {
            box_name: "ComponentDefinitionBox",
            properties: vec![(
                "components",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec!["type", "type_uri"]),
                    rows: cmpd
                        .components
                        .iter()
                        .map(|c| {
                            vec![
                                BasicPropertyValue::from(c.component_type),
                                BasicPropertyValue::from(c.component_type_uri.as_ref()),
                            ]
                        })
                        .collect(),
                }),
            )],
        },
        Any::UncC(unc_c) => AtomProperties {
            box_name: "UncompressedFrameConfigBox",
            properties: match unc_c {
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
        },
        Any::Stts(stts) => AtomProperties {
            box_name: "TimeToSampleBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec!["count", "delta"]),
                    rows: stts
                        .entries
                        .iter()
                        .map(|entry| {
                            vec![
                                BasicPropertyValue::from(entry.sample_count),
                                BasicPropertyValue::from(entry.sample_delta),
                            ]
                        })
                        .collect(),
                }),
            )],
        },
        Any::Stsc(stsc) => AtomProperties {
            box_name: "SampleToChunkBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec![
                        "first_chunk",
                        "samples_per_chunk",
                        "sample_description_index",
                    ]),
                    rows: stsc
                        .entries
                        .iter()
                        .map(|entry| {
                            vec![
                                BasicPropertyValue::from(entry.first_chunk),
                                BasicPropertyValue::from(entry.samples_per_chunk),
                                BasicPropertyValue::from(entry.sample_description_index),
                            ]
                        })
                        .collect(),
                }),
            )],
        },
        Any::Stsz(stsz) => AtomProperties {
            box_name: "SampleSizeBox",
            properties: vec![
                (
                    "sample_count",
                    match &stsz.samples {
                        mp4_atom::StszSamples::Identical { count, size: _ } => {
                            AtomPropertyValue::from(*count)
                        }
                        mp4_atom::StszSamples::Different { sizes } => {
                            AtomPropertyValue::from(sizes.len())
                        }
                    },
                ),
                match &stsz.samples {
                    mp4_atom::StszSamples::Identical { count: _, size } => {
                        ("sample_size", AtomPropertyValue::from(*size))
                    }
                    mp4_atom::StszSamples::Different { sizes } => (
                        "sample_sizes",
                        AtomPropertyValue::from(array_string_from(sizes)),
                    ),
                },
            ],
        },
        Any::Stss(stss) => AtomProperties {
            box_name: "SyncSampleBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::from(array_string_from(&stss.entries)),
            )],
        },
        Any::Stco(stco) => AtomProperties {
            box_name: "ChunkOffsetBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::from(array_string_from(&stco.entries)),
            )],
        },
        Any::Co64(co64) => AtomProperties {
            box_name: "ChunkLargeOffsetBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::from(array_string_from(&co64.entries)),
            )],
        },
        Any::Ctts(ctts) => AtomProperties {
            box_name: "CompositionOffsetBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::from(
                    ctts.entries
                        .iter()
                        .map(|entry| {
                            format!(
                                "(count: {}, offset: {})",
                                entry.sample_count, entry.sample_offset
                            )
                        })
                        .collect::<Vec<String>>()
                        .join(", "),
                ),
            )],
        },
        Any::Sbgp(sbgp) => AtomProperties {
            box_name: "SampleToGroupBox",
            properties: vec![
                ("grouping_type", AtomPropertyValue::from(sbgp.grouping_type)),
                (
                    "grouping_type_parameter",
                    AtomPropertyValue::from(sbgp.grouping_type_parameter),
                ),
                (
                    "entries",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec!["sample_count", "group_description_index"]),
                        rows: sbgp
                            .entries
                            .iter()
                            .map(|entry| {
                                vec![
                                    BasicPropertyValue::from(entry.sample_count),
                                    BasicPropertyValue::from(entry.group_description_index),
                                ]
                            })
                            .collect(),
                    }),
                ),
            ],
        },
        Any::Sgpd(sgpd) => AtomProperties {
            box_name: "SampleGroupDescriptionBox",
            properties: vec![
                ("grouping_type", AtomPropertyValue::from(sgpd.grouping_type)),
                (
                    "default_length",
                    AtomPropertyValue::from(sgpd.default_length),
                ),
                (
                    "default_group_description_index",
                    AtomPropertyValue::from(sgpd.default_group_description_index),
                ),
                (
                    "static_group_description",
                    AtomPropertyValue::from(sgpd.static_group_description),
                ),
                (
                    "static_mapping",
                    AtomPropertyValue::from(sgpd.static_mapping),
                ),
                ("essential", AtomPropertyValue::from(sgpd.essential)),
                (
                    "entries",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec!["description_length", "4CC", "data"]),
                        rows: sgpd
                            .entries
                            .iter()
                            .map(|entry| match &entry.entry {
                                AnySampleGroupEntry::UnknownGroupingType(four_cc, items) => vec![
                                    BasicPropertyValue::from(entry.description_length),
                                    BasicPropertyValue::from(*four_cc),
                                    BasicPropertyValue::Hex(items.clone()),
                                ],
                            })
                            .collect(),
                    }),
                ),
            ],
        },
        Any::Subs(subs) => AtomProperties {
            box_name: "SubSampleInformationBox",
            properties: vec![
                (
                    "flags",
                    AtomPropertyValue::from(BasicPropertyValue::Hex(subs.flags.to_vec())),
                ),
                (
                    "entries",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec![
                            "sample_delta",
                            "size",
                            "priority",
                            "discardable",
                            "params",
                        ]),
                        rows: subs
                            .entries
                            .iter()
                            .map(|entry| {
                                entry
                                    .subsamples
                                    .iter()
                                    .flat_map(|subsample| {
                                        vec![
                                            BasicPropertyValue::from(entry.sample_delta),
                                            BasicPropertyValue::from(subsample.size.value()),
                                            BasicPropertyValue::from(subsample.priority),
                                            BasicPropertyValue::from(subsample.discardable),
                                            BasicPropertyValue::Hex(
                                                subsample.codec_specific_parameters.clone(),
                                            ),
                                        ]
                                    })
                                    .collect()
                            })
                            .collect(),
                    }),
                ),
            ],
        },
        Any::Saio(saio) => AtomProperties {
            box_name: "SampleAuxiliaryInformationOffsetsBox",
            properties: vec![
                (
                    "aux_info_type",
                    AtomPropertyValue::from(saio.aux_info.as_ref().map(|a| a.aux_info_type)),
                ),
                (
                    "aux_info_type_parameter",
                    AtomPropertyValue::from(
                        saio.aux_info.as_ref().map(|a| a.aux_info_type_parameter),
                    ),
                ),
                (
                    "offsets",
                    AtomPropertyValue::from(array_string_from(&saio.offsets)),
                ),
            ],
        },
        Any::Saiz(saiz) => AtomProperties {
            box_name: "SampleAuxiliaryInformationSizesBox",
            properties: vec![
                (
                    "aux_info_type",
                    AtomPropertyValue::from(saiz.aux_info.as_ref().map(|a| a.aux_info_type)),
                ),
                (
                    "aux_info_type_parameter",
                    AtomPropertyValue::from(
                        saiz.aux_info.as_ref().map(|a| a.aux_info_type_parameter),
                    ),
                ),
                (
                    "default_sample_info_size",
                    AtomPropertyValue::from(saiz.default_sample_info_size),
                ),
                ("sample_count", AtomPropertyValue::from(saiz.sample_count)),
                (
                    "sample_info_size",
                    AtomPropertyValue::from(array_string_from(&saiz.sample_info_size)),
                ),
            ],
        },
        Any::Dref(dref) => AtomProperties {
            box_name: "DataReferenceBox",
            properties: vec![(
                "urls",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: None,
                    rows: dref
                        .urls
                        .iter()
                        .map(|url| vec![BasicPropertyValue::from(&url.location)])
                        .collect(),
                }),
            )],
        },
        Any::Smhd(smhd) => AtomProperties {
            box_name: "SoundMediaHeaderBox",
            properties: vec![(
                "balance",
                AtomPropertyValue::from(format!("{:?}", smhd.balance)),
            )],
        },
        Any::Vmhd(vmhd) => AtomProperties {
            box_name: "VideoMediaHeaderBox",
            properties: vec![
                ("graphics_mode", AtomPropertyValue::from(vmhd.graphics_mode)),
                (
                    "op_color",
                    AtomPropertyValue::from(format!(
                        "r:{}, g:{}, b:{}",
                        vmhd.op_color.red, vmhd.op_color.green, vmhd.op_color.blue
                    )),
                ),
            ],
        },
        Any::Elst(elst) => AtomProperties {
            box_name: "EditListBox",
            properties: vec![(
                "entries",
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: Some(vec![
                        "segment_duration",
                        "media_time",
                        "media_rate",
                        "media_rate_fraction",
                    ]),
                    rows: elst
                        .entries
                        .iter()
                        .map(|entry| {
                            vec![
                                BasicPropertyValue::from(entry.segment_duration),
                                BasicPropertyValue::from(entry.media_time),
                                BasicPropertyValue::from(entry.media_rate),
                                BasicPropertyValue::from(entry.media_rate_fraction),
                            ]
                        })
                        .collect(),
                }),
            )],
        },
        Any::Mehd(mehd) => AtomProperties {
            box_name: "MovieExtendsHeaderBox",
            properties: vec![(
                "fragment_duration",
                AtomPropertyValue::from(mehd.fragment_duration),
            )],
        },
        Any::Trex(trex) => AtomProperties {
            box_name: "TrackExtendsBox",
            properties: vec![
                ("track_id", AtomPropertyValue::from(trex.track_id)),
                (
                    "default_sample_description_index",
                    AtomPropertyValue::from(trex.default_sample_description_index),
                ),
                (
                    "default_sample_duration",
                    AtomPropertyValue::from(trex.default_sample_duration),
                ),
                (
                    "default_sample_size",
                    AtomPropertyValue::from(trex.default_sample_size),
                ),
                (
                    "default_sample_flags",
                    AtomPropertyValue::from(trex.default_sample_flags),
                ),
            ],
        },
        Any::Emsg(emsg) => {
            let message_data = if &emsg.scheme_id_uri == "https://aomedia.org/emsg/ID3" {
                let message_data_reader = std::io::Cursor::new(emsg.message_data.clone());
                match id3::Tag::read_from2(message_data_reader) {
                    Ok(id3_tag) => {
                        let mut tags = Vec::new();
                        for frame in id3_tag.frames() {
                            let id = frame.id();
                            let value = format!("{}", frame.content());
                            tags.push((id, value));
                        }
                        AtomPropertyValue::Table(TablePropertyValue {
                            headers: Some(vec!["id3 frame ID", "Value"]),
                            rows: tags
                                .iter()
                                .map(|(name, value)| {
                                    vec![
                                        BasicPropertyValue::from(*name),
                                        BasicPropertyValue::from(value),
                                    ]
                                })
                                .collect(),
                        })
                    }
                    Err(_) => AtomPropertyValue::from(
                        String::from_utf8_lossy(&emsg.message_data).to_string(),
                    ),
                }
            } else {
                AtomPropertyValue::from(String::from_utf8_lossy(&emsg.message_data).to_string())
            };
            AtomProperties {
                box_name: "EventMessageBox",
                properties: vec![
                    ("timescale", AtomPropertyValue::from(emsg.timescale)),
                    match emsg.presentation_time {
                        mp4_atom::EmsgTimestamp::Relative(t) => {
                            ("presentation_time_delta", AtomPropertyValue::from(t))
                        }
                        mp4_atom::EmsgTimestamp::Absolute(t) => {
                            ("presentation_time", AtomPropertyValue::from(t))
                        }
                    },
                    (
                        "event_duration",
                        AtomPropertyValue::from(emsg.event_duration),
                    ),
                    ("id", AtomPropertyValue::from(emsg.id)),
                    (
                        "scheme_id_uri",
                        AtomPropertyValue::from(&emsg.scheme_id_uri),
                    ),
                    ("value", AtomPropertyValue::from(&emsg.value)),
                    ("message_data", message_data),
                ],
            }
        }
        Any::Mfhd(mfhd) => AtomProperties {
            box_name: "MovieFragmentHeaderBox",
            properties: vec![(
                "sequence_number",
                AtomPropertyValue::from(mfhd.sequence_number),
            )],
        },
        Any::Tfhd(tfhd) => AtomProperties {
            box_name: "TrackFragmentHeaderBox",
            properties: vec![
                ("track_id", AtomPropertyValue::from(tfhd.track_id)),
                (
                    "base_data_offset",
                    AtomPropertyValue::from(tfhd.base_data_offset),
                ),
                (
                    "sample_description_index",
                    AtomPropertyValue::from(tfhd.sample_description_index),
                ),
                (
                    "default_sample_duration",
                    AtomPropertyValue::from(tfhd.default_sample_duration),
                ),
                (
                    "default_sample_size",
                    AtomPropertyValue::from(tfhd.default_sample_size),
                ),
                (
                    "default_sample_flags",
                    AtomPropertyValue::from(tfhd.default_sample_flags),
                ),
            ],
        },
        Any::Tfdt(tfdt) => AtomProperties {
            box_name: "TrackFragmentBaseMediaDecodeTimeBox",
            properties: vec![(
                "base_media_decode_time",
                AtomPropertyValue::from(tfdt.base_media_decode_time),
            )],
        },
        Any::Trun(trun) => AtomProperties {
            box_name: "TrackRunBox",
            properties: vec![
                ("data_offset", AtomPropertyValue::from(trun.data_offset)),
                (
                    "entries",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec!["duration", "size", "flags", "cts"]),
                        rows: trun
                            .entries
                            .iter()
                            .map(|entry| {
                                vec![
                                    BasicPropertyValue::from(entry.duration),
                                    BasicPropertyValue::from(entry.size),
                                    BasicPropertyValue::from(entry.flags),
                                    BasicPropertyValue::from(entry.cts),
                                ]
                            })
                            .collect(),
                    }),
                ),
            ],
        },
        Any::Skip(_) => AtomProperties {
            box_name: "FreeSpaceBox",
            properties: vec![],
        },
        Any::Free(_) => AtomProperties {
            box_name: "FreeSpaceBox",
            properties: vec![],
        },
        Any::Unknown(_, items) => AtomProperties {
            box_name: "Unknown (unhandled box parsing)",
            properties: vec![("data", AtomPropertyValue::from(items))],
        },
        Any::Meta(_) => unimplemented!(), // MetaBox
        Any::Iprp(_) => unimplemented!(), // ItemPropertiesBox
        Any::Ipco(_) => unimplemented!(), // ItemPropertyContainerBox
        Any::Ilst(_) => unimplemented!(), // MetadataItemList
        Any::Moov(_) => unimplemented!(), // MovieBox
        Any::Udta(_) => unimplemented!(), // UserDataBox
        Any::Trak(_) => unimplemented!(), // TrackBox
        Any::Mdia(_) => unimplemented!(), // MediaBox
        Any::Minf(_) => unimplemented!(), // MediaInformationBox
        Any::Stbl(_) => unimplemented!(), // SampleTableBox
        Any::Stsd(_) => unimplemented!(), // SampleDescriptionBox
        Any::Avc1(_) => unimplemented!(), // AVCSampleEntryBox
        Any::Hev1(_) => unimplemented!(), // HEVCSampleEntryBox
        Any::Hvc1(_) => unimplemented!(), // HEVCSampleEntryBox
        Any::Mp4a(_) => unimplemented!(), // MP4AudioSampleEntryBox
        Any::Vp08(_) => unimplemented!(), // VP08SampleEntryBox
        Any::Vp09(_) => unimplemented!(), // VP09SampleEntryBox
        Any::Av01(_) => unimplemented!(), // AV1SampleEntryBox
        Any::Opus(_) => unimplemented!(), // OpusSampleEntryBox
        Any::Uncv(_) => unimplemented!(), // UncompressedFrameSampleEntryBox
        Any::Dinf(_) => unimplemented!(), // DataInformationBox
        Any::Edts(_) => unimplemented!(), // EditBox
        Any::Mvex(_) => unimplemented!(), // MovieExtendsBox
        Any::Moof(_) => unimplemented!(), // MovieFragmentBox
        Any::Traf(_) => unimplemented!(), // TrackFragmentBox
        Any::Mdat(_) => unimplemented!(), // MediaDataBox
        unknown => todo!("missing props for {unknown:?}"),
    }
}

pub struct AtomPropertiesWithDepth {
    pub properties: AtomProperties,
    pub new_depth_until: Option<u64>,
}

pub fn get_properties(
    header: &Header,
    reader: &mut Cursor<Vec<u8>>,
) -> mp4_atom::Result<AtomPropertiesWithDepth> {
    let size = AtomPropertyValue::Basic(
        header
            .size
            .map(|size| BasicPropertyValue::Usize(size + 8)) // (FourCC=4 + size=4 == 8)
            .unwrap_or(BasicPropertyValue::String(String::from(
                "Extends to end of file",
            ))),
    );
    let mut properties = match header.kind {
        // Container boxes
        mp4_atom::Meta::KIND => container(header, "MetaBox", reader),
        mp4_atom::Iprp::KIND => container(header, "ItemPropertiesBox", reader),
        mp4_atom::Ipco::KIND => container(header, "ItemPropertyContainerBox", reader),
        mp4_atom::Ilst::KIND => container(header, "MetadataItemList", reader),
        mp4_atom::Moov::KIND => container(header, "MovieBox", reader),
        mp4_atom::Udta::KIND => container(header, "UserDataBox", reader),
        mp4_atom::Trak::KIND => container(header, "TrackBox", reader),
        mp4_atom::Mdia::KIND => container(header, "MediaBox", reader),
        mp4_atom::Minf::KIND => container(header, "MediaInformationBox", reader),
        mp4_atom::Stbl::KIND => container(header, "SampleTableBox", reader),
        mp4_atom::Stsd::KIND => container(header, "SampleDescriptionBox", reader),
        mp4_atom::Dinf::KIND => container(header, "DataInformationBox", reader),
        mp4_atom::Edts::KIND => container(header, "EditBox", reader),
        mp4_atom::Mvex::KIND => container(header, "MovieExtendsBox", reader),
        mp4_atom::Moof::KIND => container(header, "MovieFragmentBox", reader),
        mp4_atom::Traf::KIND => container(header, "TrackFragmentBox", reader),
        mp4_atom::Avc1::KIND => visual_entry(header, "AVCSampleEntryBox", reader),
        mp4_atom::Hev1::KIND => visual_entry(header, "HEVCSampleEntryBox", reader),
        mp4_atom::Hvc1::KIND => visual_entry(header, "HEVCSampleEntryBox", reader),
        mp4_atom::Vp08::KIND => visual_entry(header, "VP08SampleEntryBox", reader),
        mp4_atom::Vp09::KIND => visual_entry(header, "VP09SampleEntryBox", reader),
        mp4_atom::Av01::KIND => visual_entry(header, "AV1SampleEntryBox", reader),
        mp4_atom::Uncv::KIND => visual_entry(header, "UncompressedFrameSampleEntryBox", reader),
        mp4_atom::Mp4a::KIND => audio_entry(header, "MP4AudioSampleEntryBox", reader),
        mp4_atom::Opus::KIND => audio_entry(header, "OpusSampleEntryBox", reader),
        mp4_atom::Mdat::KIND => {
            let remaining_box_size = header.size.unwrap_or_else(|| reader.remaining());
            reader.set_position(reader.position() + (remaining_box_size as u64));
            Ok(AtomPropertiesWithDepth {
                properties: AtomProperties {
                    box_name: "MediaDataBox",
                    properties: vec![],
                },
                new_depth_until: None,
            })
        }
        // Custom atoms implemented in this lib
        Prft::KIND => {
            let atom = Prft::decode_atom(header, reader)?;
            Ok(AtomPropertiesWithDepth {
                properties: AtomProperties {
                    box_name: "ProducerReferenceTimeBox",
                    properties: vec![
                        (
                            "reference_track_id",
                            AtomPropertyValue::from(atom.reference_track_id),
                        ),
                        ("ntp_timestamp", AtomPropertyValue::from(atom.ntp_timestamp)),
                        ("media_time", AtomPropertyValue::from(atom.media_time)),
                        (
                            "ntp_timestamp_media_time_association",
                            AtomPropertyValue::from(format!(
                                "{}",
                                atom.ntp_timestamp_media_time_association
                            )),
                        ),
                    ],
                },
                new_depth_until: None,
            })
        }
        // Everything else
        _ => {
            let atom = Any::decode_atom(header, reader)?;
            let properties = get_properties_from_atom(&atom);
            Ok(AtomPropertiesWithDepth {
                properties,
                new_depth_until: None,
            })
        }
    }?;
    // Wow... I'm really bad at naming things
    properties.properties.properties.insert(0, ("size", size));
    Ok(properties)
}

fn decode_container_version_and_flags(
    header: &Header,
    reader: &mut Cursor<Vec<u8>>,
) -> mp4_atom::Result<Vec<(&'static str, AtomPropertyValue)>> {
    match header.kind {
        // Known full boxes that are also containers
        mp4_atom::Meta::KIND | mp4_atom::Stsd::KIND => {
            let version = u8::decode(reader)?;
            let flags = [
                u8::decode(reader)?,
                u8::decode(reader)?,
                u8::decode(reader)?,
            ];
            if header.kind == mp4_atom::Stsd::KIND {
                // The number of entries in the `stsd` is read from the container box
                // > unsigned int(32) entry_count;
                _ = u32::decode(reader)?;
            }
            Ok(vec![
                ("version", AtomPropertyValue::from(version)),
                (
                    "flags",
                    AtomPropertyValue::from(
                        flags
                            .iter()
                            .map(|byte| format!("{byte:08b}"))
                            .collect::<Vec<String>>()
                            .join(" "),
                    ),
                ),
            ])
        }
        // Everything else
        _ => Ok(vec![]),
    }
}

fn container(
    header: &Header,
    name: &'static str,
    reader: &mut Cursor<Vec<u8>>,
) -> mp4_atom::Result<AtomPropertiesWithDepth> {
    let header_size = header.size.unwrap_or_else(|| reader.remaining());
    let new_depth_until = reader.position() + (header_size as u64);
    let version_and_flags = decode_container_version_and_flags(header, reader)?;
    Ok(AtomPropertiesWithDepth {
        properties: AtomProperties {
            box_name: name,
            properties: version_and_flags,
        },
        new_depth_until: Some(new_depth_until),
    })
}

fn visual_entry(
    header: &Header,
    name: &'static str,
    reader: &mut Cursor<Vec<u8>>,
) -> mp4_atom::Result<AtomPropertiesWithDepth> {
    let header_size = header.size.unwrap_or_else(|| reader.remaining());
    let new_depth_until = reader.position() + (header_size as u64);

    let visual = Visual::decode(reader)?;
    Ok(AtomPropertiesWithDepth {
        properties: AtomProperties {
            box_name: name,
            properties: vec![
                (
                    "data_reference_index",
                    AtomPropertyValue::from(visual.data_reference_index),
                ),
                ("width", AtomPropertyValue::from(visual.width)),
                ("height", AtomPropertyValue::from(visual.height)),
                (
                    "horizresolution",
                    AtomPropertyValue::from(format!("{:?}", visual.horizresolution)),
                ),
                (
                    "vertresolution",
                    AtomPropertyValue::from(format!("{:?}", visual.vertresolution)),
                ),
                ("frame_count", AtomPropertyValue::from(visual.frame_count)),
                (
                    "compressor",
                    AtomPropertyValue::from(String::from(visual.compressor)),
                ),
                ("depth", AtomPropertyValue::from(visual.depth)),
            ],
        },
        new_depth_until: Some(new_depth_until),
    })
}

fn audio_entry(
    header: &Header,
    name: &'static str,
    reader: &mut Cursor<Vec<u8>>,
) -> mp4_atom::Result<AtomPropertiesWithDepth> {
    let header_size = header.size.unwrap_or_else(|| reader.remaining());
    let new_depth_until = reader.position() + (header_size as u64);

    let audio = Audio::decode(reader)?;
    Ok(AtomPropertiesWithDepth {
        properties: AtomProperties {
            box_name: name,
            properties: vec![
                (
                    "data_reference_index",
                    AtomPropertyValue::from(audio.data_reference_index),
                ),
                (
                    "channel_count",
                    AtomPropertyValue::from(audio.channel_count),
                ),
                ("sample_size", AtomPropertyValue::from(audio.sample_size)),
                (
                    "sample_rate",
                    AtomPropertyValue::from(format!("{:?}", audio.sample_rate)),
                ),
            ],
        },
        new_depth_until: Some(new_depth_until),
    })
}

fn byte_array_from(bytes: &[u8]) -> BasicPropertyValue {
    BasicPropertyValue::Hex(bytes.to_vec())
}

fn array_string_from<T: Display>(items: &[T]) -> String {
    items
        .iter()
        .map(|item| format!("{item}"))
        .collect::<Vec<String>>()
        .join(", ")
}
