use std::fs;
use std::io;
use std::path::Path;

use crate::core::manifest::{Manifest, ManifestEntry};
use crate::core::paths;
use crate::core::slug;

pub fn run(
    path: &str,
    domain: &str,
    origin: &str,
    title: Option<&str>,
    filename: Option<&str>,
) -> io::Result<()> {
    let source = Path::new(path);
    if !source.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("File not found: {path}"),
        ));
    }

    // Validate origin
    if !["authored", "researched"].contains(&origin) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Origin must be 'authored' or 'researched'",
        ));
    }

    // Ensure domain directory exists
    let domain_dir = paths::raw_domain_dir(domain);
    fs::create_dir_all(&domain_dir)?;

    // Determine filename and title
    let source_name = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid filename"))?;

    let display_title = title.unwrap_or_else(|| {
        Path::new(source_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(source_name)
    });

    // Destination name: --as wins, else the title's slug when a title was
    // given, else the source basename.
    //
    // Basenames repeat constantly in real corpora — SKILL.md, README.md,
    // index.md, chapter-1.md under per-book directories, exported notes. With
    // only the basename available, ingesting the second one failed and there
    // was no flag to resolve it, so a whole class of source collection could
    // not be ingested at all.
    let extension = Path::new(source_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("md");
    let dest_name = match (filename, title) {
        (Some(explicit), _) => explicit.to_string(),
        (None, Some(title)) => format!("{}.{extension}", slug::canonical(title)),
        (None, None) => source_name.to_string(),
    };

    let dest = domain_dir.join(&dest_name);
    if dest.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "File already exists: {}\n\
                 Give it a different name with `--as <FILENAME>`, or a different \
                 title with `--title` (which is used to derive the name).",
                dest.display()
            ),
        ));
    }
    fs::copy(source, &dest)?;

    // Build relative path from archive root
    let rel_path = format!("raw/{domain}/{dest_name}");

    // Update manifest
    let mut manifest = Manifest::load()?;
    manifest.upsert(ManifestEntry {
        raw_path: rel_path.clone(),
        title: display_title.to_string(),
        domain: domain.to_string(),
        origin: origin.to_string(),
        ingested_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        wiki_articles: vec![],
        source_type: "document".to_string(),
    });
    manifest.save()?;

    crate::core::log::append("ingest", &format!("{display_title} → {rel_path}"))?;

    println!("Ingested: {rel_path}");
    println!("  title:  {display_title}");
    println!("  domain: {domain}");
    println!("  origin: {origin}");

    Ok(())
}
