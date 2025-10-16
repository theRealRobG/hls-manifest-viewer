// MIT License
//
// Copyright (c) 2024-2025 Eric Marsden
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use quick_xml::events::{BytesCData, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use std::{
    error::Error,
    fmt::{Debug, Display},
    io::{Cursor, Read},
    string::FromUtf16Error,
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PlayReadyPsshData {
    pub record: Vec<PlayReadyRecord>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct PlayReadyRecord {
    pub record_type: PlayReadyRecordType,
    pub record_value: WRMHeader,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayReadyRecordType {
    #[default]
    RightsManagement = 1,
    Reserved = 2,
    EmbeddedLicenseStore = 3,
}
impl TryFrom<u16> for PlayReadyRecordType {
    type Error = ParseError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::RightsManagement),
            2 => Ok(Self::Reserved),
            3 => Ok(Self::EmbeddedLicenseStore),
            _ => Err(ParseError::UnknownType(value)),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct WRMHeader {
    pub xmlns: String,
    pub version: String,
    pub data: WRMData,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct WRMData {
    pub kids: Vec<PlayReadyKid>,
    pub protect_info: Option<ProtectInfo>,
    pub checksum: Option<String>,
    pub la_url: Option<String>,
    pub lui_url: Option<String>,
    pub ds_id: Option<String>,
    pub custom_attributes: Option<String>,
    pub decryptor_setup: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct PlayReadyKid {
    pub value: Option<String>,
    pub algid: Option<String>,
    pub checksum: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct ProtectInfo {
    pub keylen: Option<u32>,
    pub algid: Option<String>,
    pub kids: Vec<PlayReadyKid>,
}

pub fn parse_pssh_data(buf: &[u8]) -> Result<PlayReadyPsshData, ParseError> {
    let mut rdr = Cursor::new(buf);
    let blen = buf.len() as u32;
    let length = rdr.read_u32()?;
    if length != blen {
        return Err(ParseError::UnexpectedDataLength {
            actual: blen,
            expected: length,
        });
    }
    let record_count = rdr.read_u16()?;
    let mut records = Vec::with_capacity(record_count.into());
    for _ in 1..=record_count {
        records.push(parse_playready_record(&mut rdr)?);
    }
    Ok(PlayReadyPsshData { record: records })
}

macro_rules! set_text_data {
    ($data:ident, $bytes:ident, $val:ident, $method:ident) => {{
        let text = $method($bytes).unwrap_or_default();
        if let Some(val) = &mut $data.$val {
            val.push_str(&text);
        } else {
            $data.$val = Some(text);
        }
    }};
}

fn parse_playready_record(rdr: &mut Cursor<&[u8]>) -> Result<PlayReadyRecord, ParseError> {
    let record_type = rdr.read_u16()?;
    if record_type != 1 {
        return Err(ParseError::UnknownType(record_type));
    }
    let record_length = rdr.read_u16()?;
    let mut wrmh_u8 = Vec::new();
    rdr.take(record_length.into()).read_to_end(&mut wrmh_u8)?;
    let wrmh_u16 = wrmh_u8
        .chunks(2)
        .map(|e| u16::from_le_bytes(e.try_into().unwrap()))
        .collect::<Vec<_>>();
    let xml = String::from_utf16(&wrmh_u16)?;

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    let mut custom_attr_writer = Writer::new(Cursor::new(Vec::new()));

    let mut current_element = Element::default();
    let mut xmlns = None;
    let mut version = None;
    let mut data_found = false;
    let mut data = WRMData::default();
    let mut kid = None;
    let mut protect_info = None;
    loop {
        let event = reader.read_event()?;
        match current_element {
            Element::Header => match event {
                Event::Start(bytes) => match bytes.name().as_ref() {
                    b"WRMHEADER" => {
                        for a in bytes.attributes().flatten() {
                            match a.key.as_ref() {
                                b"xmlns" => xmlns = String::from_utf8(a.value.to_vec()).ok(),
                                b"version" => version = String::from_utf8(a.value.to_vec()).ok(),
                                _ => (),
                            }
                        }
                    }
                    b"DATA" => {
                        data_found = true;
                        current_element = Element::Data;
                    }
                    _ => (),
                },
                Event::Eof => break,
                _ => (),
            },
            Element::Data => match event {
                Event::Start(ref bytes) => match bytes.name().as_ref() {
                    b"KID" => {
                        kid = Some(key_id(bytes));
                        current_element = Element::Kid(KidParent::WrmData);
                    }
                    b"PROTECTINFO" => {
                        protect_info = Some(ProtectInfo::default());
                        current_element = Element::ProtectInfo;
                    }
                    b"CHECKSUM" => current_element = Element::Checksum,
                    b"LA_URL" => current_element = Element::LaUrl,
                    b"LUI_URL" => current_element = Element::LuiUrl,
                    b"DS_ID" => current_element = Element::DsId,
                    b"DECRYPTORSETUP" => current_element = Element::DecryptorSetup,
                    b"CUSTOMATTRIBUTES" => {
                        custom_attr_writer.write_event(event.into_owned())?;
                        current_element = Element::CustomAttributes;
                    }
                    _ => (),
                },
                Event::End(bytes) if bytes.name().as_ref() == b"DATA" => current_element.close(),
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => (),
            },
            Element::Kid(parent) => match event {
                Event::Text(bytes) => {
                    if let Some(kid) = &mut kid
                        && let Some(text) = text(bytes)
                    {
                        if let Some(value) = &mut kid.value {
                            value.push_str(&text);
                        } else {
                            kid.value = Some(text);
                        }
                    }
                }
                Event::CData(bytes) => {
                    if let Some(kid) = &mut kid
                        && let Some(text) = cdata(bytes)
                    {
                        if let Some(value) = &mut kid.value {
                            value.push_str(&text);
                        } else {
                            kid.value = Some(text);
                        }
                    }
                }
                Event::End(bytes) if bytes.name().as_ref() == b"KID" => {
                    if let Some(kid) = kid.take() {
                        match parent {
                            KidParent::Kids | KidParent::ProtectInfo => {
                                if let Some(pi) = &mut protect_info {
                                    pi.kids.push(kid);
                                }
                            }
                            KidParent::WrmData => data.kids.push(kid),
                        }
                    }
                    current_element.close();
                }
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => (),
            },
            Element::ProtectInfo => match event {
                Event::Start(bytes) => match bytes.name().as_ref() {
                    b"KIDS" => current_element = Element::Kids,
                    b"KID" => {
                        kid = Some(key_id(&bytes));
                        current_element = Element::Kid(KidParent::ProtectInfo);
                    }
                    b"ALGID" => current_element = Element::AlgId,
                    b"KEYLEN" => current_element = Element::KeyLen,
                    _ => (),
                },
                Event::End(bytes) if bytes.name().as_ref() == b"PROTECTINFO" => {
                    data.protect_info = protect_info.take();
                    current_element.close()
                }
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => (),
            },
            Element::Checksum => match event {
                Event::Text(bytes) => set_text_data!(data, bytes, checksum, text),
                Event::CData(bytes) => set_text_data!(data, bytes, checksum, cdata),
                Event::End(bytes) if bytes.name().as_ref() == b"CHECKSUM" => {
                    current_element.close()
                }
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => (),
            },
            Element::LaUrl => match event {
                Event::Text(bytes) => set_text_data!(data, bytes, la_url, text),
                Event::CData(bytes) => set_text_data!(data, bytes, la_url, cdata),
                Event::End(bytes) if bytes.name().as_ref() == b"LA_URL" => current_element.close(),
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => (),
            },
            Element::LuiUrl => match event {
                Event::Text(bytes) => set_text_data!(data, bytes, lui_url, text),
                Event::CData(bytes) => set_text_data!(data, bytes, lui_url, cdata),
                Event::End(bytes) if bytes.name().as_ref() == b"LUI_URL" => current_element.close(),
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => (),
            },
            Element::DsId => match event {
                Event::Text(bytes) => set_text_data!(data, bytes, ds_id, text),
                Event::CData(bytes) => set_text_data!(data, bytes, ds_id, cdata),
                Event::End(bytes) if bytes.name().as_ref() == b"DS_ID" => current_element.close(),
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => (),
            },
            Element::DecryptorSetup => match event {
                Event::Text(bytes) => set_text_data!(data, bytes, decryptor_setup, text),
                Event::CData(bytes) => set_text_data!(data, bytes, decryptor_setup, cdata),
                Event::End(bytes) if bytes.name().as_ref() == b"DECRYPTORSETUP" => {
                    current_element.close()
                }
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => (),
            },
            Element::KeyLen => match event {
                Event::Text(bytes) => {
                    if let Some(pi) = &mut protect_info {
                        pi.keylen = bytes
                            .xml_content()
                            .ok()
                            .and_then(|xml| xml.parse::<u32>().ok());
                    }
                }
                Event::End(bytes) if bytes.name().as_ref() == b"KEYLEN" => current_element.close(),
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => (),
            },
            Element::AlgId => {
                let Some(pi) = &mut protect_info else {
                    continue;
                };
                match event {
                    Event::Text(bytes) => set_text_data!(pi, bytes, algid, text),
                    Event::CData(bytes) => set_text_data!(pi, bytes, algid, cdata),
                    Event::End(bytes) if bytes.name().as_ref() == b"ALGID" => {
                        current_element.close()
                    }
                    Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                    _ => (),
                }
            }
            Element::Kids => match event {
                Event::Start(ref bytes) => {
                    if bytes.name().as_ref() == b"KID" {
                        kid = Some(key_id(bytes));
                        current_element = Element::Kid(KidParent::Kids);
                    }
                }
                Event::End(bytes) if bytes.name().as_ref() == b"KIDS" => current_element.close(),
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => (),
            },
            Element::CustomAttributes => match event {
                Event::End(ref bytes) if bytes.name().as_ref() == b"CUSTOMATTRIBUTES" => {
                    custom_attr_writer.write_event(event.into_owned())?;
                    current_element.close();
                }
                Event::Eof => return Err(ParseError::UnexpectedEndOfXml),
                _ => custom_attr_writer.write_event(event.into_owned())?,
            },
        }
    }

    if !data_found {
        return Err(ParseError::NoWrmData);
    }
    let xmlns =
        xmlns.unwrap_or("http://schemas.microsoft.com/DRM/2007/03/PlayReadyHeader".to_string());
    let Some(version) = version else {
        return Err(ParseError::NoVersion);
    };
    let custom_attributes = String::from_utf8(custom_attr_writer.into_inner().into_inner()).ok();
    data.custom_attributes = custom_attributes;

    let wrm_header = WRMHeader {
        xmlns,
        version,
        data,
    };
    Ok(PlayReadyRecord {
        record_type: PlayReadyRecordType::try_from(record_type)?,
        record_value: wrm_header,
    })
}

fn key_id(bytes: &BytesStart) -> PlayReadyKid {
    let mut kid = PlayReadyKid::default();
    for a in bytes.attributes().flatten() {
        match a.key.as_ref() {
            b"VALUE" => kid.value = String::from_utf8(a.value.to_vec()).ok(),
            b"ALGID" => kid.algid = String::from_utf8(a.value.to_vec()).ok(),
            b"CHECKSUM" => kid.checksum = String::from_utf8(a.value.to_vec()).ok(),
            _ => (),
        }
    }
    kid
}

fn text(bytes: BytesText) -> Option<String> {
    bytes.xml_content().ok().map(|s| s.to_string())
}

fn cdata(bytes: BytesCData) -> Option<String> {
    String::from_utf8(bytes.into_inner().to_vec()).ok()
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum KidParent {
    Kids,
    ProtectInfo,
    WrmData,
}
#[derive(Debug, PartialEq, Clone, Copy, Default)]
enum Element {
    #[default]
    Header,
    Data,
    Kid(KidParent),
    ProtectInfo,
    Checksum,
    LaUrl,
    LuiUrl,
    DsId,
    CustomAttributes,
    DecryptorSetup,
    KeyLen,
    AlgId,
    Kids,
}
impl Element {
    fn close(&mut self) {
        match self {
            Element::Header => (),
            Element::Data => *self = Element::Header,
            Element::Kid(parent) => match parent {
                KidParent::Kids => *self = Element::Kids,
                KidParent::ProtectInfo => *self = Element::ProtectInfo,
                KidParent::WrmData => *self = Element::Data,
            },
            Element::ProtectInfo => *self = Element::Data,
            Element::Checksum => *self = Element::Data,
            Element::LaUrl => *self = Element::Data,
            Element::LuiUrl => *self = Element::Data,
            Element::DsId => *self = Element::Data,
            Element::CustomAttributes => *self = Element::Data,
            Element::DecryptorSetup => *self = Element::Data,
            Element::KeyLen => *self = Element::ProtectInfo,
            Element::AlgId => *self = Element::ProtectInfo,
            Element::Kids => *self = Element::ProtectInfo,
        }
    }
}

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Utf16(FromUtf16Error),
    Xml(quick_xml::Error),
    UnknownType(u16),
    UnexpectedDataLength { actual: u32, expected: u32 },
    NoWrmData,
    NoVersion,
    UnexpectedEndOfXml,
}
impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Io(e) => write!(f, "PlayReady pssh io error {e}"),
            ParseError::Utf16(e) => write!(f, "PlayReady pssh utf16 error {e}"),
            ParseError::Xml(e) => write!(f, "PlayReady pssh xml error {e}"),
            ParseError::UnknownType(t) => write!(f, "can't parse PlayReady record of type {t}"),
            ParseError::UnexpectedDataLength {
                actual: a,
                expected: e,
            } => {
                write!(f, "header length {e} different from buffer length {a}")
            }
            ParseError::NoWrmData => write!(f, "no DATA in PlayReady pssh"),
            ParseError::NoVersion => write!(f, "no version in PlayReady pssh"),
            ParseError::UnexpectedEndOfXml => write!(f, "unexpected end of PlayReady pssh XML"),
        }
    }
}
impl Error for ParseError {}
impl From<std::io::Error> for ParseError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<FromUtf16Error> for ParseError {
    fn from(value: FromUtf16Error) -> Self {
        Self::Utf16(value)
    }
}
impl From<quick_xml::Error> for ParseError {
    fn from(value: quick_xml::Error) -> Self {
        Self::Xml(value)
    }
}

trait LittleEndianReader {
    fn read_u16(&mut self) -> Result<u16, std::io::Error>;
    fn read_u32(&mut self) -> Result<u32, std::io::Error>;
}
impl LittleEndianReader for Cursor<&[u8]> {
    fn read_u16(&mut self) -> Result<u16, std::io::Error> {
        let mut buf = [0; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    fn read_u32(&mut self) -> Result<u32, std::io::Error> {
        let mut buf = [0; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }
}
