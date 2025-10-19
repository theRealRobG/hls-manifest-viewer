use crate::utils::pssh_data::playready::{self, PlayReadyPsshData};
use hex_literal::hex;
use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};
use protobuf::Message;
use std::{borrow::Cow, fmt::Display};
use widevine_proto::license_protocol::WidevinePsshData;

/// ProducerReferenceTimeBox, ISO/IEC 14496-12:2024 Sect 8.16.5
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Prft {
    pub reference_track_id: u32,
    pub ntp_timestamp: u64,
    pub media_time: u64,
    pub ntp_timestamp_media_time_association: NtpTimestampMediaTimeAssociation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NtpTimestampMediaTimeAssociation {
    /// The UTC time is the time at which the frame belonging to the reference track in the
    /// following movie fragment and whose presentation time is `media_time` was input to the
    /// encoder.
    #[default]
    ReferenceTrackInFollowingMoofEncoderInput,
    /// The UTC time is the time at which the frame belonging to the reference track in the
    /// following movie fragment and whose presentation time is `media_time` was output from the
    /// encoder.
    ReferenceTrackInFollowingMoofEncoderOutput,
    /// The UTC time is the time at which the following MovieFragmentBox was finalized. `media_time`
    /// is set to the presentation of the earliest frame of the reference track in presentation
    /// order of the movie fragment.
    FollowingMoofFinalization,
    /// The UTC time is the time at which the following MovieFragmentBox was written to file.
    /// `media_time` is set to the presentation of the earliest frame of the reference track in
    /// presentation order of the movie fragment.
    FollowingMoofFileWrite,
    /// The association between the `media_time` and UTC time is arbitrary but consistent between
    /// multiple occurrences of this box in the same track
    Arbitrary,
    /// The UTC time has a consistent, small (ideally zero), offset from the real-time of the
    /// experience depicted in the media at `media_time`
    ConsistentSmallOffset,
    /// The association is unknown because an unknown flag was set in the `prft` atom.
    Unknown,
}
impl From<u32> for NtpTimestampMediaTimeAssociation {
    fn from(v: u32) -> Self {
        match v & 0b00000000_11111111_11111111_11111111 {
            0b00000000_00000000_00000000 => Self::ReferenceTrackInFollowingMoofEncoderInput,
            0b00000000_00000000_00000001 => Self::ReferenceTrackInFollowingMoofEncoderOutput,
            0b00000000_00000000_00000010 => Self::FollowingMoofFinalization,
            0b00000000_00000000_00000100 => Self::FollowingMoofFileWrite,
            0b00000000_00000000_00001000 => Self::Arbitrary,
            0b00000000_00000000_00011000 => Self::ConsistentSmallOffset,
            _ => Self::Unknown,
        }
    }
}
impl Display for NtpTimestampMediaTimeAssociation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReferenceTrackInFollowingMoofEncoderInput => write!(
                f,
                concat!(
                    "The UTC time is the time at which the frame belonging to the reference track ",
                    "in the following movie fragment and whose presentation time is media_time ",
                    "was input to the encoder.",
                ),
            ),
            Self::ReferenceTrackInFollowingMoofEncoderOutput => write!(
                f,
                concat!(
                    "The UTC time is the time at which the frame belonging to the reference track ",
                    "in the following movie fragment and whose presentation time is `media_time` ",
                    "was output from the encoder.",
                ),
            ),
            Self::FollowingMoofFinalization => write!(
                f,
                concat!(
                    "The UTC time is the time at which the following MovieFragmentBox was ",
                    "finalized. `media_time` is set to the presentation of the earliest frame of ",
                    "the reference track in presentation order of the movie fragment.",
                ),
            ),
            Self::FollowingMoofFileWrite => write!(
                f,
                concat!(
                    "The UTC time is the time at which the following MovieFragmentBox was written ",
                    "to file. `media_time` is set to the presentation of the earliest frame of ",
                    "the reference track in presentation order of the movie fragment.",
                ),
            ),
            Self::Arbitrary => write!(
                f,
                concat!(
                    "The association between the `media_time` and UTC time is arbitrary but ",
                    "consistent between multiple occurrences of this box in the same track",
                ),
            ),
            Self::ConsistentSmallOffset => write!(
                f,
                concat!(
                    "The UTC time has a consistent, small (ideally zero), offset from the real-",
                    "time of the experience depicted in the media at `media_time`",
                ),
            ),
            Self::Unknown => write!(
                f,
                concat!(
                    "The association is unknown because an unknown flag was set in the `prft` ",
                    "atom.",
                ),
            ),
        }
    }
}
impl Atom for Prft {
    const KIND: FourCC = FourCC::new(b"prft");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let ext = u32::decode(buf)?;
        let version = ext >> 24;
        let ntp_timestamp_media_time_association = NtpTimestampMediaTimeAssociation::from(ext);
        let reference_track_id = u32::decode(buf)?;
        let ntp_timestamp = u64::decode(buf)?;
        let media_time = if version == 0 {
            u32::decode(buf)? as u64
        } else {
            u64::decode(buf)?
        };
        Ok(Self {
            reference_track_id,
            ntp_timestamp,
            media_time,
            ntp_timestamp_media_time_association,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

/// OriginalFormatBox, ISO/IEC 14496-12:2024 Sect 13.4.3
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frma {
    pub data_format: FourCC,
}
impl Atom for Frma {
    const KIND: FourCC = FourCC::new(b"frma");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let data_format = FourCC::decode(buf)?;
        Ok(Self { data_format })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

/// SchemeTypeBox, ISO/IEC 14496-12:2024 Sect 13.4.6
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schm {
    pub scheme_type: FourCC,
    pub scheme_version: u32,
    pub scheme_uri: Option<String>,
}
impl Atom for Schm {
    const KIND: FourCC = FourCC::new(b"schm");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let ext = u32::decode(buf)?;
        let has_browser_uri = ext & 1 == 1;
        let scheme_type = FourCC::decode(buf)?;
        let scheme_version = u32::decode(buf)?;
        let scheme_uri = if has_browser_uri {
            Some(String::decode(buf)?)
        } else {
            None
        };
        Ok(Self {
            scheme_type,
            scheme_version,
            scheme_uri,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

/// ProtectionSystemSpecificHeaderBox, ISO/IEC 23001-7:2016 Sect 8.1.1
#[derive(Debug, Clone, PartialEq)]
pub struct Pssh {
    pub system_id: [u8; 16],
    pub key_ids: Vec<[u8; 16]>,
    pub data: Option<PsshData>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum PsshData {
    Widevine(Box<WidevinePsshData>),
    PlayReady(PlayReadyPsshData),
    Raw(Vec<u8>),
}
const ABV_DRM_SYSTEM_ID: [u8; 16] = hex!("6dd8b3c3 45f4 4a68 bf3a 64168d01a4a6");
const ADOBE_DRM_SYSTEM_ID: [u8; 16] = hex!("f239e769 efa3 4850 9c16 a903c6932efb");
const ALTICAST_DRM_SYSTEM_ID: [u8; 16] = hex!("616c7469 6361 7374 2d50 726f74656374");
const FAIRPLAY_DRM_SYSTEM_ID: [u8; 16] = hex!("94ce86fb 07ff 4f43 adb8 93d2fa968ca2");
const ARRIS_TITANIUM_DRM_SYSTEM_ID: [u8; 16] = hex!("279fe473 512c 48fe ade8 d176fee6b40f");
const CHINA_DRM_DRM_SYSTEM_ID: [u8; 16] = hex!("3d5e6d35 9b9a 41e8 b843 dd3c6e72c42c");
const CLEAR_AES_128_DRM_SYSTEM_ID: [u8; 16] = hex!("3ea8778f 7742 4bf9 b18b e834b2acbd47");
const CLEAR_SAMPLE_AES_DRM_SYSTEM_ID: [u8; 16] = hex!("be58615b 19c4 4684 88b3 c8c57e99e957");
const CLEAR_DASH_IF_DRM_SYSTEM_ID: [u8; 16] = hex!("e2719d58 a985 b3c9 781a b030af78d30e");
const CMLA_DRM_SYSTEM_ID: [u8; 16] = hex!("644fe7b5 260f 4fad 949a 0762ffb054B4");
const COMMSCOPE_TITANIUM_V3_DRM_SYSTEM_ID: [u8; 16] = hex!("37c33258 7b99 4c7e b15d 19af74482154");
const CORE_CRYPT_DRM_SYSTEM_ID: [u8; 16] = hex!("45d481cb 8fe0 49c0 ada9 ab2d2455b2f2");
const DIGI_CAP_SMART_XESS_DRM_SYSTEM_ID: [u8; 16] = hex!("dcf4e3e3 62f1 5818 7ba6 0a6fe33ff3dd");
const DIV_X_5_DRM_SYSTEM_ID: [u8; 16] = hex!("35bf197b 530e 42d7 8b65 1b4bf415070f");
const IRDETO_DRM_SYSTEM_ID: [u8; 16] = hex!("80a6be7e 1448 4c37 9e70 d5aebe04c8d2");
const MARLIN_DRM_SYSTEM_ID: [u8; 16] = hex!("5e629af5 38da 4063 8977 97ffbd9902d4");
const PLAY_READY_DRM_SYSTEM_ID: [u8; 16] = hex!("9a04f079 9840 4286 ab92 e65be0885f95");
const MOBI_TV_DRM_SYSTEM_ID: [u8; 16] = hex!("6a99532d 869f 5922 9a91 113ab7b1e2f3");
const NAGRA_DRM_SYSTEM_ID: [u8; 16] = hex!("adb41c24 2dbf 4a6d 958b 4457c0d27b95");
const SECURE_MEDIA_DRM_SYSTEM_ID: [u8; 16] = hex!("1f83e1e8 6ee9 4f0d ba2f 5ec4e3ed1a66");
const SECURE_MEDIA_STEEL_KNOT_SYSTEM_ID: [u8; 16] = hex!("992c46e6 c437 4899 b6a0 50fa91ad0e39");
const VIDEO_GUARD_DRM_SYSTEM_ID: [u8; 16] = hex!("a68129d3 575b 4f1a 9cba 3223846cf7c3");
const UNITEND_DRM_SYSTEM_ID: [u8; 16] = hex!("aa11967f cc01 4a4a 8e99 c5d3dddfea2d");
const VERIMATRIX_DRM_SYSTEM_ID: [u8; 16] = hex!("9a27dd82 fde2 4725 8cbc 4234aa06ec09");
const VIACCESS_ORCA_DRM_SYSTEM_ID: [u8; 16] = hex!("b4413586 c58c ffb0 94a5 d4896c1af6c3");
const VISION_CRYPT_DRM_SYSTEM_ID: [u8; 16] = hex!("793b7956 9f94 4946 a942 23e7ef7e44b4");
const W3C_COMMON_PSSH_DRM_SYSTEM_ID: [u8; 16] = hex!("1077efec c0b2 4d02 ace3 3c1e52e2fb4b");
const WIDEVINE_DRM_SYSTEM_ID: [u8; 16] = hex!("edef8ba9 79d6 4ace a3c8 27dcd51d21ed");
impl Pssh {
    pub fn system_reference(&self) -> Cow<'static, str> {
        // Mapping based on https://dashif.org/identifiers/content_protection/
        match self.system_id {
            ABV_DRM_SYSTEM_ID => "ABV DRM (MoDRM)".into(),
            ADOBE_DRM_SYSTEM_ID => "Adobe Primetime DRM version 4".into(),
            ALTICAST_DRM_SYSTEM_ID => "Alticast".into(),
            FAIRPLAY_DRM_SYSTEM_ID => "Apple FairPlay".into(),
            ARRIS_TITANIUM_DRM_SYSTEM_ID => "Arris Titanium".into(),
            CHINA_DRM_DRM_SYSTEM_ID => "ChinaDRM".into(),
            CLEAR_AES_128_DRM_SYSTEM_ID => "Clear Key AES-128".into(),
            CLEAR_SAMPLE_AES_DRM_SYSTEM_ID => "Clear Key SAMPLE-AES".into(),
            CLEAR_DASH_IF_DRM_SYSTEM_ID => "Clear Key DASH-IF".into(),
            CMLA_DRM_SYSTEM_ID => "CMLA (OMA DRM)".into(),
            COMMSCOPE_TITANIUM_V3_DRM_SYSTEM_ID => "Commscope Titanium V3".into(),
            CORE_CRYPT_DRM_SYSTEM_ID => "CoreCrypt".into(),
            DIGI_CAP_SMART_XESS_DRM_SYSTEM_ID => "DigiCAP SmartXess".into(),
            DIV_X_5_DRM_SYSTEM_ID => "DivX DRM Series 5".into(),
            IRDETO_DRM_SYSTEM_ID => "Irdeto Content Protection".into(),
            MARLIN_DRM_SYSTEM_ID => "Marlin Adaptive Streaming Simple Profile V1.0".into(),
            PLAY_READY_DRM_SYSTEM_ID => "Microsoft PlayReady".into(),
            MOBI_TV_DRM_SYSTEM_ID => "MobiTV DRM".into(),
            NAGRA_DRM_SYSTEM_ID => "Nagra MediaAccess PRM 3.0".into(),
            SECURE_MEDIA_DRM_SYSTEM_ID => "SecureMedia".into(),
            SECURE_MEDIA_STEEL_KNOT_SYSTEM_ID => "SecureMedia SteelKnot".into(),
            VIDEO_GUARD_DRM_SYSTEM_ID => "Synamedia/Cisco/NDS VideoGuard DRM".into(),
            UNITEND_DRM_SYSTEM_ID => "Unitend DRM (UDRM)".into(),
            VERIMATRIX_DRM_SYSTEM_ID => "Verimatrix VCAS".into(),
            VIACCESS_ORCA_DRM_SYSTEM_ID => "Viaccess-Orca DRM (VODRM)".into(),
            VISION_CRYPT_DRM_SYSTEM_ID => "VisionCrypt".into(),
            W3C_COMMON_PSSH_DRM_SYSTEM_ID => "W3C Common PSSH box".into(),
            WIDEVINE_DRM_SYSTEM_ID => "Widevine Content Protection".into(),
            id => String::from_utf8_lossy(&id).to_string().into(),
        }
    }
}
impl Atom for Pssh {
    const KIND: FourCC = FourCC::new(b"pssh");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let ext = u32::decode(buf)?;
        let version = ext >> 24;
        let system_id = <[u8; 16]>::decode(buf)?;
        let key_ids = if version > 0 {
            let kid_count = u32::decode(buf)?;
            if kid_count > 4096 {
                return Err(mp4_atom::Error::OutOfMemory);
            }
            let mut kid_vec = Vec::with_capacity(kid_count as usize);
            for _ in 0..kid_count {
                kid_vec.push(<[u8; 16]>::decode(buf)?);
            }
            kid_vec
        } else {
            Vec::new()
        };
        let data_size = u32::decode(buf)? as usize;
        if data_size > 4096 {
            return Err(mp4_atom::Error::OutOfMemory);
        }
        if buf.remaining() < data_size {
            return Err(mp4_atom::Error::OutOfBounds);
        }
        if data_size == 0 {
            return Ok(Self {
                system_id,
                key_ids,
                data: None,
            });
        }
        let mut data = vec![0; data_size];
        data.copy_from_slice(buf.slice(data_size));
        buf.advance(data_size);
        match system_id {
            PLAY_READY_DRM_SYSTEM_ID => {
                let pssh_data = playready::parse_pssh_data(&data).map_err(|e| match e {
                    playready::ParseError::Io(error) => mp4_atom::Error::Io(error),
                    playready::ParseError::Utf16(_) => mp4_atom::Error::Unsupported("UTF-16 error"),
                    playready::ParseError::Xml(_) => mp4_atom::Error::Unsupported("XML error"),
                    playready::ParseError::UnknownType(_) => {
                        mp4_atom::Error::Unsupported("Unknown record type")
                    }
                    playready::ParseError::UnexpectedDataLength { .. } => {
                        mp4_atom::Error::InvalidSize
                    }
                    playready::ParseError::NoWrmData => {
                        mp4_atom::Error::Unsupported("Missing WRMDATA")
                    }
                    playready::ParseError::NoVersion => {
                        mp4_atom::Error::Unsupported("Missing version")
                    }
                    playready::ParseError::UnexpectedEndOfXml => mp4_atom::Error::UnexpectedEof,
                })?;
                Ok(Self {
                    system_id,
                    key_ids,
                    data: Some(PsshData::PlayReady(pssh_data)),
                })
            }
            WIDEVINE_DRM_SYSTEM_ID => {
                let pssh_data = WidevinePsshData::parse_from_bytes(&data)
                    .map_err(|e| mp4_atom::Error::InvalidString(format!("{e:?}")))?;
                Ok(Self {
                    system_id,
                    key_ids,
                    data: Some(PsshData::Widevine(Box::new(pssh_data))),
                })
            }
            _ => Ok(Self {
                system_id,
                key_ids,
                data: Some(PsshData::Raw(data)),
            }),
        }
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

/// TrackEncryptionBox, ISO/IEC 23001-7:2016 Sect 8.2.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tenc {
    pub default_is_protected: u8,
    pub default_per_sample_iv_size: u8,
    pub default_key_id: [u8; 16],
    pub default_constant_iv: Option<Vec<u8>>,
    pub default_crypt_byte_block: Option<u8>,
    pub default_skip_byte_block: Option<u8>,
}
// Field semantics, ISO/IEC 23001-7:2016 Sect 9.1
impl Tenc {
    pub fn is_protected(&self) -> &'static str {
        match self.default_is_protected {
            0 => "Not protected",
            1 => "Protected",
            _ => "Reserved",
        }
    }

    pub fn per_sample_iv_size(&self) -> &'static str {
        match self.default_per_sample_iv_size {
            0 if self.default_is_protected == 0 => "Not protected",
            0 => "Constant IV",
            8 => "64-bit",
            16 => "128-bit",
            _ => "Undocumented in ISO/IEC 23001-7:2016",
        }
    }

    pub fn constant_iv_size(&self) -> &'static str {
        let Some(ref constant_iv) = self.default_constant_iv else {
            return "None";
        };
        match constant_iv.len() {
            8 => "64-bit",
            16 => "128-bit",
            _ => "Undocumented size in ISO/IEC 23001-7:2016",
        }
    }
}
impl Atom for Tenc {
    const KIND: FourCC = FourCC::new(b"tenc");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let ext = u32::decode(buf)?;
        let version = ext >> 24;
        u8::decode(buf)?;
        let (default_crypt_byte_block, default_skip_byte_block) = if version == 0 {
            u8::decode(buf)?;
            (None, None)
        } else {
            let byte_block_info = u8::decode(buf)?;
            let crypt = byte_block_info >> 4;
            let skip = byte_block_info & 0b1111;
            (Some(crypt), Some(skip))
        };
        let default_is_protected = u8::decode(buf)?;
        let default_per_sample_iv_size = u8::decode(buf)?;
        let default_key_id = <[u8; 16]>::decode(buf)?;
        let default_constant_iv = if default_is_protected == 1 && default_per_sample_iv_size == 0 {
            let iv_size = u8::decode(buf)?;
            let mut iv = Vec::with_capacity(iv_size.into());
            for _ in 0..iv_size {
                iv.push(u8::decode(buf)?);
            }
            Some(iv)
        } else {
            None
        };
        Ok(Self {
            default_is_protected,
            default_per_sample_iv_size,
            default_key_id,
            default_constant_iv,
            default_crypt_byte_block,
            default_skip_byte_block,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
