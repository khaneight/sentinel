use std::io;

use colored::Colorize;

use crate::core::compilation::Compilation;
use crate::core::links::LinkGraph;
use crate::core::manifest::Manifest;
use crate::core::paths;
use crate::core::wiki;

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

    // Count raw docs. Compilation status is derived from what the wiki cites,
    // so it stays correct even if `sentinel index` has not been run since the
    // last article was written.
    let raw_count = manifest.count();
    let compilation = Compilation::derive(&articles, &manifest);
    let uncompiled_count = compilation.uncompiled(&manifest).len();
    let unresolved_count = compilation.unresolved.len();

    let wiki_count = articles.len();

    // Count domains with content
    let raw_domains = count_nonempty_subdirs(&paths::raw_dir());
    let wiki_domains = count_nonempty_subdirs(&paths::wiki_dir());

    // Load link graph for orphan count
    let graph = LinkGraph::load().unwrap_or_default();
    let orphan_count = if graph.forward.is_empty() {
        0
    } else {
        let all_slugs: std::collections::HashSet<String> = graph.forward.keys().cloned().collect();
        graph.orphans(&all_slugs).len()
    };

    println!("{}", "Archive Status".bold());
    println!("─────────────────────────────");
    println!("  Raw documents:   {}", raw_count.to_string().cyan());
    println!("  Wiki articles:   {}", wiki_count.to_string().green());
    println!("  Uncompiled:      {}", format_count(uncompiled_count));
    println!("  Orphan pages:    {}", format_count(orphan_count));
    println!("  Raw domains:     {raw_domains}");
    println!("  Wiki domains:    {wiki_domains}");
    if unresolved_count > 0 {
        println!(
            "\n  {} {unresolved_count} source citation(s) match no raw document — run `sentinel lint`",
            "!".yellow()
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
