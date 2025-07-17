use std::{error::Error, fmt::Display, num::ParseIntError};

pub const SUPPLEMENTAL_VIEW_QUERY_NAME: &str = "supplemental_view_context";

#[derive(Debug, Clone, PartialEq)]
pub struct MediaSegmentContext {
    pub url: String,
    pub media_sequence: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupplementalViewQueryContext {
    Segment(MediaSegmentContext),
    Map(MediaSegmentContext),
}

impl SupplementalViewQueryContext {
    pub fn encode(&self) -> String {
        match self {
            Self::Segment(c) => format!("SEGMENT,{},{}", c.media_sequence, c.url),
            Self::Map(c) => format!("MAP,{},{}", c.media_sequence, c.url),
        }
    }

    pub fn encode_segment(url: String, media_sequence: u64) -> String {
        Self::Segment(MediaSegmentContext {
            url,
            media_sequence,
        })
        .encode()
    }

    pub fn encode_map(url: String, media_sequence: u64) -> String {
        Self::Map(MediaSegmentContext {
            url,
            media_sequence,
        })
        .encode()
    }

    pub fn try_decode(value: &str) -> Result<Self, SupplementalViewQueryContextDecodeError> {
        let mut split = value.splitn(2, ',');
        let Some(type_part) = split.next() else {
            return Err(SupplementalViewQueryContextDecodeError::NoContextType);
        };
        match type_part {
            "SEGMENT" => {
                let Some(value) = split.next() else {
                    return Err(SupplementalViewQueryContextDecodeError::EmptyContextValue);
                };
                Ok(Self::Segment(Self::try_decode_segment_context(value)?))
            }
            "MAP" => {
                let Some(value) = split.next() else {
                    return Err(SupplementalViewQueryContextDecodeError::EmptyContextValue);
                };
                Ok(Self::Map(Self::try_decode_segment_context(value)?))
            }
            _ => Err(SupplementalViewQueryContextDecodeError::UnknownContextType(
                type_part.to_string(),
            )),
        }
    }

    fn try_decode_segment_context(
        value: &str,
    ) -> Result<MediaSegmentContext, SupplementalViewQueryContextDecodeError> {
        let mut split = value.splitn(2, ',');
        let Some(media_sequence_part) = split.next() else {
            return Err(SupplementalViewQueryContextDecodeError::MissingMediaSequencePart);
        };
        let media_sequence = media_sequence_part.parse::<u64>().map_err(|e| {
            SupplementalViewQueryContextDecodeError::MediaSequencePartParseIntFailure(e)
        })?;
        let Some(url) = split.next().map(str::to_string) else {
            return Err(SupplementalViewQueryContextDecodeError::MissingUrlPart);
        };
        Ok(MediaSegmentContext {
            url,
            media_sequence,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupplementalViewQueryContextDecodeError {
    NoContextType,
    UnknownContextType(String),
    EmptyContextValue,
    MissingMediaSequencePart,
    MediaSequencePartParseIntFailure(ParseIntError),
    MissingUrlPart,
}
impl Display for SupplementalViewQueryContextDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoContextType => write!(
                f,
                "invalid format with no context type (no comma delimitation in value)"
            ),
            Self::UnknownContextType(s) => write!(f, "unknown context type: {s}"),
            Self::EmptyContextValue => {
                write!(f, "context contained no value after type declaration")
            }
            Self::MissingMediaSequencePart => {
                write!(f, "missing expected media sequence information")
            }
            Self::MediaSequencePartParseIntFailure(e) => {
                write!(f, "media sequence failed to parse: {e}")
            }
            Self::MissingUrlPart => write!(f, "missing expected url information"),
        }
    }
}
impl Error for SupplementalViewQueryContextDecodeError {}
