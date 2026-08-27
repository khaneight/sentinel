use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use crate::core::frontmatter;
use crate::core::manifest::{self, Manifest, ManifestEntry};
use crate::core::output;
use crate::core::paths;
use crate::core::slug;

pub fn run(
    path: &str,
    domain: &str,
    origin: &str,
    title: Option<&str>,
    filename: Option<&str>,
    publish: bool,
) -> io::Result<()> {
    let source = Path::new(path);
    if !source.exists() {
        // A relative path resolves against the working directory, not the
        // archive — easy to get wrong when the two are far apart, which is the
        // normal arrangement once `--set-default` is in use.
        let resolved = std::env::current_dir()
            .map(|cwd| cwd.join(path).display().to_string())
            .unwrap_or_else(|_| path.to_string());
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "File not found: {path}\n\
                 Looked in {resolved} — paths are relative to the working \
                 directory, not to the archive."
            ),
        ));
    }

    // Validated against the same constant `sentinel schema` publishes and the
    // lint rule enforces. A private copy of this list is how `hybrid` came to
    // be advertised as valid and rejected here.
    // The *ingestable* set, not every origin an article can carry. A raw
    // document is never `extrapolated`: `raw/` is the provenance floor, and a
    // generated file sitting in it could later be cited as evidence for what
    // its supposed author believes.
    if !frontmatter::INGESTABLE_ORIGINS.contains(&origin) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Unknown origin '{origin}'. Expected one of: {}.\n\
                 Run `sentinel schema` for the full frontmatter contract.",
                frontmatter::INGESTABLE_ORIGINS.join(", ")
            ),
        ));
    }

    // Before anything is created. `-d "/tmp/x"` used to write there, because
    // `Path::join` on an absolute path discards the base; `-d "../.."` wrote
    // above the archive root; and both recorded the traversal verbatim in the
    // manifest as `raw/../../x.md`. All three exited 0.
    let domain = &paths::archive_component("domain", domain)?;

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

    let dest_name = paths::archive_component("filename", &dest_name)?;
    let dest = domain_dir.join(&dest_name);
    if dest.exists() {
        // On a case-insensitive filesystem the clashing file may be listed
        // under different capitalisation, so naming the path that exists is
        // more use than echoing the one that was asked for.
        let existing = std::fs::canonicalize(&dest)
            .map(|p| paths::rel(&p))
            .unwrap_or_else(|_| paths::rel(&dest));
        let note = if existing.rsplit('/').next() != Some(dest_name.as_str()) {
            format!(
                "\nYou asked for `{dest_name}`; this filesystem does not \
                 distinguish filenames by case."
            )
        } else {
            String::new()
        };
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "File already exists: {existing}{note}\n\
                 Give it a different name with `--as <FILENAME>`, or a different \
                 title with `--title` (which is used to derive the name).",
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
        content_hash: manifest::hash_file(&dest),
        publish,
    });
    // A save conflict must not leave the copied file behind unregistered:
    // `sync` would adopt it as `authored`, which is the #16 provenance loss.
    if let Err(e) = manifest.save() {
        let _ = fs::remove_file(&dest);
        return Err(e);
    }

    crate::core::log::append("ingest", &format!("{display_title} → {rel_path}"))?;

    if output::is_json() {
        #[derive(Serialize)]
        struct Ingested {
            raw_path: String,
            title: String,
            domain: String,
            origin: String,
        }
        return output::emit(
            "ingest",
            Ingested {
                raw_path: rel_path,
                title: display_title.to_string(),
                domain: domain.to_string(),
                origin: origin.to_string(),
            },
        );
    }

    println!("Ingested: {rel_path}");
    println!("  title:  {display_title}");
    println!("  domain: {domain}");
    println!("  origin: {origin}");

    Ok(())
}
