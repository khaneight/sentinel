use std::io;

use chrono::NaiveDate;
use colored::Colorize;
use serde::Serialize;

use crate::core::compilation::Compilation;
use crate::core::links::{self, LinkGraph};
use crate::core::lint::{self, Severity};
use crate::core::manifest::Manifest;
use crate::core::output;
use crate::core::wiki::{self, LoadedArticle};

/// A draft untouched for this long is treated as stalled rather than in progress.
const STALE_DRAFT_DAYS: i64 = 30;

/// How many targets are named in a single recommendation.
const MAX_TARGETS: usize = 5;

/// What sentinel thinks should happen next.
///
/// The priority order below is deliberate and documented in CLAUDE.md, because
/// it encodes editorial judgement about what is most worth doing. It is a
/// recommendation, not a constraint: `backlog` reports every category so a
/// caller that disagrees can pick its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    /// The archive is malformed. Nothing else is worth doing first.
    FixErrors,
    /// Raw documents no wiki article cites.
    Compile,
    /// Concepts the wiki links to but has not written.
    Write,
    /// Articles nothing links to.
    Connect,
    /// Drafts that have stopped moving.
    Review,
    /// Nothing outstanding.
    None,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Action::FixErrors => "fix-errors",
            Action::Compile => "compile",
            Action::Write => "write",
            Action::Connect => "connect",
            Action::Review => "review",
            Action::None => "none",
        }
    }
}

#[derive(Serialize)]
struct Target {
    id: String,
    label: String,
    /// Why this target in particular — link demand, source title, age.
    detail: String,
}

#[derive(Serialize)]
struct BacklogEntry {
    action: Action,
    count: usize,
}

#[derive(Serialize)]
struct Recommendation {
    action: Action,
    reason: String,
    targets: Vec<Target>,
    /// The skill invocation that would act on this, ready to run.
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_command: Option<String>,
    /// Every category with outstanding work, in priority order.
    backlog: Vec<BacklogEntry>,
}

pub fn run() -> io::Result<()> {
    let articles = wiki::load_all().unwrap_or_default();
    let manifest = Manifest::load()?;

    let findings = lint::analyze(&articles, &manifest);
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .collect();

    let compilation = Compilation::derive(&articles, &manifest);
    let uncompiled = compilation.uncompiled(&manifest);
    let wanted = links::wanted(&articles);
    let orphans = orphan_articles(&articles);
    let stale = stale_drafts(&articles);

    let backlog: Vec<BacklogEntry> = [
        (Action::FixErrors, errors.len()),
        (Action::Compile, uncompiled.len()),
        (Action::Write, wanted.len()),
        (Action::Connect, orphans.len()),
        (Action::Review, stale.len()),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(action, count)| BacklogEntry { action, count })
    .collect();

    // Priority order. Errors first because every later judgement is made on
    // data the errors call into question; compile before write because an
    // uncompiled source is knowledge already in hand.
    let recommendation = if !errors.is_empty() {
        Recommendation {
            action: Action::FixErrors,
            reason: format!(
                "{} lint error(s) — the archive is malformed and later steps would build on bad data",
                errors.len()
            ),
            targets: errors
                .iter()
                .take(MAX_TARGETS)
                .map(|f| Target {
                    id: f.path.clone().unwrap_or_else(|| f.rule.to_string()),
                    label: f.rule.to_string(),
                    detail: f.message.clone(),
                })
                .collect(),
            suggested_command: Some("sentinel lint".to_string()),
            backlog,
        }
    } else if !uncompiled.is_empty() {
        Recommendation {
            action: Action::Compile,
            reason: format!(
                "{} raw document(s) that no wiki article cites",
                uncompiled.len()
            ),
            targets: uncompiled
                .iter()
                .take(MAX_TARGETS)
                .map(|e| Target {
                    id: e.raw_path.clone(),
                    label: e.title.clone(),
                    detail: format!("{} · {}", e.domain, e.origin),
                })
                .collect(),
            suggested_command: uncompiled
                .first()
                .map(|e| format!("/sentinel-compile {}", e.raw_path)),
            backlog,
        }
    } else if !wanted.is_empty() {
        // The wiki naming its own gaps: a wikilink with no article behind it is
        // existing knowledge asking for the next article, ranked by demand.
        let top = &wanted[0];
        Recommendation {
            action: Action::Write,
            reason: format!(
                "{} concept(s) linked but not yet written; '{}' is referenced by {} article(s)",
                wanted.len(),
                top.slug,
                top.referrers.len()
            ),
            targets: wanted
                .iter()
                .take(MAX_TARGETS)
                .map(|w| Target {
                    id: w.slug.clone(),
                    label: w.slug.clone(),
                    detail: format!(
                        "referenced by {}",
                        if w.referrers.len() == 1 {
                            w.referrers[0].clone()
                        } else {
                            format!("{} articles", w.referrers.len())
                        }
                    ),
                })
                .collect(),
            suggested_command: Some(format!("/sentinel-research {}", top.slug)),
            backlog,
        }
    } else if !orphans.is_empty() {
        Recommendation {
            action: Action::Connect,
            reason: format!(
                "{} article(s) that nothing links to — knowledge that cannot be reached by following the graph",
                orphans.len()
            ),
            targets: orphans
                .iter()
                .take(MAX_TARGETS)
                .map(|a| Target {
                    id: a.rel_path().to_string(),
                    label: a.title().to_string(),
                    detail: "no incoming links".to_string(),
                })
                .collect(),
            suggested_command: Some("/sentinel-improve connect orphan pages".to_string()),
            backlog,
        }
    } else if !stale.is_empty() {
        Recommendation {
            action: Action::Review,
            reason: format!(
                "{} draft(s) untouched for over {STALE_DRAFT_DAYS} days",
                stale.len()
            ),
            targets: stale
                .iter()
                .take(MAX_TARGETS)
                .map(|(a, updated)| Target {
                    id: a.rel_path().to_string(),
                    label: a.title().to_string(),
                    detail: format!("last updated {updated}"),
                })
                .collect(),
            suggested_command: Some("/sentinel-improve promote stale drafts".to_string()),
            backlog,
        }
    } else {
        Recommendation {
            action: Action::None,
            reason: "Nothing outstanding. Every source is compiled, every link resolves, and no draft has stalled.".to_string(),
            targets: Vec::new(),
            suggested_command: None,
            backlog,
        }
    };

    if output::is_json() {
        return output::emit("next", recommendation);
    }

    report_human(&recommendation);
    Ok(())
}

fn report_human(rec: &Recommendation) {
    if rec.action == Action::None {
        println!("{} {}", "✓".green(), rec.reason);
        return;
    }

    println!("{} {}", "Next:".bold(), rec.action.as_str().cyan().bold());
    println!("  {}", rec.reason);

    if !rec.targets.is_empty() {
        println!();
        for target in &rec.targets {
            println!("  {} {}", "•".dimmed(), target.label.bold());
            println!("    {} — {}", target.id.cyan(), target.detail.dimmed());
        }
    }

    if let Some(command) = &rec.suggested_command {
        println!("\n  {} {}", "run:".dimmed(), command.green());
    }

    let rest: Vec<String> = rec
        .backlog
        .iter()
        .filter(|e| e.action != rec.action)
        .map(|e| format!("{} {}", e.count, e.action.as_str()))
        .collect();
    if !rest.is_empty() {
        println!("\n  {} {}", "also pending:".dimmed(), rest.join(", "));
    }
}

/// Articles with no incoming links, per the last-built graph.
///
/// Read from the saved graph rather than recomputed, because `orphans` is
/// defined over the graph `index` publishes — recomputing it here could
/// disagree with `_orphans.md` and leave the agent chasing a phantom.
fn orphan_articles(articles: &[LoadedArticle]) -> Vec<&LoadedArticle> {
    let Ok(graph) = LinkGraph::load() else {
        return Vec::new();
    };
    if graph.forward.is_empty() {
        return Vec::new();
    }
    articles
        .iter()
        .filter(|a| {
            graph
                .backlinks
                .get(&a.slug())
                .is_none_or(|refs| refs.is_empty())
        })
        .collect()
}

/// Drafts whose `updated` date is older than the staleness threshold.
fn stale_drafts(articles: &[LoadedArticle]) -> Vec<(&LoadedArticle, String)> {
    let today = chrono::Local::now().date_naive();
    let mut stale: Vec<(&LoadedArticle, String)> = articles
        .iter()
        .filter(|a| a.article.frontmatter.status.as_deref() == Some("draft"))
        .filter_map(|a| {
            let updated = a.article.frontmatter.updated.as_deref()?;
            let date = NaiveDate::parse_from_str(updated, "%Y-%m-%d").ok()?;
            ((today - date).num_days() > STALE_DRAFT_DAYS).then(|| (a, updated.to_string()))
        })
        .collect();
    // Oldest first — the most stalled draft is the most worth revisiting.
    stale.sort_by(|(a, da), (b, db)| da.cmp(db).then_with(|| a.rel_path().cmp(b.rel_path())));
    stale
}
