//! `sentinel export` — the publishable subset of the wiki.
//!
//! Publishing is not copying `wiki/` somewhere. Three things about this archive
//! are true internally and wrong in public:
//!
//! - **Drafts.** `status:` exists precisely to mark what is not finished.
//! - **Forward-declared links.** `broken-link` is a *warning*, not an error,
//!   because the compile loop names concepts before writing them — that is the
//!   growth signal the `write` rung reads. To a reader it is a dead link.
//! - **Provenance.** `raw/` holds source documents whose licence is the user's
//!   to know, `meta/` is a working record, and neither belongs on a website.
//!
//! So this command decides *what is publishable* and writes only that. It does
//! not render HTML. A static site generator that already understands wikilinks
//! — Quartz, Obsidian Publish — takes the output from here.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use colored::Colorize;
use serde::Serialize;

use crate::core::{output, paths, slug, wiki};

/// Statuses considered finished enough to publish, in the absence of `--status`.
///
/// `stable` only. `review` means someone still has to look at it, and the whole
/// point of the field is that the archive knows the difference.
const DEFAULT_PUBLISHABLE: &[&str] = &["stable"];

#[derive(Serialize)]
struct Excluded {
    path: String,
    reason: String,
}

#[derive(Serialize)]
struct Report {
    destination: String,
    /// Articles written.
    published: usize,
    /// Articles held back, and why. The true total, not a sample.
    excluded_count: usize,
    excluded: Vec<Excluded>,
    /// Wikilinks pointing outside the published set, rewritten to plain text.
    links_defused: usize,
    /// Files already in the destination that this export would not write —
    /// articles unpublished since a previous run. Still readable until removed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stale: Vec<String>,
    /// True when `--clean` removed them.
    stale_removed: bool,
    /// True when nothing was written because `--dry-run` was given.
    dry_run: bool,
}

pub fn run(
    destination: Option<&Path>,
    statuses: Option<&str>,
    dry_run: bool,
    include_drafts: bool,
    clean: bool,
    flat: bool,
    data: Option<&Path>,
) -> io::Result<i32> {
    // A partial view would silently publish less than the archive holds, and a
    // reader has no way to tell a missing article from one that was never
    // written. This writes durable state outside the archive; it gets the same
    // treatment as the commands that rewrite state inside it.
    let articles = wiki::load_all()?.require_complete()?;

    let allowed: HashSet<String> = match (statuses, include_drafts) {
        (Some(list), _) => list
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect(),
        (None, true) => ["stable", "review", "draft"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        (None, false) => DEFAULT_PUBLISHABLE.iter().map(|s| s.to_string()).collect(),
    };

    // `--status ""` selected nothing and reported "Publishable statuses: ." —
    // indistinguishable, to a reader of the output, from an archive where
    // nothing happens to qualify.
    if allowed.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "`--status` named no status. Give a comma-separated list, for \
                 example `--status stable,review`. Valid values: {}.",
                crate::core::frontmatter::STATUSES.join(", ")
            ),
        ));
    }

    let destination = destination
        .map(PathBuf::from)
        .unwrap_or_else(|| paths::archive_root().join("publish"));

    // Refusing to write into the archive is not fussiness: `wiki/` and `index/`
    // are rebuilt by `index`, and an export underneath them would be walked as
    // article content on the next run.
    if destination.starts_with(paths::archive_root()) {
        let rel = paths::rel(&destination);
        if rel.starts_with("wiki/") || rel.starts_with("raw/") || rel.starts_with("index/") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Refusing to export into {rel} — `sentinel index` walks it, \
                     so the export would be indexed as archive content on the \
                     next rebuild. Choose a path outside wiki/, raw/, and index/."
                ),
            ));
        }
    }

    let mut published = Vec::new();
    let mut excluded = Vec::new();
    for article in &articles {
        let status = article
            .article
            .frontmatter
            .status
            .as_deref()
            .unwrap_or("unset");
        if allowed.contains(&status.to_lowercase()) {
            published.push(article);
        } else {
            excluded.push(Excluded {
                path: article.rel_path().to_string(),
                reason: format!("status: {status}"),
            });
        }
    }

    // Links are defused against the *published* set, not the archive. An
    // article that survives the filter can still cite one that did not, and
    // publishing a link to a page nobody can reach is the same dead end as
    // publishing a forward declaration.
    let reachable: HashSet<String> = published.iter().map(|a| a.canonical_slug()).collect();

    // Titles for every article, published or not, so a defused link can read
    // as prose. `[[dichotomy-of-control]]` becoming "dichotomy-of-control" puts
    // a filename in a sentence; the archive knows the article is called
    // "Dichotomy of Control".
    let titles: std::collections::HashMap<String, String> = articles
        .iter()
        .map(|a| (a.canonical_slug(), a.title().to_string()))
        .collect();

    // A site generator turns directories into URL segments, so `wiki/` would
    // appear in every URL meaning nothing to a reader. The domain does mean
    // something, so it stays unless the caller asks for flat.
    let mut seen: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for article in &published {
        let out = output_path(article.rel_path(), flat);
        if let Some(other) = seen.insert(out.clone(), article.rel_path()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "`--flat` would write {} and {other} to the same file ({out}). \
                     Drop `--flat` to keep them under their domains.",
                    article.rel_path()
                ),
            ));
        }
    }

    let mut links_defused = 0usize;
    let mut writes: Vec<(PathBuf, String)> = Vec::new();
    for article in &published {
        let (text, defused) = defuse_links(&article.content, &reachable, &titles);
        links_defused += defused;
        writes.push((
            destination.join(output_path(article.rel_path(), flat)),
            text,
        ));
    }

    // An article unpublished since the last run is still sitting in the
    // destination, still readable. For a publish command that is the dangerous
    // direction of wrong: the likeliest reason to unpublish something is that
    // it should not be public. Report it always; remove it only when asked.
    let intended: HashSet<PathBuf> = writes.iter().map(|(p, _)| p.clone()).collect();
    let mut stale: Vec<String> = existing_markdown(&destination)
        .into_iter()
        .filter(|p| !intended.contains(p))
        .map(|p| {
            p.strip_prefix(&destination)
                .unwrap_or(&p)
                .display()
                .to_string()
        })
        .collect();
    stale.sort();

    if let Some(data_path) = data
        && !dry_run
    {
        let generated_at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        let payload = bundle(&published, &reachable, &generated_at)?;
        if let Some(parent) = data_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::core::atomic::write(data_path, serde_json::to_string_pretty(&payload)?)?;
    }

    let report = Report {
        stale: stale.clone(),
        stale_removed: clean && !dry_run && !stale.is_empty(),
        destination: destination.display().to_string(),
        published: published.len(),
        excluded_count: excluded.len(),
        excluded: excluded.into_iter().take(20).collect(),
        links_defused,
        dry_run,
    };

    if !dry_run {
        if clean {
            for rel in &stale {
                std::fs::remove_file(destination.join(rel))?;
            }
        }
        for (path, text) in &writes {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            crate::core::atomic::write(path, text)?;
        }
        crate::core::log::append(
            "export",
            &format!("{} article(s) to {}", report.published, report.destination),
        )?;
    }

    if output::is_json() {
        output::emit("export", report)?;
        return Ok(0);
    }

    if report.published == 0 {
        println!(
            "{} No article matched. Publishable statuses: {}.",
            "Nothing exported.".yellow(),
            allowed.iter().cloned().collect::<Vec<_>>().join(", ")
        );
        println!("  `sentinel status` shows what this archive actually holds.");
        return Ok(0);
    }

    println!(
        "{} {} article(s) → {}",
        if dry_run {
            "Would export:"
        } else {
            "Exported:"
        }
        .green(),
        report.published,
        report.destination
    );
    if report.excluded_count > 0 {
        println!("  {} held back (not publishable):", report.excluded_count);
        for e in report.excluded.iter().take(5) {
            println!("    {} — {}", e.path, e.reason.dimmed());
        }
        if report.excluded_count > 5 {
            println!("    ... and {} more", report.excluded_count - 5);
        }
    }
    if !report.stale.is_empty() {
        if report.stale_removed {
            println!("  {} stale file(s) removed.", report.stale.len());
        } else {
            println!(
                "  {} {} file(s) in the destination were not written by this \
                 export and are still readable:",
                "!".yellow(),
                report.stale.len()
            );
            for rel in report.stale.iter().take(5) {
                println!("      {rel}");
            }
            if report.stale.len() > 5 {
                println!("      ... and {} more", report.stale.len() - 5);
            }
            println!("      Re-run with `--clean` to remove them.");
        }
    }
    if report.links_defused > 0 {
        println!(
            "  {} link(s) pointed outside the published set and were rendered \
             as plain text.",
            report.links_defused
        );
    }
    if dry_run {
        println!("\n{}", "Dry run: nothing written.".yellow());
    }
    Ok(0)
}

/// Everything a front end needs, in one file.
///
/// A UI that called the commands one at a time would need a process to call
/// them, which means a server next to an archive that lives on a laptop. One
/// bundle is a static asset: 17 KB for 27 articles, 66 KB for 400, and it
/// gzips to a tenth of that. Nothing here needs to be live to be worth looking
/// at — the archive changes when its owner works on it, not continuously.
#[derive(Serialize)]
struct Bundle {
    generated_at: String,
    schema_version: u32,
    /// Only published articles, so the bundle can be served beside them.
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    progress: Vec<crate::core::history::Snapshot>,
    /// Snapshots that could not be parsed. A history with holes should say so.
    unreadable_snapshots: usize,
}

#[derive(Serialize)]
struct Node {
    slug: String,
    title: String,
    domain: String,
    origin: String,
    status: String,
    tags: Vec<String>,
    /// Incoming links from other published articles — how central it is.
    inbound: usize,
    outbound: usize,
}

#[derive(Serialize)]
struct Edge {
    from: String,
    to: String,
}

fn bundle(
    published: &[&wiki::LoadedArticle],
    reachable: &HashSet<String>,
    generated_at: &str,
) -> io::Result<Bundle> {
    let mut edges = Vec::new();
    let mut inbound: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut outbound: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for article in published {
        let from = article.canonical_slug();
        // Against the published set, so the graph matches the site: an edge to
        // an article nobody can open is not a connection a reader can follow.
        for target in crate::core::links::extract_wikilinks(&article.content) {
            let to = slug::canonical(&target);
            if to.is_empty() || to == from || !reachable.contains(&to) {
                continue;
            }
            *outbound.entry(from.clone()).or_default() += 1;
            *inbound.entry(to.clone()).or_default() += 1;
            edges.push(Edge {
                from: from.clone(),
                to,
            });
        }
    }
    edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    edges.dedup_by(|a, b| a.from == b.from && a.to == b.to);

    let nodes = published
        .iter()
        .map(|a| {
            let slug = a.canonical_slug();
            let fm = &a.article.frontmatter;
            Node {
                title: a.title().to_string(),
                domain: fm.domain.clone().unwrap_or_default(),
                origin: fm.origin.clone().unwrap_or_default(),
                status: fm.status.clone().unwrap_or_default(),
                tags: fm.tags.clone(),
                inbound: inbound.get(&slug).copied().unwrap_or(0),
                outbound: outbound.get(&slug).copied().unwrap_or(0),
                slug,
            }
        })
        .collect();

    let (progress, unreadable_snapshots) = crate::core::history::read()?;
    Ok(Bundle {
        generated_at: generated_at.to_string(),
        schema_version: output::SCHEMA_VERSION,
        nodes,
        edges,
        progress,
        unreadable_snapshots,
    })
}

/// Where an article lands in the export.
///
/// The archive stores `wiki/<domain>/<slug>.md`. That `wiki/` prefix is how the
/// archive separates compiled articles from raw sources, and it means nothing
/// once only articles are being published — it would just be a segment in every
/// URL. The domain is kept because it groups the site the way the archive
/// groups the wiki; `--flat` drops it too, for a site with one subject.
fn output_path(rel_path: &str, flat: bool) -> String {
    let without_prefix = rel_path.strip_prefix("wiki/").unwrap_or(rel_path);
    if flat {
        without_prefix
            .rsplit('/')
            .next()
            .unwrap_or(without_prefix)
            .to_string()
    } else {
        without_prefix.to_string()
    }
}

/// Markdown already in the destination, so an export can see what it is
/// replacing.
fn existing_markdown(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    out
}

/// Rewrite `[[targets]]` that no published article provides into plain text.
///
/// Kept as text rather than deleted: the sentence was written to mention the
/// concept, and removing the words changes what it says. Only the link is
/// dropped, and `[[alias|Label]]` keeps its label.
fn defuse_links(
    content: &str,
    reachable: &HashSet<String>,
    titles: &std::collections::HashMap<String, String>,
) -> (String, usize) {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    let mut defused = 0;

    while let Some(start) = rest.find("[[") {
        let Some(end) = rest[start..].find("]]") else {
            break;
        };
        let inner = &rest[start + 2..start + end];
        out.push_str(&rest[..start]);

        let (target, label) = match inner.split_once('|') {
            Some((t, l)) => (t, l),
            None => (inner, inner),
        };
        let canonical = slug::canonical(target);
        if reachable.contains(&canonical) {
            out.push_str(&rest[start..start + end + 2]);
        } else {
            // An explicit `[[slug|Label]]` was written for the reader already;
            // keep it. Otherwise prefer the article's title over its filename,
            // and fall back to the raw target when nothing is known about it —
            // a forward-declared concept that was never written.
            match (inner.contains('|'), titles.get(&canonical)) {
                (false, Some(title)) => out.push_str(title),
                _ => out.push_str(label),
            }
            defused += 1;
        }
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    (out, defused)
}
