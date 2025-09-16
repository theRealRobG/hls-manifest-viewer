use crate::{
    components::{UrlInputForm, Viewer, ViewerLoading},
    utils::{
        href::{
            query_value_from_leptos_url, DEFINITIONS_QUERY_NAME, PLAYLIST_URL_QUERY_NAME,
            SUPPLEMENTAL_VIEW_QUERY_NAME,
        },
        network::fetch_text,
        query_codec::{decode_definitions, percent_decode},
    },
};
use leptos::prelude::*;
use leptos_router::hooks::use_url;

#[component]
pub fn Home() -> impl IntoView {
    let playlist_url = query_string_signal(PLAYLIST_URL_QUERY_NAME, true);
    let supplemental_context = query_string_signal(SUPPLEMENTAL_VIEW_QUERY_NAME, true);
    // definitions are decoded separately so we do not decode the raw query value.
    let imported_definitions = query_string_signal(DEFINITIONS_QUERY_NAME, false);
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
                        .and_then(|def| {
                            decode_definitions(&def)
                                .inspect_err(|e| {
                                    log::error!("query parsing for definitions failed due to {e}")
                                })
                                .ok()
                        })
                        .unwrap_or_default()
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

// We define our own function to extract the query memoized query value, rather than using
// leptos_router::hooks::query_signal, because the existing query_signal method has issues with
// double decoding the URL. This method allows us more control over the percent decode.
//
// Also note, we can't access the Location directly, because the reactive signal fires before the
// location is updated by the router, so the Location value that the browser gives us is not updated
// when the change signal fires.
fn query_string_signal(query_name: &'static str, decode: bool) -> Memo<Option<String>> {
    Memo::new(move |_| {
        let url = use_url().read();
        query_value_from_leptos_url(&url, query_name).map(|cow| {
            if decode {
                percent_decode(&cow).to_string()
            } else {
                cow.to_string()
            }
        })
    })
}
