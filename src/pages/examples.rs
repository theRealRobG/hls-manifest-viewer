use crate::utils::href::PLAYLIST_URL_QUERY_NAME;
use leptos::prelude::*;

#[component]
pub fn Examples() -> impl IntoView {
    view! {
        <h1 class="body-content">"Examples"</h1>
        <p class="body-content body-text">
            r#"I realize that some people may want to test this site out without necessarily having
            their own HLS demo content to try."#
        </p>
        <br />
        <p class="body-content body-text">
            r#"This page contains a collection of examples that I've found across a few sites that
            offer demo HLS content. This aims to show a variety of different stream types, including
            VOD, live, byterange segment addressing, SCTE35 examples, as well as a variety of
            different codec types and variant combinations. Each section indicates the page where I
            have found the examples. I don't own any of these streams so it is entirely possible
            that some of them may stop working. Please raise a bug if an example stream stops
            working."#
        </p>
        <br />
        <p class="body-content body-text">
            "For clarity, here are the full links to the HLS examples pages that I used:"
        </p>
        <ul class="body-content body-text body-list unordered-list">
            <li>
                <a
                    href="https://developer.apple.com/streaming/examples/"
                    target="_blank"
                    class="body-link"
                >
                    "https://developer.apple.com/streaming/examples/"
                </a>
            </li>
            <li>
                <a
                    href="https://demo.unified-streaming.com/k8s/features/stable/#!/hls"
                    target="_blank"
                    class="body-link"
                >
                    "https://demo.unified-streaming.com/k8s/features/stable/#!/hls"
                </a>
            </li>
            <li>
                <a
                    href="https://optiview.dolby.com/resources/demos/test-stream/"
                    target="_blank"
                    class="body-link"
                >
                    "https://optiview.dolby.com/resources/demos/test-stream/"
                </a>
            </li>
        </ul>
        <br />
        <p class="body-content body-text">
            r#"Each table of examples includes a link to the actual manifest and also the link to
            view the manifest within this site."#
        </p>
        <br />
        <ExamplesSection
            header="Apple Vision Pro Examples"
            examples_link="https://developer.apple.com/streaming/examples/"
            examples=APPLE_VISION_PRO_EXAMPLES
        />
        <ExamplesSection
            header="Apple Advanced Examples"
            examples_link="https://developer.apple.com/streaming/examples/"
            examples=APPLE_ADVANCED_EXAMPLES
        />
        <ExamplesSection
            header="Universal Streaming Packager Demo Page"
            examples_link="https://demo.unified-streaming.com/k8s/features/stable/#!/hls"
            examples=USP_EXAMPLES
        />
        <ExamplesSection
            header="Dolby Optiview Demo Page"
            examples_link="https://optiview.dolby.com/resources/demos/test-stream/"
            examples=DOLBY_DEMO_PAGE_EXAMPLES
        />
    }
}

#[component]
fn ExamplesSection<const N: usize>(
    header: &'static str,
    examples_link: &'static str,
    examples: [Example; N],
) -> impl IntoView {
    view! {
        <h2 class="body-content">
            <a href=examples_link class="body-link">
                {header}
            </a>
        </h2>
        <table class="body-content examples-table">
            <tr>
                <th class="body-text">"Description"</th>
                <th class="body-text">"Manifest URL"</th>
                <th class="body-text">"Viewer link"</th>
            </tr>
            {examples
                .into_iter()
                .map(|ex| {
                    view! {
                        <tr>
                            <td class="body-text">{ex.description}</td>
                            <td class="centered body-text">
                                <a href=ex.playlist_url target="_blank" class="body-link">
                                    "Manifest"
                                </a>
                            </td>
                            <td class="centered body-text">
                                <a href=ex.site_url() class="body-link">
                                    "View"
                                </a>
                            </td>
                        </tr>
                    }
                })
                .collect_view()}
        </table>
        <br />
    }
}

#[derive(Debug, Clone, Copy)]
struct Example {
    description: &'static str,
    playlist_url: &'static str,
}
impl Example {
    fn site_url(&self) -> String {
        format!(
            "/hls-manifest-viewer?{}={}",
            PLAYLIST_URL_QUERY_NAME, self.playlist_url
        )
    }
}

// https://developer.apple.com/streaming/examples/
const APPLE_VISION_PRO_EXAMPLES: [Example; 6] = [
    Example {
        description: "Apple Immersive Video stream",
        playlist_url: "https://devstreaming-cdn.apple.com/videos/streaming/examples/immersive-media/apple-immersive-video/primary.m3u8",
    },
    Example {
        description: "Spatial video stream",
        playlist_url: "https://devstreaming-cdn.apple.com/videos/streaming/examples/immersive-media/spatialLighthouseFlowersWaves/mvp.m3u8",
    },
    Example {
        description: "Apple Projected Media Profile stream (180)",
        playlist_url: "https://devstreaming-cdn.apple.com/videos/streaming/examples/immersive-media/180Lighthouse/mvp.m3u8",
    },
    Example {
        description: "3D movie stream",
        playlist_url: "https://devstreaming-cdn.apple.com/videos/streaming/examples/historic_planet_content_2023-10-26-3d-video/main.m3u8",
    },
    Example {
        description: "Apple Projected Media Profile stream (360)",
        playlist_url: "https://devstreaming-cdn.apple.com/videos/streaming/examples/immersive-media/360Lighthouse/mvp.m3u8",
    },
    Example {
        description: "Apple Projected Media Profile stream",
        playlist_url: "https://devstreaming-cdn.apple.com/videos/streaming/examples/immersive-media/wfovCausewayWalk/mvp.m3u8",
    },
];
// https://developer.apple.com/streaming/examples/
const APPLE_ADVANCED_EXAMPLES: [Example; 3] = [
    Example {
        description: "BipBop, AVC + HEVC, AAC-LC + AC-3 + E-AC-3, WebVTT + CEA-608, 30fps + 60fps",
        playlist_url: "https://devstreaming-cdn.apple.com/videos/streaming/examples/bipbop_adv_example_hevc/master.m3u8",
    },
    Example {
        description: "Becoming You trailer (many renditions)",
        playlist_url: "https://devstreaming-cdn.apple.com/videos/streaming/examples/adv_dv_atmos/main.m3u8",
    },
    Example {
        description: "BipBop (segments addressed with EXT-X-BYTERANGE)",
        playlist_url: "https://devstreaming-cdn.apple.com/videos/streaming/examples/img_bipbop_adv_example_fmp4/master.m3u8",
    },
];
// https://demo.unified-streaming.com/k8s/features/stable/#!/hls
const USP_EXAMPLES: [Example; 3] = [
    Example {
        description: "Trickplay with byterange addressing",
        playlist_url: "https://demo.unified-streaming.com/k8s/features/stable/no-handler-origin/tears-of-steel/tears-of-steel-trickplay.m3u8"
    },
    Example {
        description: "AV1",
        playlist_url: "https://demo.unified-streaming.com/k8s/features/stable/video/tears-of-steel/tears-of-steel-av1.ism/.m3u8"
    },
    Example {
        description: "Live with SCTE35 messages in EXT-X-DATERANGE tags (TS segments)",
        playlist_url: "https://demo.unified-streaming.com/k8s/vod2live/stable/unified-learning.isml/.m3u8"
    },
];
// https://optiview.dolby.com/resources/demos/test-stream/
const DOLBY_DEMO_PAGE_EXAMPLES: [Example; 3] = [
    Example {
        description: concat!(
            "FairPlay + PlayReady encrypted stream with byterange addressing on segments (offsets for ",
            "segments after the first are omitted which is another interesting use case to test ",
            "byterange handling)"
        ),
        playlist_url:
            "https://media.axprod.net/TestVectors/Cmaf/protected_1080p_h264_cbcs/manifest.m3u8",
    },
    Example {
        description: "Low Latency - AirenSoft",
        playlist_url: "https://llhls-demo.ovenmediaengine.com/app/stream/llhls.m3u8"
    },
    Example {
        description: "Low Latency - Nimble Streamer",
        playlist_url: "https://ll-hls.softvelum.com/sldp/bbloop/playlist.m3u8"
    },
];
