use std::fmt::Display;

use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

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
