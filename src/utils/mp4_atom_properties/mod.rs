use crate::utils::mp4_parsing::{
    dvcc::Dvcc, Blin, Colr, Corg, Dac3, Dac4, Dadj, Dec3, Dvvc, Equi, Fish, Frma, Hequ, Hero, Hfov,
    Hvce, Lac4, Ldst, Lfad, Lhvc, Lnhd, Lnin, Must, Pkin, Prft, Prim, Prji, Pssh, Rdim, Rect, Schm,
    Senc, Stri, Tenc, Uqua,
};
use mp4_atom::{Any, Atom, Audio, Buf, Decode, DecodeAtom, FourCC, Header, Visual};
use std::{borrow::Cow, fmt::Display, io::Cursor};

mod auxc;
mod av1c;
mod avcc;
mod blin;
mod btrt;
mod ccst;
mod clap;
mod cmpd;
mod co64;
mod colr;
mod corg;
mod covr;
mod ctts;
mod dac3;
mod dac4;
mod dadj;
mod dec3;
mod desc;
mod dops;
mod dref;
mod dvcc;
mod dvvc;
mod elst;
mod emsg;
mod equi;
mod esds;
mod fish;
mod free;
mod frma;
mod ftyp;
mod hdlr;
mod hequ;
mod hero;
mod hfov;
mod hvcc;
mod hvce;
mod idat;
mod iinf;
mod iloc;
mod imir;
mod ipma;
mod iref;
mod irot;
mod iscl;
mod ispe;
mod lac4;
mod ldst;
mod lfad;
mod lhvc;
mod lnhd;
mod lnin;
mod mdhd;
mod mehd;
mod mfhd;
mod must;
mod mvhd;
mod name;
mod pasp;
mod pitm;
mod pixi;
mod pkin;
mod prft;
mod prim;
mod prji;
mod pssh;
mod rdim;
mod rect;
mod rref;
mod saio;
mod saiz;
mod sbgp;
mod schm;
mod senc;
mod sgpd;
mod skip;
mod smhd;
mod stco;
mod stri;
mod stsc;
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
mod trex;
mod trun;
mod tx3g;
mod uncc;
mod uqua;
mod vmhd;
mod vpcc;
mod year;

#[derive(Debug, Clone, PartialEq)]
pub struct AtomProperties {
    pub box_name: &'static str,
    pub properties: Vec<(Cow<'static, str>, AtomPropertyValue)>,
}
impl AtomProperties {
    fn from_static_keys(
        box_name: &'static str,
        properties: Vec<(&'static str, AtomPropertyValue)>,
    ) -> Self {
        let mut v = Vec::with_capacity(properties.len());
        for e in properties {
            v.push((Cow::Borrowed(e.0), e.1));
        }
        Self {
            box_name,
            properties: v,
        }
    }
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
    BinaryMask(Vec<u8>),
}
impl BasicPropertyValue {
    pub fn is_hex(&self) -> bool {
        matches!(self, Self::Hex(_))
    }

    pub fn is_binary_mask(&self) -> bool {
        matches!(self, Self::BinaryMask(_))
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
            BasicPropertyValue::BinaryMask(bytes) => bytes
                .iter()
                .map(|b| format!("{b:08b}"))
                .collect::<Vec<String>>()
                .join(" "),
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
            properties: vec![("data".into(), AtomPropertyValue::from(items))],
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
    const DVHE: FourCC = FourCC::new(b"dvhe");
    const DVH1: FourCC = FourCC::new(b"dvh1");
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
        DVHE => visual_entry(header, "DolbyVisionHEVCSampleEntryBox", reader),
        DVH1 => visual_entry(header, "DolbyVisionHVC1SampleEntryBox", reader),
        mp4_atom::Vp08::KIND => visual_entry(header, "VP08SampleEntryBox", reader),
        mp4_atom::Vp09::KIND => visual_entry(header, "VP09SampleEntryBox", reader),
        mp4_atom::Av01::KIND => visual_entry(header, "AV1SampleEntryBox", reader),
        mp4_atom::Uncv::KIND => visual_entry(header, "UncompressedFrameSampleEntryBox", reader),
        four_cc if four_cc == FourCC::new(b"encv") => {
            visual_entry(header, "EncryptedVisualSampleEntryBox", reader)
        }
        mp4_atom::Mp4a::KIND => audio_entry(header, "MP4AudioSampleEntryBox", reader),
        mp4_atom::Opus::KIND => audio_entry(header, "OpusSampleEntryBox", reader),
        four_cc if four_cc == FourCC::new(b"ac-3") => {
            audio_entry(header, "AC3SampleEntryBox", reader)
        }
        four_cc if four_cc == FourCC::new(b"ec-3") => {
            audio_entry(header, "EC3SampleEntryBox", reader)
        }
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
        // VEXU container boxes (QuickTime and ISO Base Media File Formats and Spatial and Immersive Media, Version 1.9.8 (Beta))
        four_cc if four_cc == FourCC::new(b"vexu") => {
            container(header, "VideoExtendedUsageBox", reader)
        }
        four_cc if four_cc == FourCC::new(b"eyes") => container(header, "StereoViewBox", reader),
        four_cc if four_cc == FourCC::new(b"cams") => {
            container(header, "StereoCameraSystemBox", reader)
        }
        four_cc if four_cc == FourCC::new(b"cmfy") => container(header, "StereoComfortBox", reader),
        four_cc if four_cc == FourCC::new(b"proj") => container(header, "ProjectionBox", reader),
        four_cc if four_cc == FourCC::new(b"pack") => container(header, "ViewPackingBox", reader),
        four_cc if four_cc == FourCC::new(b"lnsc") => {
            container(header, "CameraSystemLensCollectionBox", reader)
        }
        four_cc if four_cc == FourCC::new(b"lens") => {
            container(header, "CameraSystemLensBox", reader)
        }
        four_cc if four_cc == FourCC::new(b"lnex") => {
            container(header, "CameraSystemLensExtrinsicsBox", reader)
        }
        four_cc if four_cc == FourCC::new(b"cxfm") => {
            container(header, "CameraSystemTransformBox", reader)
        }
        // VEXU data boxes
        Must::KIND => try_properties_from::<Must>(header, reader),
        Stri::KIND => try_properties_from::<Stri>(header, reader),
        Hero::KIND => try_properties_from::<Hero>(header, reader),
        Blin::KIND => try_properties_from::<Blin>(header, reader),
        Dadj::KIND => try_properties_from::<Dadj>(header, reader),
        Prji::KIND => try_properties_from::<Prji>(header, reader),
        Pkin::KIND => try_properties_from::<Pkin>(header, reader),
        Hfov::KIND => try_properties_from::<Hfov>(header, reader),
        Lnhd::KIND => try_properties_from::<Lnhd>(header, reader),
        Rdim::KIND => try_properties_from::<Rdim>(header, reader),
        Lnin::KIND => try_properties_from::<Lnin>(header, reader),
        Ldst::KIND => try_properties_from::<Ldst>(header, reader),
        Lfad::KIND => try_properties_from::<Lfad>(header, reader),
        Corg::KIND => try_properties_from::<Corg>(header, reader),
        Uqua::KIND => try_properties_from::<Uqua>(header, reader),
        Rect::KIND => try_properties_from::<Rect>(header, reader),
        Equi::KIND => try_properties_from::<Equi>(header, reader),
        Hequ::KIND => try_properties_from::<Hequ>(header, reader),
        Fish::KIND => try_properties_from::<Fish>(header, reader),
        Prim::KIND => try_properties_from::<Prim>(header, reader),
        // Other custom atoms
        Prft::KIND => try_properties_from::<Prft>(header, reader),
        Frma::KIND => try_properties_from::<Frma>(header, reader),
        Schm::KIND => try_properties_from::<Schm>(header, reader),
        Pssh::KIND => try_properties_from::<Pssh>(header, reader),
        Tenc::KIND => try_properties_from::<Tenc>(header, reader),
        Dac3::KIND => try_properties_from::<Dac3>(header, reader),
        Dec3::KIND => try_properties_from::<Dec3>(header, reader),
        Lac4::KIND => try_properties_from::<Lac4>(header, reader),
        Dac4::KIND => try_properties_from::<Dac4>(header, reader),
        Dvvc::KIND => try_properties_from::<Dvvc>(header, reader),
        Dvcc::KIND => try_properties_from::<Dvcc>(header, reader),
        Hvce::KIND => try_properties_from::<Hvce>(header, reader),
        Lhvc::KIND => try_properties_from::<Lhvc>(header, reader),
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
                            properties: vec![(
                                "IV".into(),
                                AtomPropertyValue::from("Unsupported size"),
                            )],
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
    properties
        .properties
        .properties
        .insert(0, ("size".into(), size));
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
    const LENS: FourCC = FourCC::new(b"lens");
    match header.kind {
        // Known full boxes that are also containers
        mp4_atom::Meta::KIND | mp4_atom::Stsd::KIND | LENS => {
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
        properties: AtomProperties::from_static_keys(name, version_and_flags),
        new_depth_until: Some(new_depth_until),
    })
}

// The QuickTime definition on this is found here:
// https://developer.apple.com/documentation/quicktime-file-format/timed_metadata_media
//
// The mebx and downloaded segment come from `hls/AivBeachWWDC_VideoVar_5/playlist.m3u8` in this
// Apple HLS example stream:
// https://devstreaming-cdn.apple.com/videos/streaming/examples/immersive-media/apple-immersive-video/primary.m3u8
#[cfg(test)]
mod tests {
    use std::{fs::File, io::Read};

    use super::*;
    use mp4_atom::ReadFrom;
    use pretty_assertions::assert_eq;

    const MEBX: &[u8] = &[
        0x00, 0x00, 0x00, 0x72, 0x6D, 0x65, 0x62, 0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x62, 0x6B, 0x65, 0x79, 0x73, 0x00, 0x00, 0x00, 0x5A, 0x00, 0x00,
        0x00, 0x01, 0x00, 0x00, 0x00, 0x42, 0x6B, 0x65, 0x79, 0x64, 0x6D, 0x64, 0x74, 0x61, 0x63,
        0x6F, 0x6D, 0x2E, 0x61, 0x70, 0x70, 0x6C, 0x65, 0x2E, 0x71, 0x75, 0x69, 0x63, 0x6B, 0x74,
        0x69, 0x6D, 0x65, 0x2E, 0x76, 0x69, 0x64, 0x65, 0x6F, 0x2E, 0x70, 0x72, 0x65, 0x73, 0x65,
        0x6E, 0x74, 0x61, 0x74, 0x69, 0x6F, 0x6E, 0x2E, 0x69, 0x6D, 0x6D, 0x65, 0x72, 0x73, 0x69,
        0x76, 0x65, 0x2D, 0x6D, 0x65, 0x64, 0x69, 0x61, 0x00, 0x00, 0x00, 0x10, 0x64, 0x74, 0x79,
        0x70, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn play_with_mebx() {
        let mut reader = Cursor::new(MEBX.to_vec());
        let header = Header::read_from(&mut reader).expect("should parse mebx header");
        let _ = meta_sample_entry(&header, "BoxedMetadataSampleEntry", &mut reader)
            .expect("should get meta sample entry for mebx");
        println!("{header:?}");

        let header = Header::read_from(&mut reader).expect("should parse box after mebx");
        println!("{header:?}");

        let header = Header::read_from(&mut reader).expect("should parse box after keys");
        println!("{header:?}");
        assert_eq!(FourCC::from(1), header.kind);

        let header = Header::read_from(&mut reader).expect("should parse box after keys");
        let atom = Keyd::decode_atom(&header, &mut reader).expect("should decode keyd");
        println!("{header:?}");
        println!("{atom:?}");

        let header = Header::read_from(&mut reader).expect("should parse header after keyd");
        println!("{header:?}");
        let atom = Dtyp::decode_atom(&header, &mut reader).expect("should decode dtyp");
        println!("{atom:?}");
    }

    #[test]
    fn play_with_mdat_and_trun() {
        // data_offset = 1843658
        let mut file = File::open("fileSequence1.m4s").expect("could not open file");
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).expect("error reading file");
        let sample_size = 179;
        for i in 0..45 {
            let data_offset = 1843658 + (i * sample_size);
            let end = data_offset + sample_size;
            // Each metadata sample is a box (i.e., size: u32, type: u32, contents: [u8; size])
            let size = u32::from_be_bytes([
                bytes[data_offset],
                bytes[data_offset + 1],
                bytes[data_offset + 2],
                bytes[data_offset + 3],
            ]);
            let box_type = &bytes[(data_offset + 4)..(data_offset + 8)];
            // This is actually wrong in general. I believe I need to just go through the entire
            // sample size and parse boxes as I find them. So, the size value derived from the box
            // above should be used, and if there is left over, then parse out another box. The box
            // types are then used to match against data found in the mebx to know if they should be
            // ignored or not. But, for this example, I know there is only one type of box found in
            // the mdat I have downloaded, so I'm just sticking with this for the POC.
            let s = String::from_utf8_lossy(&bytes[(data_offset + 8)..end]);
            let sample_number = i + 1;
            println!(
                "sample {sample_number:02}\n---------\nsize: {size}\ntype: {box_type:?}\n{s}\n"
            );
        }
    }
}

#[derive(Debug)]
pub struct Keyd {
    pub key_namespace: FourCC,
    pub key_value: String,
}
impl Atom for Keyd {
    const KIND: FourCC = FourCC::new(b"keyd");

    fn decode_body<B: Buf>(buf: &mut B) -> mp4_atom::Result<Self> {
        let key_namespace = FourCC::decode(buf)?;
        let key_value = String::decode(buf)?;
        Ok(Self {
            key_namespace,
            key_value,
        })
    }

    fn encode_body<B: mp4_atom::BufMut>(&self, _: &mut B) -> mp4_atom::Result<()> {
        unimplemented!()
    }
}

// https://developer.apple.com/documentation/quicktime-file-format/metadata_datatype_definition_atom
#[derive(Debug)]
pub struct Dtyp {
    pub data_namespace: u32,
    pub data_value: String,
}
impl Atom for Dtyp {
    const KIND: FourCC = FourCC::new(b"dtyp");

    fn decode_body<B: Buf>(buf: &mut B) -> mp4_atom::Result<Self> {
        let data_namespace = u32::decode(buf)?;
        let data_value = if data_namespace == 0 {
            match u32::decode(buf)? {
                0 => String::from("RESERVED"),
                1 => String::from("UTF-8"),
                2 => String::from("UTF-16"),
                3 => String::from("S/JIS"),
                4 => String::from("UTF-8 sort"),
                5 => String::from("UTF-16 sort"),
                13 => String::from("JPEG"),
                14 => String::from("PNG"),
                21 => String::from("BE Signed Integer"),
                22 => String::from("BE Unsigned Integer"),
                23 => String::from("BE Float32"),
                24 => String::from("BE Float64"),
                27 => String::from("BMP"),
                28 => String::from("QuickTime Metadata atom"),
                65 => String::from("8-bit Signed Integer"),
                66 => String::from("BE 16-bit Signed Integer"),
                67 => String::from("BE 32-bit Signed Integer"),
                70 => String::from("BE PointF32"),
                71 => String::from("BE DimensionsF32"),
                72 => String::from("BE RectF32"),
                74 => String::from("BE 64-bit Signed Integer"),
                75 => String::from("8-bit Unsigned Integer"),
                76 => String::from("BE 16-bit Unsigned Integer"),
                77 => String::from("BE 32-bit Unsigned Integer"),
                78 => String::from("BE 64-bit Unsigned Integer"),
                79 => String::from("AffineTransformF64"),
                n => format!("{n}"),
            }
        } else {
            let mut bytes = Vec::new();
            while buf.has_remaining() {
                let byte = u8::decode(buf)?;
                bytes.push(byte);
            }
            String::from_utf8_lossy(&bytes).to_string()
        };
        Ok(Self {
            data_namespace,
            data_value,
        })
    }

    fn encode_body<B: mp4_atom::BufMut>(&self, _: &mut B) -> mp4_atom::Result<()> {
        unimplemented!()
    }
}

fn meta_sample_entry(
    header: &Header,
    name: &'static str,
    reader: &mut Cursor<Vec<u8>>,
) -> mp4_atom::Result<AtomPropertiesWithDepth> {
    let header_size = header.size.unwrap_or_else(|| reader.remaining());
    let new_depth_until = reader.position() + (header_size as u64);

    let meta = MetaSampleEntry::decode(reader)?;
    Ok(AtomPropertiesWithDepth {
        properties: AtomProperties::from_static_keys(
            name,
            vec![(
                "data_reference_index",
                AtomPropertyValue::from(meta.data_reference_index),
            )],
        ),
        new_depth_until: Some(new_depth_until),
    })
}

struct MetaSampleEntry {
    data_reference_index: u16,
}
impl Decode for MetaSampleEntry {
    fn decode<B: Buf>(buf: &mut B) -> mp4_atom::Result<Self> {
        <[u8; 6]>::decode(buf)?;
        let data_reference_index = u16::decode(buf)?;
        Ok(Self {
            data_reference_index,
        })
    }
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
        properties: AtomProperties::from_static_keys(
            name,
            vec![
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
        ),
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
        properties: AtomProperties::from_static_keys(
            name,
            vec![
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
        ),
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
