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

use crate::core::review;
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
    /// Of those, how many were finished but unsigned. Counted separately
    /// because it is the one exclusion the owner can clear in a command, and
    /// folding it into "status" would read as work that is not ready.
    held_for_approval: usize,
    /// Wikilinks pointing outside the published set, rewritten to plain text.
    links_defused: usize,
    /// Files already in the destination that this export would not write —
    /// articles unpublished since a previous run. Still readable until removed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stale: Vec<String>,
    /// True when `--clean` removed them.
    stale_removed: bool,
    /// Raw documents copied because they are opted in and something cites them.
    sources_published: usize,
    /// Documents an article cites that are *not* opted in. Reported so a reader
    /// of the output knows the trail stops somewhere deliberate rather than
    /// wondering why some citations link and others do not.
    sources_withheld: usize,
    /// Where the showcase page was written, when it was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    ui: Option<String>,
    /// True when the destination had no landing page and one was scaffolded.
    wrote_landing: bool,
    /// True when nothing was written because `--dry-run` was given.
    dry_run: bool,
}

/// Where opted-in raw documents land, relative to the destination.
///
/// A directory of its own so nothing can collide with an article, and so a
/// reader can tell source material from writing at a glance in the URL.
const SOURCES_DIR: &str = "sources";

/// The showcase page, compiled into the binary.
///
/// `include_str!` rather than a file the user is told to copy: a page that has
/// to be kept in step by hand is a page that renders last month's bundle
/// shape. Versioning it with the tool means the two cannot disagree.
const UI_PAGE: &str = include_str!("../../ui/index.html");

/// What an export run was asked to do.
///
/// A struct rather than eight positional arguments, five of which are bools:
/// at that width a caller can transpose `clean` and `flat` and the compiler is
/// perfectly happy, which for a command that deletes files in a destination is
/// not a risk worth carrying for brevity.
pub struct Options<'a> {
    pub destination: Option<&'a Path>,
    pub statuses: Option<&'a str>,
    pub dry_run: bool,
    pub include_drafts: bool,
    pub clean: bool,
    pub flat: bool,
    pub data: Option<&'a Path>,
    pub with_sources: bool,
    /// Write the showcase page and its bundle into this directory.
    ///
    /// Separate from the markdown export on purpose. That output is for
    /// reading, and a static site generator owns it; this is one self-contained
    /// page for looking at the archive as a working system. Mixing them would
    /// mean handing a generator an HTML file it does not know what to do with.
    pub ui: Option<&'a Path>,
}

pub fn run(options: Options<'_>) -> io::Result<i32> {
    let Options {
        destination,
        statuses,
        dry_run,
        include_drafts,
        clean,
        flat,
        data,
        with_sources,
        ui,
    } = options;
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

    let traits = crate::core::persona::load_all()?.require_complete()?;
    let manifest = crate::core::manifest::Manifest::load()?;

    let mut published = Vec::new();
    let mut excluded = Vec::new();
    let mut held_for_approval = 0usize;
    for article in &articles {
        let fm = &article.article.frontmatter;
        let status = fm.status.as_deref().unwrap_or("unset");
        if !allowed.contains(&status.to_lowercase()) {
            excluded.push(Excluded {
                path: article.rel_path().to_string(),
                reason: format!("status: {status}"),
            });
            continue;
        }
        // The approval gate. A separate axis from maturity: `stable` means
        // finished, `approved` means the archive's owner signed it. Work the
        // clone wrote in their voice does not go out on the tool's opinion of
        // it, and `--status` cannot override this — a flag that could would
        // make the gate advisory.
        if fm.is_extrapolated() && !review::is_approved(&fm.review) {
            let standing = review::standing(&fm.review);
            held_for_approval += 1;
            excluded.push(Excluded {
                path: article.rel_path().to_string(),
                reason: match standing {
                    Some(e) => format!("written by the clone; latest verdict is '{}'", e.verdict),
                    None => "written by the clone and not approved".to_string(),
                },
            });
            continue;
        }
        published.push(article);
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

    // Which cited documents a reader will be able to open. Resolved through the
    // same matcher `sources:` citations use, so a bare filename means here what
    // it means in an article.
    let index = crate::core::compilation::SourceIndex::new(&manifest);
    let mut source_writes: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut sources_withheld = 0usize;
    let mut public_sources: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if with_sources {
        let mut cited: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for article in &published {
            for source in &article.article.frontmatter.sources {
                if let Some(resolved) = index.resolve(source) {
                    cited.insert(resolved);
                }
            }
        }
        for rel in cited {
            let Some(entry) = manifest.entries.get(&rel) else {
                continue;
            };
            // Opted in per document, and only ever that. There is no flag that
            // publishes `raw/` wholesale, because nothing about a file says
            // whether it is the owner's to publish.
            if !entry.publish {
                sources_withheld += 1;
                continue;
            }
            let out_rel = source_output_path(&rel);
            match std::fs::read(paths::archive_root().join(&rel)) {
                Ok(bytes) => {
                    public_sources.insert(rel.clone(), out_rel.clone());
                    source_writes.push((destination.join(&out_rel), bytes));
                }
                // A source that cannot be read is not a source that can be
                // published, and it must not be silently dropped from a count
                // that says how much of the trail a reader can follow.
                Err(_) => sources_withheld += 1,
            }
        }
    }

    let mut links_defused = 0usize;
    let mut writes: Vec<(PathBuf, String)> = Vec::new();
    // The prose as published, for the bundle to carry. Taken from the text
    // actually written rather than from the article on disk, so a reader in the
    // graph sees the defused links and the attribution notice — the same words
    // the site shows, not a second rendering of them.
    let mut bodies: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for article in &published {
        let (mut text, defused) = defuse_links(&article.content, &reachable, &titles);
        links_defused += defused;
        // `sources:` names paths under `raw/`, and `raw/` is not published.
        // Copying the field verbatim put the *path* of every cited document on
        // the site — including ones deliberately withheld, whose filenames can
        // be the private part. The published copy names only what a reader can
        // actually open, and nothing when that is nothing.
        let visible: Vec<String> = article
            .article
            .frontmatter
            .sources
            .iter()
            .filter_map(|src| index.resolve(src))
            .filter_map(|rel| public_sources.get(&rel).cloned())
            .collect();
        text = crate::core::frontmatter::set_list(&text, "sources", &visible);
        if !visible.is_empty() {
            text = format!(
                "{}{}",
                text.trim_end(),
                source_footer(article, &index, &manifest, &public_sources, flat)
            );
        }
        // Written here, by the exporter, rather than by whatever produced the
        // article. An agent that composes its own disclosure is an agent that
        // can leave it out, and this is the notice that stops a reader taking
        // machine prose for the author's own.
        if article.article.frontmatter.is_extrapolated() {
            text = format!("{}\n{}", text.trim_end(), attribution(article, &traits));
        }
        bodies.insert(article.canonical_slug(), strip_frontmatter(&text));
        writes.push((
            destination.join(output_path(article.rel_path(), flat)),
            text,
        ));
    }

    // An article unpublished since the last run is still sitting in the
    // destination, still readable. For a publish command that is the dangerous
    // direction of wrong: the likeliest reason to unpublish something is that
    // it should not be public. Report it always; remove it only when asked.
    // Only files this tool plausibly wrote are candidates. `--clean` used to
    // remove any markdown it had not just written, which deleted the site
    // generator's own landing page and would have taken a hand-written
    // `about.md` with it — the documented workflow produced a site whose home
    // page was "Not Found".
    //
    // No bookkeeping needed to tell them apart: a file whose stem matches an
    // article in this archive is one an export produced. Anything else belongs
    // to whoever put it there.
    let known: HashSet<String> = articles.iter().map(|a| a.canonical_slug()).collect();
    let intended: HashSet<PathBuf> = writes.iter().map(|(p, _)| p.clone()).collect();
    let mut stale: Vec<String> = existing_markdown(&destination)
        .into_iter()
        .filter(|p| !intended.contains(p))
        .filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| known.contains(&slug::canonical(stem)))
        })
        .map(|p| {
            p.strip_prefix(&destination)
                .unwrap_or(&p)
                .display()
                .to_string()
        })
        .collect();

    // Copied sources are not markdown-only and do not carry article slugs, so
    // they need their own reckoning. A file under `sources/` whose path matches
    // a manifest entry is one an export produced — which makes a *withdrawn*
    // document stale, and leaving it readable is the failure this whole opt-in
    // exists to prevent.
    let intended_sources: HashSet<PathBuf> = source_writes.iter().map(|(p, _)| p.clone()).collect();
    for path in existing_files(&destination.join(SOURCES_DIR)) {
        if intended_sources.contains(&path) {
            continue;
        }
        let Ok(rel) = path.strip_prefix(&destination) else {
            continue;
        };
        let under_raw = format!(
            "raw/{}",
            rel.strip_prefix(SOURCES_DIR).unwrap_or(rel).display()
        );
        if manifest.entries.contains_key(&under_raw) {
            stale.push(rel.display().to_string());
        }
    }
    stale.sort();
    stale.dedup();

    // `--ui` implies its own bundle: the page reads `bundle.json` from beside
    // itself, and a flag that wrote the page without the data it needs would
    // produce a site whose only content is an error message.
    let bundle_targets: Vec<PathBuf> = data
        .map(PathBuf::from)
        .into_iter()
        .chain(ui.map(|dir| dir.join("bundle.json")))
        .collect();
    if !bundle_targets.is_empty() && !dry_run {
        let generated_at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        let payload = bundle(BundleInput {
            published: &published,
            all: &articles,
            traits: &traits,
            reachable: &reachable,
            bodies: &bodies,
            sources: &source_writes,
            public_sources: &public_sources,
            manifest: &manifest,
            index: &index,
            generated_at: &generated_at,
        })?;
        let text = serde_json::to_string_pretty(&payload)?;
        for target in &bundle_targets {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            crate::core::atomic::write(target, &text)?;
        }
    }
    if let Some(dir) = ui
        && !dry_run
    {
        std::fs::create_dir_all(dir)?;
        crate::core::atomic::write(&dir.join("index.html"), UI_PAGE)?;
    }

    // A site with no landing page serves a 404 at its root. Written only when
    // the destination has none, the way `init` treats the files it scaffolds:
    // the first export produces a working site, and anything the user writes
    // afterwards is theirs.
    let landing = destination.join("index.md");
    let wrote_landing = !dry_run && !landing.exists();

    let report = Report {
        ui: ui.map(|d| d.display().to_string()),
        held_for_approval,
        wrote_landing,
        stale: stale.clone(),
        stale_removed: clean && !dry_run && !stale.is_empty(),
        destination: destination.display().to_string(),
        published: published.len(),
        excluded_count: excluded.len(),
        excluded: excluded.into_iter().take(20).collect(),
        links_defused,
        sources_published: source_writes.len(),
        sources_withheld,
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
        for (path, bytes) in &source_writes {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            crate::core::atomic::write(path, bytes)?;
        }
        // After the articles, so the destination directory exists.
        if wrote_landing {
            std::fs::create_dir_all(&destination)?;
            crate::core::atomic::write(&landing, landing_page(&published))?;
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
    // Said separately and said plainly. This is the one exclusion the owner
    // can clear with a single command, and burying it in a list headed "not
    // publishable" reads as work that is not finished.
    if report.sources_published > 0 || report.sources_withheld > 0 {
        println!(
            "  {} source document(s) copied; {} withheld (not marked publishable).",
            report.sources_published, report.sources_withheld
        );
    }
    if let Some(dir) = &report.ui {
        println!("  showcase page → {dir}/index.html");
    }
    if report.held_for_approval > 0 {
        println!(
            "\n  {} {} article(s) written by the clone are finished but unsigned.",
            "!".yellow(),
            report.held_for_approval
        );
        println!(
            "    {}",
            "sentinel review           # see them\n                 sentinel review <slug> --approve"
                .dimmed()
        );
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
    if report.wrote_landing {
        println!("  index.md scaffolded — the destination had no landing page.");
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
/// bundle is a static asset. Nothing here needs to be live to be worth looking
/// at — the archive changes when its owner works on it, not continuously.
///
/// It carries the published prose, not just the graph, so the page can be read
/// as well as looked at without a request per article. That is what makes the
/// showcase two files you can copy anywhere. The cost is real and worth stating:
/// metadata alone was 17 KB for 27 articles; with bodies it scales with the
/// wiki rather than with its shape. It still gzips to roughly a fifth, and a
/// static host serves it once.
#[derive(Serialize)]
struct Bundle {
    generated_at: String,
    schema_version: u32,
    /// The layers, in order, with the names a reader should see.
    layers: &'static [Layer],
    /// The kinds of connection between them.
    edge_kinds: &'static [EdgeKind],
    /// Only published articles, so the bundle can be served beside them.
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// The author's model of themselves — affirmed traits only.
    ///
    /// A `proposed` trait is an unconfirmed reading, and publishing one would
    /// put a claim about a person in front of readers before the person has
    /// seen it. The same gate the articles get, for the same reason.
    persona: Vec<PublishedTrait>,
    /// What the archive is in the middle of. The point of showing a wiki that
    /// builds itself is that the building is visible; a bundle of finished
    /// pages is a website.
    in_progress: InProgress,
    progress: Vec<crate::core::history::Snapshot>,
    /// Snapshots that could not be parsed. A history with holes should say so.
    unreadable_snapshots: usize,
}

/// A trait, as a reader outside the archive may see it.
///
/// Deliberately not the whole file. `evidence:` is a list of `raw/` paths, and
/// `raw/` is not published — a bundle naming documents nobody can open would
/// be citing sources at readers who cannot check them. The count is honest
/// about how much stands behind the claim without leaking the corpus.
#[derive(Serialize)]
struct PublishedTrait {
    id: String,
    kind: String,
    claim: String,
    confidence: String,
    evidence_count: usize,
    /// Published articles written from this trait.
    expressed_in: Vec<String>,
}

/// Counts of work in flight, for a front end that shows the loop running.
#[derive(Serialize)]
struct InProgress {
    /// Articles in the archive that this export did not publish.
    unpublished: usize,
    /// Generated articles waiting on the owner's verdict.
    awaiting_approval: usize,
    /// Traits proposed and not yet answered.
    unconfirmed_traits: usize,
    /// Concepts the wiki links to and has not written — what it wants next.
    wanted: usize,
}

#[derive(Serialize)]
struct Node {
    slug: String,
    title: String,
    domain: String,
    origin: String,
    status: String,
    /// `source` for a raw document, `article` for something in `wiki/`.
    ///
    /// Sources appear only with `--with-sources`, and only the opted-in ones,
    /// so a bundle built without that flag holds exactly what it always did.
    kind: &'static str,
    /// Distance from the author's own hand, 0 outward.
    ///
    /// Derived here rather than in a front end, because it encodes what the
    /// archive means by provenance and a page that recomputed it would be a
    /// second opinion about whose work something is:
    ///
    /// | 0 | a source document — what they actually wrote |
    /// | 1 | `authored` — compiled from it, their thinking |
    /// | 2 | `hybrid` — theirs, enriched |
    /// | 3 | `researched` — gathered from the world |
    /// | 4 | `extrapolated` — written by the clone |
    layer: u8,
    /// Written by the clone. A front end that renders this the same as an
    /// article its author wrote is a front end that misleads its readers.
    extrapolated: bool,
    tags: Vec<String>,
    /// The published prose, frontmatter removed.
    ///
    /// Carried so the page can be *read* without a request per article, which
    /// is what keeps "one HTML file and one JSON file" true. It is the same
    /// text that was written to the site — links already defused, attribution
    /// notice already appended — so a reader in the graph and a reader on the
    /// site see the same words.
    body: String,
    /// Incoming links from other published articles — how central it is.
    inbound: usize,
    outbound: usize,
}

/// How far a piece of work sits from the author's own hand.
/// The three layers, published so a front end can name them without inventing
/// its own account of what the archive is made of.
///
/// This is the shape of the whole system, not a display choice: the author
/// writes, the archive distils a model of them from it, and the clone produces
/// work from that model. Each layer is derived from *what a document is*, never
/// from anything a page decides.
pub const LAYERS: &[Layer] = &[
    Layer {
        index: 0,
        id: "source",
        name: "Source material",
        description: "Documents the author wrote. Nothing here was produced by the archive.",
    },
    Layer {
        index: 1,
        id: "persona",
        name: "Persona",
        description: "Cited traits distilled from that writing — how they argue and what they hold. Each one names the documents it was read out of.",
    },
    Layer {
        index: 2,
        id: "work",
        name: "The clone's work",
        description: "Articles the archive produced: compiled from sources, or written from the persona and signed off by the author.",
    },
];

#[derive(Serialize, Clone, Copy)]
pub struct Layer {
    pub index: u8,
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

/// Which layer a wiki article belongs to.
///
/// All of them: an article is the archive's output whether it was compiled from
/// a source or extrapolated from the persona. The difference between those two
/// is carried by `origin` and `extrapolated`, which is what marks generated
/// work — depth answers a different question, and conflating the two would put
/// a compiled article and a machine-written one on different rings while
/// claiming the rings mean provenance layer.
fn layer_of(_origin: &str) -> u8 {
    2
}

/// A connection, always pointing **outward** — from what something came from
/// towards what came of it.
///
/// The direction is the claim. A source document did not cite an article; the
/// article was compiled from it. A trait did not reference a source; it was
/// distilled out of one. Recording the arrows the other way round would draw a
/// picture in which the work produced the corpus.
#[derive(Clone, Serialize)]
struct Edge {
    from: String,
    to: String,
    /// What kind of connection, so a reader is not left inferring it from the
    /// endpoints and a front end does not have to reimplement the layer rules
    /// to colour it.
    kind: &'static str,
}

/// Every edge kind, published so a page can key its colours off the set rather
/// than off a guess about what the endpoints mean.
pub const EDGE_KINDS: &[EdgeKind] = &[
    EdgeKind {
        id: "distils",
        from_layer: 0,
        to_layer: 1,
        description: "A persona trait was read out of this document.",
        primary: true,
    },
    EdgeKind {
        id: "writes",
        from_layer: 1,
        to_layer: 2,
        description: "This article was written from that trait.",
        primary: true,
    },
    EdgeKind {
        id: "grounds",
        from_layer: 0,
        to_layer: 2,
        description: "This article is evidenced by that document — what it cites, not what wrote it.",
        primary: false,
    },
    EdgeKind {
        id: "links",
        from_layer: 2,
        to_layer: 2,
        description: "One article's [[wikilink]] to another.",
        primary: true,
    },
];

#[derive(Serialize, Clone, Copy)]
pub struct EdgeKind {
    pub id: &'static str,
    pub from_layer: u8,
    pub to_layer: u8,
    pub description: &'static str,
    /// Whether this is part of the chain the picture is about.
    ///
    /// Authorship radiates: corpus → persona → work, and every piece of work
    /// arrives along it. A citation is a different relation — it reaches back
    /// past whatever produced the text to whatever the text rests on — so it is
    /// drawn as a secondary connection rather than as another way in.
    pub primary: bool,
}

/// Everything `bundle` needs. A struct because it is nine values, and at that
/// width a transposed pair of `&[...]` arguments compiles perfectly happily.
struct BundleInput<'a> {
    published: &'a [&'a wiki::LoadedArticle],
    all: &'a [wiki::LoadedArticle],
    traits: &'a [crate::core::persona::LoadedTrait],
    reachable: &'a HashSet<String>,
    bodies: &'a std::collections::HashMap<String, String>,
    sources: &'a [(PathBuf, Vec<u8>)],
    public_sources: &'a std::collections::HashMap<String, String>,
    manifest: &'a crate::core::manifest::Manifest,
    index: &'a crate::core::compilation::SourceIndex<'a>,
    generated_at: &'a str,
}

/// The prose, with the frontmatter block removed.
///
/// Uses the one parser rather than looking for the second `---`: a horizontal
/// rule partway down an article is not a delimiter, and the attribution notice
/// appended to generated work opens with exactly that.
fn strip_frontmatter(text: &str) -> String {
    match crate::core::frontmatter::block_end(text) {
        Some(end) => text[end..].trim_start().to_string(),
        None => text.trim_start().to_string(),
    }
}

/// The identity a persona trait goes by in the graph.
///
/// Prefixed for the same reason sources are: node ids share one namespace, and
/// a trait called `virtue` must not collide with an article about it.
fn trait_slug(id: &str) -> String {
    format!("trait:{}", crate::core::slug::canonical(id))
}

/// The identity a published source document goes by in the graph.
///
/// Prefixed and pathful: an article slug never contains `/`, and two sources
/// with the same filename in different domains have to stay distinct.
fn source_slug(published_path: &str) -> String {
    let stem = published_path
        .strip_prefix(&format!("{SOURCES_DIR}/"))
        .unwrap_or(published_path);
    format!("src:{}", stem.trim_end_matches(".md"))
}

fn bundle(input: BundleInput<'_>) -> io::Result<Bundle> {
    let BundleInput {
        published,
        all,
        traits,
        reachable,
        bodies,
        sources,
        public_sources,
        manifest,
        index,
        generated_at,
    } = input;
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
                kind: "links",
            });
        }
    }
    // Grounding edges, source → article. Not authorship: the document did not
    // write the article, the clone did, through the persona. This records what
    // the article rests on, which is a citation and legitimately reaches back
    // past whatever produced the text. The same `sources:` the compile loop
    // reads, resolved the same way.
    for article in published {
        let to = article.canonical_slug();
        for cited in &article.article.frontmatter.sources {
            let Some(resolved) = index.resolve(cited) else {
                continue;
            };
            let Some(out_rel) = public_sources.get(&resolved) else {
                continue;
            };
            let from = source_slug(out_rel);
            *outbound.entry(from.clone()).or_default() += 1;
            *inbound.entry(to.clone()).or_default() += 1;
            edges.push(Edge {
                from,
                to: to.clone(),
                kind: "grounds",
            });
        }
    }
    edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    edges.dedup_by(|a, b| a.from == b.from && a.to == b.to);

    // The persona, between the corpus and the work. Affirmed traits only — the
    // same gate the `persona` list downstream uses, and for the same reason: a
    // `proposed` trait is an unconfirmed reading, and drawing one in the middle
    // of the picture asserts it to every reader before the author has seen it.
    //
    // Its edges are the two claims the layer makes, both pointing outward: the
    // documents it was distilled from flow into it, and it flows into the work
    // written from it. Following those arrows from a source document is the
    // whole story — this is what I read, this is what I concluded from it,
    // this is what got written.
    for t in traits.iter().filter(|t| t.is_affirmed()) {
        let id = trait_slug(&t.id());
        for cited in &t.frontmatter.evidence {
            let Some(resolved) = index.resolve(cited) else {
                continue;
            };
            let Some(out_rel) = public_sources.get(&resolved) else {
                continue;
            };
            let from = source_slug(out_rel);
            *outbound.entry(from.clone()).or_default() += 1;
            *inbound.entry(id.clone()).or_default() += 1;
            edges.push(Edge {
                from,
                to: id.clone(),
                kind: "distils",
            });
        }
        for article in published {
            let canonical = t.canonical_id();
            if article
                .article
                .frontmatter
                .persona
                .iter()
                .any(|c| slug::canonical(c) == canonical)
            {
                let to = article.canonical_slug();
                *outbound.entry(id.clone()).or_default() += 1;
                *inbound.entry(to.clone()).or_default() += 1;
                edges.push(Edge {
                    from: id.clone(),
                    to,
                    kind: "writes",
                });
            }
        }
    }
    edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    edges.dedup_by(|a, b| a.from == b.from && a.to == b.to);

    // Source documents, at the core. Only the opted-in ones are here at all —
    // `public_sources` is already filtered by `sentinel sources --publish`.
    let mut source_text: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    for (path, bytes) in sources {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Lossy: a source is whatever its owner ingested, and a byte that
            // is not UTF-8 should cost that character rather than the document.
            source_text.insert(name, String::from_utf8_lossy(bytes).to_string());
        }
    }

    let mut nodes: Vec<Node> = public_sources
        .iter()
        .map(|(raw_path, out_rel)| {
            let entry = manifest.entries.get(raw_path);
            let name = out_rel.rsplit('/').next().unwrap_or(out_rel);
            let slug = source_slug(out_rel);
            Node {
                title: entry.map_or_else(|| name.to_string(), |e| e.title.clone()),
                domain: entry.map_or_else(String::new, |e| e.domain.clone()),
                origin: entry.map_or_else(String::new, |e| e.origin.clone()),
                status: "published".to_string(),
                kind: "source",
                layer: 0,
                extrapolated: false,
                tags: Vec::new(),
                body: source_text.get(name).cloned().unwrap_or_default(),
                inbound: inbound.get(&slug).copied().unwrap_or(0),
                outbound: 0,
                slug,
            }
        })
        .collect();
    nodes.sort_by(|a, b| a.slug.cmp(&b.slug));

    nodes.extend(traits.iter().filter(|t| t.is_affirmed()).map(|t| {
        let slug = trait_slug(&t.id());
        Node {
            title: t.frontmatter.claim.clone().unwrap_or_else(|| t.id()),
            domain: String::new(),
            origin: t.kind().to_string(),
            status: t.status().to_string(),
            kind: "trait",
            layer: 1,
            extrapolated: false,
            tags: vec![t.kind().to_string()],
            // The reasoning behind the claim, which is what makes the trait
            // checkable rather than merely asserted. Same rule as everywhere
            // else here: `evidence:` names `raw/` paths and is left out.
            body: t.body.clone(),
            inbound: inbound.get(&slug).copied().unwrap_or(0),
            outbound: outbound.get(&slug).copied().unwrap_or(0),
            slug,
        }
    }));

    nodes.extend(published.iter().map(|a| {
        let slug = a.canonical_slug();
        let fm = &a.article.frontmatter;
        let origin = fm.origin.clone().unwrap_or_default();
        Node {
            title: a.title().to_string(),
            domain: fm.domain.clone().unwrap_or_default(),
            status: fm.status.clone().unwrap_or_default(),
            kind: "article",
            layer: layer_of(&origin),
            origin,
            tags: fm.tags.clone(),
            extrapolated: fm.is_extrapolated(),
            body: bodies.get(&slug).cloned().unwrap_or_default(),
            inbound: inbound.get(&slug).copied().unwrap_or(0),
            outbound: outbound.get(&slug).copied().unwrap_or(0),
            slug,
        }
    }));

    let persona = traits
        .iter()
        .filter(|t| t.is_affirmed())
        .map(|t| {
            let id = t.canonical_id();
            PublishedTrait {
                kind: t.kind().to_string(),
                claim: t.frontmatter.claim.clone().unwrap_or_else(|| t.id()),
                confidence: t
                    .frontmatter
                    .confidence
                    .clone()
                    .unwrap_or_else(|| "unstated".to_string()),
                evidence_count: t.frontmatter.evidence.len(),
                expressed_in: published
                    .iter()
                    .filter(|a| {
                        a.article
                            .frontmatter
                            .persona
                            .iter()
                            .any(|c| slug::canonical(c) == id)
                    })
                    .map(|a| a.canonical_slug())
                    .collect(),
                id,
            }
        })
        .collect();

    let in_progress = InProgress {
        unpublished: all.len().saturating_sub(published.len()),
        awaiting_approval: all
            .iter()
            .filter(|a| a.article.frontmatter.is_extrapolated())
            .filter(|a| !review::is_approved(&a.article.frontmatter.review))
            .count(),
        unconfirmed_traits: traits.iter().filter(|t| t.status() == "proposed").count(),
        wanted: crate::core::links::wanted(all).len(),
    };

    let (progress, unreadable_snapshots) = crate::core::history::read()?;
    Ok(Bundle {
        generated_at: generated_at.to_string(),
        schema_version: output::SCHEMA_VERSION,
        layers: LAYERS,
        edge_kinds: EDGE_KINDS,
        nodes,
        edges,
        persona,
        in_progress,
        progress,
        unreadable_snapshots,
    })
}

/// Where an opted-in raw document lands in the destination.
///
/// Under `sources/`, keeping the path it had beneath `raw/`. Two documents with
/// the same filename in different domains stay distinct, and nothing here can
/// collide with an article even under `--flat`.
fn source_output_path(rel: &str) -> String {
    let without_prefix = rel.strip_prefix("raw/").unwrap_or(rel);
    format!("{SOURCES_DIR}/{without_prefix}")
}

/// Links from an article to the source documents a reader can actually open.
///
/// Only the opted-in ones. A citation to a document that was withheld is left
/// out entirely rather than rendered as a dead link — the export's whole
/// premise is that the output has no dead ends, and a link to `raw/` from a
/// published page is the worst kind, because it names a file and then denies it.
fn source_footer(
    article: &wiki::LoadedArticle,
    index: &crate::core::compilation::SourceIndex<'_>,
    manifest: &crate::core::manifest::Manifest,
    public: &std::collections::HashMap<String, String>,
    flat: bool,
) -> String {
    let mut links: Vec<String> = Vec::new();
    for source in &article.article.frontmatter.sources {
        let Some(resolved) = index.resolve(source) else {
            continue;
        };
        let Some(out_rel) = public.get(&resolved) else {
            continue;
        };
        let title = manifest
            .entries
            .get(&resolved)
            .map_or(resolved.as_str(), |e| e.title.as_str());
        // Articles sit one directory deep unless `--flat`, so the link back up
        // has to account for that. A site generator resolving a relative link
        // from the wrong depth is a 404 that only appears once it is deployed.
        let prefix = if flat { "" } else { "../" };
        links.push(format!("- [{title}]({prefix}{out_rel})"));
    }
    if links.is_empty() {
        return String::new();
    }
    links.sort();
    links.dedup();
    format!("\n\n## Sources\n\n{}\n", links.join("\n"))
}

/// The notice appended to every published extrapolated article.
///
/// Not composable by an agent and not suppressible by a flag: the exporter
/// writes it, unconditionally, for anything marked as the clone's own work.
/// A reader who takes generated prose for the author's own writing is the
/// harm this whole feature is arranged around.
fn attribution(
    article: &wiki::LoadedArticle,
    traits: &[crate::core::persona::LoadedTrait],
) -> String {
    let fm = &article.article.frontmatter;
    let mut out = String::from(
        "\n---\n\n*Written by a language model working from this archive, extending its author's own writing rather than reproducing it.",
    );

    // The claims it was written from, in the author's own words where the
    // trait records them. A reader who disagrees can then disagree with the
    // premise rather than only with the conclusion.
    let claims: Vec<String> = fm
        .persona
        .iter()
        .filter_map(|id| {
            let wanted = slug::canonical(id);
            traits
                .iter()
                .find(|t| t.canonical_id() == wanted)
                .map(|t| t.frontmatter.claim.clone().unwrap_or_else(|| t.id()))
        })
        // A claim is written as a sentence and usually ends in a full stop.
        // Joining them with punctuation of our own produced "generalising..".
        .map(|c| c.trim().trim_end_matches('.').to_string())
        .filter(|c| !c.is_empty())
        .collect();
    if !claims.is_empty() {
        out.push_str(&format!(" Written from: {}.", claims.join("; ")));
    }

    match review::standing(&fm.review) {
        Some(e) if e.verdict == "approved" => {
            out.push_str(&format!(" Approved by {} on {}.", e.by, e.at));
        }
        // Unreachable while the gate above holds; stated rather than assumed,
        // because a silent fall-through here would publish unsigned work with
        // a notice implying somebody signed it.
        _ => out.push_str(" Not approved."),
    }
    out.push_str("*\n");
    out
}

/// A starting landing page, so the first export serves something at `/`.
///
/// Deliberately plain and short. It exists so the site works, and says it is
/// meant to be replaced — a generated front page that tried to be good would
/// be one more thing asserting facts nobody maintains.
fn landing_page(published: &[&wiki::LoadedArticle]) -> String {
    let mut domains: Vec<&str> = published
        .iter()
        .filter_map(|a| a.article.frontmatter.domain.as_deref())
        .collect();
    domains.sort_unstable();
    domains.dedup();

    let mut out = String::from("---\ntitle: Home\n---\n\n");
    out.push_str(&format!(
        "{} article(s) across {}.\n\n",
        published.len(),
        if domains.is_empty() {
            "no domains yet".to_string()
        } else {
            domains.join(", ")
        }
    ));
    out.push_str(
        "*`sentinel export` wrote this page because the destination had none. \
         Replace it — it will not be overwritten.*\n",
    );
    out
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
/// Every file under `dir`, whatever its extension.
///
/// Sources are copied verbatim, so a `.txt` or a `.pdf` in the destination is
/// as much this tool's output as a `.md` is — and as much of a problem when it
/// should no longer be there.
fn existing_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
        if entry.file_type().is_file() {
            out.push(entry.into_path());
        }
    }
    out.sort();
    out
}

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
