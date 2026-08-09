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

impl std::str::FromStr for Action {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fix-errors" => Ok(Action::FixErrors),
            "compile" => Ok(Action::Compile),
            "write" => Ok(Action::Write),
            "connect" => Ok(Action::Connect),
            "review" => Ok(Action::Review),
            other => Err(format!(
                "unknown action '{other}' (expected fix-errors, compile, write, connect, or review)"
            )),
        }
    }
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

#[derive(Serialize, Clone)]
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
    /// True when the caller asked for this action rather than being recommended
    /// it, so a consumer can tell a scheduling choice from sentinel's advice.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    requested: bool,
}

pub fn run(requested: Option<Action>) -> io::Result<()> {
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

    // Each category is built independently so `--action` can return any of
    // them. Scheduling across categories belongs to the caller: `next` ranks,
    // it does not budget. A large ingest makes `compile` win every time, and a
    // loop that follows the recommendation blindly never reaches `write` —
    // which is the step the archive actually grows by.
    let build = |action: Action| -> Option<Recommendation> {
        match action {
            Action::FixErrors if !errors.is_empty() => Some(fix_errors(&errors, &backlog)),
            Action::Compile if !uncompiled.is_empty() => Some(compile(&uncompiled, &backlog)),
            Action::Write if !wanted.is_empty() => Some(write_gap(&wanted, &backlog)),
            Action::Connect if !orphans.is_empty() => Some(connect(&orphans, &backlog)),
            Action::Review if !stale.is_empty() => Some(review(&stale, &backlog)),
            _ => None,
        }
    };

    if let Some(action) = requested {
        let mut rec = build(action).unwrap_or_else(|| Recommendation {
            action: Action::None,
            reason: format!("Nothing outstanding for '{}'.", action.as_str()),
            targets: Vec::new(),
            suggested_command: None,
            backlog: backlog.clone(),
            requested: true,
        });
        rec.requested = true;
        if output::is_json() {
            return output::emit("next", rec);
        }
        report_human(&rec);
        return Ok(());
    }

    // Priority order. Errors first because every later judgement is made on
    // data the errors call into question; compile before write because an
    // uncompiled source is knowledge already in hand.
    let recommendation = build(Action::FixErrors)
        .or_else(|| build(Action::Compile))
        .or_else(|| build(Action::Write))
        .or_else(|| build(Action::Connect))
        .or_else(|| build(Action::Review))
        .unwrap_or_else(|| Recommendation {
            action: Action::None,
            reason: "Nothing outstanding. Every source is compiled, every link resolves, and no draft has stalled.".to_string(),
            targets: Vec::new(),
            suggested_command: None,
            backlog: backlog.clone(),
            requested: false,
        });

    if output::is_json() {
        return output::emit("next", recommendation);
    }

    report_human(&recommendation);
    Ok(())
}

fn fix_errors(errors: &[&lint::Finding], backlog: &[BacklogEntry]) -> Recommendation {
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
        backlog: backlog.to_vec(),
        requested: false,
    }
}

fn compile(
    uncompiled: &[&crate::core::manifest::ManifestEntry],
    backlog: &[BacklogEntry],
) -> Recommendation {
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
        backlog: backlog.to_vec(),
        requested: false,
    }
}

/// The wiki naming its own gaps: a wikilink with no article behind it is
/// existing knowledge asking for the next article, ranked by demand.
fn write_gap(wanted: &[links::WantedArticle], backlog: &[BacklogEntry]) -> Recommendation {
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
        backlog: backlog.to_vec(),
        requested: false,
    }
}

fn connect(orphans: &[&LoadedArticle], backlog: &[BacklogEntry]) -> Recommendation {
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
        backlog: backlog.to_vec(),
        requested: false,
    }
}

fn review(stale: &[(&LoadedArticle, String)], backlog: &[BacklogEntry]) -> Recommendation {
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
        backlog: backlog.to_vec(),
        requested: false,
    }
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
                .get(&a.canonical_slug())
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
