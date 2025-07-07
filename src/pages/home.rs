use crate::{components::{url_input_form::UrlInputForm, viewer::{Viewer, ViewerError, ViewerLoading}}, PLAYLIST_URL_QUERY_NAME};
use leptos::{either::Either, prelude::*};
use leptos_router::hooks::use_query_map;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

#[component]
pub fn Home() -> impl IntoView {
    let query = use_query_map();
    let playlist_result = LocalResource::new(move || {
        let playlist_url = query.read().get(PLAYLIST_URL_QUERY_NAME).unwrap_or_default();
        fetch_url(playlist_url)
    });
    view! {
        <main>
            <HomeHeader />
            <UrlInputForm />
            <Suspense fallback=ViewerLoading>
                {move || {
                    playlist_result
                        .get()
                        .map(|result| match result {
                            Ok(playlist) => {
                                Either::Left(
                                    view! {
                                        <Viewer
                                            playlist
                                            base_url=query
                                                .read()
                                                .get(PLAYLIST_URL_QUERY_NAME)
                                                .unwrap_or_default()
                                        />
                                    },
                                )
                            }
                            Err(error) => Either::Right(view! { <ViewerError error /> }),
                        })
                }}
            </Suspense>
            <HomeFooter />
        </main>
    }
}

async fn fetch_url(url: String) -> Result<String, String> {
    if url.is_empty() {
        return Ok(String::new());
    }
    let window = web_sys::window().expect("Window must be defined");
    let response = JsFuture::from(
        window.fetch_with_str(&url)
    ).await.map_err(|e| e.as_string().unwrap_or("Fetch failed".to_string()))?;
    let response: Response = response.dyn_into().expect("Fetch must resolve to a Response");
    let text = JsFuture::from(
        response
            .text()
            .map_err(|e| e.as_string().unwrap_or("Response contained no text".to_string()))?
    ).await.map_err(|e| e.as_string().unwrap_or("Could not obtain text from Response".to_string()))?;
    Ok(text.as_string().expect("text() on a fetch Response must provide a String"))
}

#[component]
fn HomeHeader() -> impl IntoView {
    view! {
        <h1 class="body-content">"HLS Manifest Viewer"</h1>
        <p class="body-content body-text">
            r#"Enter your HLS playlist URL in the input below. If possible, it is best to provide
            the URL to the multivariant playlist (MVP) rather than the media, as this allows for
            any "# <code>"EXT-X-DEFINE:IMPORT"</code> r#" declarations in the media to be resolved
            correctly against the MVP."#
        </p>
    }
}

#[component]
fn HomeFooter() -> impl IntoView {
    view! {
        <h1 class="body-content">"Why?"</h1>
        <p class="body-content body-text">
            r#"This tool provides a way to view HLS playlists (m3u8 files) in the browser with
            extended handling for links and other associated views. Most HLS playlists are delivered
            with a base multivariant playlist (MVP) and child media playlists. This allows a
            streaming provider to deliver multiple renditions of the same content all described in a
            single parent manifest. While this is convenient from a delivery size perspective, it
            does make exploring HLS playlists outside of a player (e.g. for debugging purposes) a
            little more tricky, as this would normally involve:"#
        </p>
        <ul class="body-content body-text body-list">
            <li>"Downloading the MVP"</li>
            <li>"Finding the media playlist URLs"</li>
            <li>"Computing the absolute URLs using the base MVP URL"</li>
            <li>"Downloading the media playlist"</li>
        </ul>
        <p class="body-content body-text">
            r#"This tool aims to simplify that by resolving and providing the links between
            playlists directly in the browser so that it is easier to go back and forth between
            renditions. Longer term I hope to also add associated functionality, such as providing a
            view for parsed SCTE35 messages found in EXT-X-DATERANGE tags (SCTE35-OUT, SCTE35-IN,
            SCTE35-CMD), and also providing a view for the parsed mp4 boxes from media segments
            found in the media playlist. Essentially, I hope that this can become a useful tool for
            investigating all parts of a HLS stream."#
        </p>
    }
}
