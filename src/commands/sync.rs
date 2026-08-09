use std::collections::HashMap;
use std::io;
use std::path::Path;

use colored::Colorize;
use walkdir::{DirEntry, WalkDir};

use crate::core::manifest::{self, Manifest, ManifestEntry};
use crate::core::paths;

/// Reconcile the manifest with what is actually on disk under `raw/`.
///
/// Registers files the manifest has never seen, recognises documents that were
/// renamed or moved by hand, and drops entries whose source is genuinely gone.
pub fn run(dry_run: bool) -> io::Result<()> {
    let raw_dir = paths::raw_dir();
    if !raw_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "raw/ directory not found. Run `sentinel init` first.",
        ));
    }

    let mut manifest = Manifest::load()?;
    let root = paths::archive_root();

    // Everything on disk that the manifest has not seen, with its hash.
    let mut discovered: Vec<(String, Option<String>)> = Vec::new();
    for entry in WalkDir::new(&raw_dir)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let rel_path = paths::rel(entry.path());
        if manifest.entries.contains_key(&rel_path) {
            continue;
        }
        discovered.push((rel_path, manifest::hash_file(entry.path())));
    }

    // Entries whose file is gone.
    let orphaned: Vec<String> = manifest
        .entries
        .keys()
        .filter(|rel| !root.join(rel).exists())
        .cloned()
        .collect();

    // A file renamed by hand looks like a deletion plus an addition. Treating
    // it as such destroys metadata that cannot be recovered from disk — most
    // importantly `origin`, which is the whole authored/researched distinction,
    // and which re-registration silently resets to "authored". Matching on
    // content recognises the move and carries the record across.
    let mut by_hash: HashMap<&str, &str> = HashMap::new();
    for rel in &orphaned {
        if let Some(hash) = manifest.entries[rel].content_hash.as_deref() {
            by_hash.insert(hash, rel.as_str());
        }
    }

    let mut moved: Vec<(String, String)> = Vec::new();
    let mut added: Vec<(String, Option<String>)> = Vec::new();
    for (rel_path, hash) in discovered {
        match hash.as_deref().and_then(|h| by_hash.remove(h)) {
            Some(from) => moved.push((from.to_string(), rel_path)),
            None => added.push((rel_path, hash)),
        }
    }
    let removed: Vec<String> = by_hash.into_values().map(str::to_string).collect();
    // Entries with no recorded hash predate the field and cannot be matched.
    let unmatched: Vec<String> = orphaned
        .iter()
        .filter(|rel| manifest.entries[*rel].content_hash.is_none())
        .filter(|rel| !moved.iter().any(|(from, _)| from == *rel))
        .cloned()
        .collect();
    let removed: Vec<String> = removed.into_iter().chain(unmatched).collect();

    for (from, to) in &moved {
        println!("  {} {from} → {to}", "→".cyan());
    }
    for (rel_path, _) in &added {
        println!("  {} {rel_path}", "+".green());
    }
    for rel_path in &removed {
        let entry = &manifest.entries[rel_path];
        // Say what is being lost. `origin` and `ingested_at` are not derivable
        // from disk, so a wrong prune is not recoverable by re-syncing.
        println!(
            "  {} {rel_path} {}",
            "-".red(),
            format!("(origin: {}, ingested {})", entry.origin, entry.ingested_at).dimmed()
        );
    }

    if dry_run {
        println!(
            "\n{} {} to add, {} moved, {} to remove. Nothing written.",
            "Dry run:".yellow(),
            added.len(),
            moved.len(),
            removed.len()
        );
        return Ok(());
    }

    for (from, to) in &moved {
        if let Some(mut entry) = manifest.entries.remove(from) {
            entry.raw_path = to.clone();
            if let Some(domain) = domain_of(to) {
                entry.domain = domain;
            }
            manifest.upsert(entry);
        }
    }

    for (rel_path, hash) in &added {
        let path = root.join(rel_path);
        let domain = path
            .strip_prefix(&raw_dir)
            .ok()
            .and_then(|p| p.components().next())
            .and_then(|c| c.as_os_str().to_str())
            .unwrap_or("uncategorized")
            .to_string();
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();

        manifest.upsert(ManifestEntry {
            raw_path: rel_path.clone(),
            title,
            domain,
            origin: "authored".to_string(),
            ingested_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            wiki_articles: vec![],
            source_type: infer_source_type(&path),
            content_hash: hash.clone(),
        });
    }

    for rel_path in &removed {
        manifest.entries.remove(rel_path);
    }

    // Backfill hashes so entries written before the field can be matched on a
    // future rename. Without this the fix only protects documents ingested
    // from now on.
    let mut backfilled = 0;
    for entry in manifest.entries.values_mut() {
        if entry.content_hash.is_none()
            && let Some(hash) = manifest::hash_file(&root.join(&entry.raw_path))
        {
            entry.content_hash = Some(hash);
            backfilled += 1;
        }
    }

    manifest.save()?;

    if added.is_empty() && removed.is_empty() && moved.is_empty() {
        println!("{}", "Manifest is already in sync.".green());
        if backfilled > 0 {
            println!("  {} content hash(es) recorded.", backfilled);
        }
    } else {
        let summary = format!(
            "{} added, {} moved, {} removed",
            added.len(),
            moved.len(),
            removed.len()
        );
        crate::core::log::append("sync", &summary)?;
        println!("\nSynced: {}.", summary.green());
    }

    Ok(())
}

/// `raw/<domain>/file.md` → `domain`.
fn domain_of(rel_path: &str) -> Option<String> {
    let mut parts = rel_path.split('/');
    if parts.next()? != "raw" {
        return None;
    }
    let domain = parts.next()?;
    parts.next().is_some().then(|| domain.to_string())
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
