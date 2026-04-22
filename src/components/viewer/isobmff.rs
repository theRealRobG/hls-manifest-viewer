use crate::{
    components::viewer::ISOBMFF_VIEW_CLASS,
    utils::{
        cea608::{self, CaptionEntry, CodecType},
        mp4_atom_properties::{
            get_properties, AtomProperties, AtomPropertyValue, BasicPropertyValue,
            TablePropertyValue,
        },
    },
};
use leptos::{
    either::{Either, EitherOf3},
    prelude::*,
};
use mp4_atom::{Atom, Avcc, Buf, FourCC, Header, Hvcc, ReadFrom};
use std::{borrow::Cow, io::Cursor, sync::Arc};
use web_sys::MouseEvent;

const ATOMS_CLASS: &str = "mp4-atoms";
const PROPERTIES_CLASS: &str = "mp4-properties";
const INNER_TABLE_CLASS: &str = "mp4-inner-table";
const CAPTIONS_SECTION_CLASS: &str = "mp4-captions";

#[component]
pub fn IsobmffViewer(
    data: Vec<u8>,
    init_segment_data: Option<Vec<u8>>,
) -> mp4_atom::Result<impl IntoView> {
    let (highlighted, set_highlighted) = signal(0);
    let data_arc = Arc::new(data);
    let mut reader = Cursor::new(data_arc.as_ref().clone());
    let mut atoms = Vec::new();
    let mut properties = Vec::new();
    let mut index = 0usize;
    let mut container_box_end_positions = Vec::new();
    let mut mdat_ranges: Vec<(usize, usize, usize)> = Vec::new();
    loop {
        let header = Header::read_from(&mut reader)?;
        // Handle popping out of depths when we have reached the end of container boxes. Multiple
        // boxes may end at the same depth and so we need to check more than just one.
        //
        // For context, this is all in an effort to build up a view where the FourCC values (in the
        // `atoms_view` side-view) appear indented according to their depth, like such:
        // ```
        //   styp
        //   prft
        //   moof
        //     mfhd
        //     traf
        //       tfhd
        //       tfdt
        //       trun
        //       saiz
        //       saio
        //       senc
        //   mdat
        // ```
        //
        // In the example above, you can see that both the `traf` and the `moof` finish at the same
        // data position (at the end of the `senc`), and so we would pop off two depths in that
        // case.
        while let Some(depth_until) = container_box_end_positions.last() {
            if reader.position() >= (*depth_until) {
                container_box_end_positions.pop();
            } else {
                break;
            }
        }
        // The depth is then the size of the depths vector. We take the depth now (before the new
        // info) because a new container box should still appear at the same depth as its sibling
        // boxes.
        let depth = container_box_end_positions.len();
        let is_mdat = header.kind == FourCC::new(b"mdat");
        let body_start = reader.position() as usize;
        // We then get the property information for this box.
        let info = get_properties(&header, &mut reader)?;
        let body_end = reader.position() as usize;
        if is_mdat {
            mdat_ranges.push((index, body_start, body_end));
        }
        // If the new info is a container box then we will receive a new "depth until" that
        // indicates at what reader position this box will end at. Above we handle tracking how deep
        // we are into any given box and at what size the box ends.
        if let Some(new_depth_until) = info.new_depth_until {
            container_box_end_positions.push(new_depth_until);
        }

        let atoms_view = view! {
            <AtomName
                atom=header.kind
                depth
                highlighted=move || highlighted.get() == index
                on_click=move |_| set_highlighted.set(index)
            />
        };
        atoms.push(atoms_view);

        let properties_view = view! {
            <Show when=move || highlighted.get() == index>
                <AtomInfo properties=info.properties.clone() />
            </Show>
        };
        properties.push(properties_view);

        if !reader.has_remaining() {
            break;
        }
        index += 1;
    }

    // Build caption sections for each mdat when init segment data is available.
    // The "Parse Captions" button visibility is determined by whether valid codec
    // configuration can be extracted from the init segment data.
    let caption_views: Vec<AnyView> = if init_segment_data.is_some() {
        let init_data_arc = Arc::new(init_segment_data);
        mdat_ranges
            .into_iter()
            .map(|(mdat_index, body_start, body_end)| {
                let mdat_body = data_arc[body_start..body_end].to_vec();
                let init_data = init_data_arc.clone();
                view! {
                    <Show when=move || highlighted.get() == mdat_index>
                        <CaptionParser
                            mdat_body=mdat_body.clone()
                            init_segment_data=init_data.as_ref().clone()
                        />
                    </Show>
                }
                .into_any()
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(view! {
        <div class=ISOBMFF_VIEW_CLASS>
            <div class=ATOMS_CLASS>{atoms}</div>
            <div class=PROPERTIES_CLASS>{properties} {caption_views}</div>
        </div>
    })
}

#[component]
fn AtomName(
    atom: FourCC,
    depth: usize,
    highlighted: impl Fn() -> bool + Send + Sync + 'static,
    on_click: impl FnMut(MouseEvent) + 'static,
) -> impl IntoView {
    let mut space = String::new();
    for _ in 0..depth {
        space.push_str("  ");
    }
    view! {
        <pre class:highlighted=highlighted on:click=on_click>
            {format!("{space}{atom}")}
        </pre>
    }
}

#[component]
fn AtomInfo(properties: AtomProperties) -> impl IntoView {
    view! {
        <p>{properties.box_name}</p>
        <table>
            <tr>
                <th>"Property"</th>
                <th>"Value"</th>
            </tr>
            {properties
                .properties
                .iter()
                .map(|(key, value)| {
                    view! {
                        <tr>
                            <td>
                                {match key {
                                    Cow::Borrowed(k) => Either::Left(*k),
                                    Cow::Owned(s) => Either::Right(s.clone()),
                                }}
                            </td>
                            <td>
                                {match value {
                                    AtomPropertyValue::Basic(v) => Either::Left(view_from_prop(v)),
                                    AtomPropertyValue::Table(v) => {
                                        Either::Right(view! { <InnerTable properties=v.clone() /> })
                                    }
                                }}
                            </td>
                        </tr>
                    }
                })
                .collect_view()}
        </table>
    }
}

#[component]
fn InnerTable(properties: TablePropertyValue) -> impl IntoView {
    if properties.rows.is_empty() || properties.rows.first().is_some_and(|row| row.is_empty()) {
        EitherOf3::A(String::new())
    } else if let Some(headers) = properties.headers {
        EitherOf3::B(view! {
            <table class=INNER_TABLE_CLASS>
                <tr>{headers.iter().map(|header| view! { <th>{*header}</th> }).collect_view()}</tr>
                {properties
                    .rows
                    .iter()
                    .map(|row| {
                        view! {
                            <tr>
                                {row
                                    .iter()
                                    .map(|col| view! { <td>{view_from_prop(col)}</td> })
                                    .collect_view()}
                            </tr>
                        }
                    })
                    .collect_view()}
            </table>
        })
    } else {
        EitherOf3::C(view! {
            <table>
                {properties
                    .rows
                    .iter()
                    .map(|row| {
                        view! {
                            <tr>
                                {row
                                    .iter()
                                    .map(|col| view! { <td>{view_from_prop(col)}</td> })
                                    .collect_view()}
                            </tr>
                        }
                    })
                    .collect_view()}
            </table>
        })
    }
}

// Naming the type, rather than using impl IntoView, helps the borrow checker calm down when passing
// the property by reference in the map closures.
fn view_from_prop(
    property: &BasicPropertyValue,
) -> Either<View<leptos::html::HtmlElement<leptos::html::Pre, (), (String,)>>, String> {
    let string = String::from(property);
    if property.is_hex() || property.is_binary_mask() {
        Either::Left(view! { <pre>{string}</pre> })
    } else {
        Either::Right(view! { {string} })
    }
}

/// Component that shows a "Parse Captions" button on mdat when valid codec config
/// can be extracted from the init segment data. On click, parses captions synchronously.
#[component]
fn CaptionParser(mdat_body: Vec<u8>, init_segment_data: Option<Vec<u8>>) -> impl IntoView {
    let codec_config = init_segment_data.as_deref().and_then(extract_codec_config);

    // Only show the Parse Captions button if we could extract a valid codec config.
    let Some((nal_length_size, codec_type)) = codec_config else {
        return Either::Left(());
    };

    let (captions, set_captions) = signal::<Option<Vec<CaptionEntry>>>(None);
    let mdat_body = Arc::new(mdat_body);

    Either::Right(view! {
        <div class=CAPTIONS_SECTION_CLASS>
            <Show when=move || {
                captions.get().is_none()
            }>
                {
                    let mdat_body = mdat_body.clone();
                    view! {
                        <button on:click=move |_| {
                            let entries = cea608::extract_captions(
                                &mdat_body,
                                nal_length_size,
                                codec_type,
                            );
                            set_captions.set(Some(entries));
                        }>"Parse Captions"</button>
                    }
                }
            </Show>
            <Show when=move || captions.get().is_some()>
                <CaptionTable entries=captions.get().unwrap_or_default() />
            </Show>
        </div>
    })
}

/// Extract NAL length size and codec type from an init segment's moov/trak/mdia/minf/stbl/stsd.
fn extract_codec_config(init_data: &[u8]) -> Option<(u8, CodecType)> {
    const MOOV: FourCC = FourCC::new(b"moov");
    const TRAK: FourCC = FourCC::new(b"trak");
    const MDIA: FourCC = FourCC::new(b"mdia");
    const MINF: FourCC = FourCC::new(b"minf");
    const STBL: FourCC = FourCC::new(b"stbl");
    const STSD: FourCC = FourCC::new(b"stsd");
    const AVC1: FourCC = FourCC::new(b"avc1");
    const AVC3: FourCC = FourCC::new(b"avc3");
    const HEV1: FourCC = FourCC::new(b"hev1");
    const HVC1: FourCC = FourCC::new(b"hvc1");
    const DVH1: FourCC = FourCC::new(b"dvh1");
    const DVHE: FourCC = FourCC::new(b"dvhe");
    const ENCV: FourCC = FourCC::new(b"encv");
    const SINF: FourCC = FourCC::new(b"sinf");
    const AVCC: FourCC = FourCC::new(b"avcC");
    const HVCC: FourCC = FourCC::new(b"hvcC");
    const HVCE: FourCC = FourCC::new(b"hvcE");

    let mut reader = Cursor::new(init_data.to_vec());
    while reader.has_remaining() {
        let Ok(header) = Header::read_from(&mut reader) else {
            break;
        };
        let body_size = header.size.unwrap_or(reader.remaining());
        let body_end = reader.position() + body_size as u64;
        match header.kind {
            // Container boxes: descend into them
            MOOV | TRAK | MDIA | MINF | STBL => continue,
            STSD => {
                // Skip version(4) + entry_count(4) to get to sample entries
                if reader.remaining() >= 8 {
                    reader.set_position(reader.position() + 8);
                }
                continue;
            }
            // AVC sample entries - skip 78 bytes of VisualSampleEntry fields
            AVC1 | AVC3 => {
                let skip = 78.min(reader.remaining());
                reader.set_position(reader.position() + skip as u64);
                continue;
            }
            // HEVC sample entries
            HEV1 | HVC1 | DVH1 | DVHE => {
                let skip = 78.min(reader.remaining());
                reader.set_position(reader.position() + skip as u64);
                continue;
            }
            // Encrypted video sample entry — same VisualSampleEntry layout,
            // wraps the original codec config boxes (avcC/hvcC/hvcE) plus sinf
            ENCV => {
                let skip = 78.min(reader.remaining());
                reader.set_position(reader.position() + skip as u64);
                continue;
            }
            // sinf is a container box (SchemeInformationBox) — skip it
            SINF => {
                reader.set_position(body_end.min(init_data.len() as u64));
                continue;
            }
            AVCC => {
                if let Ok(avcc) = Avcc::decode_body(&mut reader) {
                    return Some((avcc.length_size, CodecType::H264));
                }
            }
            HVCC | HVCE => {
                if let Ok(hvcc) = Hvcc::decode_body(&mut reader) {
                    return Some((hvcc.length_size_minus_one + 1, CodecType::H265));
                }
            }
            _ => {
                // Skip unknown boxes
                reader.set_position(body_end.min(init_data.len() as u64));
            }
        }
    }
    None
}

#[component]
fn CaptionTable(entries: Vec<CaptionEntry>) -> impl IntoView {
    if entries.is_empty() {
        return Either::Left(view! { <p>"No caption data found in this mdat."</p> });
    }

    let text_entries: Vec<_> = entries
        .iter()
        .filter_map(|e| e.text.as_ref().map(|t| (e.nal_index, e.field, t.clone())))
        .collect();

    Either::Right(view! {
        <div>
            <p>
                {format!(
                    "Found {} cc_data entries ({} with text)",
                    entries.len(),
                    text_entries.len(),
                )}
            </p>
            <table class=INNER_TABLE_CLASS>
                <tr>
                    <th>"NAL"</th>
                    <th>"Field"</th>
                    <th>"cc_type"</th>
                    <th>"Data"</th>
                    <th>"Text"</th>
                </tr>
                {entries
                    .iter()
                    .map(|entry| {
                        view! {
                            <tr>
                                <td>{entry.nal_index}</td>
                                <td>
                                    {match entry.field {
                                        1 => "CC1",
                                        2 => "CC2",
                                        _ => "DTVCC",
                                    }}
                                </td>
                                <td>{entry.cc_type}</td>
                                <td>
                                    <pre>
                                        {format!("{:02X} {:02X}", entry.cc_data1, entry.cc_data2)}
                                    </pre>
                                </td>
                                <td>{entry.text.clone().unwrap_or_default()}</td>
                            </tr>
                        }
                    })
                    .collect_view()}
            </table>
        </div>
    })
}
