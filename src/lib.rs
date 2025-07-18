use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::*, path};

// Modules
mod components;
mod pages;
mod utils;
// Pages
use crate::pages::{about::About, home::Home, not_found::NotFound};

pub(crate) const PLAYLIST_URL_QUERY_NAME: &str = "playlist_url";

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
            <nav>
                <a class="button" href="/hls-manifest-viewer">
                    "Home"
                </a>
                <a class="button" href="/hls-manifest-viewer/about">
                    "About"
                </a>
                <a
                    class="button"
                    href="https://github.com/theRealRobG/hls-manifest-viewer"
                    target="_blank"
                >
                    <img src="/hls-manifest-viewer/github-mark-white.svg" />
                </a>
            </nav>
            <main>
                <Routes fallback=NotFound>
                    <Route path=path!("/hls-manifest-viewer") view=Home />
                    <Route path=path!("/hls-manifest-viewer/about") view=About />
                </Routes>
            </main>
        </Router>
    }
}
