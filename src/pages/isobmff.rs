use crate::{
    components::{IsobmffViewer, UrlInputForm, ViewerError, ViewerLoading, VIEWER_CLASS},
    utils::{href::PLAYLIST_URL_QUERY_NAME, network::fetch_array_buffer},
};
use leptos::{either::Either, prelude::*};
use leptos_router::hooks::use_query_map;

#[component]
pub fn Isobmff() -> impl IntoView {
    let query = use_query_map();
    let playlist_url = move || {
        query
            .read()
            .get(PLAYLIST_URL_QUERY_NAME)
            .unwrap_or_default()
    };
    let playlist_result = LocalResource::new(move || {
        let playlist_url = playlist_url();
        fetch_array_buffer(playlist_url, None)
    });
    view! {
        <h1 class="body-content">"ISOBMFF Box Viewer"</h1>
        <p class="body-content body-text">"Enter your MP4 file URL in the input below."</p>
        <UrlInputForm />
        <Suspense fallback=ViewerLoading>
            {move || {
                playlist_result
                    .get()
                    .map(|fetch_response| {
                        match fetch_response {
                            Ok(response) => {
                                Either::Left(
                                    view! {
                                        <div class=VIEWER_CLASS>
                                            <ErrorBoundary fallback=|errors| {
                                                view! {
                                                    {move || {
                                                        errors
                                                            .get()
                                                            .into_iter()
                                                            .map(|(_, error)| {
                                                                let header = String::from("Error parsing ISOBMFF boxes");
                                                                let error = Some(error.to_string());
                                                                view! { <ViewerError error=header extra_info=error /> }
                                                            })
                                                            .collect::<Vec<_>>()
                                                    }}
                                                }
                                            }>
                                                <IsobmffViewer data=response.response_body />
                                            </ErrorBoundary>
                                        </div>
                                    },
                                )
                            }
                            Err(e) => {
                                Either::Right(
                                    view! {
                                        <div class=VIEWER_CLASS>
                                            <ViewerError error=e.error extra_info=e.extra_info />
                                        </div>
                                    },
                                )
                            }
                        }
                    })
            }}
        </Suspense>
    }
}
