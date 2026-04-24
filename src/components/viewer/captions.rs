use crate::utils::{
    cea608::{self, CaptionEntry, CodecType},
    mp4_atom_properties::{get_properties, AtomProperties, AtomPropertyValue, BasicPropertyValue},
    mp4_parsing::Hvce,
};
use leptos::{either::Either, prelude::*};
use mp4_atom::{Atom, Avcc, Buf, Decode, FourCC, Header, Hvcc, ReadFrom};
use std::{io::Cursor, sync::Arc};

const INNER_TABLE_CLASS: &str = "mp4-inner-table";
const CAPTIONS_SECTION_CLASS: &str = "mp4-captions";

/// Configuration for caption extraction, determined from the init segment.
#[derive(Debug, Clone, Copy)]
enum CaptionConfig {
    /// Caption data embedded in video NAL unit SEI messages (H.264/H.265).
    Sei {
        nal_length_size: u8,
        codec_type: CodecType,
    },
    /// Raw CEA-608 Apple format in a separate `c608` track.
    RawC608 { track_id: u32 },
}

/// Component that parses and displays captions from an mdat box.
///
/// For SEI-based captions (H.264/H.265), uses the mdat body directly.
/// For raw c608 Apple format, uses the full segment data to locate c608 track samples.
#[component]
pub fn CaptionParser(
    mdat_body: Vec<u8>,
    segment_data: Arc<Vec<u8>>,
    init_segment_data: Option<Vec<u8>>,
) -> impl IntoView {
    let caption_config = init_segment_data
        .as_deref()
        .and_then(extract_caption_config);

    let Some(config) = caption_config else {
        return Either::Left(());
    };

    let caption_entries = match config {
        CaptionConfig::Sei {
            nal_length_size,
            codec_type,
        } => cea608::extract_captions(&mdat_body, nal_length_size, codec_type),
        CaptionConfig::RawC608 { track_id } => {
            cea608::extract_c608_captions(&segment_data, track_id)
        }
    };

    Either::Right(view! {
        <div class=CAPTIONS_SECTION_CLASS>
            <CaptionTable entries=caption_entries />
        </div>
    })
}

/// Extract caption configuration from an init segment.
///
/// First checks for raw c608 Apple caption tracks, then falls back to SEI-based detection.
fn extract_caption_config(init_data: &[u8]) -> Option<CaptionConfig> {
    // Try c608 Apple format detection first
    if let Some(config) = extract_c608_config(init_data) {
        return Some(config);
    }
    // Try SEI-based detection (avcC/hvcC/hvcE via get_properties)
    extract_sei_config(init_data)
}

/// Detect SEI-based codec config (avcC/hvcC/hvcE) using `get_properties`.
fn extract_sei_config(init_data: &[u8]) -> Option<CaptionConfig> {
    let mut reader = Cursor::new(init_data.to_vec());
    while reader.has_remaining() {
        let header = Header::read_from(&mut reader).ok()?;
        let info = get_properties(&header, &mut reader).ok()?;
        match header.kind {
            Avcc::KIND => {
                if let Some(length_size) = find_u8_property(&info.properties, "length_size") {
                    return Some(CaptionConfig::Sei {
                        nal_length_size: length_size,
                        codec_type: CodecType::H264,
                    });
                }
            }
            Hvcc::KIND | Hvce::KIND => {
                if let Some(val) = find_u8_property(&info.properties, "length_size_minus_one") {
                    return Some(CaptionConfig::Sei {
                        nal_length_size: val + 1,
                        codec_type: CodecType::H265,
                    });
                }
            }
            _ => {}
        }
    }
    None
}

/// Detect raw c608 Apple caption track by scanning init segment for a `c608` sample entry.
///
/// Walks the box hierarchy manually (without `get_properties`) to find a `c608` box
/// within `stsd`, and extracts the `track_id` from the preceding `tkhd` in the same `trak`.
fn extract_c608_config(init_data: &[u8]) -> Option<CaptionConfig> {
    const C608: FourCC = FourCC::new(b"c608");

    let mut reader = Cursor::new(init_data.to_vec());
    let mut last_track_id: u32 = 0;

    while reader.has_remaining() {
        let Ok(header) = Header::read_from(&mut reader) else {
            break;
        };
        let body_size = header.size.unwrap_or(reader.remaining()) as u64;
        let body_start = reader.position();

        // Container boxes: descend into children without advancing past them
        if is_caption_config_container(header.kind) {
            // For stsd (fullbox + entry_count), skip version(1) + flags(3) + entry_count(4)
            if header.kind == mp4_atom::Stsd::KIND {
                if body_size < 8 {
                    break;
                }
                reader.advance(8);
            }
            // For meta (fullbox), skip version(1) + flags(3)
            else if header.kind == mp4_atom::Meta::KIND {
                if body_size < 4 {
                    break;
                }
                reader.advance(4);
            }
            continue;
        }

        if header.kind == mp4_atom::Tkhd::KIND {
            // Parse version to determine field sizes, then extract track_id
            if body_size >= 12 {
                let version = u8::decode(&mut reader).unwrap_or(0);
                reader.advance(3); // flags
                if version >= 1 {
                    reader.advance(16); // creation_time(8) + modification_time(8)
                } else {
                    reader.advance(8); // creation_time(4) + modification_time(4)
                }
                last_track_id = u32::decode(&mut reader).unwrap_or(0);
            }
            reader.set_position(body_start + body_size);
            continue;
        }

        if header.kind == C608 && last_track_id > 0 {
            return Some(CaptionConfig::RawC608 {
                track_id: last_track_id,
            });
        }

        // Skip all other non-container boxes
        reader.set_position(body_start + body_size);
    }
    None
}

fn is_caption_config_container(kind: FourCC) -> bool {
    kind == mp4_atom::Moov::KIND
        || kind == mp4_atom::Trak::KIND
        || kind == mp4_atom::Mdia::KIND
        || kind == mp4_atom::Minf::KIND
        || kind == mp4_atom::Stbl::KIND
        || kind == mp4_atom::Stsd::KIND
        || kind == mp4_atom::Meta::KIND
        || kind == mp4_atom::Mvex::KIND
}

/// Look up a `U8` property by name from an `AtomProperties` list.
fn find_u8_property(properties: &AtomProperties, name: &str) -> Option<u8> {
    properties.properties.iter().find_map(|(prop_name, value)| {
        if *prop_name == name
            && let AtomPropertyValue::Basic(BasicPropertyValue::U8(v)) = value
        {
            return Some(*v);
        }
        None
    })
}

#[component]
fn CaptionTable(entries: Vec<CaptionEntry>) -> impl IntoView {
    if entries.is_empty() {
        return Either::Left(view! {  });
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