use std::fs;
use std::io;
use std::path::Path;

use crate::core::manifest::{Manifest, ManifestEntry};
use crate::core::paths;

pub fn run(path: &str, domain: &str, origin: &str, title: Option<&str>) -> io::Result<()> {
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
    let filename = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid filename"))?;

    let display_title = title.unwrap_or_else(|| {
        Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(filename)
    });

    // Copy file to raw/{domain}/
    let dest = domain_dir.join(filename);
    if dest.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("File already exists: {}", dest.display()),
        ));
    }
    fs::copy(source, &dest)?;

    // Build relative path from archive root
    let rel_path = format!("raw/{domain}/{filename}");

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
