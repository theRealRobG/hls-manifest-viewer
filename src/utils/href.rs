use crate::utils::{
    network::RequestRange,
    query_codec::{encode_definitions, encode_map, encode_segment, percent_encode},
};
use leptos::prelude::GetUntracked;
use leptos_router::hooks::use_query_map;
use std::{borrow::Cow, collections::HashMap};
use url::Url;

pub const PLAYLIST_URL_QUERY_NAME: &str = "playlist_url";
pub const SUPPLEMENTAL_VIEW_QUERY_NAME: &str = "supplemental_view_context";
pub const DEFINITIONS_QUERY_NAME: &str = "imported_definitions";

pub fn media_playlist_href(
    relative_uri: &str,
    definitions: &HashMap<String, String>,
) -> Option<String> {
    playlist_href(base_url()?, relative_uri, definitions)
}

pub fn segment_href(
    segment_uri: &str,
    media_sequence: u64,
    byterange: Option<RequestRange>,
    definitions: &HashMap<String, String>,
) -> Option<String> {
    media_segment_href(
        base_url()?,
        segment_uri,
        media_sequence,
        byterange,
        SegmentType::Segment,
        definitions_query_value(),
        definitions,
    )
}

pub fn map_href(
    segment_uri: &str,
    media_sequence: u64,
    byterange: Option<RequestRange>,
    definitions: &HashMap<String, String>,
) -> Option<String> {
    media_segment_href(
        base_url()?,
        segment_uri,
        media_sequence,
        byterange,
        SegmentType::Map,
        definitions_query_value(),
        definitions,
    )
}

// These functions can't be run in tests because `use_query_map` must be run from within a Leptos
// `Router` context (tests crash otherwise). Therefore, the bulk of the logic is extracted to below
// so that it is testable.
fn base_url() -> Option<Url> {
    let base_url_query_parameter = use_query_map()
        .get_untracked()
        .get(PLAYLIST_URL_QUERY_NAME)?;
    Url::parse(&base_url_query_parameter).ok()
}
fn definitions_query_value() -> Option<String> {
    use_query_map().get_untracked().get(DEFINITIONS_QUERY_NAME)
}

fn playlist_href(
    base_url: Url,
    relative_uri: &str,
    local_definitions: &HashMap<String, String>,
) -> Option<String> {
    let relative_uri = replace_hls_variables(relative_uri, local_definitions);
    let absolute_url = base_url.join(&relative_uri).ok()?;
    let query_encoded_url = percent_encode(absolute_url.as_str());
    if local_definitions.is_empty() {
        Some(format!("?{PLAYLIST_URL_QUERY_NAME}={query_encoded_url}"))
    } else {
        let encoded_definitions = encode_definitions(&local_definitions);
        Some(format!("?{PLAYLIST_URL_QUERY_NAME}={query_encoded_url}&{DEFINITIONS_QUERY_NAME}={encoded_definitions}"))
    }
}

fn media_segment_href(
    base_url: Url,
    segment_uri: &str,
    media_sequence: u64,
    byterange: Option<RequestRange>,
    segment_type: SegmentType,
    definitions_query_value: Option<String>,
    local_definitions: &HashMap<String, String>,
) -> Option<String> {
    let segment_uri = replace_hls_variables(segment_uri, local_definitions);
    let absolute_segment_url = base_url.join(&segment_uri).ok()?;
    let query_encoded_base_url = percent_encode(base_url.as_str());
    let query_encoded_segment_url = percent_encode(absolute_segment_url.as_str());
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
    if let Some(definitions_query_value) = definitions_query_value {
        Some(format!(
            "?{}={}&{}={}&{}={}",
            PLAYLIST_URL_QUERY_NAME,
            query_encoded_base_url,
            DEFINITIONS_QUERY_NAME,
            definitions_query_value,
            SUPPLEMENTAL_VIEW_QUERY_NAME,
            encoded_supplemental_context,
        ))
    } else {
        Some(format!(
            "?{}={}&{}={}",
            PLAYLIST_URL_QUERY_NAME,
            query_encoded_base_url,
            SUPPLEMENTAL_VIEW_QUERY_NAME,
            encoded_supplemental_context,
        ))
    }
}

fn replace_hls_variables<'a>(
    uri: &'a str,
    definitions: &'a HashMap<String, String>,
) -> Cow<'a, str> {
    if definitions.is_empty() {
        Cow::Borrowed(uri)
    } else {
        Cow::Owned(
            definitions
                .iter()
                .fold(uri.to_string(), |uri, (key, value)| {
                    uri.replace(&format!("{{${key}}}"), &value)
                }),
        )
    }
}

#[derive(Clone, Copy)]
enum SegmentType {
    Segment,
    Map,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::tests::assert_definitions_string_equality;
    use pretty_assertions::assert_eq;

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_relative_uri() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8";
        test_media_and_segment_href(base_url, uri, "https://example.com/hls/hi/video-only.m3u8");
    }

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_absolute_uri() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "https://ads.server.com/ads/hls/hi/video-only.m3u8";
        test_media_and_segment_href(
            base_url,
            uri,
            "https://ads.server.com/ads/hls/hi/video-only.m3u8",
        );
    }

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_uri_stepping_out_of_base_path() {
        let base_url = Url::parse("https://ads.com/1234/main/mvp.m3u8").unwrap();
        let uri = "../media/3.m3u8";
        test_media_and_segment_href(base_url, uri, "https://ads.com/1234/media/3.m3u8");
    }

    #[test]
    fn resolve_href_should_escape_query_pairs() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8?token=1234&id=abcd";
        test_media_and_segment_href(
            base_url,
            uri,
            "https://example.com/hls/hi/video-only.m3u8?token%3D1234%26id%3Dabcd",
        );
    }

    #[test]
    fn resolve_href_should_escape_fragment() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8?token=1234#fragment";
        test_media_and_segment_href(
            base_url,
            uri,
            "https://example.com/hls/hi/video-only.m3u8?token%3D1234%23fragment",
        );
    }

    fn test_media_and_segment_href(base_url: Url, uri: &str, expected: &str) {
        assert_eq!(
            Some(format!("?playlist_url={expected}")),
            playlist_href(base_url.clone(), uri, &HashMap::new())
        );
        assert_eq!(
            Some(format!(
                "?playlist_url={}&supplemental_view_context={}",
                base_url.as_str(),
                format!("SEGMENT,100,-,{expected}")
            )),
            media_segment_href(
                base_url.clone(),
                uri,
                100,
                None,
                SegmentType::Segment,
                None,
                &HashMap::new()
            )
        );
        assert_eq!(
            Some(format!(
                "?playlist_url={}&supplemental_view_context={}",
                base_url.as_str(),
                format!("MAP,100,-,{expected}")
            )),
            media_segment_href(
                base_url,
                uri,
                100,
                None,
                SegmentType::Map,
                None,
                &HashMap::new()
            )
        );
    }

    // Definitions tests are separate as they are some more complicated scenarios to tease out
    #[test]
    fn playlist_href_should_replace_variables_and_set_local_definitions_as_query_value() {
        // The local definitions on a playlist request will always replace those defined via the
        // query parameter. This perhaps is lazy thinking... The idea being that a playlist request
        // coming into this flow is always for a media playlist coming from an MVP, which would mean
        // that locally defined values in the MVP need to be included in the new href (for the child
        // media playlist), and in fact there shouldn't ever be a query defined value (for the MVP).
        // But... In the future, if we add linking between playlists via EXT-X-RENDITION-REPORT,
        // then in that case we actually do want to keep the query defined values... So maybe, the
        // most accurate way of doing this is to use the query defined value if it exists, otherwise
        // use the locally defined definitions. That being said, I'll cross that bridge when we come
        // to adding support for linking via rendition report, and thinking about it a bit more, I
        // prefer to be a little less "magical" and define a dedicated method for rendition report
        // href to be more deliberate.
        let local_definitions = HashMap::from([
            (String::from("DOMAIN"), String::from("https://cdn.com")),
            (String::from("TOKEN"), String::from("1234")),
        ]);
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "{$DOMAIN}/hi/video-only.m3u8?token={$TOKEN}";
        let actual =
            playlist_href(base_url, uri, &local_definitions).expect("href should be defined");
        let mut parameter_split = actual.splitn(2, '&');
        let playlist_part = parameter_split
            .next()
            .expect("playlist query component should be defined");
        assert_eq!(
            "?playlist_url=https://cdn.com/hi/video-only.m3u8?token%3D1234",
            playlist_part
        );
        let definitions_part = parameter_split
            .next()
            .expect("definitions query component should be defined");
        let mut definitions_split = definitions_part.splitn(2, '=');
        assert_eq!(Some(DEFINITIONS_QUERY_NAME), definitions_split.next());
        let definitions_query_value = definitions_split
            .next()
            .expect("definitions query value should be defined");
        assert_definitions_string_equality(
            "DOMAIN%3Dhttps://cdn.com%0ATOKEN%3D1234",
            definitions_query_value,
        );
    }

    #[test]
    fn media_segment_href_should_maintain_query_for_definitions_and_replace_with_local() {
        // This is simulating a situation where the DOMAIN variable has been imported, while the
        // TOKEN variable is not (perhaps obtained from QUERYPARAM, or just defined locally). In
        // this case we expect that the existing query string value is maintained, but all the local
        // definitions are used to replace HLS variables in the URI.
        let query_definitions = String::from("DOMAIN%3Dhttps://cdn.com");
        let local_definitions = HashMap::from([
            (String::from("DOMAIN"), String::from("https://cdn.com")),
            (String::from("TOKEN"), String::from("1234")),
        ]);
        let base_url = Url::parse("https://example.com/hls/media.m3u8").unwrap();
        let uri = "{$DOMAIN}/hi/segment-100.mp4?token={$TOKEN}";
        assert_eq!(
            Some(format!(
                "?{}={}&{}={}&{}={}",
                PLAYLIST_URL_QUERY_NAME,
                "https://example.com/hls/media.m3u8",
                DEFINITIONS_QUERY_NAME,
                "DOMAIN%3Dhttps://cdn.com",
                SUPPLEMENTAL_VIEW_QUERY_NAME,
                "SEGMENT,100,-,https://cdn.com/hi/segment-100.mp4?token%3D1234"
            )),
            media_segment_href(
                base_url.clone(),
                uri,
                100,
                None,
                SegmentType::Segment,
                Some(query_definitions.clone()),
                &local_definitions
            )
        );
        assert_eq!(
            Some(format!(
                "?{}={}&{}={}&{}={}",
                PLAYLIST_URL_QUERY_NAME,
                "https://example.com/hls/media.m3u8",
                DEFINITIONS_QUERY_NAME,
                "DOMAIN%3Dhttps://cdn.com",
                SUPPLEMENTAL_VIEW_QUERY_NAME,
                "MAP,100,-,https://cdn.com/hi/segment-100.mp4?token%3D1234"
            )),
            media_segment_href(
                base_url,
                uri,
                100,
                None,
                SegmentType::Map,
                Some(query_definitions),
                &local_definitions
            )
        );
    }

    #[test]
    fn media_segment_href_should_not_add_query_definitions_but_still_use_local_for_replacement() {
        // This example is assuming that there were no imported values and everything is just
        // defined locally. I expect that we won't have the definitions query component added.
        let local_definitions = HashMap::from([(String::from("TOKEN"), String::from("1234"))]);
        let base_url = Url::parse("https://example.com/hls/hi/media.m3u8").unwrap();
        let uri = "segment-100.mp4?token={$TOKEN}";
        assert_eq!(
            Some(format!(
                "?{}={}&{}={}",
                PLAYLIST_URL_QUERY_NAME,
                "https://example.com/hls/hi/media.m3u8",
                SUPPLEMENTAL_VIEW_QUERY_NAME,
                "SEGMENT,100,-,https://example.com/hls/hi/segment-100.mp4?token%3D1234"
            )),
            media_segment_href(
                base_url.clone(),
                uri,
                100,
                None,
                SegmentType::Segment,
                None,
                &local_definitions
            )
        );
        assert_eq!(
            Some(format!(
                "?{}={}&{}={}",
                PLAYLIST_URL_QUERY_NAME,
                "https://example.com/hls/hi/media.m3u8",
                SUPPLEMENTAL_VIEW_QUERY_NAME,
                "MAP,100,-,https://example.com/hls/hi/segment-100.mp4?token%3D1234"
            )),
            media_segment_href(
                base_url,
                uri,
                100,
                None,
                SegmentType::Map,
                None,
                &local_definitions
            )
        );
    }
}
