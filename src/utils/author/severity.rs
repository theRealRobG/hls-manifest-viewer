//! RFC 2119 keyword → validator [`Severity`] mapping for Author rules.

use crate::utils::validator::types::Severity;

/// Map an Authoring Spec requirement keyword to an issue severity.
#[allow(dead_code)]
pub fn keyword_severity(keyword: &str) -> Option<Severity> {
    match keyword.to_ascii_uppercase().as_str() {
        "MUST" | "MUST NOT" | "SHALL" | "SHALL NOT" | "REQUIRED" => Some(Severity::Error),
        "SHOULD" | "SHOULD NOT" | "RECOMMENDED" => Some(Severity::Warn),
        "MAY" | "OPTIONAL" => None,
        _ => None,
    }
}

/// Convenience: Error for MUST-class rules.
pub fn must() -> Severity {
    Severity::Error
}

/// Convenience: Warn for SHOULD-class rules.
pub fn should() -> Severity {
    Severity::Warn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_must_to_error() {
        assert_eq!(keyword_severity("MUST"), Some(Severity::Error));
        assert_eq!(keyword_severity("must not"), Some(Severity::Error));
    }

    #[test]
    fn maps_should_to_warn() {
        assert_eq!(keyword_severity("SHOULD"), Some(Severity::Warn));
        assert_eq!(keyword_severity("RECOMMENDED"), Some(Severity::Warn));
    }

    #[test]
    fn skips_may() {
        assert_eq!(keyword_severity("MAY"), None);
    }
}
