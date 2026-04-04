use std::io;

use colored::Colorize;
use walkdir::WalkDir;

use crate::core::links::LinkGraph;
use crate::core::manifest::Manifest;
use crate::core::paths;

pub fn run() -> io::Result<()> {
    let root = paths::archive_root();
    if !root.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Archive not initialized. Run `sentinel init` first.",
        ));
    }

    let manifest = Manifest::load()?;

    // Count raw docs
    let raw_count = manifest.count();
    let uncompiled_count = manifest.uncompiled().len();

    // Count wiki articles
    let wiki_count = count_md_files(&paths::wiki_dir());

    // Count domains with content
    let raw_domains = count_nonempty_subdirs(&paths::raw_dir());
    let wiki_domains = count_nonempty_subdirs(&paths::wiki_dir());

    // Load link graph for orphan count
    let graph = LinkGraph::load().unwrap_or_default();
    let orphan_count = if graph.forward.is_empty() {
        0
    } else {
        let all_slugs: std::collections::HashSet<String> =
            graph.forward.keys().cloned().collect();
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

    Ok(())
}

fn count_md_files(dir: &std::path::Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .is_some_and(|ext| ext == "md")
        })
        .count()
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
                .filter(|e| {
                    std::fs::read_dir(e.path())
                        .is_ok_and(|mut d| d.next().is_some())
                })
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
