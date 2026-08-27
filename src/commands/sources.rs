//! `sentinel sources` — the raw corpus, and which of it may be published.
//!
//! `export` never copies `raw/` on its own. That directory holds whatever its
//! owner put there: material under someone else's copyright, private notes,
//! correspondence, drafts nobody was meant to see. Nothing about a file tells
//! the tool which of those it is, so there is no flag that could safely publish
//! the directory — only a decision made once per document, by hand.
//!
//! This is where that decision is made and read back. Like `sentinel review`,
//! it lists with no target and changes with one.

use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::compilation::SourceIndex;
use crate::core::manifest::Manifest;
use crate::core::output;

#[derive(Serialize)]
struct Source {
    raw_path: String,
    title: String,
    domain: String,
    origin: String,
    publish: bool,
    /// Wiki articles that cite it, derived live rather than read from the
    /// manifest's projection.
    cited_by: usize,
}

#[derive(Serialize)]
struct Listing {
    count: usize,
    /// How many are opted in. The number a reader of the site would see.
    published: usize,
    sources: Vec<Source>,
}

#[derive(Serialize)]
struct Changed {
    raw_path: String,
    publish: bool,
}

pub fn run(target: Option<&str>, publish: Option<bool>) -> io::Result<i32> {
    let mut manifest = Manifest::load()?;

    let Some(target) = target else {
        return list(&manifest);
    };

    let Some(publish) = publish else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "no change given for '{target}'. Pass --publish to allow \
                 `export --with-sources` to copy it, or --private to withdraw it."
            ),
        ));
    };

    // The same matcher `sources:` citations go through, so a document is named
    // here exactly the way an article names it.
    let resolved = SourceIndex::new(&manifest).resolve(target).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no raw document matching '{target}'. `sentinel sources --json` \
                 lists what is registered."
            ),
        )
    })?;

    let entry = manifest
        .entries
        .get_mut(&resolved)
        .expect("resolve returns a manifest key");
    let was = entry.publish;
    entry.publish = publish;
    let title = entry.title.clone();

    // Written even when nothing changed would append a log line saying a
    // decision was made that was already in force.
    if was != publish {
        manifest.save()?;
        crate::core::log::append(
            "sources",
            &format!(
                "{resolved} — {}",
                if publish { "published" } else { "withdrawn" }
            ),
        )?;
    }

    if output::is_json() {
        output::emit(
            "sources",
            Changed {
                raw_path: resolved,
                publish,
            },
        )?;
        return Ok(0);
    }

    if was == publish {
        println!(
            "{title} was already {}.",
            if publish { "published" } else { "private" }
        );
        return Ok(0);
    }
    if publish {
        println!("{} {resolved}", "published:".green());
        println!(
            "  {}",
            "readers can see this document once you run `export --with-sources`".dimmed()
        );
    } else {
        println!("{} {resolved}", "withdrawn:".yellow());
        println!(
            "  {}",
            "already-exported copies stay on disk until `export --clean` removes them".dimmed()
        );
    }
    Ok(0)
}

fn list(manifest: &Manifest) -> io::Result<i32> {
    let mut sources: Vec<Source> = manifest
        .entries
        .values()
        .map(|e| Source {
            raw_path: e.raw_path.clone(),
            title: e.title.clone(),
            domain: e.domain.clone(),
            origin: e.origin.clone(),
            publish: e.publish,
            cited_by: e.wiki_articles.len(),
        })
        .collect();
    sources.sort_by(|a, b| a.raw_path.cmp(&b.raw_path));

    let listing = Listing {
        count: sources.len(),
        published: sources.iter().filter(|s| s.publish).count(),
        sources,
    };

    if output::is_json() {
        output::emit("sources", listing)?;
        return Ok(0);
    }

    if listing.count == 0 {
        println!("{}", "No raw documents registered.".bold());
        println!(
            "\n  {}",
            "sentinel ingest <file> -d <domain> -t \"<title>\"".cyan()
        );
        return Ok(0);
    }

    println!(
        "{} — {} document(s), {} publishable",
        "Sources".bold(),
        listing.count,
        listing.published
    );
    for s in &listing.sources {
        let mark = if s.publish {
            "public".green()
        } else {
            "private".dimmed()
        };
        println!("\n  {mark}  {}", s.title.cyan());
        println!(
            "      {} · {} · cited by {} article(s)",
            s.raw_path.dimmed(),
            s.origin.dimmed(),
            s.cited_by
        );
    }
    println!(
        "\n  {}",
        "sentinel sources <path> --publish | --private".dimmed()
    );
    Ok(0)
}
