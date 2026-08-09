use std::io;
use std::path::Path;

use colored::Colorize;
use serde::Serialize;

use crate::core::compilation::SourceIndex;
use crate::core::frontmatter;
use crate::core::manifest::Manifest;
use crate::core::output;
use crate::core::paths;
use crate::core::wiki;

#[derive(Serialize)]
struct Rename {
    from: String,
    to: String,
    /// Wiki articles whose `sources:` were rewritten.
    updated_articles: Vec<String>,
    dry_run: bool,
}

/// Move a raw document and repoint every citation to it.
///
/// Reorganising `raw/` is inevitable in a real archive, and doing it by hand
/// breaks provenance: every article citing the old path becomes an
/// `unresolved-source` error. Lint is loud about it, but the repair was manual
/// and easy to do incompletely.
pub fn run(from: &str, to: &str, dry_run: bool) -> io::Result<()> {
    let mut manifest = Manifest::load()?;
    let root = paths::archive_root();

    // Accept whatever spelling the caller has to hand — the same shapes a
    // `sources:` citation may take — so `mv` works from a lint message, a
    // `next` target, or a bare filename.
    let index = SourceIndex::new(&manifest);
    let from_key = index.resolve(from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "No raw document matches '{from}'.\n\
                 Give its archive-relative path (raw/<domain>/<file>), or a \
                 filename unique within the archive. `sentinel uncompiled --json` \
                 lists what is registered."
            ),
        )
    })?;

    let to_key = destination(&from_key, to)?;
    if to_key == from_key {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source and destination are the same.",
        ));
    }
    if root.join(&to_key).exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("Destination already exists: {to_key}"),
        ));
    }

    // Find every citation of the old path, in whatever form it was written.
    //
    // A complete view is required, not preferred: `mv` rewrites the articles it
    // can see and moves the file regardless. An article missed here keeps a
    // citation to a path that no longer exists, and `mv` would report "(no
    // articles cited it)" — a false statement, made confidently.
    let articles = wiki::load_all()?.require_complete()?;
    let mut edits: Vec<(String, String)> = Vec::new(); // (rel_path, new content)
    for article in &articles {
        let citing: Vec<&String> = article
            .article
            .frontmatter
            .sources
            .iter()
            .filter(|s| index.resolve(s).as_deref() == Some(from_key.as_str()))
            .collect();
        if citing.is_empty() {
            continue;
        }
        let Some(end) = frontmatter::block_end(&article.content) else {
            continue;
        };
        // Edit only inside the frontmatter block, and only whole citation
        // entries. Round-tripping through serde would reorder keys and strip
        // comments from a file the user may also edit by hand; blind substring
        // replacement corrupts neighbours — renaming `a.md` next to a cited
        // `data.md` turns the latter into `datraw/.../alpha.md`, which then
        // resolves by basename to the wrong source with lint reporting clean.
        let (block, body) = article.content.split_at(end);
        let written: Vec<&str> = citing.iter().map(|s| s.as_str()).collect();
        let block = repoint_sources(block, &written, &to_key);
        edits.push((article.rel_path().to_string(), format!("{block}{body}")));
    }

    let report = Rename {
        from: from_key.clone(),
        to: to_key.clone(),
        updated_articles: edits.iter().map(|(p, _)| p.clone()).collect(),
        dry_run,
    };

    if dry_run {
        if output::is_json() {
            return output::emit("mv", report);
        }
        println!("{} {from_key} → {to_key}", "Would move:".yellow());
        for (path, _) in &edits {
            println!("  {} {path}", "~".yellow());
        }
        println!(
            "\n{} {} article(s) would be rewritten. Nothing written.",
            "Dry run:".yellow(),
            edits.len()
        );
        return Ok(());
    }

    // Move the file first: if it fails, nothing else has been touched.
    let dest = root.join(&to_key);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(root.join(&from_key), &dest)?;

    for (rel_path, content) in &edits {
        crate::core::atomic::write(&root.join(rel_path), content)?;
    }

    if let Some(mut entry) = manifest.entries.remove(&from_key) {
        entry.raw_path = to_key.clone();
        // A move across domains changes which domain the source belongs to.
        if let Some(domain) = domain_of(&to_key) {
            entry.domain = domain;
        }
        manifest.upsert(entry);
    }
    manifest.save()?;

    crate::core::log::append(
        "mv",
        &format!(
            "{from_key} → {to_key} ({} article(s) repointed)",
            edits.len()
        ),
    )?;

    if output::is_json() {
        return output::emit("mv", report);
    }

    println!("{} {from_key} → {to_key}", "Moved:".green());
    for (path, _) in &edits {
        println!("  {} {path}", "~".green());
    }
    if edits.is_empty() {
        println!("  (no articles cited it)");
    }
    Ok(())
}

/// Replace whole citation entries under `sources:` with `to`.
///
/// Whole entries, never substrings: a citation is a complete YAML scalar, and
/// rewriting part of one produces a path that may still resolve — to the wrong
/// document, silently.
///
/// Handles both YAML list forms, since an agent writes either:
///   sources:
///     - raw/d/x.md
///   sources: [raw/d/x.md, raw/d/y.md]
fn repoint_sources(block: &str, written: &[&str], to: &str) -> String {
    let mut out = String::with_capacity(block.len());
    let mut in_sources = false;

    for line in block.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        let indent = trimmed.len() - trimmed.trim_start().len();
        let content = trimmed.trim_start();

        // A new top-level key ends the sources block.
        if in_sources && indent == 0 && !content.starts_with('-') {
            in_sources = false;
        }

        if let Some(rest) = content.strip_prefix("sources:") {
            let rest = rest.trim();
            if let Some(items) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                let rebuilt: Vec<String> = items
                    .split(',')
                    .map(|item| {
                        let (prefix, value, suffix) = split_scalar(item);
                        if written.contains(&value) {
                            format!("{prefix}{to}{suffix}")
                        } else {
                            item.to_string()
                        }
                    })
                    .collect();
                out.push_str(&trimmed[..indent]);
                out.push_str("sources: [");
                out.push_str(&rebuilt.join(","));
                out.push(']');
                out.push_str(&line[trimmed.len()..]);
                continue;
            }
            in_sources = true;
            out.push_str(line);
            continue;
        }

        if in_sources && let Some(item) = content.strip_prefix('-') {
            let (prefix, value, suffix) = split_scalar(item);
            if written.contains(&value) {
                out.push_str(&trimmed[..indent]);
                out.push('-');
                out.push_str(prefix);
                out.push_str(to);
                out.push_str(suffix);
                out.push_str(&line[trimmed.len()..]);
                continue;
            }
        }

        out.push_str(line);
    }
    out
}

/// Split a YAML scalar so that `prefix + value + suffix` reconstructs the
/// input, with `value` the bare citation and any quotes landing in the affixes.
///
/// Keeping the affixes verbatim is what preserves the author's formatting —
/// indentation, quoting style, trailing spaces — through a rewrite.
fn split_scalar(raw: &str) -> (&str, &str, &str) {
    let mut lo = raw.len() - raw.trim_start().len();
    let mut hi = raw.trim_end().len();

    let inner = &raw[lo..hi];
    for quote in ['"', '\''] {
        if inner.len() >= 2 && inner.starts_with(quote) && inner.ends_with(quote) {
            lo += quote.len_utf8();
            hi -= quote.len_utf8();
            break;
        }
    }
    (&raw[..lo], &raw[lo..hi], &raw[hi..])
}

/// Resolve the destination against the source's location.
///
/// A bare filename keeps the source's domain; a path is taken as
/// archive-relative and must stay under `raw/`.
fn destination(from_key: &str, to: &str) -> io::Result<String> {
    let to = to.trim().trim_start_matches("./");
    if !to.contains('/') {
        let dir = Path::new(from_key)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("raw");
        return Ok(format!("{dir}/{to}"));
    }

    let normalized = to.trim_start_matches('/');
    if !normalized.starts_with("raw/") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Destination must be under raw/ — got '{to}'.\n\
                 Raw documents are the archive's provenance floor; moving one \
                 out of raw/ would orphan every article compiled from it."
            ),
        ));
    }
    Ok(normalized.to_string())
}

/// `raw/<domain>/file.md` → `domain`.
fn domain_of(rel_path: &str) -> Option<String> {
    let mut parts = rel_path.split('/');
    if parts.next()? != "raw" {
        return None;
    }
    let domain = parts.next()?;
    // Only a real subdirectory names a domain; `raw/file.md` has none.
    parts.next().is_some().then(|| domain.to_string())
}
