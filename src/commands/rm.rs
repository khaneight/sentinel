use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::compilation::SourceIndex;
use crate::core::manifest::Manifest;
use crate::core::output;
use crate::core::paths;
use crate::core::wiki;

#[derive(Serialize)]
struct Removal {
    removed: String,
    /// Wiki articles that cite it and will be left with an unresolved source.
    orphaned_citations: Vec<String>,
    dry_run: bool,
}

/// Delete a raw document.
///
/// The symmetric case to `mv`, and the more dangerous one: a move can be
/// repaired, a delete cannot. `raw/` is the archive's provenance floor, so
/// removing a document that articles were compiled from breaks the trail back
/// from a claim to its source permanently.
///
/// The whole design here is refusal-first — say what will break, and require
/// the caller to mean it.
pub fn run(target: &str, force: bool, dry_run: bool) -> io::Result<()> {
    let mut manifest = Manifest::load()?;
    let root = paths::archive_root();

    let index = SourceIndex::new(&manifest);
    let key = index.resolve(target).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "No raw document matches '{target}'.\n\
                 Give its archive-relative path (raw/<domain>/<file>), or a \
                 filename unique within the archive. `sentinel uncompiled --json` \
                 lists what is registered."
            ),
        )
    })?;

    // A complete view is required before reporting what cites it: an article
    // that could not be read is one whose citation would not be counted, and
    // this command's entire value is telling the caller what they are about to
    // break.
    let articles = wiki::load_all()?.require_complete()?;
    let citing: Vec<String> = articles
        .iter()
        .filter(|a| {
            a.article
                .frontmatter
                .sources
                .iter()
                .any(|s| index.resolve(s).as_deref() == Some(key.as_str()))
        })
        .map(|a| a.rel_path().to_string())
        .collect();

    let report = Removal {
        removed: key.clone(),
        orphaned_citations: citing.clone(),
        dry_run,
    };

    if !citing.is_empty() && !force {
        let list = citing
            .iter()
            .map(|p| format!("  {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{key} is cited by {} wiki article(s):\n{list}\n\n\
                 Deleting it breaks the provenance trail from those articles to \
                 their source, and nothing can restore it.\n\
                 If the document moved, use `sentinel mv` instead — it repoints \
                 every citation.\n\
                 To delete anyway: `sentinel rm {target} --force`.",
                citing.len()
            ),
        ));
    }

    if dry_run {
        if output::is_json() {
            return output::emit("rm", report);
        }
        println!("{} {key}", "Would remove:".yellow());
        for path in &citing {
            println!("  {} {path} would lose its source", "!".red());
        }
        println!("\n{} Nothing written.", "Dry run:".yellow());
        return Ok(());
    }

    // Remove the file first: if it fails, the manifest still describes reality.
    std::fs::remove_file(root.join(&key))?;
    manifest.entries.remove(&key);
    manifest.save()?;

    crate::core::log::append(
        "rm",
        &format!("{key} removed ({} citation(s) orphaned)", citing.len()),
    )?;

    if output::is_json() {
        return output::emit("rm", report);
    }

    println!("{} {key}", "Removed:".green());
    for path in &citing {
        println!(
            "  {} {path} now cites a source that does not exist",
            "!".red()
        );
    }
    if !citing.is_empty() {
        println!(
            "\n{}",
            "Run `sentinel lint` — those articles now report unresolved-source errors.".dimmed()
        );
    }
    Ok(())
}
