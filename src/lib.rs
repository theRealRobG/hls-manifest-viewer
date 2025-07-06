use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::*, path};

// Modules
mod components;
mod pages;
// Pages
use crate::pages::{home::Home, not_found::NotFound};

/// An app router which renders the homepage and handles 404's
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Html attr:lang="en" attr:dir="ltr" />
        <Meta charset="UTF-8" />
        <Meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <Title text="HLS Manifest Viewer" />

        <Router>
            <Routes fallback=NotFound>
                <Route path=path!("/") view=Home />
            </Routes>
        </Router>
    }
}
