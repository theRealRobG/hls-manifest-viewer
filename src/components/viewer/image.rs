use super::{IMAGE_VIEW_CLASS, SUPPLEMENTAL_VIEW_CLASS};
use base64::prelude::*;
use leptos::prelude::*;

#[component]
pub fn ImageViewer(contents: Vec<u8>, content_type: String) -> impl IntoView {
    view! {
        <div class=SUPPLEMENTAL_VIEW_CLASS>
            <img
                class=IMAGE_VIEW_CLASS
                src=format!("data:{content_type};base64,{}", BASE64_STANDARD.encode(&contents))
            />
        </div>
    }
}
