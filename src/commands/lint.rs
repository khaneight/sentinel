use std::collections::{BTreeMap, HashSet};
use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::compilation::Compilation;
use crate::core::links;
use crate::core::lint::{self, Finding, Severity};
use crate::core::manifest::Manifest;
use crate::core::output;
use crate::core::wiki;

#[derive(Serialize)]
struct Report {
    errors: usize,
    warnings: usize,
    findings: Vec<Finding>,
}

/// Validate the archive. Returns the process exit code.
pub fn run(strict: bool) -> io::Result<i32> {
    let articles = wiki::load_all()?;
    let mut findings: Vec<Finding> = Vec::new();

    let all_slugs: HashSet<String> = articles.iter().map(|a| a.slug()).collect();

    // A wikilink names a slug, and a slug is just a filename stem. Two articles
    // sharing one across domains collapse into a single node in the link graph:
    // backlinks merge and one article's forward links overwrite the other's.
    // Nothing else in the pipeline notices, so it has to be caught here.
    let mut slug_owners: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for article in &articles {
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

    for loaded in &articles {
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
    let manifest = Manifest::load()?;
    let compilation = Compilation::derive(&articles, &manifest);

    for (article, source) in &compilation.unresolved {
        findings.push(Finding::error(
            "unresolved-source",
            article.clone(),
            format!("source '{source}' matches no raw document in the manifest"),
        ));
    }
    for entry in compilation.uncompiled(&manifest) {
        findings.push(Finding::warning(
            "uncompiled-source",
            entry.raw_path.clone(),
            format!("not yet compiled into any wiki article ({})", entry.title),
        ));
    }

    lint::sort(&mut findings);
    let errors = lint::count(&findings, Severity::Error);
    let warnings = lint::count(&findings, Severity::Warning);

    crate::core::log::append("lint", &format!("{errors} error(s), {warnings} warning(s)"))?;

    if output::is_json() {
        output::emit(
            "lint",
            Report {
                errors,
                warnings,
                findings,
            },
        )?;
    } else {
        report_human(&findings, errors, warnings);
    }

    // Exit non-zero only for things that are actually wrong. An archive with
    // uncompiled sources and forward-declared wikilinks is healthy, and a lint
    // that fails on it would be one nobody could gate on.
    let failing = if strict { errors + warnings } else { errors };
    Ok(if failing > 0 {
        output::EXIT_FINDINGS
    } else {
        0
    })
}

fn report_human(findings: &[Finding], errors: usize, warnings: usize) {
    if findings.is_empty() {
        println!("{}", "No issues found.".green());
        return;
    }

    println!(
        "{} error(s), {} warning(s):\n",
        errors.to_string().red(),
        warnings.to_string().yellow()
    );

    for finding in findings {
        let tag = match finding.severity {
            Severity::Error => finding.severity.label().red(),
            Severity::Warning => finding.severity.label().yellow(),
        };
        let location = finding
            .path
            .as_deref()
            .map(|p| format!("{}: ", p.cyan()))
            .unwrap_or_default();
        println!(
            "  {tag} {}{}  {}",
            location,
            finding.message,
            format!("[{}]", finding.rule).dimmed()
        );
    }
}
