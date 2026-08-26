use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::compilation::Compilation;
use crate::core::links::{self, LinkGraph};
use crate::core::lint::{self, Severity};
use crate::core::manifest::Manifest;
use crate::core::output;
use crate::core::persona;
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
    /// The author's own documents no persona trait has been read from.
    Learn,
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
            "learn" => Ok(Action::Learn),
            "compile" => Ok(Action::Compile),
            "write" => Ok(Action::Write),
            "connect" => Ok(Action::Connect),
            "review" => Ok(Action::Review),
            // Derived from the ladder, so a rung added above cannot leave this
            // message naming four of five actions.
            other => Err(format!(
                "unknown action '{other}' (expected {})",
                Action::LADDER
                    .iter()
                    .map(|a| a.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

impl Action {
    /// The priority ladder, in order.
    ///
    /// One list. `run` walked it as a chain of `or_else`, the backlog built it
    /// as an array, and the dashboard needed it again — three orderings that
    /// could disagree. #39 was two of five actions missing from a counter
    /// written the same way.
    pub const LADDER: &'static [Action] = &[
        Action::FixErrors,
        Action::Compile,
        // Below `compile`, above `write`. docs/clone.md originally argued for
        // the top of the ladder — "a corpus read after the fact shaped
        // nothing" — and that was overstated. Compiling a document *is* the
        // close reading that makes mining it cheap, and what a thin profile
        // actually degrades is generated work, which sits below this. What
        // `learn` does earn is a place above `write`: the profile shapes how
        // the next article is written.
        Action::Learn,
        Action::Write,
        Action::Connect,
        Action::Review,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Action::FixErrors => "fix-errors",
            Action::Learn => "learn",
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
pub struct Target {
    pub id: String,
    pub label: String,
    /// Why this target in particular — link demand, source title, age.
    pub detail: String,
    /// Total referrers, when `refs` is a sample of them. A truncated list that
    /// did not say so would read as complete.
    #[serde(skip_serializing_if = "is_zero")]
    pub ref_count: usize,
    /// Articles that reference this target, for `write`.
    ///
    /// Without these the recommendation is not actionable on its own:
    /// `/sentinel-grow` is told to read a gap's referrers before writing it,
    /// because they define what the concept means *here* rather than in
    /// general — and a bare count cannot be read.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
    /// Spellings actually used for this target, when they differ from the
    /// canonical slug. Tells the writer what the article will be called and
    /// flags inconsistent naming worth tidying.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<String>,
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
pub struct BacklogEntry {
    pub action: Action,
    pub count: usize,
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
pub struct Progress {
    pub wiki_articles: usize,
    pub raw_documents: usize,
    pub uncompiled: usize,
    pub errors: usize,
    /// Articles nothing links to. Moves when `connect` does its work — which
    /// changes no other counter, so without this a correct `connect` iteration
    /// registers as no progress at all.
    pub orphans: usize,
    /// Articles still `draft`. Moves when `review` promotes one, for the same
    /// reason.
    pub drafts: usize,
    /// Documents the author wrote that no persona trait has been read from.
    /// Moves when `learn` does its work — which changes nothing else, so
    /// without this a correct `learn` iteration reads as no progress and halts
    /// the loop.
    pub unmined: usize,
    /// Set when the link graph exists but could not be parsed. Orphans could
    /// not be counted, so `connect` is absent from the backlog for that reason
    /// rather than because there is nothing to do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_graph_error: Option<String>,
    /// Set when the graph parsed but no longer matches disk. Separate from the
    /// error above so an agent can tell "nothing could be counted" from "this
    /// count describes an older archive".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_graph_stale: Option<String>,
    /// Files under wiki/ that could not be read. Every count above, and the
    /// whole ladder below, is computed without them — so a non-zero value means
    /// the recommendation describes a smaller archive than the one on disk.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unreadable: Vec<wiki::Unreadable>,
}

#[derive(Serialize)]
pub struct Recommendation {
    pub action: Action,
    pub reason: String,
    /// Targets in this category, before `MAX_TARGETS` was applied. Present
    /// because `targets` is a sample: a truncated list that does not say so
    /// reads as complete, which is the same reason `Target::ref_count` exists.
    pub target_count: usize,
    pub targets: Vec<Target>,
    /// The skill invocation that would act on this, ready to run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_command: Option<String>,
    /// Every category with outstanding work, in priority order.
    pub backlog: Vec<BacklogEntry>,
    /// What the archive contains right now. Compare across iterations to tell
    /// real progress from churn.
    pub progress: Progress,
    /// True when the caller asked for this action rather than being recommended
    /// it, so a consumer can tell a scheduling choice from sentinel's advice.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub requested: bool,
}

pub fn run(requested: Option<Action>) -> io::Result<()> {
    let recommendation = recommend(requested)?;
    if output::is_json() {
        return output::emit("next", recommendation);
    }
    report_human(&recommendation);
    Ok(())
}

/// The recommendation, as a value.
///
/// Split out from `run` so `sentinel index` can put the same numbers in the
/// dashboard it generates. A second implementation of "what is worth doing"
/// would drift from this one, and the dashboard is exactly the surface where a
/// stale definition would go unnoticed — nobody diffs a generated page against
/// a command they did not run.
pub fn recommend(requested: Option<Action>) -> io::Result<Recommendation> {
    // Not `unwrap_or_default()`. That discarded both the error and the list of
    // files that could not be read, so `next` ranked the whole archive from
    // whatever happened to be legible and said nothing about the rest.
    let loaded = wiki::load_all()?;
    let persona_loaded = persona::load_all()?;
    // Both directories feed one disclosure. A trait that could not be read
    // costs the same thing an article does — every count below is computed
    // without it — so it belongs in the same list rather than a second one a
    // caller has to remember to check.
    let mut unreadable = loaded.unreadable.clone();
    unreadable.extend(persona_loaded.unreadable.iter().cloned());
    unreadable.sort_by(|a, b| a.path.cmp(&b.path));
    let articles = loaded.articles;
    let manifest = Manifest::load()?;

    let findings = lint::analyze(
        &articles,
        &persona_loaded.traits,
        &manifest,
        &crate::core::paths::archive_root(),
    );
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .collect();

    let coverage = persona::Coverage::derive(&persona_loaded.traits, &manifest);
    let unmined = coverage.unmined();
    let compilation = Compilation::derive(&articles, &manifest);
    let uncompiled = compilation.uncompiled(&manifest);
    let wanted = links::wanted(&articles);
    let (mut orphans, graph_error, graph_stale) = orphan_articles(&articles);
    // An orphan is a task only if the archive holds something that could link
    // to it. With one article there is nothing to link *from*, so `connect`
    // recommended a repair no amount of work could make: the same target on
    // every run, forever, which is also `sentinel-grow`'s "same target twice"
    // stop condition firing on the tool's own advice.
    if articles.len() < 2 {
        orphans.clear();
    }
    let stale = stale_drafts(&articles);

    let progress = Progress {
        wiki_articles: articles.len(),
        raw_documents: manifest.count(),
        uncompiled: uncompiled.len(),
        errors: errors.len(),
        orphans: orphans.len(),
        drafts: articles
            .iter()
            .filter(|a| a.article.frontmatter.status.as_deref() == Some("draft"))
            .count(),
        unmined: unmined.len(),
        link_graph_error: graph_error,
        link_graph_stale: graph_stale,
        unreadable: unreadable.clone(),
    };

    let backlog: Vec<BacklogEntry> = [
        (Action::FixErrors, errors.len()),
        (Action::Learn, unmined.len()),
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
            Action::Learn if !unmined.is_empty() => {
                Some(learn(&unmined, &manifest, &backlog, &progress))
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
        return Ok(rec);
    }

    // Priority order. Errors first because every later judgement is made on
    // data the errors call into question; compile before write because an
    // uncompiled source is knowledge already in hand.
    Ok(Action::LADDER
        .iter()
        .find_map(|action| build(*action))
        .unwrap_or_else(|| Recommendation {
            action: Action::None,
            reason: nothing_outstanding(&articles, manifest.count()),
            target_count: 0,
            targets: Vec::new(),
            suggested_command: None,
            backlog: backlog.clone(),
            progress: progress.clone(),
            requested: false,
        }))
}

/// The terminal message.
///
/// "Nothing outstanding" is true of the mechanical work and can still be
/// misleading: an archive where every article is a draft is complete by every
/// measure the tool takes and has not been reviewed by anyone. Say so rather
/// than let the silence imply otherwise.
fn nothing_outstanding(articles: &[LoadedArticle], raw_documents: usize) -> String {
    // An archive with nothing in it has not finished; it has not begun. Saying
    // "nothing outstanding" to somebody who just ran `init` is the worst
    // available first message, because it reads as "you are done".
    if articles.is_empty() && raw_documents == 0 {
        return "This archive is empty. Bring in a source document to start:\n  \
                sentinel ingest <file> -d <domain> -t \"<title>\"\n\
                Then `sentinel next` will have something to recommend."
            .to_string();
    }

    let base = "Nothing outstanding. Every source is compiled, every link resolves, and no draft has stalled.";
    let drafts = articles
        .iter()
        .filter(|a| a.article.frontmatter.status.as_deref() == Some("draft"))
        .count();
    if !articles.is_empty() && drafts == articles.len() {
        return format!(
            "{base}\n  Note: all {drafts} article(s) are still `draft` — the \
             mechanical work is done, the reading has not been."
        );
    }
    base.to_string()
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

fn learn(
    unmined: &[&str],
    manifest: &crate::core::manifest::Manifest,
    backlog: &[BacklogEntry],
    progress: &Progress,
) -> Recommendation {
    // Oldest first. Unlike `write`, there is no demand signal here — nothing in
    // the archive asks to be read before anything else — so the ranking is
    // simply the order the corpus arrived. Saying that plainly beats inventing
    // a relevance score the archive cannot actually support.
    let mut ordered: Vec<&str> = unmined.to_vec();
    ordered.sort_by_key(|p| {
        manifest
            .entries
            .get(*p)
            .map(|e| e.ingested_at.clone())
            .unwrap_or_default()
    });

    Recommendation {
        action: Action::Learn,
        // Names the field, not just the idea. `ingest` and `sync` default to
        // `origin: authored`, so a research paper brought in without `-o`
        // lands here looking like the author's own writing — and a reader who
        // sees it listed can tell at once that the origin is what is wrong.
        reason: format!(
            "{} document(s) registered `origin: authored` that no persona trait has \
             been read from — the clone cannot write in a voice it has not read",
            unmined.len()
        ),
        target_count: unmined.len(),
        targets: ordered
            .iter()
            .take(MAX_TARGETS)
            .map(|path| {
                let entry = manifest.entries.get(*path);
                let title = entry.map_or_else(|| (*path).to_string(), |e| e.title.clone());
                // Whether it is already compiled is context, not ranking: an
                // agent that has just written an article from this document
                // has its content in hand and can mine it cheaply.
                let compiled = entry.is_some_and(|e| !e.wiki_articles.is_empty());
                let detail = if compiled {
                    format!(
                        "{} · already compiled, so its content is in hand",
                        entry.map_or("", |e| e.domain.as_str())
                    )
                } else {
                    entry.map_or(String::new(), |e| e.domain.clone())
                };
                Target::plain((*path).to_string(), title, detail)
            })
            .collect(),
        suggested_command: ordered.first().map(|p| format!("/sentinel-clone {p}")),
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
        suggested_command: Some("/sentinel-improve orphans".to_string()),
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
        suggested_command: Some("/sentinel-improve stale drafts".to_string()),
        backlog: backlog.to_vec(),
        progress: progress.clone(),
        requested: false,
    }
}

fn report_human(rec: &Recommendation) {
    if let Some(note) = &rec.progress.link_graph_stale {
        println!("  {} {note}", "!".yellow());
    }
    wiki::warn_partial(
        &rec.progress.unreadable,
        "this recommendation was ranked without them",
    );
    if let Some(note) = &rec.progress.link_graph_error {
        println!("  {} {note}", "!".red());
        println!("     orphans could not be counted.\n");
    }
    if rec.action == Action::None {
        // A tick means "done". An archive nobody has put anything in yet is at
        // the start, not the end, and the marker is the first thing read.
        let empty = rec.progress.wiki_articles == 0 && rec.progress.raw_documents == 0;
        let marker = if empty { "→".cyan() } else { "✓".green() };
        println!("{marker} {}", rec.reason);
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
#[allow(clippy::type_complexity)]
fn orphan_articles(
    articles: &[LoadedArticle],
) -> (Vec<&LoadedArticle>, Option<String>, Option<String>) {
    let graph = match LinkGraph::load() {
        Ok(graph) => graph,
        // A corrupt graph is not an empty one. Returning no orphans would drop
        // `connect` from the backlog entirely, so the caller is told instead.
        Err(e) => return (Vec::new(), Some(links::corrupt_graph_note(&e)), None),
    };
    // The comment above applies just as much here. An unbuilt graph yields no
    // orphans, which drops `connect` out of the backlog — the very outcome the
    // corrupt branch exists to prevent. It was silent about it.
    let stale = links::staleness(&graph, articles);
    if graph.forward.is_empty() {
        return (Vec::new(), None, stale.note());
    }
    // A stale graph still has real orphans in it; they are just computed
    // against an older archive. Report them, and say so.
    (orphans_from(&graph, articles), None, stale.note())
}

fn orphans_from<'a>(graph: &LinkGraph, articles: &'a [LoadedArticle]) -> Vec<&'a LoadedArticle> {
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
            // Unparseable dates are an `invalid-date` lint error, and
            // `fix-errors` outranks `review`, so they are surfaced there
            // rather than silently dropped here.
            let date = crate::core::frontmatter::parse_date(updated).ok()?;
            ((today - date).num_days() > STALE_DRAFT_DAYS).then(|| (a, updated.to_string()))
        })
        .collect();
    // Oldest first — the most stalled draft is the most worth revisiting.
    stale.sort_by(|(a, da), (b, db)| da.cmp(db).then_with(|| a.rel_path().cmp(b.rel_path())));
    stale
}
