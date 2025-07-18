use leptos::prelude::*;
use leptos_use::{use_clipboard, UseClipboardReturn};

// Takes a closure to get the string so to only clone when the button is clicked.
#[component]
pub fn CopyButton<F>(text: F) -> impl IntoView
where
    F: Fn() -> String + Clone + Send + Sync + 'static,
{
    let UseClipboardReturn {
        is_supported,
        text: _,
        copied: _,
        copy,
    } = use_clipboard();
    let class = "button copy-button";
    view! {
        <Show when=move || is_supported.get()>
            <button
                class=class
                on:click={
                    let copy = copy.clone();
                    let text = text.clone();
                    move |_| copy(&text())
                }
            >
                <img src="/hls-manifest-viewer/copy.svg" />
            </button>
        </Show>
    }
}
