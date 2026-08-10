use std::collections::BTreeMap;
use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::compilation::Compilation;
use crate::core::links::{self, LinkGraph};
use crate::core::manifest::Manifest;
use crate::core::output;
use crate::core::paths;
use crate::core::wiki;

#[derive(Serialize)]
pub struct Status {
    raw_documents: usize,
    wiki_articles: usize,
    uncompiled: usize,
    orphan_pages: usize,
    unresolved_sources: usize,
    raw_domains: usize,
    wiki_domains: usize,
    /// Article counts by `status`. Nothing in the tool promotes an article, and
    /// `next` only surfaces a draft once it has gone stale — so an archive can
    /// be complete by every other measure while nothing in it has been
    /// reviewed, and report "nothing outstanding".
    maturity: BTreeMap<String, usize>,
    /// Files under wiki/ that could not be read. Every count above is computed
    /// without them, so a non-zero value means the whole report is partial.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unreadable: Vec<wiki::Unreadable>,
    /// Set when the link graph exists but could not be parsed, in which case
    /// `orphan_pages` above is 0 because nothing could be counted.
    #[serde(skip_serializing_if = "Option::is_none")]
    link_graph_error: Option<String>,
    /// Set when the graph parsed fine but no longer matches disk. Distinct from
    /// the error above: the count is real, it just describes an older archive.
    #[serde(skip_serializing_if = "Option::is_none")]
    link_graph_stale: Option<String>,
}

pub fn run() -> io::Result<()> {
    let status = summarize()?;
    if output::is_json() {
        return output::emit("status", status);
    }
    report_human(&status);
    Ok(())
}

/// The status report, as a value.
///
/// Split out for the same reason as `next::recommend`: `sentinel index` puts
/// these counts in the dashboard, and computing them a second time is how two
/// surfaces come to disagree about how many articles an archive has.
pub fn summarize() -> io::Result<Status> {
    let root = paths::archive_root();
    if !root.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Archive not initialized. Run `sentinel init` first.",
        ));
    }

    let manifest = Manifest::load()?;
    let loaded = wiki::load_all()?;
    let articles = loaded.articles;

    // Compilation status is derived from what the wiki cites, so it stays
    // correct even if `sentinel index` has not been run since the last article
    // was written.
    let compilation = Compilation::derive(&articles, &manifest);

    // Load link graph for orphan count
    let (graph, graph_error) = match LinkGraph::load() {
        Ok(graph) => (graph, None),
        Err(e) => (LinkGraph::default(), Some(links::corrupt_graph_note(&e))),
    };
    // Same trap as `next`: an unbuilt graph reported "0 orphans", which reads
    // as a healthy archive rather than one nothing has looked at yet.
    let graph_stale = links::staleness(&graph, &articles).note();
    let orphan_pages = if graph.forward.is_empty() {
        0
    } else {
        let all_slugs: std::collections::HashSet<String> = graph.forward.keys().cloned().collect();
        graph.orphans(&all_slugs).len()
    };

    let status = Status {
        raw_documents: manifest.count(),
        wiki_articles: articles.len(),
        uncompiled: compilation.uncompiled(&manifest).len(),
        orphan_pages,
        unresolved_sources: compilation.unresolved.len(),
        raw_domains: count_nonempty_subdirs(&paths::raw_dir()),
        wiki_domains: count_nonempty_subdirs(&paths::wiki_dir()),
        maturity: {
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for a in &articles {
                let status = a
                    .article
                    .frontmatter
                    .status
                    .clone()
                    .unwrap_or_else(|| "unset".to_string());
                *counts.entry(status).or_default() += 1;
            }
            counts
        },
        unreadable: loaded.unreadable,
        link_graph_error: graph_error,
        link_graph_stale: graph_stale,
    };

    Ok(status)
}

fn report_human(status: &Status) {
    println!("{}", "Archive Status".bold());
    println!("─────────────────────────────");
    println!(
        "  Raw documents:   {}",
        status.raw_documents.to_string().cyan()
    );
    println!(
        "  Wiki articles:   {}",
        status.wiki_articles.to_string().green()
    );
    println!("  Uncompiled:      {}", format_count(status.uncompiled));
    println!("  Orphan pages:    {}", format_count(status.orphan_pages));
    println!("  Raw domains:     {}", status.raw_domains);
    println!("  Wiki domains:    {}", status.wiki_domains);
    if !status.maturity.is_empty() {
        let summary = status
            .maturity
            .iter()
            .map(|(k, v)| format!("{v} {k}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  Maturity:        {summary}");
    }
    if let Some(note) = &status.link_graph_error {
        println!("\n  {} {note}", "!".red());
        println!("      orphan count above is 0 because none could be counted.");
    }
    if let Some(note) = &status.link_graph_stale {
        println!("\n  {} {note}", "!".yellow());
    }
    wiki::warn_partial(&status.unreadable, "every count above excludes them");
    if status.unresolved_sources > 0 {
        println!(
            "\n  {} {} source citation(s) match no raw document — run `sentinel lint`",
            "!".yellow(),
            status.unresolved_sources
        );
    }
}

fn count_nonempty_subdirs(dir: &std::path::Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter(|e| std::fs::read_dir(e.path()).is_ok_and(|mut d| d.next().is_some()))
                .count()
        })
        .unwrap_or(0)
}

fn format_count(count: usize) -> String {
    if count > 0 {
        count.to_string().yellow().to_string()
    } else {
        count.to_string().green().to_string()
    }
}
