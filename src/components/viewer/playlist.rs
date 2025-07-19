use super::{
    BLANK_CLASS, COMMENT_CLASS, HIGHLIGHTED, HIGHLIGHTED_URI_CLASS, MAIN_VIEW_CLASS,
    MAIN_VIEW_WITH_SUPPLEMENTAL_CLASS, TAG_CLASS, URI_CLASS,
};
use crate::{
    components::CopyButton,
    utils::{
        href::{map_href, media_playlist_href, segment_href},
        network::RequestRange,
    },
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
use std::{error::Error, fmt::Display};

pub struct HighlightedMapInfo {
    pub url: String,
    pub min_media_sequence: u64,
}

#[component]
pub fn PlaylistViewer(
    playlist: String,
    #[prop(default = false)] supplemental_showing: bool,
    #[prop(optional)] highlighted_segment: Option<u64>,
    #[prop(optional)] highlighted_map_info: Option<HighlightedMapInfo>,
) -> Result<impl IntoView, PlaylistError> {
    if playlist.is_empty() {
        return Ok(EitherOf3::A(view! { <div class=MAIN_VIEW_CLASS /> }));
    }
    match try_get_lines(&playlist, highlighted_segment, highlighted_map_info) {
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
    highlighted_segment: Option<u64>,
    highlighted_map_info: Option<HighlightedMapInfo>,
) -> Result<Vec<AnyView>, PlaylistError> {
    let mut reader = Reader::from_str(
        playlist,
        ParsingOptionsBuilder::new()
            .with_parsing_for_m3u()
            .with_parsing_for_map()
            .with_parsing_for_media()
            .with_parsing_for_media_sequence()
            .with_parsing_for_byterange()
            .with_parsing_for_i_frame_stream_inf()
            .build(),
    );
    let mut lines = Vec::new();

    // Segment state
    let mut media_sequence = 0;
    let mut is_media_playlist = false;
    let mut highlighted_one_map = false;
    let mut previous_segment_byterange_end = 0u64;
    let mut segment_byterange = None;

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
                            lines.push(view_from_uri_tag(UriTagViewOptions {
                                uri,
                                tag_inner,
                                uri_type: UriType::Playlist,
                                media_sequence,
                                is_highlighted: false,
                                byterange: None,
                            }));
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
                        lines.push(view_from_uri_tag(UriTagViewOptions {
                            uri,
                            tag_inner,
                            uri_type: UriType::Playlist,
                            media_sequence,
                            is_highlighted: false,
                            byterange: None,
                        }));
                    }
                    hls::Tag::Map(map) => {
                        let uri = map.uri().to_string();
                        let byterange = map.byterange().map(RequestRange::from);
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
                        lines.push(view_from_uri_tag(UriTagViewOptions {
                            uri,
                            tag_inner,
                            uri_type: UriType::Map,
                            media_sequence,
                            is_highlighted,
                            byterange,
                        }));
                    }
                    hls::Tag::MediaSequence(tag) => {
                        media_sequence = tag.media_sequence();
                        let line = tag.into_inner();
                        lines.push(
                            view! { <p class=TAG_CLASS>{String::from_utf8_lossy(line.value())}</p> }.into_any()
                        );
                    }
                    hls::Tag::Byterange(tag) => {
                        let offset = tag.offset().unwrap_or(previous_segment_byterange_end);
                        let length = tag.length();
                        let byterange = RequestRange::from_length_with_offset(length, offset);
                        segment_byterange = Some(byterange);
                        previous_segment_byterange_end = byterange.end + 1;
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
                let byterange = segment_byterange;
                segment_byterange = None;
                lines.push(
                    view! {
                        <a
                            href=resolve_href(ResolveOptions {
                                uri,
                                uri_type,
                                media_sequence,
                                byterange,
                            })
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

struct ResolveOptions<'a> {
    uri: &'a str,
    uri_type: UriType,
    media_sequence: u64,
    byterange: Option<RequestRange>,
}

fn resolve_href(opts: ResolveOptions) -> String {
    let ResolveOptions {
        uri,
        uri_type,
        media_sequence,
        byterange,
    } = opts;
    match uri_type {
        UriType::Playlist => media_playlist_href(uri),
        UriType::Segment => segment_href(uri, media_sequence, byterange),
        UriType::Map => map_href(uri, media_sequence, byterange),
    }
    .unwrap_or(String::from("#"))
}

struct UriTagViewOptions<'a> {
    uri: String,
    tag_inner: TagInner<'a>,
    uri_type: UriType,
    media_sequence: u64,
    is_highlighted: bool,
    byterange: Option<RequestRange>,
}

fn view_from_uri_tag(opts: UriTagViewOptions) -> AnyView {
    let UriTagViewOptions {
        uri,
        tag_inner,
        uri_type,
        media_sequence,
        is_highlighted,
        byterange,
    } = opts;
    let line = String::from_utf8_lossy(tag_inner.value());
    let uri_index = line.find(uri.as_str()).unwrap();
    let (first, second) = line.split_at(uri_index);
    let (second, third) = second.split_at(uri.len());
    let class = if is_highlighted { HIGHLIGHTED } else { "" };
    view! {
        <p class=TAG_CLASS>
            {first}
            <a
                class=class
                href=resolve_href(ResolveOptions {
                    uri: &uri,
                    uri_type,
                    media_sequence,
                    byterange,
                })
            >
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
