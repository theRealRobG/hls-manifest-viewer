use crate::{
    PLAYLIST_URL_QUERY_NAME,
    components::{
        url_input_form::UrlInputForm,
        viewer::{Viewer, ViewerError, ViewerLoading},
    },
};
use leptos::{either::Either, prelude::*};
use leptos_router::hooks::use_query_map;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{DomException, Response, js_sys::TypeError};

#[component]
pub fn Home() -> impl IntoView {
    let query = use_query_map();
    let playlist_result = LocalResource::new(move || {
        let playlist_url = query
            .read()
            .get(PLAYLIST_URL_QUERY_NAME)
            .unwrap_or_default();
        fetch_url(playlist_url)
    });
    view! {
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
    }
}

async fn fetch_url(url: String) -> Result<String, String> {
    if url.is_empty() {
        return Ok(String::new());
    }
    let window = web_sys::window().expect("Window must be defined");
    let response = JsFuture::from(window.fetch_with_str(&url))
        .await
        .map_err(fetch_failed)?;
    let response: Response = response
        .dyn_into()
        .expect("Fetch must resolve to a Response");
    if !response.ok() {
        return Err(response_failure_string(response).await);
    }
    let text = JsFuture::from(response.text().map_err(fetch_failed)?)
        .await
        .map_err(fetch_failed)?;
    Ok(text
        .as_string()
        .expect("text() on a fetch Response must provide a String"))
}

fn fetch_failed(e: JsValue) -> String {
    match e.dyn_into::<TypeError>() {
        Ok(e) => String::from(e.to_string()),
        Err(e) => match e.dyn_into::<DomException>() {
            Ok(e) => String::from(e.to_string()),
            Err(e) => format!("Fetch failed: {e:?}"),
        },
    }
}

async fn response_failure_string(response: Response) -> String {
    let mut base_message = format!(
        "Bad HTTP status code: {} {}",
        response.status(),
        response.status_text()
    );
    let Ok(Some(content_type)) = response.headers().get("Content-Type") else {
        return base_message;
    };
    if content_type.contains("text/plain")
        || content_type.contains("application/json")
        || content_type.contains("application/x-www-form-urlencoded")
    {
        let Ok(response_text_promise) = response.text() else {
            return base_message;
        };
        let Ok(text) = JsFuture::from(response_text_promise).await else {
            return base_message;
        };
        let text = text
            .as_string()
            .expect("text() on a fetch Response must provide a String");
        base_message.push('\n');
        base_message.push_str(&text);
    }
    base_message
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
