use super::{ERROR_CLASS, ERROR_CONTAINER_CLASS};
use leptos::{either::Either, prelude::*};
use std::str::FromStr;

#[component]
pub fn ViewerError(
    error: String,
    #[prop(default = None)] extra_info: Option<String>,
) -> impl IntoView {
    view! {
        {move || {
            let error = error.to_owned();
            let extra_info = extra_info.to_owned();
            if let Some(extra_info) = extra_info {
                Either::Left(
                    view! {
                        <div class=ERROR_CONTAINER_CLASS>
                            <p class=ERROR_CLASS>{error}</p>
                            <pre class=ERROR_CLASS>{format(extra_info)}</pre>
                        </div>
                    },
                )
            } else {
                Either::Right(
                    view! {
                        <div class=ERROR_CONTAINER_CLASS>
                            <p class=ERROR_CLASS>{error}</p>
                        </div>
                    },
                )
            }
        }}
    }
}

fn format(error: String) -> String {
    let Ok(json) = serde_json::Value::from_str(&error) else {
        return error;
    };
    if let Ok(pretty_error) = serde_json::to_string_pretty(&json) {
        pretty_error
    } else {
        error
    }
}
