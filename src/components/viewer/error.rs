use super::ERROR_CLASS;
use leptos::prelude::*;

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
                view! {
                    <p class=ERROR_CLASS>{error}</p>
                    <pre class=ERROR_CLASS>{extra_info}</pre>
                }
                    .into_any()
            } else {
                view! { <p class=ERROR_CLASS>{error}</p> }.into_any()
            }
        }}
    }
}
