//! Apple Authoring Spec §14 — Low-Latency HLS.

use super::context::AuthoringContext;
use super::helpers::*;
use super::severity::must;
use crate::utils::validator::types::Issue;

pub fn check(ctx: &AuthoringContext<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();

    for pl in ctx.playlists {
        // Treat presence of PART-INF / parts as LL-HLS
        let is_ll = pl.part_target.is_some() || !pl.parts.is_empty();
        if !is_ll {
            continue;
        }
        // SERVER-CONTROL carries the delivery directives an LL-HLS playlist cannot do without,
        // so a missing tag is itself the finding rather than a reason to skip the playlist.
        let Some(sc) = &pl.server_control else {
            issues.push(author_error(
                "14.1",
                format!(
                    "LL-HLS playlist '{}' has partial segments but no EXT-X-SERVER-CONTROL (CAN-BLOCK-RELOAD=YES and PART-HOLD-BACK are required)",
                    pl.name
                ),
            ));
            continue;
        };

        // §14.3 — PART-HOLD-BACK ≥ 3× PART-TARGET
        if let (Some(phb), Some(pt)) = (sc.part_hold_back, pl.part_target) {
            if phb + f64::EPSILON < 3.0 * pt {
                issues.push(author_error(
                    "14.3",
                    format!(
                        "'{}' PART-HOLD-BACK ({phb}) MUST be ≥ 3× PART-TARGET ({pt})",
                        pl.name
                    ),
                ));
            }
        } else if pl.part_target.is_some() && sc.part_hold_back.is_none() {
            issues.push(author_error(
                "14.2",
                format!(
                    "'{}' has PART-TARGET but missing PART-HOLD-BACK",
                    pl.name
                ),
            ));
        }

        // Delta + CAN-SKIP-DATERANGES
        if sc.can_skip_until.is_some() && !sc.can_skip_dateranges {
            issues.push(author_warn(
                "14.5",
                format!(
                    "'{}' advertises CAN-SKIP-UNTIL without CAN-SKIP-DATERANGES",
                    pl.name
                ),
            ));
        }

        if !sc.can_block_reload {
            issues.push(author_issue(
                must(),
                "14.1",
                format!(
                    "LL-HLS playlist '{}' MUST set CAN-BLOCK-RELOAD=YES",
                    pl.name
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
    use super::*;
    use crate::utils::validator::parser::parse_media_playlist;
    use crate::utils::validator::types::{MediaPlaylist, Severity};

    /// A low-latency live media playlist: a sliding window with no EXT-X-ENDLIST, one
    /// closed segment and the partial segments of the segment still being produced.
    ///
    /// `part_inf` and `server_control` are the attribute lists of EXT-X-PART-INF and
    /// EXT-X-SERVER-CONTROL, and `None` leaves the tag out altogether — an absent tag is
    /// what §14.1 and §14.2 are about, so each has to be droppable on its own.
    fn ll_playlist(part_inf: Option<&str>, server_control: Option<&str>) -> MediaPlaylist {
        let tag = |name: &str, attrs: Option<&str>| {
            attrs.map_or(String::new(), |a| format!("#EXT-X-{name}:{a}\n"))
        };
        let content = format!(
            "#EXTM3U\n\
             #EXT-X-VERSION:9\n\
             #EXT-X-TARGETDURATION:4\n\
             #EXT-X-MEDIA-SEQUENCE:100\n\
             {}{}\
             #EXT-X-PROGRAM-DATE-TIME:2026-01-01T00:00:00.000Z\n\
             #EXTINF:4.00000,\n\
             100.m4s\n\
             #EXT-X-PART:DURATION=1.00000,URI=\"101.0.m4s\",INDEPENDENT=YES\n\
             #EXT-X-PART:DURATION=1.00000,URI=\"101.1.m4s\"\n",
            tag("SERVER-CONTROL", server_control),
            tag("PART-INF", part_inf),
        );
        let mut pl = MediaPlaylist::new(
            "video/1280x720".into(),
            "https://example.com/v.m3u8".into(),
        );
        parse_media_playlist("https://example.com/v.m3u8", &content, &mut pl);
        pl.media_type = "VIDEO".into();
        pl
    }

    /// §14 findings citing exactly `rule`. The citation is followed by a colon, which
    /// keeps §14.1 from also matching a hypothetical §14.10.
    fn rule_issues(pl: &MediaPlaylist, rule: &str) -> Vec<Issue> {
        let playlists = std::slice::from_ref(pl);
        let opts = ValidateAuthorOptions::default();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(None, playlists, &opts, &inits, &segs, &vtts);
        let needle = format!("§{rule}:");
        check(&ctx)
            .into_iter()
            .filter(|i| i.message.contains(&needle))
            .collect()
    }

    /// Every §14 finding, for the cases that assert silence across the whole section.
    fn all_issues(pl: &MediaPlaylist) -> Vec<Issue> {
        let playlists = std::slice::from_ref(pl);
        let opts = ValidateAuthorOptions::default();
        let inits: Vec<InitProbeEntry> = Vec::new();
        let segs: Vec<SegmentSample> = Vec::new();
        let vtts: Vec<WebVttSample> = Vec::new();
        let ctx = AuthoringContext::new(None, playlists, &opts, &inits, &segs, &vtts);
        check(&ctx)
    }

    /// SERVER-CONTROL as a conforming low-latency origin writes it, against the
    /// `PART-TARGET=1.00000` the fixtures declare.
    const COMPLIANT_SERVER_CONTROL: &str =
        "CAN-BLOCK-RELOAD=YES,PART-HOLD-BACK=3.0,CAN-SKIP-UNTIL=24.0,CAN-SKIP-DATERANGES=YES";

    #[test]
    fn author_14_1_errors_when_a_low_latency_playlist_has_no_server_control() {
        let pl = ll_playlist(Some("PART-TARGET=1.00000"), None);
        let issues = rule_issues(&pl, "14.1");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("CAN-BLOCK-RELOAD=YES")
                && issues[0].message.contains("PART-HOLD-BACK"),
            "the finding should name the directives the missing tag carries, got: {}",
            issues[0].message
        );
    }

    /// Without EXT-X-SERVER-CONTROL there is nothing to read PART-HOLD-BACK from, so the
    /// missing tag is reported once rather than once per directive it would have carried.
    #[test]
    fn author_14_1_is_the_only_finding_when_server_control_is_absent() {
        let issues = all_issues(&ll_playlist(Some("PART-TARGET=1.00000"), None));
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].message.contains("§14.1:"), "{issues:?}");
    }

    #[test]
    fn author_14_1_errors_when_can_block_reload_is_not_yes() {
        let pl = ll_playlist(
            Some("PART-TARGET=1.00000"),
            Some("PART-HOLD-BACK=3.0,CAN-BLOCK-RELOAD=NO"),
        );
        let issues = rule_issues(&pl, "14.1");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("MUST set CAN-BLOCK-RELOAD=YES"),
            "got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_14_2_errors_when_part_target_has_no_part_hold_back() {
        let pl = ll_playlist(Some("PART-TARGET=1.00000"), Some("CAN-BLOCK-RELOAD=YES"));
        let issues = rule_issues(&pl, "14.2");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("PART-TARGET but missing PART-HOLD-BACK"),
            "got: {}",
            issues[0].message
        );
        assert!(
            rule_issues(&pl, "14.3").is_empty(),
            "there is no PART-HOLD-BACK to measure against 3× PART-TARGET"
        );
    }

    #[test]
    fn author_14_3_errors_when_part_hold_back_is_under_three_part_targets() {
        let pl = ll_playlist(
            Some("PART-TARGET=1.00000"),
            Some("CAN-BLOCK-RELOAD=YES,PART-HOLD-BACK=2.5"),
        );
        let issues = rule_issues(&pl, "14.3");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(
            issues[0].message.contains("PART-HOLD-BACK (2.5)")
                && issues[0].message.contains("PART-TARGET (1)"),
            "the finding should name both values it compared, got: {}",
            issues[0].message
        );
        assert!(
            rule_issues(&pl, "14.2").is_empty(),
            "PART-HOLD-BACK is present, so §14.2 is satisfied"
        );
    }

    /// The rule is `≥`, so a hold-back of exactly three part targets conforms.
    #[test]
    fn author_14_3_accepts_part_hold_back_at_exactly_three_part_targets() {
        let pl = ll_playlist(
            Some("PART-TARGET=1.00000"),
            Some("CAN-BLOCK-RELOAD=YES,PART-HOLD-BACK=3.0"),
        );
        assert!(rule_issues(&pl, "14.3").is_empty(), "{:?}", all_issues(&pl));
    }

    /// 3 × 0.2 is 0.6000000000000001 in binary floating point, so a playlist that declares
    /// exactly three part targets of 200 ms reads as one ulp short of the requirement. The
    /// comparison's epsilon is what stops that arithmetic from failing a conforming stream.
    #[test]
    fn author_14_3_accepts_three_part_targets_that_do_not_divide_exactly() {
        assert_ne!(3.0 * 0.2_f64, 0.6_f64, "the fixture must exercise the rounding");
        let pl = ll_playlist(
            Some("PART-TARGET=0.20000"),
            Some("CAN-BLOCK-RELOAD=YES,PART-HOLD-BACK=0.6"),
        );
        assert!(rule_issues(&pl, "14.3").is_empty(), "{:?}", all_issues(&pl));
    }

    #[test]
    fn author_14_5_warns_when_delta_updates_omit_can_skip_dateranges() {
        let pl = ll_playlist(
            Some("PART-TARGET=1.00000"),
            Some("CAN-BLOCK-RELOAD=YES,PART-HOLD-BACK=3.0,CAN-SKIP-UNTIL=24.0"),
        );
        let issues = rule_issues(&pl, "14.5");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Warn);
        assert!(
            issues[0].message.contains("CAN-SKIP-DATERANGES"),
            "got: {}",
            issues[0].message
        );
    }

    #[test]
    fn author_14_rules_stay_quiet_on_a_compliant_low_latency_playlist() {
        let issues = all_issues(&ll_playlist(
            Some("PART-TARGET=1.00000"),
            Some(COMPLIANT_SERVER_CONTROL),
        ));
        assert!(
            issues.is_empty(),
            "a conforming LL-HLS playlist has nothing to report, got: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
    }

    /// §14 is about low-latency delivery, which a playlist announces with PART-INF or with
    /// partial segments. An ordinary live playlist declares neither and is left alone, even
    /// though it has no EXT-X-SERVER-CONTROL either.
    #[test]
    fn author_14_rules_ignore_a_playlist_with_no_partial_segments() {
        let mut pl = ll_playlist(None, None);
        pl.parts.clear();
        assert!(pl.part_target.is_none() && pl.server_control.is_none());
        assert!(all_issues(&pl).is_empty(), "{:?}", all_issues(&pl));
    }

    /// Partial segments alone make a playlist low-latency, so §14.1 still asks for
    /// EXT-X-SERVER-CONTROL. §14.2 and §14.3 measure PART-HOLD-BACK against PART-TARGET,
    /// which only EXT-X-PART-INF declares, so they have nothing to say here.
    #[test]
    fn author_14_1_applies_to_parts_declared_without_part_inf() {
        let missing_control = ll_playlist(None, None);
        assert!(missing_control.part_target.is_none() && !missing_control.parts.is_empty());
        let issues = rule_issues(&missing_control, "14.1");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);

        let with_control = ll_playlist(None, Some("CAN-BLOCK-RELOAD=YES"));
        assert!(
            all_issues(&with_control).is_empty(),
            "no PART-TARGET was declared to compare a hold-back against, got: {:?}",
            all_issues(&with_control)
                .iter()
                .map(|i| &i.message)
                .collect::<Vec<_>>()
        );
    }
}
