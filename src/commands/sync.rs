use std::io;
use std::path::Path;

use colored::Colorize;
use walkdir::WalkDir;

use crate::core::manifest::{Manifest, ManifestEntry};
use crate::core::paths;

/// Scan raw/ for files not in the manifest and register them.
pub fn run() -> io::Result<()> {
    let raw_dir = paths::raw_dir();
    if !raw_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "raw/ directory not found. Run `sentinel init` first.",
        ));
    }

    let mut manifest = Manifest::load()?;
    let mut added = 0;

    for entry in WalkDir::new(&raw_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let rel_path = path
            .strip_prefix(paths::archive_root())
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

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

    manifest.save()?;

    if added == 0 {
        println!("{}", "Manifest is already in sync.".green());
    } else {
        println!(
            "\nSynced {} new file(s) into manifest.",
            added.to_string().green()
        );
    }

    Ok(())
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
