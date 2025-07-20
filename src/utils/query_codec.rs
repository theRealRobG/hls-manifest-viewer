use crate::utils::network::RequestRange;
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use std::{
    borrow::Cow, collections::HashMap, error::Error, fmt::Display, num::ParseIntError,
    str::Utf8Error,
};

#[derive(Debug, Clone, PartialEq)]
pub struct MediaSegmentContext {
    pub url: String,
    pub media_sequence: u64,
    pub byterange: Option<RequestRange>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartSegmentContext {
    pub segment_context: MediaSegmentContext,
    pub part_index: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupplementalViewQueryContext {
    Segment(MediaSegmentContext),
    Map(MediaSegmentContext),
    Part(PartSegmentContext),
}

pub fn encode_segment(url: &str, media_sequence: u64, byterange: Option<RequestRange>) -> String {
    format!("SEGMENT,{}", encode(url, media_sequence, byterange))
}

pub fn encode_map(url: &str, media_sequence: u64, byterange: Option<RequestRange>) -> String {
    format!("MAP,{}", encode(url, media_sequence, byterange))
}

pub fn encode_part(
    url: &str,
    media_sequence: u64,
    part_index: u32,
    byterange: Option<RequestRange>,
) -> String {
    format!(
        "PART,{part_index},{}",
        encode(url, media_sequence, byterange)
    )
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

pub fn encode_definitions(definitions: &HashMap<String, String>) -> String {
    // EXT-X-DEFINE:VALUE is defined to be a "quoted-string". The HLS definition for a quoted string
    // is (https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-4.2):
    //   * quoted-string: a string of characters within a pair of double
    //     quotes (0x22).  The following characters MUST NOT appear in a
    //     quoted-string: line feed (0xA), carriage return (0xD), or double
    //     quote (0x22).  The string MUST be non-empty, unless specifically
    //     allowed.  Quoted-string AttributeValues SHOULD be constructed so
    //     that byte-wise comparison is sufficient to test two quoted-string
    //     AttributeValues for equality.  Note that this implies case-
    //     sensitive comparison.
    // The implication is that, when looking for a separator for the values in the map, the only
    // safe characters we have to choose are 0x0A, 0x0D, and 0x22. I suppose any are as good as
    // each other, but stylistically, I think using new line is nicest (it will be percent encoded
    // anyway).
    percent_encode(
        &definitions
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<String>>()
            .join("\n"),
    )
    .to_string()
}

pub fn decode_definitions(
    query_value: &str,
) -> Result<HashMap<String, String>, DecodeDefinitionsError> {
    let percent_decoded = percent_decode_str(query_value)
        .decode_utf8()
        .map_err(DecodeDefinitionsError::Utf8Error)?;
    let split = percent_decoded.split('\n');
    let mut map = HashMap::new();
    for key_value in split {
        let mut key_value_split = key_value.splitn(2, '=');
        let Some(key) = key_value_split.next() else {
            return Err(DecodeDefinitionsError::MalformedDefinitionMissingName);
        };
        let value = key_value_split.next().unwrap_or_default();
        map.insert(key.to_string(), value.to_string());
    }
    Ok(map)
}

// https://url.spec.whatwg.org/#query-percent-encode-set
// The query percent-encode set is the C0 control percent-encode set and U+0020 SPACE, U+0022 ("),
// U+0023 (#), U+003C (<), and U+003E (>).
//
// Given that the values will be URLs contained within a query value, I also need to encode b'&' and
// b'=', as I don't want to inadvertently split the query value if the source URL has multiple query
// parameters.
const QUERY: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'&')
    .add(b'=');

pub fn percent_encode(value: &str) -> Cow<str> {
    Cow::from(utf8_percent_encode(value, QUERY))
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
            "PART" => {
                let Some(value) = split.next() else {
                    return Err(SupplementalViewQueryContextDecodeError::EmptyContextValue);
                };
                let mut split = value.splitn(2, ',');
                let Some(part_index_part) = split.next() else {
                    return Err(SupplementalViewQueryContextDecodeError::MissingPartIndex);
                };
                let part_index = part_index_part
                    .parse::<u32>()
                    .map_err(SupplementalViewQueryContextDecodeError::PartIndexParseIntFailure)?;
                let Some(value) = split.next() else {
                    return Err(SupplementalViewQueryContextDecodeError::MissingMediaSequencePart);
                };
                let segment_context = MediaSegmentContext::try_from(value)?;
                Ok(Self::Part(PartSegmentContext {
                    segment_context,
                    part_index,
                }))
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
            Self::Part(p) => encode_part(
                &p.segment_context.url,
                p.segment_context.media_sequence,
                p.part_index,
                p.segment_context.byterange,
            ),
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
    MissingPartIndex,
    PartIndexParseIntFailure(ParseIntError),
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
            Self::MissingPartIndex => write!(f, "missing expected part index information"),
            Self::PartIndexParseIntFailure(e) => {
                write!(f, "part index failed to parse: {e}")
            }
        }
    }
}
impl Error for SupplementalViewQueryContextDecodeError {}

#[derive(Debug, Clone, PartialEq)]
pub enum DecodeDefinitionsError {
    Utf8Error(Utf8Error),
    MalformedDefinitionMissingName,
}
impl Display for DecodeDefinitionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Utf8Error(e) => write!(f, "invalid utf-8 when percent decoding: {e}"),
            Self::MalformedDefinitionMissingName => write!(f, "definition had no name"),
        }
    }
}
impl Error for DecodeDefinitionsError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::tests::assert_definitions_string_equality;
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
    fn encode_decode_part_with_byterange() {
        let string = format!("PART,0,{MS},{BYTERANGE},{URL}");
        let context = SupplementalViewQueryContext::Part(PartSegmentContext {
            segment_context: MediaSegmentContext {
                url: URL.to_string(),
                media_sequence: MS,
                byterange: Some(BYTERANGE),
            },
            part_index: 0,
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

    #[test]
    fn encode_decode_part_without_byterange() {
        let string = format!("PART,0,{MS},-,{URL}");
        let context = SupplementalViewQueryContext::Part(PartSegmentContext {
            segment_context: MediaSegmentContext {
                url: URL.to_string(),
                media_sequence: MS,
                byterange: None,
            },
            part_index: 0,
        });
        assert_eq!(string, context.encode());
        assert_eq!(
            Ok(context),
            SupplementalViewQueryContext::try_from(string.as_str())
        );
    }

    #[test]
    fn encode_decode_definitions_for_single_definition() {
        let query_value = String::from("hello%3Dworld");
        let definitions = definitions_from([("hello", "world")]);
        assert_definitions_string_equality(
            query_value.as_str(),
            encode_definitions(&definitions).as_str(),
        );
        assert_eq!(Ok(definitions), decode_definitions(&query_value));
    }

    #[test]
    fn encode_decode_definitions_for_multiple_definitions() {
        let query_value = String::from("hello%3Dworld%0Ameaning%3D42%0Aquestion%3Dunknown");
        let definitions = definitions_from([
            ("hello", "world"),
            ("meaning", "42"),
            ("question", "unknown"),
        ]);
        assert_definitions_string_equality(
            query_value.as_str(),
            encode_definitions(&definitions).as_str(),
        );
        assert_eq!(Ok(definitions), decode_definitions(&query_value));
    }

    #[test]
    fn encode_decode_definitions_with_some_characters_not_allowed_in_query() {
        let query_value = String::from("first%3D%23%20%3Cwow%3E%26%3Cnow%3E%0Anext%3D%3C%3D%3E");
        let definitions = definitions_from([("first", "# <wow>&<now>"), ("next", "<=>")]);
        assert_definitions_string_equality(
            query_value.as_str(),
            encode_definitions(&definitions).as_str(),
        );
        assert_eq!(Ok(definitions), decode_definitions(&query_value));
    }

    fn definitions_from<const N: usize>(
        values: [(&'static str, &'static str); N],
    ) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for (key, value) in values {
            map.insert(key.to_string(), value.to_string());
        }
        map
    }
}
