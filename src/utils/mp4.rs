use mp4_atom::{Any, FourCC, Header};
use std::io::{Read, Seek};

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
    Usize(usize),
    Bool(bool),
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
            BasicPropertyValue::Usize(u) => format!("{u}"),
            BasicPropertyValue::Bool(b) => format!("{b}"),
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
impl From<String> for BasicPropertyValue {
    fn from(value: String) -> Self {
        Self::String(value)
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

pub fn get_properties_from_atom(header: &Header, atom: &Any) -> AtomProperties {
    let size = AtomPropertyValue::Basic(header.size.map(BasicPropertyValue::Usize).unwrap_or(
        BasicPropertyValue::String(String::from("Extends to end of file")),
    ));
    match atom {
        Any::Ftyp(ftyp) => AtomProperties {
            box_name: "FileTypeBox",
            properties: vec![
                ("size", size),
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
                ("size", size),
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
                ("size", size),
                ("handler", AtomPropertyValue::from(hdlr.handler)),
                ("name", AtomPropertyValue::from(&hdlr.name)),
            ],
        },
        Any::Pitm(pitm) => AtomProperties {
            box_name: "PrimaryItemBox",
            properties: vec![
                ("size", size),
                ("item_id", AtomPropertyValue::from(pitm.item_id)),
            ],
        },
        Any::Iloc(iloc) => AtomProperties {
            box_name: "ItemLocationBox",
            properties: vec![
                ("size", size),
                (
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
                                                    ext.item_reference_index,
                                                    ext.offset,
                                                    ext.length
                                                )
                                            })
                                            .collect::<Vec<String>>()
                                            .join(", "),
                                    ),
                                ]
                            })
                            .collect(),
                    }),
                ),
            ],
        },
        Any::Iinf(iinf) => AtomProperties {
            box_name: "ItemInfoBox",
            properties: vec![
                ("size", size),
                (
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
                ),
            ],
        },
        Any::Auxc(auxc) => AtomProperties {
            box_name: "AuxiliaryTypeProperty",
            properties: vec![
                ("size", size),
                ("aux_type", AtomPropertyValue::from(&auxc.aux_type)),
                ("aux_subtype", AtomPropertyValue::from(&auxc.aux_subtype)),
            ],
        },
        Any::Clap(clap) => AtomProperties {
            box_name: "CleanApertureBox",
            properties: vec![
                ("size", size),
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
            properties: vec![("size", size), ("axis", AtomPropertyValue::from(imir.axis))],
        },
        Any::Irot(irot) => AtomProperties {
            box_name: "ImageRotation",
            properties: vec![
                ("size", size),
                ("angle", AtomPropertyValue::from(irot.angle)),
            ],
        },
        Any::Iscl(iscl) => AtomProperties {
            box_name: "ImageScaling",
            properties: vec![
                ("size", size),
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
                ("size", size),
                ("width", AtomPropertyValue::from(ispe.width)),
                ("height", AtomPropertyValue::from(ispe.height)),
            ],
        },
        Any::Pixi(pixi) => AtomProperties {
            box_name: "PixelInformationProperty",
            properties: vec![
                ("size", size),
                (
                    "bits_per_channel",
                    AtomPropertyValue::from(
                        pixi.bits_per_channel
                            .iter()
                            .map(|bits| format!("{bits}"))
                            .collect::<Vec<String>>()
                            .join(", "),
                    ),
                ),
            ],
        },
        Any::Rref(rref) => AtomProperties {
            box_name: "RequiredReferenceTypesProperty",
            properties: vec![
                ("size", size),
                (
                    "reference_types",
                    AtomPropertyValue::from(&rref.reference_types),
                ),
            ],
        },
        Any::Ipma(ipma) => AtomProperties {
            box_name: "ItemPropertyAssociationBox",
            properties: vec![
                ("size", size),
                (
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
                ),
            ],
        },
        Any::Iref(iref) => AtomProperties {
            box_name: "ItemReferenceBox",
            properties: vec![
                ("size", size),
                (
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
                ),
            ],
        },
        Any::Idat(idat) => AtomProperties {
            box_name: "ItemDataBox",
            properties: vec![
                ("size", size),
                ("data", AtomPropertyValue::from(&idat.data)),
            ],
        },
        Any::Covr(covr) => AtomProperties {
            box_name: "Covr MetadataItem",
            properties: vec![("size", size), ("covr", AtomPropertyValue::from(&covr.0))],
        },
        Any::Desc(desc) => AtomProperties {
            box_name: "Desc MetadataItem",
            properties: vec![("size", size), ("desc", AtomPropertyValue::from(&desc.0))],
        },
        Any::Name(name) => AtomProperties {
            box_name: "Name MetadataItem",
            properties: vec![("size", size), ("name", AtomPropertyValue::from(&name.0))],
        },
        Any::Year(year) => AtomProperties {
            box_name: "Year MetadataItem",
            properties: vec![("size", size), ("year", AtomPropertyValue::from(&year.0))],
        },
        Any::Mvhd(mvhd) => AtomProperties {
            box_name: "MovieHeaderBox",
            properties: vec![
                ("size", size),
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
                ("size", size),
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
                ("size", size),
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
                ("size", size),
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
                            .map(|bytes| vec![byte_array_from(&bytes)])
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
                            .map(|bytes| vec![byte_array_from(&bytes)])
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
                                    .map(|bytes| vec![byte_array_from(&bytes)])
                                    .collect::<Vec<Vec<BasicPropertyValue>>>(),
                            })
                        })
                        .unwrap_or(AtomPropertyValue::Basic(BasicPropertyValue::String(
                            "".to_string(),
                        ))),
                ),
            ],
        },
        Any::Meta(meta) => todo!(), // MetaBox
        Any::Iprp(iprp) => todo!(), // ItemPropertiesBox
        Any::Ipco(ipco) => todo!(), // ItemPropertyContainerBox
        Any::Ilst(ilst) => todo!(), // MetadataItemList
        Any::Moov(moov) => todo!(), // MovieBox
        Any::Udta(udta) => todo!(), // UserDataBox
        Any::Skip(skip) => todo!(), // FreeSpaceBox
        Any::Trak(trak) => todo!(), // TrackBox
        Any::Mdia(mdia) => todo!(), // MediaBox
        Any::Minf(minf) => todo!(), // MediaInformationBox
        Any::Stbl(stbl) => todo!(), // SampleTableBox
        Any::Stsd(stsd) => todo!(), // SampleDescriptionBox
        Any::Avc1(avc1) => todo!(), // AVC1FormatBox
        Any::Btrt(btrt) => todo!(),
        Any::Ccst(ccst) => todo!(),
        Any::Colr(colr) => todo!(),
        Any::Pasp(pasp) => todo!(),
        Any::Taic(taic) => todo!(),
        Any::Hev1(hev1) => todo!(),
        Any::Hvc1(hvc1) => todo!(),
        Any::Hvcc(hvcc) => todo!(),
        Any::Mp4a(mp4a) => todo!(),
        Any::Esds(esds) => todo!(),
        Any::Tx3g(tx3g) => todo!(),
        Any::Vp08(vp08) => todo!(),
        Any::Vp09(vp09) => todo!(),
        Any::VpcC(vpc_c) => todo!(),
        Any::Av01(av01) => todo!(),
        Any::Av1c(av1c) => todo!(),
        Any::Opus(opus) => todo!(),
        Any::Dops(dops) => todo!(),
        Any::Uncv(uncv) => todo!(),
        Any::Cmpd(cmpd) => todo!(),
        Any::UncC(unc_c) => todo!(),
        Any::Stts(stts) => todo!(),
        Any::Stsc(stsc) => todo!(),
        Any::Stsz(stsz) => todo!(),
        Any::Stss(stss) => todo!(),
        Any::Stco(stco) => todo!(),
        Any::Co64(co64) => todo!(),
        Any::Ctts(ctts) => todo!(),
        Any::Saio(saio) => todo!(),
        Any::Saiz(saiz) => todo!(),
        Any::Dinf(dinf) => todo!(),
        Any::Dref(dref) => todo!(),
        Any::Smhd(smhd) => todo!(),
        Any::Vmhd(vmhd) => todo!(),
        Any::Edts(edts) => todo!(),
        Any::Elst(elst) => todo!(),
        Any::Mvex(mvex) => todo!(),
        Any::Mehd(mehd) => todo!(),
        Any::Trex(trex) => todo!(),
        Any::Emsg(emsg) => todo!(),
        Any::Moof(moof) => todo!(),
        Any::Mfhd(mfhd) => todo!(),
        Any::Traf(traf) => todo!(),
        Any::Tfhd(tfhd) => todo!(),
        Any::Tfdt(tfdt) => todo!(),
        Any::Trun(trun) => todo!(),
        Any::Mdat(mdat) => todo!(),
        Any::Free(free) => todo!(),
        Any::Unknown(four_cc, items) => todo!(),
        _ => todo!(),
    }
}

pub struct AtomPropertiesWithDepth {
    pub properties: AtomProperties,
    pub new_depth_until: Option<usize>,
}

pub fn get_properties<R: Read + Seek>(
    header: &Header,
    reader: &mut R,
) -> mp4_atom::Result<AtomPropertiesWithDepth> {
    todo!()
}

pub fn is_known_container(four_cc: FourCC) -> bool {
    todo!()
}

fn byte_array_from(bytes: &[u8]) -> BasicPropertyValue {
    BasicPropertyValue::from(byte_array_string_from(&bytes))
}

fn byte_array_string_from(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:#04x}"))
        .collect::<Vec<String>>()
        .join(" ")
}
