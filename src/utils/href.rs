use crate::utils::{
    network::RequestRange,
    query_codec::{encode_map, encode_segment},
};
use leptos::prelude::GetUntracked;
use leptos_router::hooks::use_query_map;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::borrow::Cow;
use url::Url;

pub const PLAYLIST_URL_QUERY_NAME: &str = "playlist_url";
pub const SUPPLEMENTAL_VIEW_QUERY_NAME: &str = "supplemental_view_context";

// https://url.spec.whatwg.org/#query-percent-encode-set
// The query percent-encode set is the C0 control percent-encode set and U+0020 SPACE, U+0022 ("),
// U+0023 (#), U+003C (<), and U+003E (>).
const QUERY: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>');

pub fn media_playlist_href(relative_uri: &str) -> Option<String> {
    let base_url = base_url()?;
    let absolute_url = base_url.join(relative_uri).ok()?;
    let query_encoded_url = utf8_percent_encode(absolute_url.as_str(), QUERY);
    Some(format!("?{PLAYLIST_URL_QUERY_NAME}={query_encoded_url}"))
}

pub fn segment_href(
    segment_uri: &str,
    media_sequence: u64,
    byterange: Option<RequestRange>,
) -> Option<String> {
    media_segment_href(segment_uri, media_sequence, byterange, SegmentType::Segment)
}

pub fn map_href(
    segment_uri: &str,
    media_sequence: u64,
    byterange: Option<RequestRange>,
) -> Option<String> {
    media_segment_href(segment_uri, media_sequence, byterange, SegmentType::Map)
}

fn base_url() -> Option<Url> {
    let base_url_query_parameter = use_query_map()
        .get_untracked()
        .get(PLAYLIST_URL_QUERY_NAME)?;
    Url::parse(&base_url_query_parameter).ok()
}

#[derive(Clone, Copy)]
enum SegmentType {
    Segment,
    Map,
}

fn media_segment_href(
    segment_uri: &str,
    media_sequence: u64,
    byterange: Option<RequestRange>,
    segment_type: SegmentType,
) -> Option<String> {
    let base_url = base_url()?;
    let absolute_segment_url = base_url.join(segment_uri).ok()?;
    let query_encoded_base_url = utf8_percent_encode(base_url.as_str(), QUERY);
    let query_encoded_segment_url = utf8_percent_encode(absolute_segment_url.as_str(), QUERY);
    let encoded_supplemental_context = match segment_type {
        SegmentType::Segment => encode_segment(
            &Cow::from(query_encoded_segment_url),
            media_sequence,
            byterange,
        ),
        SegmentType::Map => encode_map(
            &Cow::from(query_encoded_segment_url),
            media_sequence,
            byterange,
        ),
    };
    Some(format!(
        "?{}={}&{}={}",
        PLAYLIST_URL_QUERY_NAME,
        query_encoded_base_url,
        SUPPLEMENTAL_VIEW_QUERY_NAME,
        encoded_supplemental_context,
    ))
}
