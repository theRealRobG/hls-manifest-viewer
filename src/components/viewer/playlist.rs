use super::{
    BLANK_CLASS, COMMENT_CLASS, HIGHLIGHTED, HIGHLIGHTED_URI_CLASS, MAIN_VIEW_CLASS,
    MAIN_VIEW_WITH_SUPPLEMENTAL_CLASS, TAG_CLASS, URI_CLASS,
};
use crate::{
    components::CopyButton,
    utils::query_codec::{SupplementalViewQueryContext, SUPPLEMENTAL_VIEW_QUERY_NAME},
    PLAYLIST_URL_QUERY_NAME,
};
use leptos::{either::EitherOf3, prelude::*};
use m3u8::{
    config::ParsingOptionsBuilder,
    line::HlsLine,
    tag::{
        hls::{self, TagInner, TagName, TagType},
        known, unknown,
    },
    Reader,
};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::{error::Error, fmt::Display};
use url::Url;

pub struct HighlightedMapInfo {
    pub url: String,
    pub min_media_sequence: u64,
}

#[component]
pub fn PlaylistViewer(
    playlist: String,
    base_url: String,
    #[prop(default = false)] supplemental_showing: bool,
    #[prop(optional)] highlighted_segment: Option<u64>,
    #[prop(optional)] highlighted_map_info: Option<HighlightedMapInfo>,
) -> Result<impl IntoView, PlaylistError> {
    if playlist.is_empty() {
        return Ok(EitherOf3::A(view! { <div class=MAIN_VIEW_CLASS /> }));
    }
    match try_get_lines(
        &playlist,
        &base_url,
        highlighted_segment,
        highlighted_map_info,
    ) {
        Ok(lines) => {
            if supplemental_showing {
                Ok(EitherOf3::B(view! {
                    <div class=MAIN_VIEW_WITH_SUPPLEMENTAL_CLASS>
                        <CopyButton text=move || playlist.clone() />
                        {lines}
                    </div>
                }))
            } else {
                Ok(EitherOf3::C(view! {
                    <div class=MAIN_VIEW_CLASS>
                        <CopyButton text=move || playlist.clone() />
                        {lines}
                    </div>
                }))
            }
        }
        Err(error) => Err(error),
    }
}

#[derive(Debug)]
pub enum PlaylistError {
    PlaylistIdentifierNotPresent,
}
impl Display for PlaylistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlaylistIdentifierNotPresent => {
                write!(f, "Error: playlist identifier (#EXTM3U) not present")
            }
        }
    }
}
impl Error for PlaylistError {}

fn try_get_lines(
    playlist: &str,
    base_url: &str,
    highlighted_segment: Option<u64>,
    highlighted_map_info: Option<HighlightedMapInfo>,
) -> Result<Vec<AnyView>, PlaylistError> {
    // The base_url should never fail to parse. I _could_ create a separate flow in this case that
    // makes no attempt to insert links, but I feel that is an over-complication.
    let base_url = Url::parse(base_url).ok();
    let mut reader = Reader::from_str(
        playlist,
        ParsingOptionsBuilder::new()
            .with_parsing_for_m3u()
            .with_parsing_for_map()
            .with_parsing_for_media()
            .with_parsing_for_media_sequence()
            .with_parsing_for_i_frame_stream_inf()
            .build(),
    );
    let mut lines = Vec::new();

    // Segment state
    let mut media_sequence = 0;
    let mut is_media_playlist = false;
    let mut highlighted_one_map = false;

    match reader.read_line() {
        Ok(Some(HlsLine::KnownTag(known::Tag::Hls(hls::Tag::M3u(tag))))) => {
            let line = tag.into_inner();
            lines.push(
                view! { <p class=TAG_CLASS>{String::from_utf8_lossy(line.value())}</p> }.into_any(),
            );
        }
        _ => return Err(PlaylistError::PlaylistIdentifierNotPresent),
    }
    while let Ok(Some(line)) = reader.read_line() {
        match line {
            HlsLine::KnownTag(tag) => match tag {
                known::Tag::Hls(tag) => match tag {
                    hls::Tag::Media(media) => {
                        if let Some(uri) = media.uri() {
                            let uri = uri.to_string();
                            let tag_inner = media.into_inner();
                            lines.push(view_from_uri_tag(
                                uri,
                                tag_inner,
                                &base_url,
                                UriType::Playlist,
                                media_sequence,
                                false,
                            ));
                        } else {
                            let line = media.into_inner();
                            lines.push(
                                view! { <p class=TAG_CLASS>{String::from_utf8_lossy(line.value())}</p> }.into_any()
                            );
                        }
                    }
                    hls::Tag::IFrameStreamInf(iframe_stream_inf) => {
                        let uri = iframe_stream_inf.uri().to_string();
                        let tag_inner = iframe_stream_inf.into_inner();
                        lines.push(view_from_uri_tag(
                            uri,
                            tag_inner,
                            &base_url,
                            UriType::Playlist,
                            media_sequence,
                            false,
                        ));
                    }
                    hls::Tag::Map(map) => {
                        let uri = map.uri().to_string();
                        let is_highlighted = if let Some(info) = &highlighted_map_info {
                            !highlighted_one_map
                                && info.min_media_sequence <= media_sequence
                                && info.url.contains(&uri)
                        } else {
                            false
                        };
                        if is_highlighted {
                            highlighted_one_map = true
                        }
                        let tag_inner = map.into_inner();
                        lines.push(view_from_uri_tag(
                            uri,
                            tag_inner,
                            &base_url,
                            UriType::Map,
                            media_sequence,
                            is_highlighted,
                        ));
                    }
                    hls::Tag::MediaSequence(tag) => {
                        media_sequence = tag.media_sequence();
                        let line = tag.into_inner();
                        lines.push(
                            view! { <p class=TAG_CLASS>{String::from_utf8_lossy(line.value())}</p> }.into_any()
                        );
                    }
                    tag => {
                        let line = tag.into_inner();
                        lines.push(
                            view! { <p class=TAG_CLASS>{String::from_utf8_lossy(line.value())}</p> }.into_any()
                        );
                    }
                },
                known::Tag::Custom(_) => panic!("No custom tags registered"),
            },
            HlsLine::Uri(uri) => {
                let uri_class = if Some(media_sequence) == highlighted_segment {
                    HIGHLIGHTED_URI_CLASS
                } else {
                    URI_CLASS
                };
                let uri_type = if is_media_playlist {
                    UriType::Segment
                } else {
                    UriType::Playlist
                };
                lines.push(
                    view! {
                        <a
                            href=resolve_href(&base_url, uri, uri_type, media_sequence)
                            class=uri_class
                        >
                            {uri}
                        </a>
                    }
                    .into_any(),
                );
                media_sequence += 1;
            }
            HlsLine::Comment(comment) => {
                lines.push(view! { <p class=COMMENT_CLASS>"#" {comment}</p> }.into_any())
            }
            HlsLine::UnknownTag(tag) => {
                if !is_media_playlist && is_media_tag(&tag) {
                    is_media_playlist = true;
                }
                lines.push(
                    view! { <p class=TAG_CLASS>{String::from_utf8_lossy(tag.as_bytes())}</p> }
                        .into_any(),
                );
            }
            HlsLine::Blank => lines.push(view! { <p class=BLANK_CLASS></p> }.into_any()),
        }
    }
    Ok(lines)
}

enum UriType {
    Playlist,
    Segment,
    Map,
}

fn resolve_href(
    base_url: &Option<Url>,
    uri: &str,
    uri_type: UriType,
    media_sequence: u64,
) -> String {
    let Some(base_url) = base_url else {
        return String::from("#");
    };
    if let Ok(absolute_url) = base_url.join(uri) {
        // https://url.spec.whatwg.org/#query-percent-encode-set
        // The query percent-encode set is the C0 control percent-encode set and U+0020 SPACE,
        // U+0022 ("), U+0023 (#), U+003C (<), and U+003E (>).
        const QUERY: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>');
        let query_encoded_link_url = utf8_percent_encode(absolute_url.as_str(), QUERY);
        let (playlist, supplemental) = match uri_type {
            UriType::Playlist => (query_encoded_link_url, None),
            UriType::Segment => (
                utf8_percent_encode(base_url.as_str(), QUERY),
                Some(SupplementalViewQueryContext::encode_segment(
                    query_encoded_link_url.to_string(),
                    media_sequence,
                )),
            ),
            UriType::Map => (
                utf8_percent_encode(base_url.as_str(), QUERY),
                Some(SupplementalViewQueryContext::encode_map(
                    query_encoded_link_url.to_string(),
                    media_sequence,
                )),
            ),
        };
        if let Some(supplemental) = supplemental {
            format!(
                "?{}={}&{}={}",
                PLAYLIST_URL_QUERY_NAME, playlist, SUPPLEMENTAL_VIEW_QUERY_NAME, supplemental
            )
        } else {
            format!("?{PLAYLIST_URL_QUERY_NAME}={playlist}")
        }
    } else {
        String::from("#")
    }
}

fn view_from_uri_tag(
    uri: String,
    tag_inner: TagInner,
    base_url: &Option<Url>,
    uri_type: UriType,
    media_sequence: u64,
    is_highlighted: bool,
) -> AnyView {
    let line = String::from_utf8_lossy(tag_inner.value());
    let uri_index = line.find(uri.as_str()).unwrap();
    let (first, second) = line.split_at(uri_index);
    let (second, third) = second.split_at(uri.len());
    let class = if is_highlighted { HIGHLIGHTED } else { "" };
    view! {
        <p class=TAG_CLASS>
            {first}
            <a class=class href=resolve_href(base_url, uri.as_str(), uri_type, media_sequence)>
                {second}
            </a>{third}
        </p>
    }
    .into_any()
}

fn is_media_tag(tag: &unknown::Tag) -> bool {
    let Ok(name) = TagName::try_from(tag.name()) else {
        return false;
    };
    matches!(
        name.tag_type(),
        TagType::MediaMetadata | TagType::MediaSegment
    )
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn resolve_href_does_not_need_to_escape_query_parts() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8?token=1234";
        test_media_and_segment_href(
            base_url,
            uri,
            "https://example.com/hls/hi/video-only.m3u8?token=1234",
        );
    }

    #[test]
    fn resolve_href_should_escape_fragment() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8?token=1234#fragment";
        test_media_and_segment_href(
            base_url,
            uri,
            "https://example.com/hls/hi/video-only.m3u8?token=1234%23fragment",
        );
    }

    #[test]
    fn resolve_href_when_no_base_should_just_resolve_to_hash() {
        assert_eq!(
            "#".to_string(),
            resolve_href(&None, "some/uri.m3u8", UriType::Playlist, 0)
        );
        assert_eq!(
            "#".to_string(),
            resolve_href(&None, "some/uri.m3u8", UriType::Segment, 100)
        );
        assert_eq!(
            "#".to_string(),
            resolve_href(&None, "some/uri.m3u8", UriType::Map, 100)
        );
    }

    fn test_media_and_segment_href(base_url: Url, uri: &str, expected: &str) {
        assert_eq!(
            format!("?playlist_url={expected}"),
            resolve_href(&Some(base_url.clone()), uri, UriType::Playlist, 0)
        );
        assert_eq!(
            format!(
                "?playlist_url={}&supplemental_view_context={}",
                base_url.as_str(),
                format!("SEGMENT,100,{expected}")
            ),
            resolve_href(&Some(base_url.clone()), uri, UriType::Segment, 100)
        );
        assert_eq!(
            format!(
                "?playlist_url={}&supplemental_view_context={}",
                base_url.as_str(),
                format!("MAP,100,{expected}")
            ),
            resolve_href(&Some(base_url), uri, UriType::Map, 100)
        );
    }
}
