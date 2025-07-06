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

const EXAMPLE_PLAYLIST_URI: &str = "https://example.com/mvp.m3u8";
const EXAMPLE_PLAYLIST: &str = r#"#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aac",NAME="English",DEFAULT=YES,AUTOSELECT=YES,LANGUAGE="en",URI="main/english-audio.m3u8"
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aac",NAME="Deutsch",DEFAULT=NO,AUTOSELECT=YES,LANGUAGE="de",URI="main/german-audio.m3u8"
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aac",NAME="Commentary",DEFAULT=NO,AUTOSELECT=NO,LANGUAGE="en",URI="commentary/audio-only.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=1280000,CODECS="avc1.4d401e,mp4a.40.5",AUDIO="aac"
low/video-only.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2560000,CODECS="avc1.4d401f,mp4a.40.5",AUDIO="aac"
mid/video-only.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=7680000,CODECS="avc1.64001f,mp4a.40.5",AUDIO="aac"
hi/video-only.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=65000,CODECS="mp4a.40.5",AUDIO="aac"
main/english-audio.m3u8
"#;

#[component]
pub fn Viewer() -> impl IntoView {
    let base_url = Uri::parse(EXAMPLE_PLAYLIST_URI).ok();
    let mut reader = Reader::from_str(
        EXAMPLE_PLAYLIST,
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
                                view! { <p class="hls-line tag">{String::from_utf8_lossy(line.value())}</p> }.into_any()
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
                            view! { <p class="hls-line tag">{String::from_utf8_lossy(line.value())}</p> }.into_any()
                        );
                    }
                },
                known::Tag::Custom(_) => panic!("No custom tags registered"),
            },
            HlsLine::Uri(uri) => lines.push(
                view! {
                    <a href=resolve_href(&base_url, uri) class="hls-line uri">
                        {uri}
                    </a>
                }
                .into_any(),
            ),
            HlsLine::Comment(comment) => {
                lines.push(view! { <p class="hls-line comment">"#" {comment}</p> }.into_any())
            }
            HlsLine::UnknownTag(tag) => lines.push(
                view! { <p class="hls-line tag">{String::from_utf8_lossy(tag.as_bytes())}</p> }
                    .into_any(),
            ),
            HlsLine::Blank => lines.push(view! { <p class="hls-line blank"></p> }.into_any()),
        }
    }
    view! { <div class="viewer-content">{lines}</div> }
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
                query.encode::<Data>("playlist_url");
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
        <p class="hls-line tag">
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
