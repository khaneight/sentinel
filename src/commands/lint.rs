use std::collections::{BTreeMap, HashSet};
use std::io;

use colored::Colorize;
use walkdir::WalkDir;

use crate::core::frontmatter;
use crate::core::links;
use crate::core::manifest::Manifest;
use crate::core::paths;

pub fn run() -> io::Result<()> {
    let wiki_dir = paths::wiki_dir();
    if !wiki_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Wiki directory not found. Run `sentinel init` first.",
        ));
    }

    let mut issues: Vec<String> = Vec::new();

    // Collect all wiki article slugs
    let mut all_slugs: HashSet<String> = HashSet::new();
    let mut slug_owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut all_articles = Vec::new();

    for entry in WalkDir::new(&wiki_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let path = entry.path();
        let rel_path = paths::rel(path);

        let slug = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        all_slugs.insert(slug.clone());
        slug_owners
            .entry(slug.clone())
            .or_default()
            .push(rel_path.clone());

        if let Ok(article) = frontmatter::parse_file(path, &rel_path) {
            all_articles.push((slug, article, path.to_path_buf()));
        }
    }

    // A wikilink names a slug, and a slug is just a filename stem. Two articles
    // sharing one across domains collapse into a single node in the link graph:
    // backlinks merge and one article's forward links overwrite the other's.
    // Nothing else in the pipeline notices, so it has to be caught here.
    for (slug, owners) in &slug_owners {
        if owners.len() > 1 {
            issues.push(format!(
                "duplicate slug '{slug}': {} — [[{slug}]] is ambiguous and the link graph merges them",
                owners.join(", ")
            ));
        }
    }

    // Check each article
    for (slug, article, path) in &all_articles {
        let display = path.display();

        // Check for broken wikilinks. Done first because the body is readable
        // even when the frontmatter is not.
        let content = std::fs::read_to_string(path).unwrap_or_default();
        for link in links::extract_wikilinks(&content) {
            if !all_slugs.contains(link.as_str()) {
                issues.push(format!(
                    "{display}: broken link [[{link}]] — no matching article found"
                ));
            }
        }

        // Malformed YAML yields default frontmatter, so every field check below
        // would fire at once and point the reader at five imaginary problems
        // instead of the one real one.
        if let Some(error) = &article.frontmatter_error {
            issues.push(format!("{display}: invalid frontmatter — {error}"));
            continue;
        }

        // Check required frontmatter fields
        if article.frontmatter.title.is_none() {
            issues.push(format!("{display}: missing 'title' in frontmatter"));
        }
        if article.frontmatter.domain.is_none() {
            issues.push(format!("{display}: missing 'domain' in frontmatter"));
        }
        if article.frontmatter.origin.is_none() {
            issues.push(format!("{display}: missing 'origin' in frontmatter"));
        }
        if article.frontmatter.tags.is_empty() {
            issues.push(format!("{display}: no tags defined"));
        }
        if article.frontmatter.sources.is_empty() {
            issues.push(format!("{display}: no sources listed"));
        }

        // Check origin value
        if let Some(origin) = &article.frontmatter.origin {
            if !["authored", "researched", "hybrid"].contains(&origin.as_str()) {
                issues.push(format!(
                    "{display}: invalid origin '{origin}' (expected authored/researched/hybrid)"
                ));
            }
        }

        // Check status value
        if let Some(status) = &article.frontmatter.status {
            if !["draft", "review", "stable"].contains(&status.as_str()) {
                issues.push(format!(
                    "{display}: invalid status '{status}' (expected draft/review/stable)"
                ));
            }
        }

        let _ = slug; // used for orphan detection via graph
    }

    // Check for raw docs without wiki mappings
    let manifest = Manifest::load()?;
    let uncompiled = manifest.uncompiled();
    for entry in &uncompiled {
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
