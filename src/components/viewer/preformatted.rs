use super::SUPPLEMENTAL_VIEW_CLASS;
use leptos::prelude::*;

#[component]
pub fn PreformattedViewer(contents: String) -> impl IntoView {
    view! {
        <div class=SUPPLEMENTAL_VIEW_CLASS>
            <pre>{contents}</pre>
        </div>
    }
}
