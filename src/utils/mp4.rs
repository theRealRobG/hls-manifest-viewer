use crate::utils::{
    bitter::ByteAlign,
    hex::encode_hex,
    pssh_data::playready::{self, PlayReadyPsshData},
};
use bitter::{BigEndianReader, BitReader};
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

/// SampleEncryptionBox, ISO/IEC 23001-7:2016 Sect 7.2.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Senc {
    pub entries: Vec<SencEntry>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SencEntry {
    pub initialization_vector: String,
    pub subsample_encryption: Vec<SencSubsampleEntry>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SencSubsampleEntry {
    pub bytes_of_clear_data: u16,
    pub bytes_of_protected_data: u32,
}
impl Senc {
    pub const UNKNOWN_IV_SIZE: &str = "IV Size";
}
impl Atom for Senc {
    const KIND: FourCC = FourCC::new(b"senc");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let ext = u32::decode(buf)?;
        let use_sub_sample_encryption = ext & 0x2 == 0b10;
        let sample_count = u32::decode(buf)? as usize;
        if sample_count > 4096 {
            return Err(mp4_atom::Error::OutOfMemory);
        }
        if use_sub_sample_encryption {
            // If we are using subsample encryption, then we can't really know what the
            // Per_Sample_IV_Size is, so we try with 0 first then 8 then 16, since those are the
            // only sizes defined. If not any of those then we just fail. In reality, we should be
            // getting this value from somewhere like the `tenc`; however, we don't support
            // depending on another box, so we're making best efforts here (tenc would be
            // particularly awkward because that is in the init segment while this would be in one
            // of the media segments).
            let mut entries = Vec::with_capacity(sample_count);
            // I'm allowing this clippy lint, because if I chain the last 2 else if blocks with an
            // ||, I think that is actually less readable.
            #[allow(clippy::if_same_then_else)]
            if decode_senc_entries_with_subsamples(
                // Because we are going to be trying to decode the buffer multiple times, we don't
                // want to consume the bytes each time, as then subsequent decodes will fail (over
                // decode). Therefore, we copy the remaining data for each decode, so each time
                // there is a fresh copy of the original data.
                &mut buf.slice(buf.remaining()),
                sample_count,
                &mut entries,
                |_| Ok(String::from("0")),
            )
            .is_ok()
            {
                // Since the decoding happened on a copy of the original buffer, it has not been
                // advanced, so we must advance it now. We know it is safe to do so as we have
                // already validated the correct number of bytes were used in the successful decode
                // of the entries.
                buf.advance(buf.remaining());
                Ok(Self { entries })
            } else if decode_senc_entries_with_subsamples(
                &mut buf.slice(buf.remaining()),
                sample_count,
                &mut entries,
                |buf| {
                    Ok(format!(
                        "0x{}",
                        encode_hex(&u64::decode(buf)?.to_be_bytes())
                    ))
                },
            )
            .is_ok()
            {
                buf.advance(buf.remaining());
                Ok(Self { entries })
            } else if decode_senc_entries_with_subsamples(
                &mut buf.slice(buf.remaining()),
                sample_count,
                &mut entries,
                |buf| {
                    Ok(format!(
                        "0x{}",
                        encode_hex(&u128::from_be_bytes(<[u8; 16]>::decode(buf)?).to_be_bytes())
                    ))
                },
            )
            .is_ok()
            {
                buf.advance(buf.remaining());
                Ok(Self { entries })
            } else {
                Err(mp4_atom::Error::Unsupported(Self::UNKNOWN_IV_SIZE))
            }
        } else {
            // If we aren't using subsample encryption, then we can deduce the size of the IV based
            // on how many bytes are left and the sample_count (it must divide exactly).
            let entries = decode_senc_entries_no_subsamples(buf, sample_count)?;
            Ok(Self { entries })
        }
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
fn decode_senc_entries_no_subsamples<B: Buf>(
    buf: &mut B,
    sample_count: usize,
) -> Result<Vec<SencEntry>> {
    let mut entries = Vec::with_capacity(sample_count);
    let iv_size = buf.remaining() / sample_count;
    match iv_size {
        0 => Ok(Vec::new()),
        8 => {
            for _ in 0..sample_count {
                let iv = u64::decode(buf)?;
                entries.push(SencEntry {
                    initialization_vector: format!("0x{}", encode_hex(&iv.to_be_bytes())),
                    subsample_encryption: Vec::new(),
                });
            }
            Ok(entries)
        }
        16 => {
            for _ in 0..sample_count {
                let iv = u128::from_be_bytes(<[u8; 16]>::decode(buf)?);
                entries.push(SencEntry {
                    initialization_vector: format!("0x{}", encode_hex(&iv.to_be_bytes())),
                    subsample_encryption: Vec::new(),
                });
            }
            Ok(entries)
        }
        _ => Err(mp4_atom::Error::Unsupported(Senc::UNKNOWN_IV_SIZE)),
    }
}
fn decode_senc_entries_with_subsamples<B, F>(
    buf: &mut B,
    sample_count: usize,
    entries: &mut Vec<SencEntry>,
    mut iv_string: F,
) -> Result<()>
where
    B: Buf,
    F: FnMut(&mut B) -> Result<String>,
{
    entries.clear();
    for _ in 0..sample_count {
        let initialization_vector = iv_string(buf)?;
        let subsample_encryption = decode_senc_subsamples(buf)?;
        entries.push(SencEntry {
            initialization_vector,
            subsample_encryption,
        });
    }
    if buf.has_remaining() {
        Err(mp4_atom::Error::UnderDecode(Senc::KIND))
    } else {
        Ok(())
    }
}
fn decode_senc_subsamples<B: Buf>(buf: &mut B) -> Result<Vec<SencSubsampleEntry>> {
    let subsample_count = u16::decode(buf)?;
    if subsample_count > 4096 {
        return Err(mp4_atom::Error::OutOfMemory);
    }
    let mut subsample_encryption = Vec::with_capacity(usize::from(subsample_count));
    for _ in 0..subsample_count {
        let bytes_of_clear_data = u16::decode(buf)?;
        let bytes_of_protected_data = u32::decode(buf)?;
        subsample_encryption.push(SencSubsampleEntry {
            bytes_of_clear_data,
            bytes_of_protected_data,
        });
    }
    Ok(subsample_encryption)
}

/// ColourInformationBox, ISO/IEC 14496-12:2024 Sect 12.1.5
///
/// With additional `nclc` case from QuickTime File Format:
/// https://developer.apple.com/documentation/quicktime-file-format/color_parameter_atom/color_parameter_type
///
/// This implementation copy+pastes and slightly changes the implementation in mp4-atom, providing
/// the following changes:
/// * Addition of `nclc` option defined in QuickTime File Format
/// * Loosens error case for unknown type by creating unknown case and storing vector of bytes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Colr {
    Nclx {
        colour_primaries: u16,
        transfer_characteristics: u16,
        matrix_coefficients: u16,
        full_range_flag: bool,
    },
    Ricc {
        profile: Vec<u8>,
    },
    Prof {
        profile: Vec<u8>,
    },
    Nclc {
        primaries_index: u16,
        transfer_function_index: u16,
        matrix_index: u16,
    },
    Unknown {
        colour_type: FourCC,
        bytes: Vec<u8>,
    },
}
impl Atom for Colr {
    const KIND: FourCC = FourCC::new(b"colr");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let colour_type = FourCC::decode(buf)?;
        if colour_type == FourCC::new(b"nclx") {
            let colour_primaries = u16::decode(buf)?;
            let transfer_characteristics = u16::decode(buf)?;
            let matrix_coefficients = u16::decode(buf)?;
            let full_range_flag = u8::decode(buf)? == 0x80;
            Ok(Colr::Nclx {
                colour_primaries,
                transfer_characteristics,
                matrix_coefficients,
                full_range_flag,
            })
        } else if colour_type == FourCC::new(b"prof") {
            let profile_len = buf.remaining();
            let profile = buf.slice(profile_len).to_vec();
            Ok(Colr::Prof { profile })
        } else if colour_type == FourCC::new(b"rICC") {
            let profile_len = buf.remaining();
            let profile = buf.slice(profile_len).to_vec();
            Ok(Colr::Ricc { profile })
        } else if colour_type == FourCC::new(b"nclc") {
            let primaries_index = u16::decode(buf)?;
            let transfer_function_index = u16::decode(buf)?;
            let matrix_index = u16::decode(buf)?;
            Ok(Colr::Nclc {
                primaries_index,
                transfer_function_index,
                matrix_index,
            })
        } else {
            let remaining_len = buf.remaining();
            let bytes = buf.slice(remaining_len).to_vec();
            Ok(Colr::Unknown { colour_type, bytes })
        }
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!();
    }
}

/// AC4SpecificBox, ETSI TS 103 190-2 V1.2.1 (2018-02) Sect E.5.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dac4 {
    ac4_dsi_version: u8,
    bitstream_version: u8,
    fs_index: bool,
    frame_rate_index: u8,
    short_program_id: Option<u16>,
    program_uuid: Option<[u8; 16]>,
    bit_rate_mode: Ac4BitrateMode,
    bit_rate: u32,
    bit_rate_precision: u32,
    presentations: Vec<Ac4Presentation>,
}
const READ_ERR: mp4_atom::Error = mp4_atom::Error::OutOfBounds;
impl Atom for Dac4 {
    const KIND: FourCC = FourCC::new(b"dac4");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut reader = BigEndianReader::new(buf.slice(buf.remaining()));
        let starting_bytes_remaining = reader.bytes_remaining();
        let ac4_dsi_version = reader.read_bits(3).ok_or(READ_ERR)? as u8;
        let bitstream_version = reader.read_bits(7).ok_or(READ_ERR)? as u8;
        let fs_index = reader.read_bit().ok_or(READ_ERR)?;
        let frame_rate_index = reader.read_bits(4).ok_or(READ_ERR)? as u8;
        let n_presentations = reader.read_bits(9).ok_or(READ_ERR)?;
        let (short_program_id, program_uuid) = if bitstream_version > 1 {
            let b_program_id = reader.read_bit().ok_or(READ_ERR)?;
            if b_program_id {
                let short_program_id = reader.read_u16().ok_or(READ_ERR)?;
                let b_uuid = reader.read_bit().ok_or(READ_ERR)?;
                if b_uuid {
                    let mut program_uuid = [0u8; 16];
                    reader
                        .read_bytes(&mut program_uuid)
                        .then(|| 0)
                        .ok_or(READ_ERR)?;
                    (Some(short_program_id), Some(program_uuid))
                } else {
                    (Some(short_program_id), None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        let bit_rate_mode = Ac4BitrateMode::from(reader.read_bits(2).ok_or(READ_ERR)? as u8);
        let bit_rate = reader.read_u32().ok_or(READ_ERR)?;
        let bit_rate_precision = reader.read_u32().ok_or(READ_ERR)?;
        reader.align().map_err(|_| READ_ERR)?;
        let mut presentations = Vec::with_capacity(n_presentations as usize);
        for _ in 0..n_presentations {
            let presentation_version = reader.read_u8().ok_or(READ_ERR)?;
            let mut pres_bytes = reader.read_u8().ok_or(READ_ERR)? as usize;
            if pres_bytes == 255 {
                let add_pres_bytes = reader.read_u16().ok_or(READ_ERR)? as usize;
                pres_bytes += add_pres_bytes;
            }
            let remaining_before_reading_presentation = reader.bytes_remaining();
            if presentation_version == 0 {
                presentations.push(Ac4Presentation::V0(ac4_presentation_v0_dsi(&mut reader)?));
            } else if presentation_version == 1 {
                presentations.push(Ac4Presentation::V1(ac4_presentation_v1_dsi(
                    &mut reader,
                    pres_bytes,
                )?));
            } else if presentation_version == 2 {
                // V2 extension provided by:
                // https://media.developer.dolby.com/AC4/AC4_DASH_for_BROADCAST_SPEC.pdf
                presentations.push(Ac4Presentation::V2(ac4_presentation_v1_dsi(
                    &mut reader,
                    pres_bytes,
                )?));
            } else {
                presentations.push(Ac4Presentation::UnknownVersion(presentation_version));
            }
            let presentation_bytes =
                remaining_before_reading_presentation - reader.bytes_remaining();
            if pres_bytes < presentation_bytes {
                return Err(mp4_atom::Error::Unsupported(
                    "dac4 pres_bytes < presentation_bytes",
                ));
            }
            let mut skip_bytes = vec![0u8; pres_bytes - presentation_bytes];
            reader
                .read_bytes(&mut skip_bytes)
                .then(|| 0)
                .ok_or(READ_ERR)?;
        }
        let ending_bytes_remaining = reader.bytes_remaining();
        buf.advance(starting_bytes_remaining - ending_bytes_remaining);
        Ok(Dac4 {
            ac4_dsi_version,
            bitstream_version,
            fs_index,
            frame_rate_index,
            short_program_id,
            program_uuid,
            bit_rate_mode,
            bit_rate,
            bit_rate_precision,
            presentations,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ac4BitrateMode {
    NotSpecified,
    Constant,
    Average,
    Variable,
}
impl From<u8> for Ac4BitrateMode {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Constant,
            2 => Self::Average,
            3 => Self::Variable,
            _ => Self::NotSpecified,
        }
    }
}
impl Display for Ac4BitrateMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSpecified => write!(f, "not specified"),
            Self::Constant => write!(f, "constant"),
            Self::Average => write!(f, "average"),
            Self::Variable => write!(f, "variable"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ac4Presentation {
    V0(Ac4PresentationV0),
    V1(Ac4PresentationV1),
    // V2 extension provided by:
    // https://media.developer.dolby.com/AC4/AC4_DASH_for_BROADCAST_SPEC.pdf
    V2(Ac4PresentationV1),
    UnknownVersion(u8),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4PresentationV0 {
    presentation_config: u8,
    md_compat: Option<u8>,
    presentation_id: Option<u8>,
    dsi_frame_rate_multiply_info: Option<u8>,
    presentation_emdf_version: Option<u8>,
    presentation_key_id: Option<u16>,
    presentation_channel_mask: Option<[u8; 3]>,
    b_hsf_ext: Option<bool>,
    substream_groups: Option<Vec<Ac4SubstreamGroup>>,
    b_pre_virtualized: Option<bool>,
    emdf_substreams: Vec<EmdfSubstream>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4PresentationV1 {
    presentation_config_v1: u8,
    md_compat: Option<u8>,
    presentation_id: Option<u8>,
    dsi_frame_rate_multiply_info: Option<u8>,
    dsi_frame_rate_fraction_info: Option<u8>,
    presentation_emdf_version: Option<u8>,
    presentation_key_id: Option<u16>,
    b_presentation_channel_coded: Option<bool>,
    dsi_presentation_ch_mode: Option<u8>,
    pres_b_4_back_channels_present: Option<bool>,
    pres_top_channel_pairs: Option<u8>,
    presentation_channel_mask_v1: Option<[u8; 3]>,
    b_presentation_core_differs: Option<bool>,
    b_presentation_core_channel_coded: Option<bool>,
    dsi_presentation_channel_mode_core: Option<u8>,
    b_presentation_filter: Option<bool>,
    b_enable_presentation: Option<bool>,
    filter_data: Option<Vec<u8>>,
    b_multi_pid: Option<bool>,
    substream_groups: Option<Vec<Ac4SubstreamGroup>>,
    b_pre_virtualized: Option<bool>,
    emdf_substreams: Vec<EmdfSubstream>,
    bit_rate_mode: Option<Ac4BitrateMode>,
    bit_rate: Option<u32>,
    bit_rate_precision: Option<u32>,
    alternative_info: Option<Ac4AlternativeInfo>,
    de_indicator: Option<bool>,
    immersive_audio_indicator: Option<bool>,
    extended_presentation_id: Option<u16>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4SubstreamGroup {
    b_substreams_present: bool,
    b_hsf_ext: bool,
    b_channel_coded: bool,
    substreams: Vec<Ac4PresentationSubstream>,
    content_classifier: Option<Ac4ContentClassifier>,
    language_tag: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmdfSubstream {
    emdf_version: u8,
    key_id: u16,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4PresentationSubstream {
    dsi_sf_multiplier: u8,
    bitrate_indicator: Option<u8>,
    channel_mask: Option<[u8; 3]>,
    n_dmx_objects_minus1: Option<u8>,
    n_umx_objects_minus1: Option<u8>,
    contains_bed_objects: Option<bool>,
    contains_dynamic_objects: Option<bool>,
    contains_isf_objects: Option<bool>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ac4ContentClassifier {
    CompleteMain,     // 000
    MusicAndEffects,  // 001
    VisuallyImpaired, // 010
    HearingImpaired,  // 011
    Dialogue,         // 100
    Commentary,       // 101
    Emergency,        // 110
    VoiceOver,        // 111
}
impl From<u8> for Ac4ContentClassifier {
    fn from(value: u8) -> Self {
        match value {
            0b001 => Self::MusicAndEffects,
            0b010 => Self::VisuallyImpaired,
            0b011 => Self::HearingImpaired,
            0b100 => Self::Dialogue,
            0b101 => Self::Commentary,
            0b110 => Self::Emergency,
            0b111 => Self::VoiceOver,
            _ => Self::CompleteMain,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4AlternativeInfo {
    presentation_name: String,
    targets: Vec<Ac4AlternativeInfoTarget>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4AlternativeInfoTarget {
    md_compat: u8,
    device_category: u8,
}
fn ac4_presentation_v0_dsi(reader: &mut BigEndianReader) -> Result<Ac4PresentationV0> {
    let presentation_config = reader.read_bits(5).ok_or(READ_ERR)? as u8;
    let (
        md_compat,
        presentation_id,
        dsi_frame_rate_multiply_info,
        presentation_emdf_version,
        presentation_key_id,
        presentation_channel_mask,
        b_hsf_ext,
        substream_groups,
        b_pre_virtualized,
        b_add_emdf_substreams,
    ) = if presentation_config == 0x06 {
        (None, None, None, None, None, None, None, None, None, true)
    } else {
        let md_compat = reader.read_bits(3).ok_or(READ_ERR)? as u8;
        let b_presentation_id = reader.read_bit().ok_or(READ_ERR)?;
        let presentation_id = if b_presentation_id {
            Some(reader.read_bits(5).ok_or(READ_ERR)? as u8)
        } else {
            None
        };
        let dsi_frame_rate_multiply_info = reader.read_bits(2).ok_or(READ_ERR)? as u8;
        let presentation_emdf_version = reader.read_bits(5).ok_or(READ_ERR)? as u8;
        let presentation_key_id = reader.read_bits(10).ok_or(READ_ERR)? as u16;
        let mut presentation_channel_mask = [0u8; 3];
        reader
            .read_bytes(&mut presentation_channel_mask)
            .then(|| 0)
            .ok_or(READ_ERR)?;
        // ETSI TS 103 190-1 v1.4.1 Table E.5a
        // > This virtual variable shall be considered to be `true` if presentation_config is set to
        // > 0x1F, otherwise it shall be considered as being `false`.
        let b_single_substream = presentation_config == 0x1F;
        let (b_hsf_ext, substream_groups) = if b_single_substream {
            (None, Some(vec![ac4_substream_group_dsi(reader)?]))
        } else {
            let b_hsf_ext = reader.read_bit().ok_or(READ_ERR)?;
            if [0u8, 1, 2].contains(&presentation_config) {
                (
                    Some(b_hsf_ext),
                    Some(vec![
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                    ]),
                )
            } else if [3u8, 4].contains(&presentation_config) {
                (
                    Some(b_hsf_ext),
                    Some(vec![
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                    ]),
                )
            } else if presentation_config == 5 {
                let n_substream_groups_minus2 = reader.read_bits(3).ok_or(READ_ERR)? as usize;
                let n_substream_groups = n_substream_groups_minus2 + 2;
                let mut substream_groups = Vec::with_capacity(n_substream_groups);
                for _ in 0..n_substream_groups {
                    substream_groups.push(ac4_substream_group_dsi(reader)?);
                }
                (Some(b_hsf_ext), Some(substream_groups))
            } else {
                let n_skip_bytes = reader.read_bits(7).ok_or(READ_ERR)?;
                for _ in 0..n_skip_bytes {
                    _ = reader.read_u8().ok_or(READ_ERR)?;
                }
                (Some(b_hsf_ext), None)
            }
        };
        let b_pre_virtualized = reader.read_bit().ok_or(READ_ERR)?;
        let b_add_emdf_substreams = reader.read_bit().ok_or(READ_ERR)?;
        (
            Some(md_compat),
            presentation_id,
            Some(dsi_frame_rate_multiply_info),
            Some(presentation_emdf_version),
            Some(presentation_key_id),
            Some(presentation_channel_mask),
            b_hsf_ext,
            substream_groups,
            Some(b_pre_virtualized),
            b_add_emdf_substreams,
        )
    };
    let emdf_substreams = if b_add_emdf_substreams {
        let n_add_emdf_substreams = reader.read_bits(7).ok_or(READ_ERR)? as usize;
        let mut emdf_substreams = Vec::with_capacity(n_add_emdf_substreams);
        for _ in 0..n_add_emdf_substreams {
            let emdf_version = reader.read_bits(5).ok_or(READ_ERR)? as u8;
            let key_id = reader.read_bits(10).ok_or(READ_ERR)? as u16;
            emdf_substreams.push(EmdfSubstream {
                emdf_version,
                key_id,
            });
        }
        emdf_substreams
    } else {
        Vec::new()
    };
    reader.align().map_err(|_| READ_ERR)?;

    Ok(Ac4PresentationV0 {
        presentation_config,
        md_compat,
        presentation_id,
        dsi_frame_rate_multiply_info,
        presentation_emdf_version,
        presentation_key_id,
        presentation_channel_mask,
        b_hsf_ext,
        substream_groups,
        b_pre_virtualized,
        emdf_substreams,
    })
}
fn ac4_presentation_v1_dsi(
    reader: &mut BigEndianReader,
    pres_bytes: usize,
) -> Result<Ac4PresentationV1> {
    let starting_bytes = reader.bytes_remaining();
    let presentation_config_v1 = reader.read_bits(5).ok_or(READ_ERR)? as u8;
    let (
        md_compat,
        presentation_id,
        dsi_frame_rate_multiply_info,
        dsi_frame_rate_fraction_info,
        presentation_emdf_version,
        presentation_key_id,
        b_presentation_channel_coded,
        dsi_presentation_ch_mode,
        pres_b_4_back_channels_present,
        pres_top_channel_pairs,
        presentation_channel_mask_v1,
        b_presentation_core_differs,
        b_presentation_core_channel_coded,
        dsi_presentation_channel_mode_core,
        b_presentation_filter,
        b_enable_presentation,
        filter_data,
        b_multi_pid,
        substream_groups,
        b_pre_virtualized,
        b_add_emdf_substreams,
    ) = if presentation_config_v1 == 0x06 {
        (
            None, None, None, None, None, None, None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, true,
        )
    } else {
        let md_compat = reader.read_bits(3).ok_or(READ_ERR)? as u8;
        let b_presentation_id = reader.read_bit().ok_or(READ_ERR)?;
        let presentation_id = if b_presentation_id {
            Some(reader.read_bits(5).ok_or(READ_ERR)? as u8)
        } else {
            None
        };
        let dsi_frame_rate_multiply_info = reader.read_bits(2).ok_or(READ_ERR)? as u8;
        let dsi_frame_rate_fraction_info = reader.read_bits(2).ok_or(READ_ERR)? as u8;
        let presentation_emdf_version = reader.read_bits(5).ok_or(READ_ERR)? as u8;
        let presentation_key_id = reader.read_bits(10).ok_or(READ_ERR)? as u16;
        let b_presentation_channel_coded = reader.read_bit().ok_or(READ_ERR)?;
        let (
            dsi_presentation_ch_mode,
            pres_b_4_back_channels_present,
            pres_top_channel_pairs,
            presentation_channel_mask_v1,
        ) = if b_presentation_channel_coded {
            let dsi_presentation_ch_mode = reader.read_bits(5).ok_or(READ_ERR)? as u8;
            let (pres_b_4_back_channels_present, pres_top_channel_pairs) =
                if [11u8, 12, 13, 14].contains(&dsi_presentation_ch_mode) {
                    let pres_b_4_back_channels_present = reader.read_bit().ok_or(READ_ERR)?;
                    let pres_top_channel_pairs = reader.read_bits(2).ok_or(READ_ERR)? as u8;
                    (
                        Some(pres_b_4_back_channels_present),
                        Some(pres_top_channel_pairs),
                    )
                } else {
                    (None, None)
                };
            let mut presentation_channel_mask_v1 = [0u8; 3];
            reader
                .read_bytes(&mut presentation_channel_mask_v1)
                .then(|| 0)
                .ok_or(READ_ERR)?;
            (
                Some(dsi_presentation_ch_mode),
                pres_b_4_back_channels_present,
                pres_top_channel_pairs,
                Some(presentation_channel_mask_v1),
            )
        } else {
            (None, None, None, None)
        };
        let b_presentation_core_differs = reader.read_bit().ok_or(READ_ERR)?;
        let (b_presentation_core_channel_coded, dsi_presentation_channel_mode_core) =
            if b_presentation_core_differs {
                let b_presentation_core_channel_coded = reader.read_bit().ok_or(READ_ERR)?;
                if b_presentation_core_channel_coded {
                    let dsi_presentation_channel_mode_core =
                        reader.read_bits(2).ok_or(READ_ERR)? as u8;
                    (
                        Some(b_presentation_core_channel_coded),
                        Some(dsi_presentation_channel_mode_core),
                    )
                } else {
                    (Some(b_presentation_core_channel_coded), None)
                }
            } else {
                (None, None)
            };
        let b_presentation_filter = reader.read_bit().ok_or(READ_ERR)?;
        let (b_enable_presentation, filter_data) = if b_presentation_filter {
            let b_enable_presentation = reader.read_bit().ok_or(READ_ERR)?;
            let n_filter_bytes = reader.read_u8().ok_or(READ_ERR)? as usize;
            let mut filter_data = Vec::with_capacity(n_filter_bytes);
            for _ in 0..n_filter_bytes {
                filter_data.push(reader.read_u8().ok_or(READ_ERR)?);
            }
            (Some(b_enable_presentation), Some(filter_data))
        } else {
            (None, None)
        };
        let (b_multi_pid, substream_groups) = if presentation_config_v1 == 0x1F {
            (None, Some(vec![ac4_substream_group_dsi(reader)?]))
        } else {
            let b_multi_pid = reader.read_bit().ok_or(READ_ERR)?;
            if [0u8, 1, 2].contains(&presentation_config_v1) {
                (
                    Some(b_multi_pid),
                    Some(vec![
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                    ]),
                )
            } else if [3u8, 4].contains(&presentation_config_v1) {
                (
                    Some(b_multi_pid),
                    Some(vec![
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                        ac4_substream_group_dsi(reader)?,
                    ]),
                )
            } else if presentation_config_v1 == 5 {
                let n_substream_groups_minus2 = reader.read_bits(3).ok_or(READ_ERR)? as usize;
                let n_substream_groups = n_substream_groups_minus2 + 2;
                let mut substream_groups = Vec::with_capacity(n_substream_groups);
                for _ in 0..n_substream_groups {
                    substream_groups.push(ac4_substream_group_dsi(reader)?);
                }
                (Some(b_multi_pid), Some(substream_groups))
            } else {
                let n_skip_bytes = reader.read_bits(7).ok_or(READ_ERR)?;
                for _ in 0..n_skip_bytes {
                    _ = reader.read_u8().ok_or(READ_ERR)?;
                }
                (Some(b_multi_pid), None)
            }
        };
        let b_pre_virtualized = reader.read_bit().ok_or(READ_ERR)?;
        let b_add_emdf_substreams = reader.read_bit().ok_or(READ_ERR)?;
        (
            Some(md_compat),
            presentation_id,
            Some(dsi_frame_rate_multiply_info),
            Some(dsi_frame_rate_fraction_info),
            Some(presentation_emdf_version),
            Some(presentation_key_id),
            Some(b_presentation_channel_coded),
            dsi_presentation_ch_mode,
            pres_b_4_back_channels_present,
            pres_top_channel_pairs,
            presentation_channel_mask_v1,
            Some(b_presentation_core_differs),
            b_presentation_core_channel_coded,
            dsi_presentation_channel_mode_core,
            Some(b_presentation_filter),
            b_enable_presentation,
            filter_data,
            b_multi_pid,
            substream_groups,
            Some(b_pre_virtualized),
            b_add_emdf_substreams,
        )
    };
    let emdf_substreams = if b_add_emdf_substreams {
        let n_add_emdf_substreams = reader.read_bits(7).ok_or(READ_ERR)? as usize;
        let mut emdf_substreams = Vec::with_capacity(n_add_emdf_substreams);
        for _ in 0..n_add_emdf_substreams {
            let emdf_version = reader.read_bits(5).ok_or(READ_ERR)? as u8;
            let key_id = reader.read_bits(10).ok_or(READ_ERR)? as u16;
            emdf_substreams.push(EmdfSubstream {
                emdf_version,
                key_id,
            });
        }
        emdf_substreams
    } else {
        Vec::new()
    };
    let b_presentation_bitrate_info = reader.read_bit().ok_or(READ_ERR)?;
    let (bit_rate_mode, bit_rate, bit_rate_precision) = if b_presentation_bitrate_info {
        let bit_rate_mode = Ac4BitrateMode::from(reader.read_bits(2).ok_or(READ_ERR)? as u8);
        let bit_rate = reader.read_u32().ok_or(READ_ERR)?;
        let bit_rate_precision = reader.read_u32().ok_or(READ_ERR)?;
        (
            Some(bit_rate_mode),
            Some(bit_rate),
            Some(bit_rate_precision),
        )
    } else {
        (None, None, None)
    };
    let b_alternative = reader.read_bit().ok_or(READ_ERR)?;
    let alternative_info = if b_alternative {
        reader.align().map_err(|_| READ_ERR)?;
        let name_len = reader.read_u16().ok_or(READ_ERR)? as usize;
        let mut presentation_name_vec = vec![0; name_len];
        reader
            .read_bytes(&mut presentation_name_vec)
            .then(|| 0)
            .ok_or(READ_ERR)?;
        let presentation_name = String::from_utf8_lossy(&presentation_name_vec).to_string();
        let n_targets = reader.read_bits(5).ok_or(READ_ERR)? as usize;
        let mut targets = Vec::with_capacity(n_targets);
        for _ in 0..n_targets {
            let md_compat = reader.read_bits(3).ok_or(READ_ERR)? as u8;
            let device_category = reader.read_u8().ok_or(READ_ERR)? as u8;
            targets.push(Ac4AlternativeInfoTarget {
                md_compat,
                device_category,
            });
        }
        Some(Ac4AlternativeInfo {
            presentation_name,
            targets,
        })
    } else {
        None
    };
    reader.align().map_err(|_| READ_ERR)?;
    let bits_read = (starting_bytes - reader.bytes_remaining()) * 8;
    let (de_indicator, immersive_audio_indicator, extended_presentation_id) =
        if bits_read <= (pres_bytes - 1) * 8 {
            let de_indicator = reader.read_bit().ok_or(READ_ERR)?;
            let immersive_audio_indicator = reader.read_bit().ok_or(READ_ERR)?;
            _ = reader.read_bits(4).ok_or(READ_ERR)?;
            let b_extended_presentation_id = reader.read_bit().ok_or(READ_ERR)?;
            if b_extended_presentation_id {
                let extended_presentation_id = reader.read_bits(9).ok_or(READ_ERR)? as u16;
                (
                    Some(de_indicator),
                    Some(immersive_audio_indicator),
                    Some(extended_presentation_id),
                )
            } else {
                _ = reader.read_bit().ok_or(READ_ERR)?;
                (Some(de_indicator), Some(immersive_audio_indicator), None)
            }
        } else {
            (None, None, None)
        };

    Ok(Ac4PresentationV1 {
        presentation_config_v1,
        md_compat,
        presentation_id,
        dsi_frame_rate_multiply_info,
        dsi_frame_rate_fraction_info,
        presentation_emdf_version,
        presentation_key_id,
        b_presentation_channel_coded,
        dsi_presentation_ch_mode,
        pres_b_4_back_channels_present,
        pres_top_channel_pairs,
        presentation_channel_mask_v1,
        b_presentation_core_differs,
        b_presentation_core_channel_coded,
        dsi_presentation_channel_mode_core,
        b_presentation_filter,
        b_enable_presentation,
        filter_data,
        b_multi_pid,
        substream_groups,
        b_pre_virtualized,
        emdf_substreams,
        bit_rate_mode,
        bit_rate,
        bit_rate_precision,
        alternative_info,
        de_indicator,
        immersive_audio_indicator,
        extended_presentation_id,
    })
}
fn ac4_substream_group_dsi(reader: &mut BigEndianReader) -> Result<Ac4SubstreamGroup> {
    let b_substreams_present = reader.read_bit().ok_or(READ_ERR)?;
    let b_hsf_ext = reader.read_bit().ok_or(READ_ERR)?;
    let b_channel_coded = reader.read_bit().ok_or(READ_ERR)?;
    let n_substreams = reader.read_u8().ok_or(READ_ERR)? as usize;
    let mut substreams = Vec::with_capacity(n_substreams);
    for _ in 0..n_substreams {
        let dsi_sf_multiplier = reader.read_bits(2).ok_or(READ_ERR)? as u8;
        let b_substream_bitrate_indicator = reader.read_bit().ok_or(READ_ERR)?;
        let bitrate_indicator = if b_substream_bitrate_indicator {
            Some(reader.read_bits(5).ok_or(READ_ERR)? as u8)
        } else {
            None
        };
        let (
            channel_mask,
            n_dmx_objects_minus1,
            n_umx_objects_minus1,
            contains_bed_objects,
            contains_dynamic_objects,
            contains_isf_objects,
        ) = if b_channel_coded {
            let mut channel_mask = [0u8; 3];
            reader
                .read_bytes(&mut channel_mask)
                .then(|| 0)
                .ok_or(READ_ERR)?;
            (Some(channel_mask), None, None, None, None, None)
        } else {
            let b_ajoc = reader.read_bit().ok_or(READ_ERR)?;
            let (dmx, umx) = if b_ajoc {
                let b_static_dmx = reader.read_bit().ok_or(READ_ERR)?;
                let dmx = if !b_static_dmx {
                    Some(reader.read_bits(4).ok_or(READ_ERR)? as u8)
                } else {
                    None
                };
                let umx = reader.read_bits(6).ok_or(READ_ERR)? as u8;
                (dmx, Some(umx))
            } else {
                (None, None)
            };
            let contains_bed_objects = reader.read_bit().ok_or(READ_ERR)?;
            let contains_dynamic_objects = reader.read_bit().ok_or(READ_ERR)?;
            let contains_isf_objects = reader.read_bit().ok_or(READ_ERR)?;
            _ = reader.read_bit().ok_or(READ_ERR)?;
            (
                None,
                dmx,
                umx,
                Some(contains_bed_objects),
                Some(contains_dynamic_objects),
                Some(contains_isf_objects),
            )
        };
        substreams.push(Ac4PresentationSubstream {
            dsi_sf_multiplier,
            bitrate_indicator,
            channel_mask,
            n_dmx_objects_minus1,
            n_umx_objects_minus1,
            contains_bed_objects,
            contains_dynamic_objects,
            contains_isf_objects,
        });
    }
    let b_content_type = reader.read_bit().ok_or(READ_ERR)?;
    let (content_classifier, language_tag) = if b_content_type {
        let content_classifier =
            Ac4ContentClassifier::from(reader.read_bits(3).ok_or(READ_ERR)? as u8);
        let b_language_indicator = reader.read_bit().ok_or(READ_ERR)?;
        if b_language_indicator {
            let n_language_tag_bytes = reader.read_bits(6).ok_or(READ_ERR)? as usize;
            let mut language_tag_bytes = vec![0; n_language_tag_bytes];
            reader
                .read_bytes(&mut language_tag_bytes)
                .then(|| 0)
                .ok_or(READ_ERR)?;
            (
                Some(content_classifier),
                Some(String::from_utf8_lossy(&language_tag_bytes).to_string()),
            )
        } else {
            (Some(content_classifier), None)
        }
    } else {
        (None, None)
    };

    Ok(Ac4SubstreamGroup {
        b_substreams_present,
        b_hsf_ext,
        b_channel_coded,
        substreams,
        content_classifier,
        language_tag,
    })
}

/// AC4PresentationLabelBox, ETSI TS 103 190-2 V1.3.1 (2025-07) Sect E.5a
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lac4 {
    pub language_tag: String,
    pub labels: Vec<Ac4PresentationLabel>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4PresentationLabel {
    pub id: u16,
    pub label: String,
}
impl Atom for Lac4 {
    const KIND: FourCC = FourCC::new(b"lac4");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags not used
        let num_presentation_labels = u16::decode(buf)?;
        let language_tag = String::decode(buf)?;
        let mut labels = Vec::with_capacity(usize::from(num_presentation_labels));
        for _ in 0..num_presentation_labels {
            let id = u16::decode(buf)?;
            let label = String::decode(buf)?;
            labels.push(Ac4PresentationLabel { id, label });
        }
        Ok(Self {
            language_tag,
            labels,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    // Test dac4 atoms found here:
    // https://ott.dolby.com/OnDelKits/AC-4/Dolby_AC-4_Online_Delivery_Kit_1.5/help_files/topics/kit_wrapper_MP4_multiplexed_streams.html
    use super::*;
    use pretty_assertions::assert_eq;
    use std::io::Cursor;

    #[test]
    fn dac4_test() {
        const DAC4: &[u8] = &[
            0x00, 0x00, 0x00, 0x25, 0x64, 0x61, 0x63, 0x34, 0x20, 0xA6, 0x01, 0x40, 0x00, 0x00,
            0x00, 0x1F, 0xFF, 0xFF, 0xFF, 0xE0, 0x01, 0x0F, 0xF9, 0x80, 0x00, 0x00, 0x48, 0x00,
            0x00, 0x8E, 0x50, 0x10, 0x00, 0x00, 0x8F, 0x00, 0x80,
        ];
        let mut buf = Cursor::new(DAC4);
        // Validated with https://ott.dolby.com/OnDel_tools/mp4_inspector/index.html
        assert_eq!(
            Dac4 {
                ac4_dsi_version: 1,
                bitstream_version: 2,
                fs_index: true,
                frame_rate_index: 3,
                short_program_id: None,
                program_uuid: None,
                bit_rate_mode: Ac4BitrateMode::Average,
                bit_rate: 0,
                bit_rate_precision: 4294967295,
                presentations: vec![Ac4Presentation::V1(Ac4PresentationV1 {
                    presentation_config_v1: 31,
                    md_compat: Some(1),
                    presentation_id: Some(0),
                    dsi_frame_rate_multiply_info: Some(0),
                    dsi_frame_rate_fraction_info: Some(0),
                    presentation_emdf_version: Some(0),
                    presentation_key_id: Some(0),
                    b_presentation_channel_coded: Some(true),
                    dsi_presentation_ch_mode: Some(4),
                    pres_b_4_back_channels_present: None,
                    pres_top_channel_pairs: None,
                    presentation_channel_mask_v1: Some([0, 0, 0b01000111]),
                    b_presentation_core_differs: Some(false),
                    b_presentation_core_channel_coded: None,
                    dsi_presentation_channel_mode_core: None,
                    b_presentation_filter: Some(false),
                    b_enable_presentation: None,
                    filter_data: None,
                    b_multi_pid: None,
                    substream_groups: Some(vec![Ac4SubstreamGroup {
                        b_substreams_present: true,
                        b_hsf_ext: false,
                        b_channel_coded: true,
                        substreams: vec![Ac4PresentationSubstream {
                            dsi_sf_multiplier: 0,
                            bitrate_indicator: None,
                            channel_mask: Some([0, 0, 0b01000111]),
                            n_dmx_objects_minus1: None,
                            n_umx_objects_minus1: None,
                            contains_bed_objects: None,
                            contains_dynamic_objects: None,
                            contains_isf_objects: None,
                        }],
                        content_classifier: Some(Ac4ContentClassifier::CompleteMain),
                        language_tag: None,
                    }]),
                    b_pre_virtualized: Some(false),
                    emdf_substreams: Vec::new(),
                    bit_rate_mode: None,
                    bit_rate: None,
                    bit_rate_precision: None,
                    alternative_info: None,
                    de_indicator: Some(true),
                    immersive_audio_indicator: Some(false),
                    extended_presentation_id: None,
                })]
            },
            Dac4::decode(&mut buf).expect("dac4 should decode successfully"),
        )
    }

    #[test]
    fn dac4_multi_presentation_including_v2() {
        const DAC4: &[u8] = &[
            0x00, 0x00, 0x00, 0x36, 0x64, 0x61, 0x63, 0x34, 0x20, 0xA6, 0x02, 0x40, 0x00, 0x00,
            0x00, 0x1F, 0xFF, 0xFF, 0xFF, 0xE0, 0x02, 0x0F, 0xF8, 0x80, 0x00, 0x00, 0x42, 0x00,
            0x00, 0x02, 0x50, 0x10, 0x00, 0x00, 0x03, 0x08, 0xC0, 0x01, 0x0F, 0xF8, 0x80, 0x00,
            0x00, 0x42, 0x00, 0x00, 0x02, 0x50, 0x10, 0x00, 0x00, 0x03, 0x00, 0x80,
        ];
        let mut buf = Cursor::new(DAC4);
        // Validated with https://ott.dolby.com/OnDel_tools/mp4_inspector/index.html
        assert_eq!(
            Dac4 {
                ac4_dsi_version: 1,
                bitstream_version: 2,
                fs_index: true,
                frame_rate_index: 3,
                short_program_id: None,
                program_uuid: None,
                bit_rate_mode: Ac4BitrateMode::Average,
                bit_rate: 0,
                bit_rate_precision: 4294967295,
                presentations: vec![
                    Ac4Presentation::V2(Ac4PresentationV1 {
                        presentation_config_v1: 31,
                        md_compat: Some(0),
                        presentation_id: Some(0),
                        dsi_frame_rate_multiply_info: Some(0),
                        dsi_frame_rate_fraction_info: Some(0),
                        presentation_emdf_version: Some(0),
                        presentation_key_id: Some(0),
                        b_presentation_channel_coded: Some(true),
                        dsi_presentation_ch_mode: Some(1),
                        pres_b_4_back_channels_present: None,
                        pres_top_channel_pairs: None,
                        presentation_channel_mask_v1: Some([0, 0, 1]),
                        b_presentation_core_differs: Some(false),
                        b_presentation_core_channel_coded: None,
                        dsi_presentation_channel_mode_core: None,
                        b_presentation_filter: Some(false),
                        b_enable_presentation: None,
                        filter_data: None,
                        b_multi_pid: None,
                        substream_groups: Some(vec![Ac4SubstreamGroup {
                            b_substreams_present: true,
                            b_hsf_ext: false,
                            b_channel_coded: true,
                            substreams: vec![Ac4PresentationSubstream {
                                dsi_sf_multiplier: 0,
                                bitrate_indicator: None,
                                channel_mask: Some([0, 0, 1]),
                                n_dmx_objects_minus1: None,
                                n_umx_objects_minus1: None,
                                contains_bed_objects: None,
                                contains_dynamic_objects: None,
                                contains_isf_objects: None,
                            }],
                            content_classifier: Some(Ac4ContentClassifier::CompleteMain),
                            language_tag: None,
                        }]),
                        b_pre_virtualized: Some(true),
                        emdf_substreams: Vec::new(),
                        bit_rate_mode: None,
                        bit_rate: None,
                        bit_rate_precision: None,
                        alternative_info: None,
                        de_indicator: Some(true),
                        immersive_audio_indicator: Some(true),
                        extended_presentation_id: None,
                    }),
                    Ac4Presentation::V1(Ac4PresentationV1 {
                        presentation_config_v1: 31,
                        md_compat: Some(0),
                        presentation_id: Some(0),
                        dsi_frame_rate_multiply_info: Some(0),
                        dsi_frame_rate_fraction_info: Some(0),
                        presentation_emdf_version: Some(0),
                        presentation_key_id: Some(0),
                        b_presentation_channel_coded: Some(true),
                        dsi_presentation_ch_mode: Some(1),
                        pres_b_4_back_channels_present: None,
                        pres_top_channel_pairs: None,
                        presentation_channel_mask_v1: Some([0, 0, 1]),
                        b_presentation_core_differs: Some(false),
                        b_presentation_core_channel_coded: None,
                        dsi_presentation_channel_mode_core: None,
                        b_presentation_filter: Some(false),
                        b_enable_presentation: None,
                        filter_data: None,
                        b_multi_pid: None,
                        substream_groups: Some(vec![Ac4SubstreamGroup {
                            b_substreams_present: true,
                            b_hsf_ext: false,
                            b_channel_coded: true,
                            substreams: vec![Ac4PresentationSubstream {
                                dsi_sf_multiplier: 0,
                                bitrate_indicator: None,
                                channel_mask: Some([0, 0, 1]),
                                n_dmx_objects_minus1: None,
                                n_umx_objects_minus1: None,
                                contains_bed_objects: None,
                                contains_dynamic_objects: None,
                                contains_isf_objects: None,
                            }],
                            content_classifier: Some(Ac4ContentClassifier::CompleteMain),
                            language_tag: None,
                        }]),
                        b_pre_virtualized: Some(false),
                        emdf_substreams: Vec::new(),
                        bit_rate_mode: None,
                        bit_rate: None,
                        bit_rate_precision: None,
                        alternative_info: None,
                        de_indicator: Some(true),
                        immersive_audio_indicator: Some(false),
                        extended_presentation_id: None,
                    })
                ]
            },
            Dac4::decode(&mut buf).expect("dac4 should decode successfully"),
        )
    }
}
