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

/// What a lint rule checks, for `sentinel schema`.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RuleInfo {
    pub rule: &'static str,
    pub severity: Severity,
    pub description: &'static str,
}

/// Every rule `analyze` can emit.
///
/// Published by `sentinel schema` so a skill can be written against the real
/// rule set instead of restating it in prose that drifts. Tests assert this
/// list and `analyze` agree in both directions, on rule ids and severities.
pub const RULES: &[RuleInfo] = &[
    RuleInfo {
        rule: "invalid-frontmatter",
        severity: Severity::Error,
        description: "The `---` block is present but is not valid YAML.",
    },
    RuleInfo {
        rule: "missing-field",
        severity: Severity::Error,
        description: "A required frontmatter field (title, domain, origin) is absent. These drive the generated indexes.",
    },
    RuleInfo {
        rule: "invalid-origin",
        severity: Severity::Error,
        description: "`origin` is not one of authored, researched, hybrid.",
    },
    RuleInfo {
        rule: "invalid-status",
        severity: Severity::Error,
        description: "`status` is not one of draft, review, stable.",
    },
    RuleInfo {
        rule: "duplicate-slug",
        severity: Severity::Error,
        description: "Two articles share a filename stem, so [[wikilinks]] to it are ambiguous and the link graph merges them.",
    },
    RuleInfo {
        rule: "unresolved-source",
        severity: Severity::Error,
        description: "A `sources:` entry matches no raw document in the manifest, or matches more than one ambiguously.",
    },
    RuleInfo {
        rule: "broken-link",
        severity: Severity::Warning,
        description: "A [[wikilink]] target has no article yet. Expected: the compile workflow links concepts before writing them, and `sentinel next` ranks these as the articles most worth writing.",
    },
    RuleInfo {
        rule: "missing-tags",
        severity: Severity::Warning,
        description: "No `tags:` defined.",
    },
    RuleInfo {
        rule: "missing-sources",
        severity: Severity::Warning,
        description: "No `sources:` listed, so the raw document this came from will stay uncompiled.",
    },
    RuleInfo {
        rule: "uncompiled-source",
        severity: Severity::Warning,
        description: "A raw document that no wiki article cites in its `sources:`.",
    },
];

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
            && !super::frontmatter::ORIGINS.contains(&origin.as_str())
        {
            findings.push(Finding::error(
                "invalid-origin",
                path,
                format!("invalid origin '{origin}' (expected authored/researched/hybrid)"),
            ));
        }

        if let Some(status) = &frontmatter.status
            && !super::frontmatter::STATUSES.contains(&status.as_str())
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

    use crate::core::frontmatter::{Frontmatter, WikiArticle};
    use crate::core::manifest::ManifestEntry;
    use std::collections::BTreeSet;

    fn loaded(path: &str, fm: Frontmatter, content: &str, error: Option<&str>) -> LoadedArticle {
        LoadedArticle {
            article: WikiArticle {
                frontmatter: fm,
                body: content.to_string(),
                rel_path: path.to_string(),
                frontmatter_error: error.map(ToString::to_string),
            },
            path: std::path::PathBuf::from(path),
            content: content.to_string(),
        }
    }

    fn complete(sources: &[&str]) -> Frontmatter {
        Frontmatter {
            title: Some("T".into()),
            domain: Some("philosophy".into()),
            origin: Some("authored".into()),
            tags: vec!["t".into()],
            sources: sources.iter().map(|s| (*s).to_string()).collect(),
            status: Some("draft".into()),
            ..Default::default()
        }
    }

    /// An archive rigged to trip every rule at once.
    fn everything_wrong() -> (Vec<LoadedArticle>, Manifest) {
        let mut manifest = Manifest::default();
        for raw in ["raw/philosophy/cited.md", "raw/philosophy/stranded.md"] {
            manifest.upsert(ManifestEntry {
                raw_path: raw.to_string(),
                title: "T".into(),
                domain: "philosophy".into(),
                origin: "authored".into(),
                ingested_at: "2026-01-01 00:00:00".into(),
                wiki_articles: vec![],
                source_type: "document".into(),
            });
        }

        let articles = vec![
            // invalid-frontmatter
            loaded(
                "wiki/a/broken.md",
                Frontmatter::default(),
                "x",
                Some("bad yaml"),
            ),
            // missing-field (title/domain/origin) + missing-tags + missing-sources
            loaded("wiki/a/bare.md", Frontmatter::default(), "x", None),
            // invalid-origin + invalid-status
            loaded(
                "wiki/a/enums.md",
                Frontmatter {
                    origin: Some("nonsense".into()),
                    status: Some("nonsense".into()),
                    ..complete(&["raw/philosophy/cited.md"])
                },
                "x",
                None,
            ),
            // broken-link + unresolved-source
            loaded(
                "wiki/a/links.md",
                complete(&["raw/philosophy/nowhere.md"]),
                "See [[not-written]].",
                None,
            ),
            // duplicate-slug: same stem, different domain
            loaded(
                "wiki/b/dup.md",
                complete(&["raw/philosophy/cited.md"]),
                "x",
                None,
            ),
            loaded(
                "wiki/c/dup.md",
                complete(&["raw/philosophy/cited.md"]),
                "x",
                None,
            ),
        ];
        (articles, manifest)
    }

    #[test]
    fn every_documented_rule_can_actually_fire() {
        let (articles, manifest) = everything_wrong();
        let emitted: BTreeSet<&str> = analyze(&articles, &manifest)
            .iter()
            .map(|f| f.rule)
            .collect();
        let documented: BTreeSet<&str> = RULES.iter().map(|r| r.rule).collect();

        let never_fires: Vec<&&str> = documented.difference(&emitted).collect();
        assert!(
            never_fires.is_empty(),
            "RULES documents rules that analyze() never emits: {never_fires:?}"
        );
    }

    #[test]
    fn every_emitted_rule_is_documented() {
        let (articles, manifest) = everything_wrong();
        let documented: BTreeSet<&str> = RULES.iter().map(|r| r.rule).collect();
        for finding in analyze(&articles, &manifest) {
            assert!(
                documented.contains(finding.rule),
                "rule '{}' is emitted but missing from RULES, so `sentinel schema` under-reports it",
                finding.rule
            );
        }
    }

    #[test]
    fn documented_severity_matches_emitted_severity() {
        let (articles, manifest) = everything_wrong();
        for finding in analyze(&articles, &manifest) {
            let documented = RULES
                .iter()
                .find(|r| r.rule == finding.rule)
                .unwrap_or_else(|| panic!("undocumented rule {}", finding.rule));
            assert_eq!(
                documented.severity, finding.severity,
                "rule '{}' is documented as {:?} but emitted as {:?}",
                finding.rule, documented.severity, finding.severity
            );
        }
    }

    #[test]
    fn rule_ids_are_unique() {
        let unique: BTreeSet<&str> = RULES.iter().map(|r| r.rule).collect();
        assert_eq!(unique.len(), RULES.len());
    }

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
