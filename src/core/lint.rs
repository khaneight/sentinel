use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use super::compilation::Compilation;
use super::links;
use super::manifest::Manifest;
use super::wiki::LoadedArticle;

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

/// Run every check against a loaded archive.
///
/// Lives here rather than in the `lint` command because `sentinel next` needs
/// the same findings to decide what is most worth doing — two copies of these
/// rules would drift, and the one an agent acts on would be the stale one.
pub fn analyze(articles: &[LoadedArticle], manifest: &Manifest) -> Vec<Finding> {
    let mut findings = Vec::new();
    let all_slugs: HashSet<String> = articles.iter().map(|a| a.slug()).collect();

    // A wikilink names a slug, and a slug is just a filename stem. Two articles
    // sharing one across domains collapse into a single node in the link graph:
    // backlinks merge and one article's forward links overwrite the other's.
    // Nothing else in the pipeline notices, so it has to be caught here.
    let mut slug_owners: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for article in articles {
        slug_owners
            .entry(article.slug())
            .or_default()
            .push(article.rel_path());
    }
    for (slug, owners) in &slug_owners {
        if owners.len() > 1 {
            findings.push(Finding::global(
                Severity::Error,
                "duplicate-slug",
                format!(
                    "duplicate slug '{slug}': {} — [[{slug}]] is ambiguous and the link graph merges them",
                    owners.join(", ")
                ),
            ));
        }
    }

    for loaded in articles {
        let path = loaded.rel_path();
        let frontmatter = &loaded.article.frontmatter;

        // Broken wikilinks are checked first because the body is readable even
        // when the frontmatter is not.
        for link in links::extract_wikilinks(&loaded.content) {
            if !all_slugs.contains(link.as_str()) {
                // A warning, not an error: the compile workflow deliberately
                // links concepts before their articles exist.
                findings.push(Finding::warning(
                    "broken-link",
                    path,
                    format!("broken link [[{link}]] — no matching article found"),
                ));
            }
        }

        // Malformed YAML yields default frontmatter, so every field check below
        // would fire at once and point the reader at five imaginary problems
        // instead of the one real one.
        if let Some(error) = &loaded.article.frontmatter_error {
            findings.push(Finding::error(
                "invalid-frontmatter",
                path,
                format!("invalid frontmatter — {error}"),
            ));
            continue;
        }

        // title/domain/origin drive the generated indexes; without them an
        // article is effectively missing from the knowledge base.
        for (field, missing) in [
            ("title", frontmatter.title.is_none()),
            ("domain", frontmatter.domain.is_none()),
            ("origin", frontmatter.origin.is_none()),
        ] {
            if missing {
                findings.push(Finding::error(
                    "missing-field",
                    path,
                    format!("missing '{field}' in frontmatter"),
                ));
            }
        }

        if frontmatter.tags.is_empty() {
            findings.push(Finding::warning("missing-tags", path, "no tags defined"));
        }
        if frontmatter.sources.is_empty() {
            findings.push(Finding::warning(
                "missing-sources",
                path,
                "no sources listed — its raw document will stay uncompiled",
            ));
        }

        if let Some(origin) = &frontmatter.origin
            && !["authored", "researched", "hybrid"].contains(&origin.as_str())
        {
            findings.push(Finding::error(
                "invalid-origin",
                path,
                format!("invalid origin '{origin}' (expected authored/researched/hybrid)"),
            ));
        }

        if let Some(status) = &frontmatter.status
            && !["draft", "review", "stable"].contains(&status.as_str())
        {
            findings.push(Finding::error(
                "invalid-status",
                path,
                format!("invalid status '{status}' (expected draft/review/stable)"),
            ));
        }
    }

    // Check the raw <-> wiki mapping, derived from what each article cites.
    let compilation = Compilation::derive(articles, manifest);
    for (article, source) in &compilation.unresolved {
        findings.push(Finding::error(
            "unresolved-source",
            article.clone(),
            format!("source '{source}' matches no raw document in the manifest"),
        ));
    }
    for entry in compilation.uncompiled(manifest) {
        findings.push(Finding::warning(
            "uncompiled-source",
            entry.raw_path.clone(),
            format!("not yet compiled into any wiki article ({})", entry.title),
        ));
    }

    sort(&mut findings);
    findings
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
