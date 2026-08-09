use std::io;
use std::path::Path;

use colored::Colorize;
use walkdir::{DirEntry, WalkDir};

use crate::core::manifest::{Manifest, ManifestEntry};
use crate::core::paths;

/// Reconcile the manifest with what is actually on disk under `raw/`.
///
/// Registers files the manifest has never seen and drops entries whose source
/// file is gone. Without the second half the manifest only ever grows: a raw
/// document deleted by hand stays "uncompiled" forever, permanently skewing
/// `sentinel status`, `sentinel uncompiled`, and `sentinel lint`.
pub fn run(dry_run: bool) -> io::Result<()> {
    let raw_dir = paths::raw_dir();
    if !raw_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "raw/ directory not found. Run `sentinel init` first.",
        ));
    }

    let mut manifest = Manifest::load()?;
    let mut added = 0;
    let mut removed = 0;

    for entry in WalkDir::new(&raw_dir)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let rel_path = paths::rel(path);

        // Skip if already in manifest
        if manifest.entries.contains_key(&rel_path) {
            continue;
        }

        // Infer domain from directory structure: raw/{domain}/filename
        let domain = path
            .strip_prefix(&raw_dir)
            .ok()
            .and_then(|p| p.components().next())
            .and_then(|c| c.as_os_str().to_str())
            .unwrap_or("uncategorized")
            .to_string();

        // Infer title from filename
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();

        manifest.upsert(ManifestEntry {
            raw_path: rel_path.clone(),
            title: title.clone(),
            domain,
            origin: "authored".to_string(),
            ingested_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            wiki_articles: vec![],
            source_type: infer_source_type(path),
        });

        println!("  {} {rel_path}", "+".green());
        added += 1;
    }

    let root = paths::archive_root();
    let orphaned: Vec<String> = manifest
        .entries
        .keys()
        .filter(|rel| !root.join(rel).exists())
        .cloned()
        .collect();

    for rel_path in orphaned {
        println!("  {} {rel_path}", "-".red());
        manifest.entries.remove(&rel_path);
        removed += 1;
    }

    if dry_run {
        println!(
            "\n{} {added} to add, {removed} to remove. Nothing written.",
            "Dry run:".yellow()
        );
        return Ok(());
    }

    manifest.save()?;

    if added == 0 && removed == 0 {
        println!("{}", "Manifest is already in sync.".green());
    } else {
        let summary = format!("{added} added, {removed} removed");
        crate::core::log::append("sync", &summary)?;
        println!("\nSynced: {}.", summary.green());
    }

    Ok(())
}

/// Editor scratch files, `.DS_Store`, and VCS metadata are not source documents.
fn is_hidden(entry: &DirEntry) -> bool {
    entry.depth() > 0
        && entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'))
}

fn infer_source_type(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("md" | "txt" | "org") => "document",
        Some("pdf") => "document",
        Some("png" | "jpg" | "jpeg" | "gif" | "svg") => "image",
        _ => "document",
    }
    .to_string()
}
