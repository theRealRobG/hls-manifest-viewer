use crate::utils::mp4_parsing::{Colr, Frma, Lac4, Prft, Pssh, Schm, Senc, Tenc};
use mp4_atom::{Any, Atom, Audio, Buf, Decode, DecodeAtom, FourCC, Header, Visual};
use std::{fmt::Display, io::Cursor};

mod auxc;
mod av01;
mod av1c;
mod avc1;
mod avcc;
mod btrt;
mod ccst;
mod clap;
mod cmpd;
mod co64;
mod colr;
mod covr;
mod ctts;
mod desc;
mod dinf;
mod dops;
mod dref;
mod edts;
mod elst;
mod emsg;
mod esds;
mod free;
mod frma;
mod ftyp;
mod hdlr;
mod hev1;
mod hvc1;
mod hvcc;
mod idat;
mod iinf;
mod iloc;
mod ilst;
mod imir;
mod ipco;
mod ipma;
mod iprp;
mod iref;
mod irot;
mod iscl;
mod ispe;
mod lac4;
mod mdat;
mod mdhd;
mod mdia;
mod mehd;
mod meta;
mod mfhd;
mod minf;
mod moof;
mod moov;
mod mp4a;
mod mvex;
mod mvhd;
mod name;
mod opus;
mod pasp;
mod pitm;
mod pixi;
mod prft;
mod pssh;
mod rref;
mod saio;
mod saiz;
mod sbgp;
mod schm;
mod senc;
mod sgpd;
mod skip;
mod smhd;
mod stbl;
mod stco;
mod stsc;
mod stsd;
mod stss;
mod stsz;
mod stts;
mod styp;
mod subs;
mod taic;
mod tenc;
mod tfdt;
mod tfhd;
mod tkhd;
mod traf;
mod trak;
mod trex;
mod trun;
mod tx3g;
mod udta;
mod uncc;
mod uncv;
mod vmhd;
mod vp08;
mod vp09;
mod vpcc;
mod year;

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

trait AtomWithProperties {
    fn properties(&self) -> AtomProperties;
}

pub fn get_properties_from_atom(atom: &Any) -> AtomProperties {
    #[deny(clippy::wildcard_enum_match_arm)]
    match atom {
        Any::Ftyp(ftyp) => ftyp.properties(),
        Any::Styp(styp) => styp.properties(),
        Any::Hdlr(hdlr) => hdlr.properties(),
        Any::Pitm(pitm) => pitm.properties(),
        Any::Iloc(iloc) => iloc.properties(),
        Any::Iinf(iinf) => iinf.properties(),
        Any::Auxc(auxc) => auxc.properties(),
        Any::Clap(clap) => clap.properties(),
        Any::Imir(imir) => imir.properties(),
        Any::Irot(irot) => irot.properties(),
        Any::Iscl(iscl) => iscl.properties(),
        Any::Ispe(ispe) => ispe.properties(),
        Any::Pixi(pixi) => pixi.properties(),
        Any::Rref(rref) => rref.properties(),
        Any::Ipma(ipma) => ipma.properties(),
        Any::Iref(iref) => iref.properties(),
        Any::Idat(idat) => idat.properties(),
        Any::Covr(covr) => covr.properties(),
        Any::Desc(desc) => desc.properties(),
        Any::Name(name) => name.properties(),
        Any::Year(year) => year.properties(),
        Any::Mvhd(mvhd) => mvhd.properties(),
        Any::Tkhd(tkhd) => tkhd.properties(),
        Any::Mdhd(mdhd) => mdhd.properties(),
        Any::Avcc(avcc) => avcc.properties(),
        Any::Btrt(btrt) => btrt.properties(),
        Any::Ccst(ccst) => ccst.properties(),
        Any::Pasp(pasp) => pasp.properties(),
        Any::Taic(taic) => taic.properties(),
        Any::Hvcc(hvcc) => hvcc.properties(),
        Any::Esds(esds) => esds.properties(),
        Any::Tx3g(tx3g) => tx3g.properties(),
        Any::VpcC(vpc_c) => vpc_c.properties(),
        Any::Av1c(av1c) => av1c.properties(),
        Any::Dops(dops) => dops.properties(),
        Any::Cmpd(cmpd) => cmpd.properties(),
        Any::UncC(unc_c) => unc_c.properties(),
        Any::Stts(stts) => stts.properties(),
        Any::Stsc(stsc) => stsc.properties(),
        Any::Stsz(stsz) => stsz.properties(),
        Any::Stss(stss) => stss.properties(),
        Any::Stco(stco) => stco.properties(),
        Any::Co64(co64) => co64.properties(),
        Any::Ctts(ctts) => ctts.properties(),
        Any::Sbgp(sbgp) => sbgp.properties(),
        Any::Sgpd(sgpd) => sgpd.properties(),
        Any::Subs(subs) => subs.properties(),
        Any::Saio(saio) => saio.properties(),
        Any::Saiz(saiz) => saiz.properties(),
        Any::Dref(dref) => dref.properties(),
        Any::Smhd(smhd) => smhd.properties(),
        Any::Vmhd(vmhd) => vmhd.properties(),
        Any::Elst(elst) => elst.properties(),
        Any::Mehd(mehd) => mehd.properties(),
        Any::Trex(trex) => trex.properties(),
        Any::Emsg(emsg) => emsg.properties(),
        Any::Mfhd(mfhd) => mfhd.properties(),
        Any::Tfhd(tfhd) => tfhd.properties(),
        Any::Tfdt(tfdt) => tfdt.properties(),
        Any::Trun(trun) => trun.properties(),
        Any::Skip(skip) => skip.properties(),
        Any::Free(free) => free.properties(),
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
        Any::Colr(_) => unimplemented!(), // ColourInformationBox
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
        four_cc if four_cc == FourCC::new(b"encv") => {
            visual_entry(header, "EncryptedVisualSampleEntryBox", reader)
        }
        mp4_atom::Mp4a::KIND => audio_entry(header, "MP4AudioSampleEntryBox", reader),
        mp4_atom::Opus::KIND => audio_entry(header, "OpusSampleEntryBox", reader),
        four_cc if four_cc == FourCC::new(b"ac-4") => {
            audio_entry(header, "AC4SampleEntryBox", reader)
        }
        four_cc if four_cc == FourCC::new(b"enca") => {
            audio_entry(header, "EncryptedAudioSampleEntryBox", reader)
        }
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
        four_cc if four_cc == FourCC::new(b"sinf") => {
            container(header, "ProtectionSchemeInfoBox", reader)
        }
        four_cc if four_cc == FourCC::new(b"schi") => {
            container(header, "SchemeInformationBox", reader)
        }
        Prft::KIND => try_properties_from::<Prft>(header, reader),
        Frma::KIND => try_properties_from::<Frma>(header, reader),
        Schm::KIND => try_properties_from::<Schm>(header, reader),
        Pssh::KIND => try_properties_from::<Pssh>(header, reader),
        Tenc::KIND => try_properties_from::<Tenc>(header, reader),
        Lac4::KIND => try_properties_from::<Lac4>(header, reader),
        // Overriding implementation from mp4-atom to add unknown case and nclc case defined in
        // QuickTime File Format.
        Colr::KIND => try_properties_from::<Colr>(header, reader),
        // senc doesn't quite fit in the same way as we provide a custom error in the case that we
        // find one.
        Senc::KIND => match Senc::decode_atom(header, reader) {
            Ok(atom) => Ok(AtomPropertiesWithDepth {
                properties: atom.properties(),
                new_depth_until: None,
            }),
            Err(error) => match error {
                mp4_atom::Error::Unsupported(e) if e == Senc::UNKNOWN_IV_SIZE => {
                    if let Some(size) = header.size {
                        reader.advance(size);
                    }
                    Ok(AtomPropertiesWithDepth {
                        properties: AtomProperties {
                            box_name: "SampleEncryptionBox",
                            properties: vec![("IV", AtomPropertyValue::from("Unsupported size"))],
                        },
                        new_depth_until: None,
                    })
                }
                _ => Err(error),
            },
        },
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

fn try_properties_from<T>(
    header: &Header,
    reader: &mut Cursor<Vec<u8>>,
) -> mp4_atom::Result<AtomPropertiesWithDepth>
where
    T: Atom,
    T: AtomWithProperties,
{
    let atom = T::decode_atom(header, reader)?;
    Ok(AtomPropertiesWithDepth {
        properties: atom.properties(),
        new_depth_until: None,
    })
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

fn byte_array_string_from(bytes: &[u8]) -> BasicPropertyValue {
    BasicPropertyValue::String(String::from(&byte_array_from(bytes)))
}

fn array_string_from<T: Display>(items: &[T]) -> String {
    items
        .iter()
        .map(|item| format!("{item}"))
        .collect::<Vec<String>>()
        .join(", ")
}
