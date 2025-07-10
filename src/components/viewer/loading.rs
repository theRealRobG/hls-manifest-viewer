use super::VIEWER_CLASS;
use leptos::prelude::*;

#[component]
pub fn ViewerLoading() -> impl IntoView {
    view! {
        <div class=VIEWER_CLASS>
            <p>"Loading..."</p>
        </div>
    }
}
