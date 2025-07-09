use leptos::prelude::*;

/// 404 Not Found Page
#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <h1 class="body-content">"404 Not Found"</h1>
        <p class="body-content body-text">
            "This is a simple site; just home (" <code>"/"</code> ") and about ("
            <code>"/about"</code> ")."
        </p>
    }
}
