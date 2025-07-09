use leptos::prelude::*;

#[component]
pub fn About() -> impl IntoView {
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
