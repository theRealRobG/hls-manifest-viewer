use crate::utils::network::RequestRange;
use std::{error::Error, fmt::Display, num::ParseIntError};

#[derive(Debug, Clone, PartialEq)]
pub struct MediaSegmentContext {
    pub url: String,
    pub media_sequence: u64,
    pub byterange: Option<RequestRange>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupplementalViewQueryContext {
    Segment(MediaSegmentContext),
    Map(MediaSegmentContext),
}

pub fn encode_segment(url: &str, media_sequence: u64, byterange: Option<RequestRange>) -> String {
    format!("SEGMENT,{}", encode(url, media_sequence, byterange))
}

pub fn encode_map(url: &str, media_sequence: u64, byterange: Option<RequestRange>) -> String {
    format!("MAP,{}", encode(url, media_sequence, byterange))
}

fn encode(url: &str, media_sequence: u64, byterange: Option<RequestRange>) -> String {
    format!(
        "{},{},{}",
        media_sequence,
        if let Some(byterange) = byterange {
            format!("{byterange}")
        } else {
            "-".to_string()
        },
        url
    )
}

impl TryFrom<&str> for SupplementalViewQueryContext {
    type Error = SupplementalViewQueryContextDecodeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut split = value.splitn(2, ',');
        let Some(type_part) = split.next() else {
            return Err(SupplementalViewQueryContextDecodeError::NoContextType);
        };
        match type_part {
            "SEGMENT" => {
                let Some(value) = split.next() else {
                    return Err(SupplementalViewQueryContextDecodeError::EmptyContextValue);
                };
                Ok(Self::Segment(MediaSegmentContext::try_from(value)?))
            }
            "MAP" => {
                let Some(value) = split.next() else {
                    return Err(SupplementalViewQueryContextDecodeError::EmptyContextValue);
                };
                Ok(Self::Map(MediaSegmentContext::try_from(value)?))
            }
            _ => Err(SupplementalViewQueryContextDecodeError::UnknownContextType(
                type_part.to_string(),
            )),
        }
    }
}

impl TryFrom<&str> for MediaSegmentContext {
    type Error = SupplementalViewQueryContextDecodeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut split = value.splitn(3, ',');
        let Some(media_sequence_part) = split.next() else {
            return Err(SupplementalViewQueryContextDecodeError::MissingMediaSequencePart);
        };
        let media_sequence = media_sequence_part.parse::<u64>().map_err(|e| {
            SupplementalViewQueryContextDecodeError::MediaSequencePartParseIntFailure(e)
        })?;
        let Some(request_range_part) = split.next() else {
            return Err(SupplementalViewQueryContextDecodeError::MissingRangePart);
        };
        let byterange = if request_range_part == "-" {
            None
        } else {
            let mut range_split = request_range_part.splitn(2, '-');
            let Some(Ok(start)) = range_split.next().map(|s| s.parse::<u64>()) else {
                return Err(SupplementalViewQueryContextDecodeError::RangeParseFailure);
            };
            let Some(Ok(end)) = range_split.next().map(|e| e.parse::<u64>()) else {
                return Err(SupplementalViewQueryContextDecodeError::RangeParseFailure);
            };
            Some(RequestRange { start, end })
        };
        let Some(url) = split.next().map(str::to_string) else {
            return Err(SupplementalViewQueryContextDecodeError::MissingUrlPart);
        };
        Ok(MediaSegmentContext {
            url,
            media_sequence,
            byterange,
        })
    }
}

#[cfg(test)]
impl SupplementalViewQueryContext {
    fn encode(&self) -> String {
        match self {
            Self::Segment(c) => encode_segment(&c.url, c.media_sequence, c.byterange),
            Self::Map(c) => encode_map(&c.url, c.media_sequence, c.byterange),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupplementalViewQueryContextDecodeError {
    NoContextType,
    UnknownContextType(String),
    EmptyContextValue,
    MissingMediaSequencePart,
    MediaSequencePartParseIntFailure(ParseIntError),
    MissingRangePart,
    RangeParseFailure,
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
            Self::MissingRangePart => {
                write!(f, "missing expected request range information")
            }
            Self::RangeParseFailure => {
                write!(f, "request range information malformed")
            }
            Self::MissingUrlPart => write!(f, "missing expected url information"),
        }
    }
}
impl Error for SupplementalViewQueryContextDecodeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const URL: &str = "https://example.com/file.mp4";
    const MS: u64 = 100;
    const BYTERANGE: RequestRange = RequestRange { start: 0, end: 100 };

    #[test]
    fn encode_decode_segment_with_byterange() {
        let string = format!("SEGMENT,{MS},{BYTERANGE},{URL}");
        let context = SupplementalViewQueryContext::Segment(MediaSegmentContext {
            url: URL.to_string(),
            media_sequence: MS,
            byterange: Some(BYTERANGE),
        });
        assert_eq!(string, context.encode());
        assert_eq!(
            Ok(context),
            SupplementalViewQueryContext::try_from(string.as_str())
        )
    }

    #[test]
    fn encode_decode_map_with_byterange() {
        let string = format!("MAP,{MS},{BYTERANGE},{URL}");
        let context = SupplementalViewQueryContext::Map(MediaSegmentContext {
            url: URL.to_string(),
            media_sequence: MS,
            byterange: Some(BYTERANGE),
        });
        assert_eq!(string, context.encode());
        assert_eq!(
            Ok(context),
            SupplementalViewQueryContext::try_from(string.as_str())
        );
    }

    #[test]
    fn encode_decode_segment_without_byterange() {
        let string = format!("SEGMENT,{MS},-,{URL}");
        let context = SupplementalViewQueryContext::Segment(MediaSegmentContext {
            url: URL.to_string(),
            media_sequence: MS,
            byterange: None,
        });
        assert_eq!(string, context.encode());
        assert_eq!(
            Ok(context),
            SupplementalViewQueryContext::try_from(string.as_str())
        );
    }

    #[test]
    fn encode_decode_map_without_byterange() {
        let string = format!("MAP,{MS},-,{URL}");
        let context = SupplementalViewQueryContext::Map(MediaSegmentContext {
            url: URL.to_string(),
            media_sequence: MS,
            byterange: None,
        });
        assert_eq!(string, context.encode());
        assert_eq!(
            Ok(context),
            SupplementalViewQueryContext::try_from(string.as_str())
        );
    }
}
