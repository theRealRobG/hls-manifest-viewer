use super::{SPACER_BOTTOM, SUPPLEMENTAL_VIEW_CLASS, UNDERLINED, URI_CLASS};
use crate::{
    components::viewer::{error::ViewerError, LARGER_SPACED_TABLE},
    utils::href::media_playlist_href,
};
use leptos::{either::Either, prelude::*};
use serde::Deserialize;
use std::collections::HashMap;

#[component]
pub fn AssetListView(json: String) -> impl IntoView {
    match decode(&json) {
        Ok(asset_list) => Either::Left(view! {
            <div class=SUPPLEMENTAL_VIEW_CLASS>
                <p class=UNDERLINED>"ASSETS"</p>
                <table class=SPACER_BOTTOM>
                    <tr>
                        <th>"URI"</th>
                        <th>"DURATION"</th>
                    </tr>
                    {asset_list
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
                {if let Some(skip_control) = asset_list.skip_control {
                    let mut rows = Vec::with_capacity(3);
                    rows.push((
                        String::from("OFFSET"),
                        format!("{}", skip_control.offset.unwrap_or_default()),
                    ));
                    if let Some(duration) = skip_control.duration {
                        rows.push((String::from("DURATION"), format!("{duration}")));
                    }
                    if let Some(label_id) = &skip_control.label_id {
                        rows.push((String::from("LABEL-ID"), label_id.clone()));
                    }
                    Either::Left(
                        view! {
                            <p class=UNDERLINED>"SKIP-CONTROL"</p>
                            <table class=[SPACER_BOTTOM, LARGER_SPACED_TABLE]
                                .join(
                                    " ",
                                )>
                                {rows
                                    .into_iter()
                                    .map(|(key, value)| {
                                        view! {
                                            <tr>
                                                <td>{key}</td>
                                                <td>{value}</td>
                                            </tr>
                                        }
                                    })
                                    .collect_view()}
                            </table>
                        },
                    )
                } else {
                    Either::Right(view! { <p class=UNDERLINED></p> })
                }}
                <p class=UNDERLINED>"JSON"</p>
                <code>{json}</code>
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

fn decode(json: &str) -> Result<AssetList, serde_json::Error> {
    let value = serde_json::from_str(json)?;
    let asset_list = serde_json::from_value(value)?;
    Ok(asset_list)
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
    offset: Option<u64>,
    duration: Option<u64>,
    label_id: Option<String>,
}
