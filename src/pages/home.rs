use crate::{
    components::{
        url_input_form::UrlInputForm,
        viewer::{Viewer, ViewerError, ViewerLoading},
    },
    utils::network::fetch_text,
    PLAYLIST_URL_QUERY_NAME,
};
use leptos::{either::Either, prelude::*};
use leptos_router::hooks::query_signal;

#[component]
pub fn Home() -> impl IntoView {
    let (playlist_url, _) = query_signal::<String>(PLAYLIST_URL_QUERY_NAME);
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
                playlist_result
                    .get()
                    .map(|result| match result {
                        Ok(response) => {
                            Either::Left(
                                view! {
                                    <Viewer
                                        playlist=response.response_text
                                        base_url=response.request_url
                                    />
                                },
                            )
                        }
                        Err(error) => {
                            Either::Right(
                                if let Some(extra_info) = error.extra_info {
                                    view! { <ViewerError error=error.error extra_info /> }
                                } else {
                                    view! { <ViewerError error=error.error /> }
                                },
                            )
                        }
                    })
            }}
        </Suspense>
    }
}
