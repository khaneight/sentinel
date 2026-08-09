use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::compilation::Compilation;
use crate::core::links::LinkGraph;
use crate::core::manifest::Manifest;
use crate::core::output;
use crate::core::paths;
use crate::core::wiki;

#[derive(Serialize)]
struct Status {
    raw_documents: usize,
    wiki_articles: usize,
    uncompiled: usize,
    orphan_pages: usize,
    unresolved_sources: usize,
    raw_domains: usize,
    wiki_domains: usize,
}

pub fn run() -> io::Result<()> {
    let root = paths::archive_root();
    if !root.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Archive not initialized. Run `sentinel init` first.",
        ));
    }

    let manifest = Manifest::load()?;
    let articles = wiki::load_all().unwrap_or_default();

    // Compilation status is derived from what the wiki cites, so it stays
    // correct even if `sentinel index` has not been run since the last article
    // was written.
    let compilation = Compilation::derive(&articles, &manifest);

    // Load link graph for orphan count
    let graph = LinkGraph::load().unwrap_or_default();
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
    };

    if output::is_json() {
        return output::emit("status", status);
    }

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
    if status.unresolved_sources > 0 {
        println!(
            "\n  {} {} source citation(s) match no raw document — run `sentinel lint`",
            "!".yellow(),
            status.unresolved_sources
        );
    }

    Ok(())
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
