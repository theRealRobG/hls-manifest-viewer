mod error;
mod isobmff;
mod loading;
mod playlist;
mod preformatted;

use crate::utils::{
    network::{fetch_array_buffer, FetchError, FetchTextResponse, RequestRange},
    query_codec::{MediaSegmentContext, SupplementalViewQueryContext},
    response::{determine_segment_type, SegmentType},
};
use error::ViewerError;
use isobmff::IsobmffViewer;
use leptos::prelude::*;
pub use loading::ViewerLoading;
use playlist::{HighlightedMapInfo, PlaylistViewer};
use preformatted::PreformattedViewer;
use std::collections::HashMap;

const VIEWER_CLASS: &str = "viewer-content";
const MAIN_VIEW_CLASS: &str = "viewer-main";
const SUPPLEMENTAL_VIEW_CLASS: &str = "viewer-supplemental supplemental-active";
const ISOBMFF_VIEW_CLASS: &str = "viewer-supplemental isobmff-view supplemental-active";
const MAIN_VIEW_WITH_SUPPLEMENTAL_CLASS: &str = "viewer-main supplemental-active";
const ERROR_CLASS: &str = "error";
const TAG_CLASS: &str = "hls-line tag";
const URI_CLASS: &str = "hls-line uri";
const COMMENT_CLASS: &str = "hls-line comment";
const BLANK_CLASS: &str = "hls-line blank";
const HIGHLIGHTED: &str = "highlighted";
const HIGHLIGHTED_URI_CLASS: &str = "hls-line uri highlighted";

#[component]
pub fn Viewer(
    fetch_response: Result<FetchTextResponse, FetchError>,
    supplemental_context: Option<String>,
    imported_definitions: HashMap<String, String>,
) -> impl IntoView {
    let FetchTextResponse {
        response_text: playlist,
    } = match fetch_response {
        Ok(response) => response,
        Err(error) => {
            return view! {
                <Container>
                    <ViewerError error=error.error extra_info=error.extra_info />
                </Container>
            };
        }
    };
    let Some(context) = supplemental_context else {
        return view! {
            <Container>
                <ErrorBounded>
                    <PlaylistViewer playlist imported_definitions />
                </ErrorBounded>
            </Container>
        };
    };
    let context = match SupplementalViewQueryContext::try_from(context.as_str()) {
        Ok(context) => context,
        Err(e) => {
            return view! {
                <Container>
                    <ErrorBounded>
                        <PlaylistViewer playlist imported_definitions supplemental_showing=true />
                    </ErrorBounded>
                    <div class=SUPPLEMENTAL_VIEW_CLASS>
                        <ViewerError
                            error="Error: unable to parse query parameter for supplemental view"
                                .to_string()
                            extra_info=Some(e.to_string())
                        />
                    </div>
                </Container>
            };
        }
    };
    match context {
        SupplementalViewQueryContext::Segment(media_segment_context) => {
            let MediaSegmentContext {
                url,
                media_sequence,
                byterange,
            } = media_segment_context;
            view! {
                <Container>
                    <ErrorBounded>
                        <PlaylistViewer
                            playlist
                            imported_definitions
                            supplemental_showing=true
                            highlighted_segment=media_sequence
                        />
                    </ErrorBounded>
                    <SupplementalSegmentView segment_url=url.clone() byterange />
                </Container>
            }
        }
        SupplementalViewQueryContext::Map(media_segment_context) => {
            let MediaSegmentContext {
                url,
                media_sequence,
                byterange,
            } = media_segment_context;
            let url_for_playlist_viewer = url.clone();
            let url_for_segment_viewer = url.clone();
            view! {
                <Container>
                    <ErrorBounded>
                        <PlaylistViewer
                            playlist
                            imported_definitions
                            supplemental_showing=true
                            highlighted_map_info=HighlightedMapInfo {
                                url: url_for_playlist_viewer,
                                min_media_sequence: media_sequence,
                            }
                        />
                    </ErrorBounded>
                    <SupplementalSegmentView segment_url=url_for_segment_viewer byterange />
                </Container>
            }
        }
    }
}

#[component]
fn ErrorBounded(children: Children) -> impl IntoView {
    view! {
        <ErrorBoundary fallback=|errors| {
            view! {
                {move || {
                    errors
                        .get()
                        .into_iter()
                        .map(|(_, error)| view! { <ViewerError error=error.to_string() /> })
                        .collect::<Vec<_>>()
                }}
            }
        }>{children()}</ErrorBoundary>
    }
}

#[component]
fn Container(children: Children) -> impl IntoView {
    view! { <div class=VIEWER_CLASS>{children()}</div> }
}

#[component]
fn SupplementalSegmentView(segment_url: String, byterange: Option<RequestRange>) -> impl IntoView {
    let segment_result =
        LocalResource::new(move || fetch_array_buffer(segment_url.clone(), byterange));
    view! {
        <Suspense fallback=|| {
            view! { <div class=SUPPLEMENTAL_VIEW_CLASS>"Loading..."</div> }
        }>
            <ErrorBounded>
                {move || {
                    segment_result
                        .get()
                        .map(|fetch_response| {
                            match fetch_response {
                                Ok(r) => {
                                    match determine_segment_type(&r) {
                                        SegmentType::WebVtt => {
                                            view! {
                                                <PreformattedViewer contents=String::from_utf8_lossy(
                                                        &r.response_body,
                                                    )
                                                    .to_string() />
                                            }
                                                .into_any()
                                        }
                                        SegmentType::Mp4 => {
                                            view! { <IsobmffViewer data=r.response_body /> }.into_any()
                                        }
                                        SegmentType::Unknown => {
                                            view! {
                                                <div class=SUPPLEMENTAL_VIEW_CLASS>
                                                    <ViewerError
                                                        error="Error: unsupported segment type".to_string()
                                                        extra_info=Some(
                                                            "Currently only WebVTT and Fragmented MPEG-4 segments are supported"
                                                                .to_string(),
                                                        )
                                                    />
                                                </div>
                                            }
                                                .into_any()
                                        }
                                    }
                                }
                                Err(e) => {
                                    view! { <ViewerError error=e.error extra_info=e.extra_info /> }
                                        .into_any()
                                }
                            }
                        })
                }}
            </ErrorBounded>
        </Suspense>
    }
}
