use serde::Serialize;

/// How much a finding matters.
///
/// The distinction is not cosmetic: it decides the exit code, and therefore
/// whether CI fails or an agent stops to fix something. A broken `[[wikilink]]`
/// is a deliberate TODO in this workflow — the compile skill tells the agent to
/// link concepts before their articles exist — so it cannot be an error without
/// making every healthy archive fail its own lint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The archive is malformed: something is unparseable, ambiguous, or lying.
    Error,
    /// Work that is not finished yet. Expected in a living archive.
    Warning,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// A single lint result.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub severity: Severity,
    /// Stable kebab-case identifier, so output can be filtered or grouped by
    /// rule without matching on prose that may be reworded.
    pub rule: &'static str,
    /// Archive-relative path the finding is about, when it is about a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub message: String,
}

impl Finding {
    pub fn error(rule: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            rule,
            path: Some(path.into()),
            message: message.into(),
        }
    }

    pub fn warning(
        rule: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            rule,
            path: Some(path.into()),
            message: message.into(),
        }
    }

    /// A finding about the archive as a whole rather than one file.
    pub fn global(severity: Severity, rule: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity,
            rule,
            path: None,
            message: message.into(),
        }
    }
}

/// Ordering for display: errors first, then by rule, then by path — stable
/// across runs so a diff of two lint outputs shows only real changes.
pub fn sort(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.rule.cmp(b.rule))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.message.cmp(&b.message))
    });
}

pub fn count(findings: &[Finding], severity: Severity) -> usize {
    findings.iter().filter(|f| f.severity == severity).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_sort_before_warnings() {
        let mut findings = vec![
            Finding::warning("broken-link", "b.md", "w"),
            Finding::error("duplicate-slug", "a.md", "e"),
        ];
        sort(&mut findings);
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn sorting_is_total_so_output_does_not_churn() {
        let build = || {
            vec![
                Finding::warning("broken-link", "z.md", "1"),
                Finding::warning("broken-link", "a.md", "2"),
                Finding::error("invalid-origin", "m.md", "3"),
            ]
        };
        let mut a = build();
        let mut b = build();
        b.reverse();
        sort(&mut a);
        sort(&mut b);
        let paths = |f: &[Finding]| f.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&a), paths(&b));
    }

    #[test]
    fn severity_counts() {
        let findings = vec![
            Finding::error("a", "x", "1"),
            Finding::warning("b", "y", "2"),
            Finding::warning("c", "z", "3"),
        ];
        assert_eq!(count(&findings, Severity::Error), 1);
        assert_eq!(count(&findings, Severity::Warning), 2);
    }

    #[test]
    fn severity_serializes_as_a_lowercase_string() {
        let json = serde_json::to_string(&Severity::Warning).unwrap();
        assert_eq!(json, "\"warning\"");
    }
}
