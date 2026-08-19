//! Canonical Apple HLS Authoring Specification rule IDs.
//!
//! Every Author finding is labelled with the section it comes from, and a citation is the
//! only thing a reader can check a finding against. A mislabelled one is worse than no
//! finding at all: it sends someone to a rule that says something else. So the IDs the
//! rules may cite are listed here, [`helpers::author_issue`](super::helpers::author_issue)
//! refuses anything outside the list in debug builds, and IDs that turned out to be
//! mislabels are recorded as retired so they cannot quietly come back.

/// Rule IDs the Author checks may cite, in spec order.
///
/// A rule earns its place here by being cited from a check or named by policy (the
/// visionOS exemptions and the immersive-AIV conflicts both key off these IDs). Adding an
/// ID is how a new citation is declared; nothing else needs to change.
const CITABLE_RULES: &[&str] = &[
    // §1 Video
    "1.1", "1.2", "1.3a", "1.3b", "1.4", "1.5", "1.6a", "1.6b", "1.8", "1.9", "1.9b", "1.10",
    "1.11", "1.12", "1.13", "1.18", "1.19", "1.20", "1.23", "1.24", "1.25", "1.26", "1.27",
    "1.28", "1.29", "1.30", "1.32", "1.33", "1.34", "1.35", "1.36", "1.37", "1.38", "1.39",
    "1.41",
    // §2 Audio
    "2.2", "2.3", "2.4", "2.6", "2.12", "2.13", "2.15", "2.19", "2.20", "2.21", "2.25", "2.26",
    "2.27", "2.30", "2.31",
    // §3 Ads
    "3.2", "3.5",
    // §4 Accessibility, §5 Subtitles
    "4.1", "4.3", "4.5", "4.6", "4.7", "5.2", "5.3", "5.5", "5.6", "5.8", "5.11",
    // §6 Trick play
    "6.1", "6.5", "6.6", "6.8", "6.9", "6.10", "6.11", "6.13", "6.14", "6.15", "6.16", "6.18",
    // §7 Segmentation
    "7.2", "7.3", "7.4", "7.5", "7.6", "7.7", "7.8", "7.9",
    // §8 Media playlists
    "8.1", "8.2", "8.3", "8.4", "8.6", "8.7", "8.9", "8.10", "8.11", "8.12", "8.17", "8.18",
    "8.20", "8.22", "8.23", "8.24", "8.25",
    // §9 Multivariant playlist
    "9.1", "9.2", "9.3", "9.4", "9.5", "9.6", "9.9", "9.10", "9.12", "9.13", "9.14", "9.15",
    "9.16", "9.17", "9.18", "9.19", "9.20", "9.21", "9.23", "9.24",
    // §10 Delivery, §11 Privacy, §12 Security
    "10.1", "10.2", "10.3", "11.1", "11.2", "11.3", "11.4", "12.1", "12.4",
    // §13 Content protection
    "13.2", "13.3", "13.4", "13.5", "13.6", "13.7", "13.9", "13.11",
    // §14 Low-latency HLS
    "14.1", "14.2", "14.3", "14.5",
    // §15 SharePlay, §16 Spatial / immersive
    "15.2", "16.1", "16.2", "16.3", "16.4", "16.5", "16.6", "16.7",
];

/// IDs the Author checks used to cite and must not cite again, with what the finding
/// actually belonged to. Keeping them listed is what stops a reverted citation from
/// looking like a new one.
const RETIRED_RULES: &[(&str, &str)] = &[
    (
        "9.11",
        "a mislabel of §9.12, which is the EXT-X-INDEPENDENT-SEGMENTS requirement",
    ),
    (
        "8.21",
        "never carried the EXT-X-INDEPENDENT-SEGMENTS requirement that was filed under it; \
         xHE-AAC is §8.23 and APAC is §8.25",
    ),
];

/// Shape of a rule ID: a section number, a rule number, and optionally a letter for the
/// rules the spec splits (§1.3a and §1.3b are different requirements).
fn is_well_formed(id: &str) -> bool {
    let Some((section, rule)) = id.split_once('.') else {
        return false;
    };
    let rule = rule
        .strip_suffix(|c: char| c.is_ascii_lowercase())
        .unwrap_or(rule);
    let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    numeric(section) && numeric(rule)
}

/// Whether `id` is a rule the Author checks may cite.
pub fn is_citable(id: &str) -> bool {
    CITABLE_RULES.contains(&id)
}

/// Why `id` may not be cited, for the assertion that guards the emission path.
pub fn citation_problem(id: &str) -> Option<String> {
    if is_citable(id) {
        return None;
    }
    if let Some((_, note)) = RETIRED_RULES.iter().find(|(retired, _)| *retired == id) {
        return Some(format!("§{id} is retired: it {note}"));
    }
    if !is_well_formed(id) {
        return Some(format!(
            "'{id}' is not even shaped like a rule ID (a section and a rule number, such \
             as 8.22), so the citation cannot be right"
        ));
    }
    Some(format!(
        "§{id} is not a canonical Authoring Spec rule — add it to the Author rule catalog \
         if the citation is right, or correct the citation if it is not"
    ))
}

#[cfg(test)]
mod tests {
    use super::super::profile::AuthorPolicy;
    use super::*;
    use std::collections::HashSet;

    /// Author rule modules, read as text so the catalog can be checked against the
    /// citations they actually make.
    const RULE_SOURCES: &[(&str, &str)] = &[
        ("rules_a11y_subs.rs", include_str!("rules_a11y_subs.rs")),
        ("rules_ads.rs", include_str!("rules_ads.rs")),
        ("rules_audio.rs", include_str!("rules_audio.rs")),
        ("rules_deep.rs", include_str!("rules_deep.rs")),
        (
            "rules_delivery_privacy_security.rs",
            include_str!("rules_delivery_privacy_security.rs"),
        ),
        ("rules_llhls.rs", include_str!("rules_llhls.rs")),
        (
            "rules_media_playlist.rs",
            include_str!("rules_media_playlist.rs"),
        ),
        (
            "rules_multivariant.rs",
            include_str!("rules_multivariant.rs"),
        ),
        ("rules_protection.rs", include_str!("rules_protection.rs")),
        ("rules_segmentation.rs", include_str!("rules_segmentation.rs")),
        (
            "rules_shareplay_spatial.rs",
            include_str!("rules_shareplay_spatial.rs"),
        ),
        (
            "rules_trickplay.rs",
            include_str!("rules_trickplay.rs"),
        ),
        ("rules_video.rs", include_str!("rules_video.rs")),
    ];

    /// The issue builders, all of which take the cited section as their first string
    /// argument — no argument before it is ever a literal.
    const BUILDERS: &[&str] = &[
        "author_issue(",
        "author_issue_with_confidence(",
        "author_error(",
        "author_warn(",
        "author_info(",
        "author_conflict_aware_issue(",
    ];

    /// Rule IDs cited literally in `source`. A call that passes its section in a
    /// variable yields nothing here and is covered by the debug assertion instead.
    fn cited_ids(source: &str) -> Vec<&str> {
        let mut ids = Vec::new();
        let mut rest = source;
        while let Some((at, len)) = BUILDERS
            .iter()
            .filter_map(|b| rest.find(b).map(|at| (at, b.len())))
            .min_by_key(|&(at, _)| at)
        {
            rest = &rest[at + len..];
            let Some(open) = rest.find('"') else { break };
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else { break };
            if is_well_formed(&after[..close]) {
                ids.push(&after[..close]);
            }
        }
        ids
    }

    #[test]
    fn every_cited_rule_is_in_the_catalog() {
        for (file, source) in RULE_SOURCES {
            for id in cited_ids(source) {
                assert!(
                    is_citable(id),
                    "{file} cites §{id}: {}",
                    citation_problem(id).unwrap_or_default()
                );
            }
        }
    }

    /// The scan has to actually find citations, or the test above passes by reading
    /// nothing at all.
    #[test]
    fn the_citation_scan_reads_the_rule_modules() {
        let found: HashSet<&str> = RULE_SOURCES
            .iter()
            .flat_map(|(_, source)| cited_ids(source))
            .collect();
        assert!(found.len() > 100, "only found {} citations", found.len());
        for expected in ["1.26", "7.4", "8.1", "8.22", "9.12", "13.2", "16.6"] {
            assert!(found.contains(expected), "§{expected} was not found");
        }
    }

    #[test]
    fn catalog_is_well_formed_sorted_and_free_of_duplicates() {
        let mut seen = HashSet::new();
        for id in CITABLE_RULES {
            assert!(is_well_formed(id), "'{id}' is not a rule ID");
            assert!(seen.insert(*id), "§{id} is listed twice");
        }
        // Sorted by section, then rule, so the list stays readable as it grows.
        let key = |id: &str| {
            let (section, rule) = id.split_once('.').expect("well-formed id");
            let digits: String = rule.chars().take_while(|c| c.is_ascii_digit()).collect();
            let suffix: String = rule.chars().skip_while(|c| c.is_ascii_digit()).collect();
            (
                section.parse::<u32>().expect("numeric section"),
                digits.parse::<u32>().expect("numeric rule"),
                suffix,
            )
        };
        let keys: Vec<_> = CITABLE_RULES.iter().map(|id| key(id)).collect();
        assert!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "catalog is out of spec order"
        );
    }

    #[test]
    fn retired_rules_cannot_be_cited_again() {
        for (id, _) in RETIRED_RULES {
            assert!(!is_citable(id), "§{id} is retired but still citable");
            let problem = citation_problem(id).expect("a retired rule is not citable");
            assert!(problem.contains("retired"), "{problem}");
        }
        // §9.11 was emitted for what is really §9.12, and §9.12 stays citable.
        assert!(is_citable("9.12"));
    }

    #[test]
    fn unknown_citations_are_rejected() {
        assert!(citation_problem("8.99").is_some());
        assert!(citation_problem("nonsense").is_some());
        assert!(citation_problem("8.22").is_none());
    }

    /// The visionOS amendments exempt a fixed list of general compatibility rules. Pinning
    /// it here keeps `AuthorPolicy` from exempting a rule the catalog does not know, which
    /// would silently switch the exemption off for the rule that was meant.
    #[test]
    fn visionos_exemptions_name_catalogued_rules() {
        let exempt = AuthorPolicy::vision_exempt_rules();
        let mut listed: Vec<&str> = exempt.iter().copied().collect();
        listed.sort_unstable();
        assert_eq!(
            listed,
            ["1.12", "1.24", "1.3a", "1.6a", "1.9b", "2.3", "2.6", "6.14", "6.16"]
        );
        for id in exempt {
            assert!(
                is_citable(id),
                "visionOS exempts §{id}, which is not in the catalog"
            );
        }
    }

    #[test]
    fn immersive_aiv_conflicts_name_catalogued_rules() {
        // The conflict list is what turns a general rule into an informational finding on
        // immersive AIV content, so an ID that drifts silently re-enables the failure.
        let policy = AuthorPolicy::for_profile(super::super::AuthorProfile::None, false, true);
        for id in ["1.6b", "1.19", "1.20", "1.32", "1.34"] {
            assert!(policy.aiv_spec_conflict(id), "§{id} is not a conflict rule");
            assert!(is_citable(id), "§{id} is not in the catalog");
        }
        assert!(!policy.aiv_spec_conflict("1.13"));
    }
}
