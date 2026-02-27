use super::{SUPPLEMENTAL_VIEW_CLASS, UNDERLINED};
use crate::{components::viewer::{SPACER_BOTTOM, error::ViewerError}, utils::href::{asset_list_href, media_playlist_href}};
use leptos::{either::Either, prelude::*};
use serde_json::Value;
use std::{collections::HashMap, str::FromStr};

#[component]
pub fn DaterangeScheduleView(json: String) -> impl IntoView {
    let raw_json = json.clone();
    view! {
        <div class=SUPPLEMENTAL_VIEW_CLASS>
            <p class=UNDERLINED>"DATERANGES"</p>
            {move || {
                if let Some(dateranges) = dateranges_from_json(&json) {
                    Either::Left(
                        view! {
                            {dateranges
                                .iter()
                                .map(|d| {
                                    view! {
                                        <table class=SPACER_BOTTOM>
                                            <tr>
                                                <th>"ATTRIBUTE"</th>
                                                <th>"VALUE"</th>
                                            </tr>
                                            {
                                                let id = &d.id;
                                                d.attributes
                                                    .iter()
                                                    .map(|(k, v)| {
                                                        view! {
                                                            <Row
                                                                id=id.to_string()
                                                                key=k.to_string()
                                                                value=v.to_string()
                                                            />
                                                        }
                                                    })
                                                    .collect_view()
                                            }
                                        </table>
                                    }
                                })
                                .collect_view()}
                        },
                    )
                } else {
                    Either::Right(
                        view! {
                            <ViewerError error="Could not deserialize DATERANGES from JSON"
                                .to_string() />
                            <p class=UNDERLINED></p>
                        },
                    )
                }
            }}
            <p class=UNDERLINED>"JSON"</p>
            <code>{raw_json}</code>
        </div>
    }
}

#[component]
fn Row(id: String, key: String, value: String) -> impl IntoView {
    let value = if key == "X-ASSET-URI" && let Some(href) = media_playlist_href(&value, &HashMap::new()) {
        Either::Left(view! { <a href=href>{value}</a> })
    } else if key == "X-ASSET-LIST" && let Some(href) = asset_list_href(&value, &id, &HashMap::new()) {
        Either::Left(view! { <a href=href>{value}</a> })
    } else {
        Either::Right(view! { {value} })
    };
    view! {
        <tr>
            <td>{key}</td>
            <td>{value}</td>
        </tr>
    }
}

fn dateranges_from_json(json: &str) -> Option<Vec<DaterangeAttributes>> {
    let value = Value::from_str(json).ok()?;
    let dateranges_value = value.as_object()?.get("DATERANGES")?.as_array()?;
    let mut dateranges = Vec::with_capacity(dateranges_value.len());
    for daterange_value in dateranges_value {
        let attributes_values = daterange_value.as_object()?;
        let mut attributes = Vec::with_capacity(attributes_values.len());
        for (key, value) in attributes_values {
            attributes.push((
                key.to_string(),
                if let Some(s) = value.as_str() {
                    s.to_string()
                } else {
                    format!("{value}")
                },
            ));
        }
        if let Some(id) = attributes.iter().find_map(|(k, v)| if k == "ID" { Some(v) } else { None }) {
            dateranges.push(DaterangeAttributes { id: id.to_string(), attributes });
        } else {
            return None;
        }
    }
    Some(dateranges)
}

struct DaterangeAttributes {
    id: String,
    attributes: Vec<(String, String)>,
}
