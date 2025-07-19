use crate::utils::href::PLAYLIST_URL_QUERY_NAME;
use leptos::prelude::*;
use leptos_router::{components::Form, hooks::use_query_map};

#[component]
pub fn UrlInputForm() -> impl IntoView {
    let query = use_query_map();
    let playlist_url = move || {
        query
            .read()
            .get(PLAYLIST_URL_QUERY_NAME)
            .unwrap_or_default()
    };

    view! {
        <Form attr:class="url-input-form" method="GET" action="">
            <div class="url-input-form-inner-container">
                <input
                    class="url-input"
                    type="url"
                    name=PLAYLIST_URL_QUERY_NAME
                    value=playlist_url
                    placeholder="https://example.com/mvp.m3u8"
                    pattern="https?://.*"
                    aria-label="playlist url"
                    title="url with http or https scheme (e.g. https://example.com/mvp.m3u8)"
                />
                <input class="button" type="submit" />
            </div>
        </Form>
    }
}
