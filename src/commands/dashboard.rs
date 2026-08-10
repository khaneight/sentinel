//! `index/_dashboard.md` — the one page that answers "where is this archive".
//!
//! Everything here is generated from the same functions the commands use:
//! `next::recommend`, `status::summarize`, `lint::analyze`, `log::read`,
//! `schema::FIELDS`, `lint::RULES`. Nothing is restated. A generated page is
//! the worst place to keep a second definition of anything, because nobody
//! diffs it against a command they did not run.
//!
//! It is written for a person reading the archive in Obsidian or an editor.
//! Skills keep using `sentinel next --json`, which gives the same facts without
//! spending a page of context on them.

use std::fmt::Write as _;
use std::io;

use crate::commands::{next, schema, status};
use crate::core::lint::{self, Severity};
use crate::core::{log, paths};

/// Ceiling for the generated page.
///
/// `index/_master.md` reached 16 KB on a 400-article archive and is the file
/// skills are forbidden to read. A dashboard that grows with the archive would
/// become the same trap, so every list below is capped and this is asserted.
pub const MAX_BYTES: usize = 6_000;

/// Targets shown under the recommendation. The true total is printed beside it.
const TARGET_LIMIT: usize = 5;

/// Log entries shown. `sentinel log` is the unbounded view.
const ENTRY_LIMIT: usize = 8;

/// Render the dashboard.
///
/// `generated_at` is passed in rather than read from the clock so the caller
/// controls it and a test can assert on a fixed value.
pub fn render(generated_at: &str) -> io::Result<String> {
    let rec = next::recommend(None)?;
    let st = status::summarize()?;
    let mut out = String::with_capacity(MAX_BYTES);

    // --- 1. What this is, and when it was true -----------------------------
    // A static page's largest lie is being out of date, so it says so first.
    writeln!(out, "# Dashboard\n").ok();
    writeln!(
        out,
        "*Generated {generated_at} by `sentinel index`. It describes the \
         archive as it was then — regenerate after any change. Do not edit.*\n"
    )
    .ok();

    // --- 2. What to do next ------------------------------------------------
    writeln!(out, "## Next\n").ok();
    writeln!(out, "**{}** — {}\n", rec.action.as_str(), rec.reason).ok();
    if let Some(cmd) = &rec.suggested_command {
        writeln!(out, "```\n{cmd}\n```\n").ok();
    }
    if !rec.targets.is_empty() {
        let shown = rec.targets.len().min(TARGET_LIMIT);
        writeln!(out, "{} of {} target(s):\n", shown, rec.target_count).ok();
        for t in rec.targets.iter().take(TARGET_LIMIT) {
            writeln!(out, "- **{}** — {}", t.label, t.detail).ok();
        }
        writeln!(out).ok();
    }

    // --- 3. Backlog, every rung, including the empty ones ------------------
    // Printed from the ladder rather than from `rec.backlog`, which omits
    // categories with no work: a rung missing from the page reads as a rung
    // that does not exist.
    writeln!(out, "## Backlog\n").ok();
    writeln!(out, "| action | outstanding |").ok();
    writeln!(out, "|---|---|").ok();
    for action in next::Action::LADDER {
        let count = rec
            .backlog
            .iter()
            .find(|b| b.action == *action)
            .map(|b| b.count)
            .unwrap_or(0);
        writeln!(out, "| `{}` | {count} |", action.as_str()).ok();
    }
    writeln!(out).ok();

    // --- 4. Health ---------------------------------------------------------
    // Counts by rule, never the findings themselves — the findings are
    // unbounded and `sentinel lint` already prints them.
    writeln!(out, "## Health\n").ok();
    let articles = crate::core::wiki::load_all()?;
    let manifest = crate::core::manifest::Manifest::load()?;
    let findings = lint::analyze(&articles.articles, &manifest, &paths::archive_root());
    let errors = lint::count(&findings, Severity::Error);
    let warnings = lint::count(&findings, Severity::Warning);

    if findings.is_empty() {
        writeln!(out, "No findings.\n").ok();
    } else {
        writeln!(out, "{errors} error(s), {warnings} warning(s).\n").ok();
        for rule in lint::RULES {
            let n = findings.iter().filter(|f| f.rule == rule.rule).count();
            if n > 0 {
                writeln!(out, "- `{}` × {n} ({})", rule.rule, rule.severity.label()).ok();
            }
        }
        writeln!(out, "\nRun `sentinel lint` for the findings themselves.\n").ok();
    }

    // The disclosures the commands make, made here too. A dashboard that
    // silently describes a partial archive is the failure this repo has fixed
    // five times over.
    for note in [&st.link_graph_error, &st.link_graph_stale]
        .into_iter()
        .flatten()
    {
        writeln!(out, "> {note}\n").ok();
    }
    if !st.unreadable.is_empty() {
        writeln!(
            out,
            "> {} wiki file(s) could not be read; every count on this page \
             excludes them.\n",
            st.unreadable.len()
        )
        .ok();
    }

    // --- 5. Progress -------------------------------------------------------
    writeln!(out, "## Progress\n").ok();
    writeln!(out, "| | |").ok();
    writeln!(out, "|---|---|").ok();
    writeln!(out, "| articles | {} |", st.wiki_articles).ok();
    writeln!(out, "| raw documents | {} |", st.raw_documents).ok();
    writeln!(out, "| uncompiled | {} |", st.uncompiled).ok();
    writeln!(out, "| orphans | {} |", st.orphan_pages).ok();
    for (status_name, n) in &st.maturity {
        writeln!(out, "| status: {status_name} | {n} |").ok();
    }
    writeln!(out).ok();

    // --- 6. Recent activity ------------------------------------------------
    // `index` entries are excluded, and that is load-bearing rather than
    // cosmetic. `index` appends to the log whenever it writes, so a page
    // reporting its own generation would differ on every rebuild — breaking
    // the archive-wide guarantee that a rebuild changing nothing writes
    // nothing. The "Generated" line above already says when `index` last ran.
    let entries: Vec<log::Entry> = log::read()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.operation != "index")
        .collect();
    if !entries.is_empty() {
        writeln!(out, "## Recent activity\n").ok();
        writeln!(
            out,
            "{} of {} entries:\n",
            entries.len().min(ENTRY_LIMIT),
            entries.len()
        )
        .ok();
        for e in entries.iter().take(ENTRY_LIMIT) {
            writeln!(out, "- `{}` **{}** — {}", e.date, e.operation, e.detail).ok();
        }
        writeln!(out, "\n`sentinel log` for more.\n").ok();
    }

    // --- 7. What the agent is instructed to do -----------------------------
    // Generated from the published contract, so it cannot describe a tool that
    // behaves differently. Skills are listed by name and description only; the
    // files themselves are the text.
    writeln!(out, "## Agent directives\n").ok();
    writeln!(
        out,
        "The priority ladder `sentinel next` walks, in order: {}.\n",
        next::Action::LADDER
            .iter()
            .map(|a| format!("`{}`", a.as_str()))
            .collect::<Vec<_>>()
            .join(" → ")
    )
    .ok();

    let required: Vec<&str> = schema::FIELDS
        .iter()
        .filter(|f| f.required)
        .map(|f| f.name)
        .collect();
    writeln!(
        out,
        "Required frontmatter: {}. Full contract: `sentinel schema`.\n",
        required
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .ok();
    writeln!(
        out,
        "{} lint rules enforce it; {} are errors, the rest warnings.\n",
        lint::RULES.len(),
        lint::RULES
            .iter()
            .filter(|r| r.severity == Severity::Error)
            .count()
    )
    .ok();

    match skills() {
        entries if entries.is_empty() => {
            writeln!(
                out,
                "No skills are linked into this archive. See `.claude/skills`.\n"
            )
            .ok();
        }
        entries => {
            writeln!(out, "Skills available here:\n").ok();
            for (name, description) in entries {
                writeln!(out, "- **{name}** — {description}").ok();
            }
            writeln!(out).ok();
        }
    }

    Ok(out)
}

/// Skills linked into this archive, as (name, first sentence of description).
///
/// Read from the archive rather than the repository: what matters on this page
/// is what an agent working *here* can actually invoke.
fn skills() -> Vec<(String, String)> {
    let dir = paths::archive_root().join(".claude/skills");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let Ok(text) = std::fs::read_to_string(entry.path().join("SKILL.md")) else {
            continue;
        };
        // The block is `---`, keys, `---`. Skipping the opening fence first
        // matters: a `take_while` that stops at the first `---` stops on line
        // one and every description comes back empty.
        let field = |key: &str| -> Option<String> {
            text.lines()
                .skip(1)
                .take_while(|l| l.trim_end() != "---")
                .find_map(|l| l.strip_prefix(&format!("{key}: ")))
                .map(|v| v.trim().to_string())
        };
        let name = field("name").unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
        let description = field("description")
            .map(|d| {
                // First sentence only; several run to a paragraph.
                d.split_once(". ").map(|(a, _)| a.to_string()).unwrap_or(d)
            })
            .unwrap_or_default();
        out.push((name, description));
    }
    out.sort();
    out
}
