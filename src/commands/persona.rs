//! `sentinel persona` — read the archive's model of its author.
//!
//! What `sentinel schema` is to an article, this is to the person: the thing an
//! agent reads before writing anything meant to sound like them. It also
//! reports how much of their own writing the profile was actually read out of,
//! because a profile drawn from two essays out of forty is a profile of two
//! essays.

use std::collections::BTreeMap;
use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::manifest::Manifest;
use crate::core::output;
use crate::core::persona::{self, Coverage};
use crate::core::wiki;

/// Unmined sources listed. The count is always exact; this is the sample.
const MAX_UNMINED: usize = 10;

#[derive(Serialize)]
struct Trait {
    id: String,
    path: String,
    kind: String,
    claim: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    confidence: Option<String>,
    evidence: Vec<String>,
}

/// How much of the author's writing the profile rests on.
#[derive(Serialize)]
struct CoverageReport {
    /// Raw documents eligible as evidence — the author's own writing.
    eligible: usize,
    /// Those some non-rejected trait cites.
    mined: usize,
    /// Eligible documents nothing has read yet. The true total, always.
    unmined_count: usize,
    /// A sample of them, capped at `MAX_UNMINED`.
    unmined: Vec<String>,
}

#[derive(Serialize)]
struct Profile {
    count: usize,
    /// Traits per `status`, so a caller can see at a glance how much of this
    /// the author has actually agreed to.
    by_status: BTreeMap<String, usize>,
    by_kind: BTreeMap<String, usize>,
    traits: Vec<Trait>,
    coverage: CoverageReport,
    /// Trait files that could not be read. Every count above excludes them, so
    /// a non-zero value means this describes a smaller profile than the one on
    /// disk — and "the author holds nothing about X" is the one wrong answer
    /// that reads exactly like a right one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unreadable: Vec<wiki::Unreadable>,
}

pub fn run(kind_filter: Option<&str>, affirmed_only: bool) -> io::Result<i32> {
    // Validated against the published set, for the same reason `--rule` and
    // `--action` are: an unknown filter must not read as a category with
    // nothing in it.
    if let Some(kind) = kind_filter
        && !persona::KINDS.contains(&kind)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Unknown kind '{kind}'. Expected one of: {}.\n\
                 Run `sentinel schema` for what each one means.",
                persona::KINDS.join(", ")
            ),
        ));
    }

    let loaded = persona::load_all()?;
    let manifest = Manifest::load()?;
    let coverage = Coverage::derive(&loaded.traits, &manifest);

    // Counts describe the whole profile; the filters narrow what is listed,
    // never what is counted. A `--kind belief` view must not make a profile
    // look like it holds three things.
    let mut by_status: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for t in &loaded.traits {
        *by_status.entry(t.status().to_string()).or_default() += 1;
        *by_kind.entry(t.kind().to_string()).or_default() += 1;
    }

    let shown: Vec<&persona::LoadedTrait> = loaded
        .traits
        .iter()
        .filter(|t| kind_filter.is_none_or(|k| t.kind() == k))
        .filter(|t| !affirmed_only || t.is_affirmed())
        .collect();

    let unmined = coverage.unmined();
    let profile = Profile {
        count: loaded.traits.len(),
        by_status,
        by_kind,
        traits: shown
            .iter()
            .map(|t| Trait {
                id: t.id(),
                path: t.rel_path.clone(),
                kind: t.kind().to_string(),
                claim: t.frontmatter.claim.clone().unwrap_or_default(),
                status: t.status().to_string(),
                confidence: t.frontmatter.confidence.clone(),
                evidence: t.frontmatter.evidence.clone(),
            })
            .collect(),
        coverage: CoverageReport {
            eligible: coverage.eligible.len(),
            mined: coverage.mined.len(),
            unmined_count: unmined.len(),
            unmined: unmined
                .iter()
                .take(MAX_UNMINED)
                .map(|s| (*s).to_string())
                .collect(),
        },
        unreadable: loaded.unreadable.clone(),
    };

    if output::is_json() {
        output::emit("persona", profile)?;
        return Ok(0);
    }

    if profile.count == 0 {
        println!("{}", "No persona traits yet.".bold());
        println!(
            "\n  The archive holds no model of its author, so nothing can be\n  \
             written in their voice. Traits are read out of their own writing\n  \
             in `raw/` — {} eligible document(s) are registered.",
            profile.coverage.eligible
        );
        println!("\n  see: {}", "sentinel schema".cyan());
        wiki::warn_partial(&profile.unreadable, "the profile above may be short");
        return Ok(0);
    }

    println!(
        "{} — {} trait(s), read from {} of {} of the author's own document(s)",
        "Persona".bold(),
        profile.count,
        profile.coverage.mined,
        profile.coverage.eligible
    );

    let standing: Vec<String> = persona::STATUSES
        .iter()
        .map(|s| {
            let n = profile.by_status.get(*s).copied().unwrap_or(0);
            let label = format!("{s} {n}");
            match *s {
                "affirmed" if n > 0 => label.green().to_string(),
                "rejected" if n > 0 => label.dimmed().to_string(),
                _ => label,
            }
        })
        .collect();
    println!("  {}", standing.join("   "));

    // Grouped by kind, in the order the contract publishes them, so the same
    // profile always reads the same way.
    for kind in persona::KINDS {
        let group: Vec<&Trait> = profile.traits.iter().filter(|t| t.kind == *kind).collect();
        if group.is_empty() {
            continue;
        }
        println!("\n{} ({})", kind.bold(), group.len());
        for t in group {
            let mark = match t.status.as_str() {
                "affirmed" => "*".green(),
                "rejected" => "x".dimmed(),
                _ => "?".yellow(),
            };
            println!("  {mark} {}", t.id.cyan());
            println!("      {}", t.claim);
            let confidence = t.confidence.as_deref().unwrap_or("unstated");
            println!(
                "      {}",
                format!(
                    "{} source(s) · {confidence} confidence · {}",
                    t.evidence.len(),
                    t.path
                )
                .dimmed()
            );
        }
    }

    // A trait whose kind is absent or invalid still has to appear. Filtering by
    // a fixed list and printing nothing else would hide it from the one command
    // meant to show the whole profile, while `lint` reports it and nobody looks.
    let uncategorised: Vec<&Trait> = profile
        .traits
        .iter()
        .filter(|t| !persona::KINDS.contains(&t.kind.as_str()))
        .collect();
    if !uncategorised.is_empty() {
        println!("\n{} ({})", "uncategorised".red(), uncategorised.len());
        for t in uncategorised {
            println!("  {} {} — `sentinel lint` says why", "!".red(), t.path);
        }
    }

    if profile.coverage.unmined_count > 0 {
        println!(
            "\n{} — {} of the author's document(s) no trait has been read from",
            "Unread".bold(),
            profile.coverage.unmined_count
        );
        for path in &profile.coverage.unmined {
            println!("  {path}");
        }
        if profile.coverage.unmined_count > profile.coverage.unmined.len() {
            println!(
                "  {}",
                format!(
                    "... and {} more",
                    profile.coverage.unmined_count - profile.coverage.unmined.len()
                )
                .dimmed()
            );
        }
    }

    wiki::warn_partial(&profile.unreadable, "the profile above may be short");
    Ok(0)
}
