use crate::PLAYLIST_URL_QUERY_NAME;
use leptos::{either::Either, prelude::*};
use m3u8::{
    config::ParsingOptionsBuilder,
    line::HlsLine,
    tag::{
        hls::{self, TagInner},
        known,
    },
    Reader,
};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::{error::Error, fmt::Display};
use url::Url;

const VIEWER_CLASS: &str = "viewer-content";
const ERROR_CLASS: &str = "error";
const TAG_CLASS: &str = "hls-line tag";
const URI_CLASS: &str = "hls-line uri";
const COMMENT_CLASS: &str = "hls-line comment";
const BLANK_CLASS: &str = "hls-line blank";

#[component]
pub fn ViewerLoading() -> impl IntoView {
    view! {
        <div class=VIEWER_CLASS>
            <p>"Loading..."</p>
        </div>
    }
}

#[component]
pub fn ViewerError(error: String) -> impl IntoView {
    view! {
        <div class=VIEWER_CLASS>
            <p class=ERROR_CLASS>{error}</p>
        </div>
    }
}

#[component]
pub fn Viewer(playlist: String, base_url: String) -> impl IntoView {
    if playlist.is_empty() {
        return Either::Left(view! { <div class=VIEWER_CLASS>{Vec::new()}</div> });
    }
    match try_get_lines(&playlist, &base_url) {
        Ok(lines) => Either::Left(view! { <div class=VIEWER_CLASS>{lines}</div> }),
        Err(error) => Either::Right(view! { <ViewerError error=error.to_string() /> }),
    }
}

#[derive(Debug)]
enum ViewerError {
    PlaylistIdentifierNotPresent,
}
impl Display for ViewerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlaylistIdentifierNotPresent => {
                write!(f, "Error: playlist identifier (#EXTM3U) not present")
            }
        }
    }
}
impl Error for ViewerError {}

fn try_get_lines(playlist: &str, base_url: &str) -> Result<Vec<AnyView>, ViewerError> {
    // The base_url should never fail to parse. I _could_ create a separate flow in this case that
    // makes no attempt to insert links, but I feel that is an over-complication.
    let base_url = Url::parse(base_url).ok();
    let mut reader = Reader::from_str(
        playlist,
        ParsingOptionsBuilder::new()
            .with_parsing_for_m3u()
            .with_parsing_for_media()
            .with_parsing_for_i_frame_stream_inf()
            .build(),
    );
    let mut lines = Vec::new();
    match reader.read_line() {
        Ok(Some(HlsLine::KnownTag(known::Tag::Hls(hls::Tag::M3u(tag))))) => {
            let line = tag.into_inner();
            lines.push(
                view! { <p class=TAG_CLASS>{String::from_utf8_lossy(line.value())}</p> }.into_any(),
            );
        }
        _ => return Err(ViewerError::PlaylistIdentifierNotPresent),
    }
    while let Ok(Some(line)) = reader.read_line() {
        match line {
            HlsLine::KnownTag(tag) => match tag {
                known::Tag::Hls(tag) => match tag {
                    hls::Tag::Media(media) => {
                        if let Some(uri) = media.uri() {
                            let uri = uri.to_string();
                            let tag_inner = media.into_inner();
                            lines.push(view_from_uri_tag(uri, tag_inner, &base_url));
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
                        lines.push(view_from_uri_tag(uri, tag_inner, &base_url));
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
            HlsLine::Uri(uri) => lines.push(
                view! {
                    <a href=resolve_href(&base_url, uri) class=URI_CLASS>
                        {uri}
                    </a>
                }
                .into_any(),
            ),
            HlsLine::Comment(comment) => {
                lines.push(view! { <p class=COMMENT_CLASS>"#" {comment}</p> }.into_any())
            }
            HlsLine::UnknownTag(tag) => lines.push(
                view! { <p class=TAG_CLASS>{String::from_utf8_lossy(tag.as_bytes())}</p> }
                    .into_any(),
            ),
            HlsLine::Blank => lines.push(view! { <p class=BLANK_CLASS></p> }.into_any()),
        }
    }
    Ok(lines)
}

fn resolve_href(base_url: &Option<Url>, uri: &str) -> String {
    let Some(base_url) = base_url else {
        return String::from("#");
    };
    if let Ok(absolute_url) = base_url.join(uri) {
        // https://url.spec.whatwg.org/#query-percent-encode-set
        // The query percent-encode set is the C0 control percent-encode set and U+0020 SPACE,
        // U+0022 ("), U+0023 (#), U+003C (<), and U+003E (>).
        const QUERY: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>');
        format!(
            "?{}={}",
            PLAYLIST_URL_QUERY_NAME,
            utf8_percent_encode(absolute_url.as_str(), QUERY).to_string()
        )
    } else {
        String::from("#")
    }
}

fn view_from_uri_tag(uri: String, tag_inner: TagInner, base_url: &Option<Url>) -> AnyView {
    let line = String::from_utf8_lossy(tag_inner.value());
    let uri_index = line.find(uri.as_str()).unwrap();
    let (first, second) = line.split_at(uri_index);
    let (second, third) = second.split_at(uri.len());
    view! {
        <p class=TAG_CLASS>
            {first}<a href=resolve_href(base_url, uri.as_str())>{second}</a>{third}
        </p>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_relative_uri() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8";
        assert_eq!(
            "?playlist_url=https://example.com/hls/hi/video-only.m3u8".to_string(),
            resolve_href(&Some(base_url), uri)
        );
    }

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_absolute_uri() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "https://ads.server.com/ads/hls/hi/video-only.m3u8";
        assert_eq!(
            "?playlist_url=https://ads.server.com/ads/hls/hi/video-only.m3u8".to_string(),
            resolve_href(&Some(base_url), uri)
        );
    }

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_uri_stepping_out_of_base_path() {
        let base_url = Url::parse("https://ads.com/1234/main/mvp.m3u8").unwrap();
        let uri = "../media/3.m3u8";
        assert_eq!(
            "?playlist_url=https://ads.com/1234/media/3.m3u8".to_string(),
            resolve_href(&Some(base_url), uri)
        );
    }

    #[test]
    fn resolve_href_does_not_need_to_escape_query_parts() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8?token=1234";
        assert_eq!(
            "?playlist_url=https://example.com/hls/hi/video-only.m3u8?token=1234".to_string(),
            resolve_href(&Some(base_url), uri)
        );
    }

    #[test]
    fn resolve_href_should_escape_fragment() {
        let base_url = Url::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8?token=1234#fragment";
        assert_eq!(
            "?playlist_url=https://example.com/hls/hi/video-only.m3u8?token=1234%23fragment"
                .to_string(),
            resolve_href(&Some(base_url), uri)
        );
    }

    #[test]
    fn resolve_href_when_no_base_should_just_resolve_to_hash() {
        assert_eq!("#".to_string(), resolve_href(&None, "some/uri.m3u8"));
    }
}
