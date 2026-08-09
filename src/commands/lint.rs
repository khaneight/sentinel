use std::collections::{BTreeMap, HashSet};
use std::io;

use colored::Colorize;

use crate::core::compilation::Compilation;
use crate::core::links;
use crate::core::manifest::Manifest;
use crate::core::wiki;

pub fn run() -> io::Result<()> {
    let articles = wiki::load_all()?;

    let mut issues: Vec<String> = Vec::new();

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
            issues.push(format!(
                "duplicate slug '{slug}': {} — [[{slug}]] is ambiguous and the link graph merges them",
                owners.join(", ")
            ));
        }
    }

    for loaded in &articles {
        let display = loaded.rel_path();
        let frontmatter = &loaded.article.frontmatter;

        // Check for broken wikilinks. Done first because the body is readable
        // even when the frontmatter is not.
        for link in links::extract_wikilinks(&loaded.content) {
            if !all_slugs.contains(link.as_str()) {
                issues.push(format!(
                    "{display}: broken link [[{link}]] — no matching article found"
                ));
            }
        }

        // Malformed YAML yields default frontmatter, so every field check below
        // would fire at once and point the reader at five imaginary problems
        // instead of the one real one.
        if let Some(error) = &loaded.article.frontmatter_error {
            issues.push(format!("{display}: invalid frontmatter — {error}"));
            continue;
        }

        // Check required frontmatter fields
        if frontmatter.title.is_none() {
            issues.push(format!("{display}: missing 'title' in frontmatter"));
        }
        if frontmatter.domain.is_none() {
            issues.push(format!("{display}: missing 'domain' in frontmatter"));
        }
        if frontmatter.origin.is_none() {
            issues.push(format!("{display}: missing 'origin' in frontmatter"));
        }
        if frontmatter.tags.is_empty() {
            issues.push(format!("{display}: no tags defined"));
        }
        if frontmatter.sources.is_empty() {
            // Without a source citation an article can never be linked back to
            // the raw document it came from, so its source stays "uncompiled".
            issues.push(format!(
                "{display}: no sources listed — its raw document will stay uncompiled"
            ));
        }

        // Check origin value
        if let Some(origin) = &frontmatter.origin
            && !["authored", "researched", "hybrid"].contains(&origin.as_str())
        {
            issues.push(format!(
                "{display}: invalid origin '{origin}' (expected authored/researched/hybrid)"
            ));
        }

        // Check status value
        if let Some(status) = &frontmatter.status
            && !["draft", "review", "stable"].contains(&status.as_str())
        {
            issues.push(format!(
                "{display}: invalid status '{status}' (expected draft/review/stable)"
            ));
        }
    }

    // Check the raw <-> wiki mapping, derived from what each article cites.
    let manifest = Manifest::load()?;
    let compilation = Compilation::derive(&articles, &manifest);

    for (article, source) in &compilation.unresolved {
        issues.push(format!(
            "{article}: source '{source}' matches no raw document in the manifest"
        ));
    }
    for entry in compilation.uncompiled(&manifest) {
        issues.push(format!(
            "Uncompiled raw doc: {} ({})",
            entry.raw_path, entry.title
        ));
    }

    crate::core::log::append("lint", &format!("{} issues found", issues.len()))?;

    // Report
    if issues.is_empty() {
        println!("{}", "No issues found.".green());
    } else {
        println!("{} issue(s) found:\n", issues.len().to_string().yellow());
        for issue in &issues {
            println!("  {} {issue}", "•".red());
        }
    }

    Ok(())
}
