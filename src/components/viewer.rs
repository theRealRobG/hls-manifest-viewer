use crate::PLAYLIST_URL_QUERY_NAME;
use fluent_uri::{
    encoding::{
        encoder::{Data, Query},
        EString,
    },
    Uri, UriRef,
};
use leptos::prelude::*;
use m3u8::{
    config::ParsingOptionsBuilder,
    line::HlsLine,
    tag::{
        hls::{self, TagInner},
        known,
    },
    Reader,
};

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
    let base_url = Uri::parse(base_url.as_str()).ok();
    let mut reader = Reader::from_str(
        playlist.as_str(),
        ParsingOptionsBuilder::new()
            .with_parsing_for_media()
            .with_parsing_for_i_frame_stream_inf()
            .build(),
    );
    let mut lines = Vec::new();
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
    view! { <div class=VIEWER_CLASS>{lines}</div> }
}

fn resolve_href(base_url: &Option<Uri<&str>>, uri: &str) -> String {
    base_url
        .map(|base_url| {
            if let Some(absolute_url) = UriRef::parse(uri)
                .ok()
                .and_then(|uri| uri.resolve_against(&base_url).ok())
            {
                let mut query = EString::<Query>::new();
                query.push('?');
                query.encode::<Data>(PLAYLIST_URL_QUERY_NAME);
                query.push('=');
                query.encode::<Data>(absolute_url.as_str());
                Some(query.into_string())
            } else {
                None
            }
        })
        .flatten()
        .unwrap_or(String::from("#"))
}

fn view_from_uri_tag(uri: String, tag_inner: TagInner, base_url: &Option<Uri<&str>>) -> AnyView {
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
    fn robtest() {
        let uri = "example.com";
        let x = "EXT-X-MEDIA:URI=\"example.com\",TYPE=AUDIO";
        let index = x.find(uri).unwrap();
        let (first, second) = x.split_at(index);
        let (second, third) = second.split_at(uri.len());
        println!("First: {} Second: {} Third: {}", first, second, third);
    }

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_relative_uri() {
        let base_url = Uri::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8";
        assert_eq!(
            "?playlist_url=https%3A%2F%2Fexample.com%2Fhls%2Fhi%2Fvideo-only.m3u8".to_string(),
            resolve_href(&Some(base_url), uri)
        );
    }

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_absolute_uri() {
        let base_url = Uri::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "https://ads.server.com/ads/hls/hi/video-only.m3u8";
        assert_eq!(
            "?playlist_url=https%3A%2F%2Fads.server.com%2Fads%2Fhls%2Fhi%2Fvideo-only.m3u8"
                .to_string(),
            resolve_href(&Some(base_url), uri)
        );
    }

    #[test]
    fn resolve_href_should_provide_local_uri_with_query_for_uri_stepping_out_of_base_path() {
        let base_url = Uri::parse("https://ads.com/1234/main/mvp.m3u8").unwrap();
        let uri = "../media/3.m3u8";
        assert_eq!(
            "?playlist_url=https%3A%2F%2Fads.com%2F1234%2Fmedia%2F3.m3u8".to_string(),
            resolve_href(&Some(base_url), uri)
        );
    }

    #[test]
    fn resolve_href_should_escape_query_parts() {
        let base_url = Uri::parse("https://example.com/hls/mvp.m3u8").unwrap();
        let uri = "hi/video-only.m3u8?token=1234";
        assert_eq!(
            "?playlist_url=https%3A%2F%2Fexample.com%2Fhls%2Fhi%2Fvideo-only.m3u8%3Ftoken%3D1234"
                .to_string(),
            resolve_href(&Some(base_url), uri)
        );
    }

    #[test]
    fn resolve_href_when_no_base_should_just_resolve_to_hash() {
        assert_eq!("#".to_string(), resolve_href(&None, "some/uri.m3u8"));
    }
}
