use crate::{
    components::{UrlInputForm, Viewer, ViewerLoading},
    utils::{
        href::{DEFINITIONS_QUERY_NAME, PLAYLIST_URL_QUERY_NAME, SUPPLEMENTAL_VIEW_QUERY_NAME},
        network::fetch_text,
        query_codec::decode_definitions,
    },
};
use leptos::prelude::*;
use leptos_router::hooks::query_signal;
use std::collections::HashMap;

#[component]
pub fn Home() -> impl IntoView {
    let (playlist_url, _) = query_signal::<String>(PLAYLIST_URL_QUERY_NAME);
    let (supplemental_context, _) = query_signal::<String>(SUPPLEMENTAL_VIEW_QUERY_NAME);
    let (imported_definitions, _) = query_signal::<String>(DEFINITIONS_QUERY_NAME);
    let playlist_result = LocalResource::new(move || {
        let playlist_url = playlist_url.get().unwrap_or_default();
        fetch_text(playlist_url)
    });
    view! {
        <h1 class="body-content">"HLS Manifest Viewer"</h1>
        <p class="body-content body-text">
            r#"Enter your HLS playlist URL in the input below. If possible, it is best to provide
            the URL to the multivariant playlist (MVP) rather than the media, as this allows for
            any "# <code>"EXT-X-DEFINE:IMPORT"</code> r#" declarations in the media to be resolved
            correctly against the MVP."#
        </p>
        <UrlInputForm />
        <Suspense fallback=ViewerLoading>
            {move || {
                let supplemental_context = move || supplemental_context.get();
                let imported_definitions = move || {
                    imported_definitions
                        .get()
                        .map(|def| {
                            decode_definitions(&def)
                                .inspect_err(|e| {
                                    log::error!("query parsing for definitions failed due to {e}")
                                })
                                .ok()
                        })
                        .flatten()
                        .unwrap_or(HashMap::new())
                };
                playlist_result
                    .get()
                    .map(|fetch_response| {
                        view! {
                            <Viewer
                                fetch_response
                                supplemental_context=supplemental_context()
                                imported_definitions=imported_definitions()
                            />
                        }
                    })
            }}
        </Suspense>
    }
}
