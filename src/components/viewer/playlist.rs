use super::{
    BLANK_CLASS, COMMENT_CLASS, HIGHLIGHTED, HIGHLIGHTED_URI_CLASS, MAIN_VIEW_CLASS,
    MAIN_VIEW_WITH_SUPPLEMENTAL_CLASS, TAG_CLASS, URI_CLASS,
};
use crate::{
    components::CopyButton,
    utils::{
        href::{
            asset_list_href, map_href, media_playlist_href, part_href,
            resolve_playlist_relative_url, scte35_href, segment_href,
        },
        network::RequestRange,
        query_codec::Scte35CommandType,
    },
};
use leptos::{either::EitherOf3, prelude::*};
use leptos_router::hooks::use_query_map;
use quick_m3u8::{
    config::ParsingOptionsBuilder,
    tag::{
        hls::{
            Byterange, Define, MapByterange, MediaSequence, PartByterange, Tag, TagName, TagType,
        },
        AttributeValue, IntoInnerTag, KnownTag, UnknownTag,
    },
    HlsLine, Reader,
};
use std::{borrow::Cow, collections::HashMap, error::Error, fmt::Display};

macro_rules! tag_into_view {
    ($tag:ident) => {{
        let line = $tag.into_inner();
        view! { <p class=TAG_CLASS>{String::from_utf8_lossy(line.value())}</p> }.into_any()
    }};
}

pub enum Highlighted {
    Segment {
        media_sequence: u64,
    },
    Map {
        url: String,
        min_media_sequence: u64,
    },
    Part {
        media_sequence: u64,
        part_index: u32,
    },
    Scte35 {
        daterange_id: String,
        command_type: Scte35CommandType,
    },
    AssetList {
        daterange_id: String,
    },
}

pub struct HighlightedMapInfo {
    pub url: String,
    pub min_media_sequence: u64,
}

pub struct HighlightedPartInfo {
    pub media_sequence: u64,
    pub part_index: u32,
}

pub struct HighlightedScte35Info {
    pub daterange_id: String,
    pub command_type: Scte35CommandType,
}

#[component]
pub fn PlaylistViewer(
    playlist: String,
    imported_definitions: HashMap<String, String>,
    #[prop(default = false)] supplemental_showing: bool,
    #[prop(optional)] highlighted: Option<Highlighted>,
) -> Result<impl IntoView, PlaylistError> {
    if playlist.is_empty() {
        return Ok(EitherOf3::A(view! { <div class=MAIN_VIEW_CLASS /> }));
    }
    match try_get_lines(&playlist, imported_definitions, highlighted) {
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
    highlighted: Option<Highlighted>,
) -> Result<Vec<AnyView>, PlaylistError> {
    let mut reader = Reader::from_str(
        playlist,
        ParsingOptionsBuilder::new()
            .with_parsing_for_m3u()
            .with_parsing_for_media_sequence()
            .with_parsing_for_byterange()
            .with_parsing_for_define()
            .build(),
    );
    let mut parsing_state = ParsingState::new(imported_definitions, highlighted);

    match reader.read_line() {
        Ok(Some(HlsLine::KnownTag(KnownTag::Hls(Tag::M3u(tag))))) => {
            parsing_state.lines.push(tag_into_view!(tag))
        }
        _ => return Err(PlaylistError::PlaylistIdentifierNotPresent),
    }
    while let Ok(Some(line)) = reader.read_line() {
        match line {
            HlsLine::KnownTag(tag) => match tag {
                KnownTag::Hls(tag) => match tag {
                    Tag::MediaSequence(tag) => x_media_sequence(tag, &mut parsing_state),
                    Tag::Byterange(tag) => x_byterange(tag, &mut parsing_state),
                    Tag::Define(tag) => x_define(tag, &mut parsing_state),
                    tag => {
                        parsing_state.lines.push(tag_into_view!(tag));
                    }
                },
                KnownTag::Custom(_) => panic!("No custom tags registered"),
            },
            HlsLine::Uri(uri) => uri_line(&uri, &mut parsing_state),
            HlsLine::Comment(comment) => parsing_state
                .lines
                .push(view! { <p class=COMMENT_CLASS>"#" {comment}</p> }.into_any()),
            HlsLine::UnknownTag(tag) => {
                let tag_name = TagName::try_from(tag.name()).ok();
                if !parsing_state.is_media_playlist && is_media_tag(tag_name) {
                    parsing_state.is_media_playlist = true;
                }
                match tag_name {
                    Some(TagName::Media) => playlist_uri_tag(&tag, &mut parsing_state),
                    Some(TagName::IFrameStreamInf) => playlist_uri_tag(&tag, &mut parsing_state),
                    Some(TagName::Map) => x_map(&tag, &mut parsing_state),
                    Some(TagName::Part) => x_part(&tag, &mut parsing_state),
                    Some(TagName::Daterange) => x_daterange(&tag, &mut parsing_state),
                    _ => parsing_state.lines.push(
                        view! { <p class=TAG_CLASS>{String::from_utf8_lossy(tag.as_bytes())}</p> }
                            .into_any(),
                    ),
                }
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

/// Handle a tag that links to a playlist (`EXT-X-MEDIA` or `EXT-X-I-FRAME-STREAM-INF`).
fn playlist_uri_tag(tag: &UnknownTag, state: &mut ParsingState) {
    let markup = split_tag_as_markup(
        tag,
        ["URI"],
        |_, value| {
            resolve_href(ResolveOptions {
                uri: value,
                uri_type: UriType::Playlist,
                media_sequence: state.media_sequence,
                byterange: None,
                definitions: &state.local_definitions,
            })
        },
        |_, _| false,
    );
    state.lines.push(view_from_markup(markup));
}

fn x_map(tag: &UnknownTag, state: &mut ParsingState) {
    let byterange = map_byterange(tag).map(RequestRange::from);
    let markup = split_tag_as_markup(
        tag,
        ["URI"],
        |_, value| {
            resolve_href(ResolveOptions {
                uri: value,
                uri_type: UriType::Map,
                media_sequence: state.media_sequence,
                byterange,
                definitions: &state.local_definitions,
            })
        },
        |_, value| {
            if let Some(info) = &state.highlighted_map_info {
                !state.highlighted_one_map
                    && info.min_media_sequence <= state.media_sequence
                    && resolve_playlist_relative_url(value, &state.local_definitions)
                        .map(|s| s == info.url)
                        .unwrap_or(false)
            } else {
                false
            }
        },
    );
    state.lines.push(view_from_markup(markup));
}

fn x_media_sequence(tag: MediaSequence, state: &mut ParsingState) {
    state.media_sequence = tag.media_sequence();
    state.lines.push(tag_into_view!(tag));
}

fn x_byterange(tag: Byterange, state: &mut ParsingState) {
    let offset = tag
        .offset()
        .unwrap_or(state.offset_after_last_segment_byterange);
    let length = tag.length();
    let byterange = RequestRange::from_length_with_offset(length, offset);
    state.segment_byterange = Some(byterange);
    state.offset_after_last_segment_byterange = byterange.end + 1;
    state.lines.push(tag_into_view!(tag));
}

fn x_define(tag: Define, state: &mut ParsingState) {
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

fn x_part(tag: &UnknownTag, state: &mut ParsingState) {
    // The range is a little complicated because the lack of an offset means that the current offset
    // is calculated based on the end of the previous part byterange.
    let byterange = if let Some(tag_byterange) = part_byterange(tag) {
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
    let is_highlighted = state.highlighted_part_info.as_ref().is_some_and(|info| {
        info.media_sequence == state.media_sequence && info.part_index == state.part_index
    });
    let markup = split_tag_as_markup(
        tag,
        ["URI"],
        |_, value| {
            resolve_href(ResolveOptions {
                uri: value,
                uri_type: UriType::Part {
                    part_index: state.part_index,
                },
                media_sequence: state.media_sequence,
                byterange,
                definitions: &state.local_definitions,
            })
        },
        |_, _| is_highlighted,
    );
    state.lines.push(view_from_markup(markup));
    // Based on https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-3.2
    //    Each Partial Segment has a Part Index, which is an integer indicating
    //    the position of the Partial Segment within its Parent Segment.  The
    //    first Partial Segment has a Part Index of zero.
    // So since the part_index is reset to 0 on each new segment we only increment after the new
    // part.
    state.part_index += 1;
}

fn x_daterange(tag: &UnknownTag, state: &mut ParsingState) {
    let id = tag
        .value()
        .and_then(|v| v.try_as_ordered_attribute_list().ok())
        .and_then(|v| {
            v.iter().find_map(|(name, value)| {
                if *name == "ID" {
                    value.quoted().map(String::from)
                } else {
                    None
                }
            })
        });
    let markup = split_tag_as_markup(
        tag,
        [
            "SCTE35-OUT",
            "SCTE35-IN",
            "SCTE35-CMD",
            "X-ASSET-URI",
            "X-ASSET-LIST",
        ],
        |name, value| match name {
            "SCTE35-OUT" => id
                .as_ref()
                .and_then(|id| scte35_href(value, id, Scte35CommandType::Out)),
            "SCTE35-IN" => id
                .as_ref()
                .and_then(|id| scte35_href(value, id, Scte35CommandType::In)),
            "SCTE35-CMD" => id
                .as_ref()
                .and_then(|id| scte35_href(value, id, Scte35CommandType::Cmd)),
            "X-ASSET-URI" => media_playlist_href(value, &state.local_definitions),
            "X-ASSET-LIST" => id
                .as_ref()
                .and_then(|id| asset_list_href(value, id, &state.local_definitions)),
            _ => {
                log::error!("unexpected SCTE35 attribute on daterange: {name}");
                None
            }
        },
        |name, _| {
            let scte35_highlight = state
                .highlighted_scte35_info
                .as_ref()
                .and_then(|info| {
                    id.as_ref().and_then(|id| {
                        if id == &info.daterange_id {
                            match info.command_type {
                                Scte35CommandType::Cmd => Some(name == "SCTE35-CMD"),
                                Scte35CommandType::Out => Some(name == "SCTE35-OUT"),
                                Scte35CommandType::In => Some(name == "SCTE35-IN"),
                            }
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or(false);

            let asset_list_highlight = state
                .highlighted_asset_list_daterange_id
                .as_ref()
                .map(|highlighted_id| id.as_ref() == Some(highlighted_id))
                .unwrap_or(false);

            scte35_highlight || asset_list_highlight
        },
    );
    state.lines.push(view_from_markup(markup));
}

// General href utility

fn resolve_href(opts: ResolveOptions) -> Option<String> {
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
}

fn view_from_markup(markup: Vec<Markup>) -> AnyView {
    view! {
        <p class=TAG_CLASS>
            {markup
                .into_iter()
                .map(|markup| match markup {
                    Markup::String(s) => view! { {s} }.into_any(),
                    Markup::Link { href, value, highlighted } => {
                        let class = if highlighted { HIGHLIGHTED } else { "" };
                        view! {
                            <a class=class href=href>
                                {value}
                            </a>
                        }
                            .into_any()
                    }
                })
                .collect_view()}
        </p>
    }
    .into_any()
}

/// Split up a tag into markup of strings and links. The links are intended to be wrapped in anchor
/// tags.
///
/// Both the `href_fn` and the `highlight_fn` take the attribute name and the attribute value as
/// input parameters. The `href_fn` expects to return the `href` for the anchor, and if not, then
/// the link will be merged into the `String` markup. The `highlight_fn` expects to return whether
/// the anchor should be highlighted.
fn split_tag_as_markup<const N: usize, HrefFn, HighlightFn>(
    tag: &UnknownTag,
    link_attrs: [&'static str; N],
    href_fn: HrefFn,
    highlight_fn: HighlightFn,
) -> Vec<Markup>
where
    HrefFn: Fn(&str, &str) -> Option<String>,
    HighlightFn: Fn(&str, &str) -> bool,
{
    let Some(list) = tag
        .value()
        .and_then(|v| v.try_as_ordered_attribute_list().ok())
    else {
        return vec![Markup::String(
            String::from_utf8_lossy(tag.as_bytes()).to_string(),
        )];
    };
    let mut markup = vec![];
    // The current string holds the string markup since the last found link. Any new attribute that
    // is not a link (or cannot be created as a link) will be pushed to this string. Once a link is
    // found then this string will be converted to `Markup` and pushed to the `markup` vector, then
    // will be cleared.
    let mut current_string = format!("#EXT{}:", tag.name());
    // The separator is used for separating attributes. The first entry is empty and it subsequently
    // changes to `,`.
    let mut separator = "";
    for (name, value) in list {
        let (value, quotes) = match value {
            AttributeValue::Unquoted(v) => (String::from_utf8_lossy(v.0), ""),
            AttributeValue::Quoted(s) => (Cow::Borrowed(s), "\""),
        };
        if link_attrs.contains(&name)
            && let Some(href) = href_fn(name, &value)
        {
            let mut string = std::mem::take(&mut current_string);
            string.push_str(separator);
            string.push_str(name);
            string.push('=');
            string.push_str(quotes);
            markup.push(Markup::String(string));
            markup.push(Markup::Link {
                href,
                value: value.to_string(),
                highlighted: highlight_fn(name, &value),
            });
            current_string.push_str(quotes);
        } else {
            current_string.push_str(separator);
            current_string.push_str(name);
            current_string.push('=');
            current_string.push_str(quotes);
            current_string.push_str(&value);
            current_string.push_str(quotes);
        }
        separator = ",";
    }
    if !current_string.is_empty() {
        markup.push(Markup::String(current_string));
    }
    markup
}

// Helper for determining whether playlist is mvp or media

fn is_media_tag(tag_name: Option<TagName>) -> bool {
    if let Some(tag_name) = tag_name {
        matches!(
            tag_name.tag_type(),
            TagType::MediaMetadata | TagType::MediaSegment
        )
    } else {
        false
    }
}

// Helper methods for getting byteranges for EXT-X-MAP and EXT-X-PART. These SHOULD be in quick-m3u8
// library: https://github.com/theRealRobG/m3u8/issues/9

fn map_byterange(tag: &UnknownTag) -> Option<MapByterange> {
    tag.value()
        .and_then(|v| v.try_as_ordered_attribute_list().ok())
        .and_then(|v| {
            let v = v.iter().find(|(n, _)| *n == "BYTERANGE")?;
            let byterange_str = v.1.quoted()?;
            let mut parts = byterange_str.splitn(2, '@');
            let length = parts.next().and_then(|s| s.parse().ok())?;
            let offset = parts.next().and_then(|s| s.parse().ok())?;
            Some(MapByterange { length, offset })
        })
}

fn part_byterange(tag: &UnknownTag) -> Option<PartByterange> {
    tag.value()
        .and_then(|v| v.try_as_ordered_attribute_list().ok())
        .and_then(|v| {
            let v = v.iter().find(|(n, _)| *n == "BYTERANGE")?;
            let range = v.1.quoted()?;
            let mut parts = range.splitn(2, '@');
            let length = parts.next().and_then(|s| s.parse().ok())?;
            let offset = parts.next().and_then(|s| s.parse().ok());
            Some(PartByterange { length, offset })
        })
}

// Convenience types

struct ParsingState {
    // Passed in as parameters
    imported_definitions: HashMap<String, String>,
    highlighted_segment: Option<u64>,
    highlighted_map_info: Option<HighlightedMapInfo>,
    highlighted_part_info: Option<HighlightedPartInfo>,
    highlighted_scte35_info: Option<HighlightedScte35Info>,
    highlighted_asset_list_daterange_id: Option<String>,
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
        highlighted: Option<Highlighted>,
    ) -> Self {
        let (
            highlighted_segment,
            highlighted_map_info,
            highlighted_part_info,
            highlighted_scte35_info,
            highlighted_asset_list_daterange_id,
        ) = match highlighted {
            Some(Highlighted::AssetList { daterange_id }) => {
                (None, None, None, None, Some(daterange_id))
            }
            Some(Highlighted::Map {
                url,
                min_media_sequence,
            }) => (
                None,
                Some(HighlightedMapInfo {
                    url,
                    min_media_sequence,
                }),
                None,
                None,
                None,
            ),
            Some(Highlighted::Part {
                media_sequence,
                part_index,
            }) => (
                None,
                None,
                Some(HighlightedPartInfo {
                    media_sequence,
                    part_index,
                }),
                None,
                None,
            ),
            Some(Highlighted::Scte35 {
                daterange_id,
                command_type,
            }) => (
                None,
                None,
                None,
                Some(HighlightedScte35Info {
                    daterange_id,
                    command_type,
                }),
                None,
            ),
            Some(Highlighted::Segment { media_sequence }) => {
                (Some(media_sequence), None, None, None, None)
            }
            None => (None, None, None, None, None),
        };
        Self {
            imported_definitions,
            highlighted_segment,
            highlighted_map_info,
            highlighted_part_info,
            highlighted_scte35_info,
            highlighted_asset_list_daterange_id,
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

#[derive(Debug, PartialEq)]
enum Markup {
    String(String),
    Link {
        href: String,
        value: String,
        highlighted: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use quick_m3u8::custom_parsing::tag::parse;

    #[test]
    fn split_as_markup_no_splits_when_no_link_attrs() {
        let tag = tag("#EXT-X-TEST:ONE=1,TWO=2,THREE=3");
        assert_eq!(
            vec![Markup::String(String::from(
                "#EXT-X-TEST:ONE=1,TWO=2,THREE=3"
            ))],
            split_tag_as_markup(
                &tag,
                ["FOUR"],
                |_, _| { Some(String::from("test")) },
                |_, _| { true }
            )
        );
    }

    #[test]
    fn split_as_markup_no_splits_when_href_fn_returns_none() {
        let tag = tag("#EXT-X-TEST:ONE=1,TWO=2,THREE=3");
        assert_eq!(
            vec![Markup::String(String::from(
                "#EXT-X-TEST:ONE=1,TWO=2,THREE=3"
            ))],
            split_tag_as_markup(&tag, ["TWO"], |_, _| { None }, |_, _| { true })
        );
    }

    #[test]
    fn split_as_markup_handles_split_at_first_attr() {
        // Without quotes
        let tag_1 = tag("#EXT-X-TEST:ONE=1,TWO=2,THREE=3");
        assert_eq!(
            vec![
                Markup::String(String::from("#EXT-X-TEST:ONE=")),
                Markup::Link {
                    href: String::from("test"),
                    value: String::from("1"),
                    highlighted: true
                },
                Markup::String(String::from(",TWO=2,THREE=3")),
            ],
            split_tag_as_markup(
                &tag_1,
                ["ONE"],
                |_, _| { Some(String::from("test")) },
                |_, _| { true }
            )
        );
        // With quotes
        let tag_2 = tag("#EXT-X-TEST:ONE=\"1\",TWO=\"2\",THREE=\"3\"");
        assert_eq!(
            vec![
                Markup::String(String::from("#EXT-X-TEST:ONE=\"")),
                Markup::Link {
                    href: String::from("test"),
                    value: String::from("1"),
                    highlighted: true
                },
                Markup::String(String::from("\",TWO=\"2\",THREE=\"3\"")),
            ],
            split_tag_as_markup(
                &tag_2,
                ["ONE"],
                |_, _| { Some(String::from("test")) },
                |_, _| { true }
            )
        );
    }

    #[test]
    fn split_as_markup_handles_split_in_middle_attr() {
        // Without quotes
        let tag_1 = tag("#EXT-X-TEST:ONE=1,TWO=2,THREE=3");
        assert_eq!(
            vec![
                Markup::String(String::from("#EXT-X-TEST:ONE=1,TWO=")),
                Markup::Link {
                    href: String::from("test"),
                    value: String::from("2"),
                    highlighted: false
                },
                Markup::String(String::from(",THREE=3")),
            ],
            split_tag_as_markup(
                &tag_1,
                ["TWO"],
                |_, _| { Some(String::from("test")) },
                |_, _| { false }
            )
        );
        // With quotes
        let tag_2 = tag("#EXT-X-TEST:ONE=\"1\",TWO=\"2\",THREE=\"3\"");
        assert_eq!(
            vec![
                Markup::String(String::from("#EXT-X-TEST:ONE=\"1\",TWO=\"")),
                Markup::Link {
                    href: String::from("test"),
                    value: String::from("2"),
                    highlighted: false
                },
                Markup::String(String::from("\",THREE=\"3\"")),
            ],
            split_tag_as_markup(
                &tag_2,
                ["TWO"],
                |_, _| { Some(String::from("test")) },
                |_, _| { false }
            )
        );
    }

    #[test]
    fn split_as_markup_handles_split_as_last_attr() {
        // Without quotes
        let tag_1 = tag("#EXT-X-TEST:ONE=1,TWO=2,THREE=3");
        assert_eq!(
            vec![
                Markup::String(String::from("#EXT-X-TEST:ONE=1,TWO=2,THREE=")),
                Markup::Link {
                    href: String::from("test"),
                    value: String::from("3"),
                    highlighted: false
                },
            ],
            split_tag_as_markup(
                &tag_1,
                ["THREE"],
                |_, _| { Some(String::from("test")) },
                |_, _| { false }
            )
        );
        // With quotes
        let tag_2 = tag("#EXT-X-TEST:ONE=\"1\",TWO=\"2\",THREE=\"3\"");
        assert_eq!(
            vec![
                Markup::String(String::from("#EXT-X-TEST:ONE=\"1\",TWO=\"2\",THREE=\"")),
                Markup::Link {
                    href: String::from("test"),
                    value: String::from("3"),
                    highlighted: true
                },
                Markup::String(String::from("\"")),
            ],
            split_tag_as_markup(
                &tag_2,
                ["THREE"],
                |_, _| { Some(String::from("test")) },
                |_, _| { true }
            )
        );
    }

    fn tag(input: &str) -> UnknownTag<'_> {
        parse(input).expect("should be valid tag").parsed
    }
}
