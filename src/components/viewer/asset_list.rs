use std::collections::HashMap;

use super::{SUPPLEMENTAL_VIEW_CLASS, UNDERLINED, URI_CLASS};
use crate::{components::viewer::error::ViewerError, utils::href::media_playlist_href};
use leptos::{either::Either, prelude::*};
use serde::Deserialize;
use serde_json::to_string_pretty;

#[component]
pub fn AssetListView(json: String) -> impl IntoView {
    match decode(&json) {
        Ok(decoded) => Either::Left(view! {
            <div class=SUPPLEMENTAL_VIEW_CLASS>
                <p class=UNDERLINED>"ASSETS"</p>
                <table>
                    <tr>
                        <th>"URI"</th>
                        <th>"DURATION"</th>
                    </tr>
                    {decoded
                        .asset_list
                        .assets
                        .iter()
                        .map(|asset| {
                            view! {
                                <tr>
                                    <td>{uri_link(asset.uri.clone())}</td>
                                    <td>{asset.duration}</td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </table>
                {if let Some(skip_control) = decoded.asset_list.skip_control {
                    Either::Left(
                        view! {
                            <p class=UNDERLINED>"SKIP-CONTROL"</p>
                            <table>
                                <tr>
                                    <td>"OFFSET"</td>
                                    <td>{skip_control.offset}</td>
                                </tr>
                                <tr>
                                    <td>"DURATION"</td>
                                    <td>{skip_control.duration}</td>
                                </tr>
                                <tr>
                                    <td>"LABEL-ID"</td>
                                    <td>{skip_control.label_id}</td>
                                </tr>
                            </table>
                        },
                    )
                } else {
                    Either::Right(view! { <p class=UNDERLINED></p> })
                }}
                <p class=UNDERLINED>"Prettified JSON"</p>
                <pre>{decoded.pretty_json}</pre>
            </div>
        }),
        Err(error) => Either::Right(view! {
            <div class=SUPPLEMENTAL_VIEW_CLASS>
                <ViewerError
                    error="Error deserializing JSON".to_string()
                    extra_info=Some(format!("{error}"))
                />
            </div>
        }),
    }
}

fn uri_link(uri: String) -> impl IntoView {
    if let Some(href) = media_playlist_href(&uri, &HashMap::new()) {
        Either::Left(view! {
            <a href=href class=URI_CLASS>
                {uri}
            </a>
        })
    } else {
        Either::Right(view! { {uri} })
    }
}

fn decode(json: &str) -> Result<Decoded, serde_json::Error> {
    let value = serde_json::from_str(json)?;
    let pretty_json = to_string_pretty(&value)?;
    let asset_list = serde_json::from_value(value)?;
    Ok(Decoded {
        asset_list,
        pretty_json,
    })
}

struct Decoded {
    asset_list: AssetList,
    pretty_json: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
struct AssetList {
    assets: Vec<AssetDescription>,
    skip_control: Option<SkipControl>,
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
struct AssetDescription {
    uri: String,
    duration: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
struct SkipControl {
    offset: u64,
    duration: u64,
    label_id: String,
}
