//! Apple Authoring Spec §3 — Ads / interstitials (playlist-visible).

use super::context::AuthoringContext;
use super::helpers::*;
use crate::utils::validator::types::Issue;

/// One interstitial EXT-X-DATERANGE that points at an asset.
struct AssetInterstitial {
    playlist_name: String,
    /// FRAME-RATE of the host variant, when the multivariant playlist declared one.
    host_frame_rate: Option<f64>,
}

fn asset_interstitials(ctx: &AuthoringContext<'_>) -> Vec<AssetInterstitial> {
    let mut found = Vec::new();
    for pl in ctx.playlists {
        for line in pl.raw_content.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("#EXT-X-DATERANGE:") else {
                continue;
            };
            let attrs = crate::utils::validator::parser::parse_attributes(rest);
            if attrs.get("CLASS").map(|s| s.as_str()) != Some("com.apple.hls.interstitial") {
                continue;
            }
            if attrs.contains_key("X-ASSET-URI") || attrs.contains_key("X-ASSET-LIST") {
                found.push(AssetInterstitial {
                    playlist_name: pl.name.clone(),
                    host_frame_rate: pl.frame_rate,
                });
            }
        }
    }
    found
}

/// How the interstitials are described in a summary line: the host playlist when they
/// all sit on one rendition, otherwise how many renditions carry them.
fn hosts(found: &[AssetInterstitial]) -> String {
    let mut names: Vec<&str> = found.iter().map(|i| i.playlist_name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    match names.as_slice() {
        [one] => format!("'{one}'"),
        many => format!("{} renditions", many.len()),
    }
}

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();

    let found = asset_interstitials(ctx);
    if found.is_empty() {
        return issues;
    }

    // §3.2–3.4 — the asset's codecs, aspect ratio and bit rate SHOULD match the host
    // variant. Nothing in the multivariant or media playlists describes the asset, so
    // this stays a single informational reminder for the whole stream rather than one
    // per ad break: repeating an unverifiable note per EXT-X-DATERANGE only adds noise.
    issues.push(author_info(
        "3.2",
        format!(
            "{} interstitial(s) on {} reference assets; ensure asset codecs, aspect ratio and \
             bit rate match the host variant (assets are not fetched, so this is not verified)",
            found.len(),
            hosts(&found)
        ),
    ));

    // AirPlay §3.5 — asset frame rates SHOULD be x, 2x or x/2 of the host frame rate.
    if ctx.policy.profile == super::profile::AuthorProfile::AirPlay2 {
        let mut rates: Vec<String> = found
            .iter()
            .filter_map(|i| i.host_frame_rate)
            .map(|f| format!("{f}"))
            .collect();
        rates.sort_unstable();
        rates.dedup();
        if !rates.is_empty() {
            issues.push(author_info(
                "3.5",
                format!(
                    "AirPlay2: interstitial assets SHOULD use a frame rate of x, 2x or x/2 of the \
                     host ({} fps)",
                    rates.join("/")
                ),
            ));
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::super::context::{
        InitProbeEntry, SegmentSample, ValidateAuthorOptions, WebVttSample,
    };
    use super::super::profile::AuthorProfile;
    use super::*;
    use crate::utils::validator::parser::parse_master_playlist;
    use crate::utils::validator::types::{MasterPlaylist, MediaPlaylist, Severity};

    fn master() -> MasterPlaylist {
        parse_master_playlist(
            "https://example.com/master.m3u8",
            r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=2000000,AVERAGE-BANDWIDTH=1800000,RESOLUTION=1280x720,CODECS="avc1.4d401f",FRAME-RATE=30
https://example.com/v.m3u8
"#,
        )
    }

    /// Video playlist with `count` interstitial DATERANGEs, each pointing at an asset.
    fn playlist_with_breaks(name: &str, count: usize) -> MediaPlaylist {
        let mut pl = MediaPlaylist::new(name.into(), format!("https://example.com/{name}.m3u8"));
        pl.frame_rate = Some(30.0);
        let mut raw = String::from("#EXTM3U\n#EXT-X-TARGETDURATION:6\n");
        for i in 0..count {
            raw.push_str(&format!(
                "#EXT-X-DATERANGE:ID=\"ad{i}\",CLASS=\"com.apple.hls.interstitial\",\
                 START-DATE=\"2026-01-01T00:0{i}:00.000Z\",DURATION=15,\
                 X-ASSET-URI=\"https://ads.example.com/{i}.m3u8\"\n"
            ));
        }
        pl.raw_content = raw;
        pl
    }

    fn ads_issues(playlists: &[MediaPlaylist], profile: AuthorProfile) -> Vec<Issue> {
        let master = master();
        let opts = ValidateAuthorOptions {
            profile,
            deep_checks: false,
        };
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(Some(&master), playlists, &opts, &inits, &segs, &vtts);
        check(&ctx)
    }

    #[test]
    fn author_3_2_reports_once_for_many_ad_breaks() {
        let issues = ads_issues(&[playlist_with_breaks("video/1280x720", 6)], AuthorProfile::None);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Info);
        assert!(
            issues[0].message.contains("6 interstitial(s)") && issues[0].message.contains("video/1280x720"),
            "expected one summary naming the count and host, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_3_2_summarises_breaks_across_renditions() {
        let issues = ads_issues(
            &[
                playlist_with_breaks("video/1280x720", 2),
                playlist_with_breaks("video/640x360", 2),
            ],
            AuthorProfile::None,
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0].message.contains("4 interstitial(s)") && issues[0].message.contains("2 renditions"),
            "expected a stream-wide summary, got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_3_5_reports_once_on_airplay() {
        let issues = ads_issues(
            &[playlist_with_breaks("video/1280x720", 4)],
            AuthorProfile::AirPlay2,
        );
        let airplay: Vec<_> = issues.iter().filter(|i| i.message.contains("§3.5")).collect();
        assert_eq!(airplay.len(), 1, "{issues:?}");
        assert!(
            airplay[0].message.contains("30 fps"),
            "expected the host frame rate, got: {}",
            airplay[0].message
        );
    }

    #[test]
    fn author_3_2_stays_quiet_without_interstitials() {
        let mut pl = MediaPlaylist::new("video/1280x720".into(), "https://example.com/v.m3u8".into());
        pl.raw_content = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\n0.m4s\n".into();
        assert!(ads_issues(&[pl], AuthorProfile::AirPlay2).is_empty());
    }
}
