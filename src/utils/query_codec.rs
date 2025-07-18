use m3u8::tag::hls::map::MapByterange;
use std::{error::Error, fmt::Display, num::ParseIntError};

pub const SUPPLEMENTAL_VIEW_QUERY_NAME: &str = "supplemental_view_context";

#[derive(Debug, Clone, PartialEq)]
pub struct MediaSegmentContext {
    pub url: String,
    pub media_sequence: u64,
    pub byterange: Option<RequestRange>,
}
impl MediaSegmentContext {
    fn encode(&self) -> String {
        format!(
            "{},{},{}",
            self.media_sequence,
            if let Some(byterange) = self.byterange {
                format!("{byterange}")
            } else {
                "-".to_string()
            },
            self.url
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RequestRange {
    pub start: u64,
    pub end: u64,
}
impl RequestRange {
    pub fn from_length_with_offset(length: u64, offset: u64) -> Self {
        Self {
            start: offset,
            end: (offset + length) - 1,
        }
    }

    pub fn range_header_value(&self) -> String {
        format!("bytes={}-{}", self.start, self.end)
    }
}
impl From<MapByterange> for RequestRange {
    fn from(value: MapByterange) -> Self {
        Self::from_length_with_offset(value.length, value.offset)
    }
}
impl Display for RequestRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupplementalViewQueryContext {
    Segment(MediaSegmentContext),
    Map(MediaSegmentContext),
}

impl SupplementalViewQueryContext {
    pub fn encode(&self) -> String {
        match self {
            Self::Segment(c) => format!("SEGMENT,{}", c.encode()),
            Self::Map(c) => format!("MAP,{}", c.encode()),
        }
    }

    pub fn encode_segment(
        url: String,
        media_sequence: u64,
        byterange: Option<RequestRange>,
    ) -> String {
        Self::Segment(MediaSegmentContext {
            url,
            media_sequence,
            byterange,
        })
        .encode()
    }

    pub fn encode_map(url: String, media_sequence: u64, byterange: Option<RequestRange>) -> String {
        Self::Map(MediaSegmentContext {
            url,
            media_sequence,
            byterange,
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
            SupplementalViewQueryContext::try_decode(&string)
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
            SupplementalViewQueryContext::try_decode(&string)
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
            SupplementalViewQueryContext::try_decode(&string)
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
            SupplementalViewQueryContext::try_decode(&string)
        );
    }
}
