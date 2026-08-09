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

/// Referrer paths listed per target. The count is always exact; this is the
/// sample an agent reads to learn what the concept means in this archive.
const MAX_REFS: usize = 5;

#[derive(Serialize)]
struct Target {
    id: String,
    label: String,
    /// Why this target in particular — link demand, source title, age.
    detail: String,
    /// Total referrers, when `refs` is a sample of them. A truncated list that
    /// did not say so would read as complete.
    #[serde(skip_serializing_if = "is_zero")]
    ref_count: usize,
    /// Articles that reference this target, for `write`.
    ///
    /// Without these the recommendation is not actionable on its own:
    /// `/sentinel-grow` is told to read a gap's referrers before writing it,
    /// because they define what the concept means *here* rather than in
    /// general — and a bare count cannot be read.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    refs: Vec<String>,
    /// Spellings actually used for this target, when they differ from the
    /// canonical slug. Tells the writer what the article will be called and
    /// flags inconsistent naming worth tidying.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    variants: Vec<String>,
}

impl Target {
    /// A target with nothing to say beyond its own description.
    fn plain(id: String, label: String, detail: String) -> Self {
        Self {
            id,
            label,
            detail,
            ref_count: 0,
            refs: Vec::new(),
            variants: Vec::new(),
        }
    }
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[derive(Serialize, Clone)]
struct BacklogEntry {
    action: Action,
    count: usize,
}

/// Counters describing what the archive *contains*, alongside what is left to
/// do.
///
/// A loop needs both, and needs them from the same call. Measuring iteration
/// progress by backlog size alone is wrong: an article that fills one gap and
/// legitimately opens three grows the backlog while making the archive
/// substantially richer. Progress is the archive advancing, not the queue
/// shrinking.
#[derive(Serialize, Clone)]
struct Progress {
    wiki_articles: usize,
    raw_documents: usize,
    uncompiled: usize,
    errors: usize,
    /// Set when the link graph exists but could not be parsed. Orphans could
    /// not be counted, so `connect` is absent from the backlog for that reason
    /// rather than because there is nothing to do.
    #[serde(skip_serializing_if = "Option::is_none")]
    link_graph_error: Option<String>,
}

#[derive(Serialize)]
struct Recommendation {
    action: Action,
    reason: String,
    /// Targets in this category, before `MAX_TARGETS` was applied. Present
    /// because `targets` is a sample: a truncated list that does not say so
    /// reads as complete, which is the same reason `Target::ref_count` exists.
    target_count: usize,
    targets: Vec<Target>,
    /// The skill invocation that would act on this, ready to run.
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_command: Option<String>,
    /// Every category with outstanding work, in priority order.
    backlog: Vec<BacklogEntry>,
    /// What the archive contains right now. Compare across iterations to tell
    /// real progress from churn.
    progress: Progress,
    /// True when the caller asked for this action rather than being recommended
    /// it, so a consumer can tell a scheduling choice from sentinel's advice.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    requested: bool,
}

pub fn run(requested: Option<Action>) -> io::Result<()> {
    let articles = wiki::load_all().unwrap_or_default().articles;
    let manifest = Manifest::load()?;

    let findings = lint::analyze(&articles, &manifest);
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .collect();

    let compilation = Compilation::derive(&articles, &manifest);
    let uncompiled = compilation.uncompiled(&manifest);
    let wanted = links::wanted(&articles);
    let (orphans, graph_error) = orphan_articles(&articles);
    let stale = stale_drafts(&articles);

    let progress = Progress {
        wiki_articles: articles.len(),
        raw_documents: manifest.count(),
        uncompiled: uncompiled.len(),
        errors: errors.len(),
        link_graph_error: graph_error,
    };

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
            Action::FixErrors if !errors.is_empty() => {
                Some(fix_errors(&errors, &backlog, &progress))
            }
            Action::Compile if !uncompiled.is_empty() => {
                Some(compile(&uncompiled, &backlog, &progress))
            }
            Action::Write if !wanted.is_empty() => Some(write_gap(&wanted, &backlog, &progress)),
            Action::Connect if !orphans.is_empty() => Some(connect(&orphans, &backlog, &progress)),
            Action::Review if !stale.is_empty() => Some(review(&stale, &backlog, &progress)),
            _ => None,
        }
    };

    if let Some(action) = requested {
        let mut rec = build(action).unwrap_or_else(|| Recommendation {
            action: Action::None,
            reason: format!("Nothing outstanding for '{}'.", action.as_str()),
            target_count: 0,
            targets: Vec::new(),
            suggested_command: None,
            backlog: backlog.clone(),
            progress: progress.clone(),
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
            target_count: 0,
            targets: Vec::new(),
            suggested_command: None,
            backlog: backlog.clone(),
            progress: progress.clone(),
            requested: false,
        });

    if output::is_json() {
        return output::emit("next", recommendation);
    }

    report_human(&recommendation);
    Ok(())
}

fn fix_errors(
    errors: &[&lint::Finding],
    backlog: &[BacklogEntry],
    progress: &Progress,
) -> Recommendation {
    Recommendation {
        action: Action::FixErrors,
        reason: format!(
            "{} lint error(s) — the archive is malformed and later steps would build on bad data",
            errors.len()
        ),
        target_count: errors.len(),
        targets: errors
            .iter()
            .take(MAX_TARGETS)
            .map(|f| {
                Target::plain(
                    f.path.clone().unwrap_or_else(|| f.rule.to_string()),
                    f.rule.to_string(),
                    f.message.clone(),
                )
            })
            .collect(),
        suggested_command: Some("sentinel lint".to_string()),
        backlog: backlog.to_vec(),
        progress: progress.clone(),
        requested: false,
    }
}

fn compile(
    uncompiled: &[&crate::core::manifest::ManifestEntry],
    backlog: &[BacklogEntry],
    progress: &Progress,
) -> Recommendation {
    Recommendation {
        action: Action::Compile,
        reason: format!(
            "{} raw document(s) that no wiki article cites",
            uncompiled.len()
        ),
        target_count: uncompiled.len(),
        targets: uncompiled
            .iter()
            .take(MAX_TARGETS)
            .map(|e| {
                Target::plain(
                    e.raw_path.clone(),
                    e.title.clone(),
                    format!("{} · {}", e.domain, e.origin),
                )
            })
            .collect(),
        suggested_command: uncompiled
            .first()
            .map(|e| format!("/sentinel-compile {}", e.raw_path)),
        backlog: backlog.to_vec(),
        progress: progress.clone(),
        requested: false,
    }
}

/// The wiki naming its own gaps: a wikilink with no article behind it is
/// existing knowledge asking for the next article, ranked by demand.
fn write_gap(
    wanted: &[links::WantedArticle],
    backlog: &[BacklogEntry],
    progress: &Progress,
) -> Recommendation {
    let top = &wanted[0];
    Recommendation {
        action: Action::Write,
        reason: format!(
            "{} concept(s) linked but not yet written; '{}' is referenced by {} article(s)",
            wanted.len(),
            top.slug,
            top.referrers.len()
        ),
        target_count: wanted.len(),
        targets: wanted
            .iter()
            .take(MAX_TARGETS)
            .map(|w| Target {
                id: w.slug.clone(),
                label: w.slug.clone(),
                detail: format!(
                    "referenced by {} article{}",
                    w.referrers.len(),
                    if w.referrers.len() == 1 { "" } else { "s" }
                ),
                ref_count: w.referrers.len(),
                refs: w.referrers.iter().take(MAX_REFS).cloned().collect(),
                variants: w.variants.clone(),
            })
            .collect(),
        suggested_command: Some(format!("/sentinel-research {}", top.slug)),
        backlog: backlog.to_vec(),
        progress: progress.clone(),
        requested: false,
    }
}

fn connect(
    orphans: &[&LoadedArticle],
    backlog: &[BacklogEntry],
    progress: &Progress,
) -> Recommendation {
    Recommendation {
        action: Action::Connect,
        reason: format!(
            "{} article(s) that nothing links to — knowledge that cannot be reached by following the graph",
            orphans.len()
        ),
        target_count: orphans.len(),
        targets: orphans
            .iter()
            .take(MAX_TARGETS)
            .map(|a| {
                Target::plain(
                    a.rel_path().to_string(),
                    a.title().to_string(),
                    "no incoming links".to_string(),
                )
            })
            .collect(),
        suggested_command: Some("/sentinel-improve connect orphan pages".to_string()),
        backlog: backlog.to_vec(),
        progress: progress.clone(),
        requested: false,
    }
}

fn review(
    stale: &[(&LoadedArticle, String)],
    backlog: &[BacklogEntry],
    progress: &Progress,
) -> Recommendation {
    Recommendation {
        action: Action::Review,
        reason: format!(
            "{} draft(s) untouched for over {STALE_DRAFT_DAYS} days",
            stale.len()
        ),
        target_count: stale.len(),
        targets: stale
            .iter()
            .take(MAX_TARGETS)
            .map(|(a, updated)| {
                Target::plain(
                    a.rel_path().to_string(),
                    a.title().to_string(),
                    format!("last updated {updated}"),
                )
            })
            .collect(),
        suggested_command: Some("/sentinel-improve promote stale drafts".to_string()),
        backlog: backlog.to_vec(),
        progress: progress.clone(),
        requested: false,
    }
}

fn report_human(rec: &Recommendation) {
    if let Some(note) = &rec.progress.link_graph_error {
        println!("  {} {note}", "!".red());
        println!("     orphans could not be counted.\n");
    }
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
            if !target.refs.is_empty() {
                let hidden = target.ref_count.saturating_sub(target.refs.len());
                let more = if hidden > 0 {
                    format!(", and {hidden} more")
                } else {
                    String::new()
                };
                println!("    {} {}{more}", "from:".dimmed(), target.refs.join(", "));
            }
            if !target.variants.is_empty() {
                println!("    {} {}", "spelled:".dimmed(), target.variants.join(", "));
            }
        }
    }

    let hidden = rec.target_count.saturating_sub(rec.targets.len());
    if hidden > 0 {
        println!("  {}", format!("... and {hidden} more").dimmed());
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
fn orphan_articles(articles: &[LoadedArticle]) -> (Vec<&LoadedArticle>, Option<String>) {
    let graph = match LinkGraph::load() {
        Ok(graph) => graph,
        // A corrupt graph is not an empty one. Returning no orphans would drop
        // `connect` from the backlog entirely, so the caller is told instead.
        Err(e) => return (Vec::new(), Some(links::corrupt_graph_note(&e))),
    };
    if graph.forward.is_empty() {
        return (Vec::new(), None);
    }
    let orphans = articles
        .iter()
        .filter(|a| {
            graph
                .backlinks
                .get(&a.canonical_slug())
                .is_none_or(|refs| refs.is_empty())
        })
        .collect();
    (orphans, None)
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
