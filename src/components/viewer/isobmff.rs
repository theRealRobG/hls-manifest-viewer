use crate::{
    components::viewer::SUPPLEMENTAL_VIEW_CLASS,
    utils::mp4::{get_properties, AtomProperties, AtomPropertyValue, TablePropertyValue},
};
use leptos::{either::Either, prelude::*};
use mp4_atom::{Buf, FourCC, Header, ReadFrom};
use std::io::Cursor;
use web_sys::MouseEvent;

const ATOMS_CLASS: &str = "mp4-atoms";
const PROPERTIES_CLASS: &str = "mp4-properties";
const INNER_TABLE_CLASS: &str = "mp4-inner-table";

#[component]
pub fn IsobmffViewer(data: Vec<u8>) -> mp4_atom::Result<impl IntoView> {
    let (highlighted, set_highlighted) = signal(0);
    let mut reader = Cursor::new(data);
    let mut atoms = Vec::new();
    let mut properties = Vec::new();
    let mut index = 0usize;
    let mut container_box_end_positions = Vec::new();
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
        // We then get the property information for this box.
        let info = get_properties(&header, &mut reader)?;
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
    Ok(view! {
        <div class=SUPPLEMENTAL_VIEW_CLASS>
            <div class=ATOMS_CLASS>{atoms}</div>
            <div class=PROPERTIES_CLASS>{properties}</div>
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
                            <td>{*key}</td>
                            <td>
                                {match value {
                                    AtomPropertyValue::Basic(v) => Either::Left(String::from(v)),
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
    if let Some(headers) = properties.headers {
        Either::Left(view! {
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
                                    .map(|col| view! { <td>{String::from(col)}</td> })
                                    .collect_view()}
                            </tr>
                        }
                    })
                    .collect_view()}
            </table>
        })
    } else {
        Either::Right(view! {
            <table>
                {properties
                    .rows
                    .iter()
                    .map(|row| {
                        view! {
                            <tr>
                                {row
                                    .iter()
                                    .map(|col| view! { <td>{String::from(col)}</td> })
                                    .collect_view()}
                            </tr>
                        }
                    })
                    .collect_view()}
            </table>
        })
    }
}
