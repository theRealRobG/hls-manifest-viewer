use super::{
    BLANK_CLASS, COMMENT_CLASS, HIGHLIGHTED, HIGHLIGHTED_URI_CLASS, MAIN_VIEW_CLASS,
    MAIN_VIEW_WITH_SUPPLEMENTAL_CLASS, TAG_CLASS, URI_CLASS,
};
use crate::{
    components::CopyButton,
    utils::{
        href::{map_href, media_playlist_href, part_href, segment_href},
        network::RequestRange,
    },
};
use leptos::{either::EitherOf3, prelude::*};
use leptos_router::hooks::use_query_map;
use m3u8::{
    config::ParsingOptionsBuilder,
    line::HlsLine,
    tag::{
        hls::{self, define::Define, TagInner, TagName, TagType},
        known, unknown,
    },
    Reader,
};
use std::{collections::HashMap, error::Error, fmt::Display};

macro_rules! tag_into_view {
    ($tag:ident) => {{
        let line = $tag.into_inner();
        view! { <p class=TAG_CLASS>{String::from_utf8_lossy(line.value())}</p> }.into_any()
    }};
}

pub struct HighlightedMapInfo {
    pub url: String,
    pub min_media_sequence: u64,
}

pub struct HighlightedPartInfo {
    pub media_sequence: u64,
    pub part_index: u32,
}

#[component]
pub fn PlaylistViewer(
    playlist: String,
    imported_definitions: HashMap<String, String>,
    #[prop(default = false)] supplemental_showing: bool,
    #[prop(optional)] highlighted_segment: Option<u64>,
    #[prop(optional)] highlighted_map_info: Option<HighlightedMapInfo>,
    #[prop(optional)] highlighted_part_info: Option<HighlightedPartInfo>,
) -> Result<impl IntoView, PlaylistError> {
    if playlist.is_empty() {
        return Ok(EitherOf3::A(view! { <div class=MAIN_VIEW_CLASS /> }));
    }
    match try_get_lines(
        &playlist,
        imported_definitions,
        highlighted_segment,
        highlighted_map_info,
        highlighted_part_info,
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
    imported_definitions: HashMap<String, String>,
    highlighted_segment: Option<u64>,
    highlighted_map_info: Option<HighlightedMapInfo>,
    highlighted_part_info: Option<HighlightedPartInfo>,
) -> Result<Vec<AnyView>, PlaylistError> {
    let mut reader = Reader::from_str(
        playlist,
        ParsingOptionsBuilder::new()
            .with_parsing_for_m3u()
            .with_parsing_for_map()
            .with_parsing_for_media()
            .with_parsing_for_media_sequence()
            .with_parsing_for_byterange()
            .with_parsing_for_define()
            .with_parsing_for_i_frame_stream_inf()
            .with_parsing_for_part()
            .build(),
    );
    let mut parsing_state = ParsingState::new(
        imported_definitions,
        highlighted_segment,
        highlighted_map_info,
        highlighted_part_info,
    );

    match reader.read_line() {
        Ok(Some(HlsLine::KnownTag(known::Tag::Hls(hls::Tag::M3u(tag))))) => {
            parsing_state.lines.push(tag_into_view!(tag))
        }
        _ => return Err(PlaylistError::PlaylistIdentifierNotPresent),
    }
    while let Ok(Some(line)) = reader.read_line() {
        match line {
            HlsLine::KnownTag(tag) => match tag {
                known::Tag::Hls(tag) => match tag {
                    hls::Tag::Media(tag) => x_media(tag, &mut parsing_state),
                    hls::Tag::IFrameStreamInf(tag) => x_i_frame_stream_inf(tag, &mut parsing_state),
                    hls::Tag::Map(tag) => x_map(tag, &mut parsing_state),
                    hls::Tag::MediaSequence(tag) => x_media_sequence(tag, &mut parsing_state),
                    hls::Tag::Byterange(tag) => x_byterange(tag, &mut parsing_state),
                    hls::Tag::Define(tag) => x_define(tag, &mut parsing_state),
                    hls::Tag::Part(tag) => x_part(tag, &mut parsing_state),
                    tag => {
                        parsing_state.lines.push(tag_into_view!(tag));
                    }
                },
                known::Tag::Custom(_) => panic!("No custom tags registered"),
            },
            HlsLine::Uri(uri) => uri_line(uri, &mut parsing_state),
            HlsLine::Comment(comment) => parsing_state
                .lines
                .push(view! { <p class=COMMENT_CLASS>"#" {comment}</p> }.into_any()),
            HlsLine::UnknownTag(tag) => {
                if !parsing_state.is_media_playlist && is_media_tag(&tag) {
                    parsing_state.is_media_playlist = true;
                }
                parsing_state.lines.push(
                    view! { <p class=TAG_CLASS>{String::from_utf8_lossy(tag.as_bytes())}</p> }
                        .into_any(),
                );
            }
            HlsLine::Blank => parsing_state
                .lines
                .push(view! { <p class=BLANK_CLASS></p> }.into_any()),
        }
    }
    Ok(parsing_state.lines)
}

// Uri line handling

fn uri_line(uri: &str, state: &mut ParsingState) {
    let uri_class = if Some(state.media_sequence) == state.highlighted_segment {
        HIGHLIGHTED_URI_CLASS
    } else {
        URI_CLASS
    };
    let uri_type = if state.is_media_playlist {
        UriType::Segment
    } else {
        UriType::Playlist
    };
    let byterange = state.segment_byterange;
    state.segment_byterange = None;
    state.lines.push(
        view! {
            <a
                href=resolve_href(ResolveOptions {
                    uri,
                    uri_type,
                    media_sequence: state.media_sequence,
                    byterange,
                    definitions: &state.local_definitions,
                })
                class=uri_class
            >
                {uri}
            </a>
        }
        .into_any(),
    );
    // Reset segment state.
    state.media_sequence += 1;
    state.part_index = 0;
    state.offset_after_last_part_byterange = 0;
    if state.segment_byterange.is_some() {
        state.segment_byterange = None;
    } else {
        state.offset_after_last_segment_byterange = 0;
    }
}

// Special tag handling

fn x_media(tag: hls::media::Media, state: &mut ParsingState) {
    if let Some(uri) = tag.uri() {
        let uri = uri.to_string();
        let tag_inner = tag.into_inner();
        state.lines.push(view_from_uri_tag(UriTagViewOptions {
            uri,
            tag_inner,
            uri_type: UriType::Playlist,
            media_sequence: state.media_sequence,
            is_highlighted: false,
            byterange: None,
            definitions: &state.local_definitions,
        }));
    } else {
        state.lines.push(tag_into_view!(tag));
    }
}

fn x_i_frame_stream_inf(tag: hls::i_frame_stream_inf::IFrameStreamInf, state: &mut ParsingState) {
    let uri = tag.uri().to_string();
    let tag_inner = tag.into_inner();
    state.lines.push(view_from_uri_tag(UriTagViewOptions {
        uri,
        tag_inner,
        uri_type: UriType::Playlist,
        media_sequence: state.media_sequence,
        is_highlighted: false,
        byterange: None,
        definitions: &state.local_definitions,
    }));
}

fn x_map(tag: hls::map::Map, state: &mut ParsingState) {
    let uri = tag.uri().to_string();
    let byterange = tag.byterange().map(RequestRange::from);
    let is_highlighted = if let Some(info) = &state.highlighted_map_info {
        !state.highlighted_one_map
            && info.min_media_sequence <= state.media_sequence
            && info.url.contains(&uri)
    } else {
        false
    };
    if is_highlighted {
        state.highlighted_one_map = true
    }
    let tag_inner = tag.into_inner();
    state.lines.push(view_from_uri_tag(UriTagViewOptions {
        uri,
        tag_inner,
        uri_type: UriType::Map,
        media_sequence: state.media_sequence,
        is_highlighted,
        byterange,
        definitions: &state.local_definitions,
    }));
}

fn x_media_sequence(tag: hls::media_sequence::MediaSequence, state: &mut ParsingState) {
    state.media_sequence = tag.media_sequence();
    state.lines.push(tag_into_view!(tag));
}

fn x_byterange(tag: hls::byterange::Byterange, state: &mut ParsingState) {
    let offset = tag
        .offset()
        .unwrap_or(state.offset_after_last_segment_byterange);
    let length = tag.length();
    let byterange = RequestRange::from_length_with_offset(length, offset);
    state.segment_byterange = Some(byterange);
    state.offset_after_last_segment_byterange = byterange.end + 1;
    state.lines.push(tag_into_view!(tag));
}

fn x_define(tag: hls::define::Define, state: &mut ParsingState) {
    match tag {
        Define::Name(ref name) => {
            state
                .local_definitions
                .insert(name.name().to_string(), name.value().to_string());
        }
        Define::Import(ref import) => {
            let name = import.import().to_string();
            if let Some(value) = state.imported_definitions.get(&name) {
                state.local_definitions.insert(name, value.to_string());
            } else {
                log::error!("could not resolve EXT-X-DEFINE:IMPORT=\"{name}\"");
            }
        }
        Define::Queryparam(ref queryparam) => {
            if let Some(value) = use_query_map().get_untracked().get(queryparam.queryparam()) {
                state
                    .local_definitions
                    .insert(queryparam.queryparam().to_string(), value);
            } else {
                let q = queryparam.queryparam();
                log::error!("could not resolve EXT-X-DEFINE:QUERYPARAM=\"{q}\"");
            }
        }
    }
    state.lines.push(tag_into_view!(tag));
}

fn x_part(tag: hls::part::Part, state: &mut ParsingState) {
    let uri = tag.uri().to_string();
    // The range is a little complicated because the lack of an offset means that the current offset
    // is calculated based on the end of the previous part byterange.
    let byterange = if let Some(tag_byterange) = tag.byterange() {
        let offset = tag_byterange
            .offset
            .unwrap_or(state.offset_after_last_part_byterange);
        let length = tag_byterange.length;
        let request_range = RequestRange::from_length_with_offset(length, offset);
        state.offset_after_last_part_byterange = request_range.end + 1;
        Some(request_range)
    } else {
        state.offset_after_last_part_byterange = 0;
        None
    };
    let is_highlighted = state.highlighted_part_info.as_ref().map_or(false, |info| {
        info.media_sequence == state.media_sequence && info.part_index == state.part_index
    });
    let tag_inner = tag.into_inner();
    state.lines.push(view_from_uri_tag(UriTagViewOptions {
        uri,
        tag_inner,
        uri_type: UriType::Part {
            part_index: state.part_index,
        },
        media_sequence: state.media_sequence,
        is_highlighted,
        byterange,
        definitions: &state.local_definitions,
    }));
    // Based on https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-3.2
    //    Each Partial Segment has a Part Index, which is an integer indicating
    //    the position of the Partial Segment within its Parent Segment.  The
    //    first Partial Segment has a Part Index of zero.
    // So since the part_index is reset to 0 on each new segment we only increment after the new
    // part.
    state.part_index += 1;
}

// General href utility

fn resolve_href(opts: ResolveOptions) -> String {
    let ResolveOptions {
        uri,
        uri_type,
        media_sequence,
        byterange,
        definitions,
    } = opts;
    match uri_type {
        UriType::Playlist => media_playlist_href(uri, definitions),
        UriType::Segment => segment_href(uri, media_sequence, byterange, definitions),
        UriType::Map => map_href(uri, media_sequence, byterange, definitions),
        UriType::Part { part_index } => {
            part_href(uri, media_sequence, part_index, byterange, definitions)
        }
    }
    .unwrap_or(String::from("#"))
}

fn view_from_uri_tag(opts: UriTagViewOptions) -> AnyView {
    let UriTagViewOptions {
        uri,
        tag_inner,
        uri_type,
        media_sequence,
        is_highlighted,
        byterange,
        definitions,
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
                    definitions,
                })
            >
                {second}
            </a>{third}
        </p>
    }
    .into_any()
}

// Helper for determining whether playlist is mvp or media

fn is_media_tag(tag: &unknown::Tag) -> bool {
    let Ok(name) = TagName::try_from(tag.name()) else {
        return false;
    };
    matches!(
        name.tag_type(),
        TagType::MediaMetadata | TagType::MediaSegment
    )
}

// Convenience types

struct ParsingState {
    // Passed in as parameters
    imported_definitions: HashMap<String, String>,
    highlighted_segment: Option<u64>,
    highlighted_map_info: Option<HighlightedMapInfo>,
    highlighted_part_info: Option<HighlightedPartInfo>,
    // Constructed by default
    lines: Vec<AnyView>,
    media_sequence: u64,
    part_index: u32,
    is_media_playlist: bool,
    highlighted_one_map: bool,
    offset_after_last_segment_byterange: u64,
    offset_after_last_part_byterange: u64,
    segment_byterange: Option<RequestRange>,
    local_definitions: HashMap<String, String>,
}
impl ParsingState {
    fn new(
        imported_definitions: HashMap<String, String>,
        highlighted_segment: Option<u64>,
        highlighted_map_info: Option<HighlightedMapInfo>,
        highlighted_part_info: Option<HighlightedPartInfo>,
    ) -> Self {
        Self {
            imported_definitions,
            highlighted_segment,
            highlighted_map_info,
            highlighted_part_info,
            lines: Default::default(),
            media_sequence: Default::default(),
            part_index: Default::default(),
            is_media_playlist: Default::default(),
            highlighted_one_map: Default::default(),
            offset_after_last_segment_byterange: Default::default(),
            offset_after_last_part_byterange: Default::default(),
            segment_byterange: Default::default(),
            local_definitions: Default::default(),
        }
    }
}

enum UriType {
    Playlist,
    Segment,
    Map,
    Part { part_index: u32 },
}

struct ResolveOptions<'a> {
    uri: &'a str,
    uri_type: UriType,
    media_sequence: u64,
    byterange: Option<RequestRange>,
    definitions: &'a HashMap<String, String>,
}

struct UriTagViewOptions<'a> {
    uri: String,
    tag_inner: TagInner<'a>,
    uri_type: UriType,
    media_sequence: u64,
    is_highlighted: bool,
    byterange: Option<RequestRange>,
    definitions: &'a HashMap<String, String>,
}
