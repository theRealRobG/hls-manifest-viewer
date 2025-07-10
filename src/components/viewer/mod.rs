use leptos::prelude::*;

mod error;
mod loading;
mod playlist;
pub use error::ViewerError;
pub use loading::ViewerLoading;
use playlist::PlaylistViewer;

const VIEWER_CLASS: &str = "viewer-content";
const MAIN_VIEW_CLASS: &str = "viewer-main";
// const SUPPLEMENTAL_VIEW_CLASS: &str = "viewer-supplemental"; // Will be used by supplemental view
const ERROR_CLASS: &str = "error";
const TAG_CLASS: &str = "hls-line tag";
const URI_CLASS: &str = "hls-line uri";
const COMMENT_CLASS: &str = "hls-line comment";
const BLANK_CLASS: &str = "hls-line blank";

#[component]
pub fn Viewer(playlist: String, base_url: String) -> impl IntoView {
    view! {
        <div class=VIEWER_CLASS>
            <PlaylistViewer playlist base_url />
        </div>
    }
}
