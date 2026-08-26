//! `sentinel review` — what is waiting on you, and your answer to it.
//!
//! The archive's owner is the only thing in this system that can say a claim
//! about them is wrong, or that a piece of writing may go out under their name.
//! Every other command is the tool's opinion. This one is theirs, and it is the
//! only writer of `review:`.
//!
//! Deliberately not reachable from any skill. An agent that can approve its own
//! work has a permission system in name only.

use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::persona::{self, LoadedTrait};
use crate::core::review::{self, Entry};
use crate::core::{atomic, output, paths, slug, wiki};

/// A document awaiting the owner's answer.
#[derive(Serialize)]
struct Pending {
    path: String,
    id: String,
    kind: &'static str,
    title: String,
    /// Why it is here, in one phrase.
    reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

#[derive(Serialize)]
struct Queue {
    count: usize,
    pending: Vec<Pending>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unreadable: Vec<wiki::Unreadable>,
}

#[derive(Serialize)]
struct Recorded {
    path: String,
    verdict: String,
    by: String,
    at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    /// Set when recording the verdict also moved a persona trait's `status:`,
    /// so a caller can see the second change rather than discover it later.
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

/// Who is signing. Never guessed.
///
/// A verdict attributed to a placeholder is worse than one that was never
/// recorded: it looks like somebody agreed. If nothing here identifies a
/// person, the command refuses and says how to supply one.
fn reviewer(explicit: Option<&str>) -> io::Result<String> {
    if let Some(by) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(by.to_string());
    }
    for var in ["SENTINEL_REVIEWER", "USER"] {
        if let Ok(value) = std::env::var(var)
            && !value.trim().is_empty()
        {
            return Ok(value.trim().to_string());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "cannot tell who is recording this verdict. Pass `--by <name>`, or set \
         SENTINEL_REVIEWER. A verdict signed by nobody is not a verdict."
            .to_string(),
    ))
}

/// A reviewable document: a wiki article or a persona trait.
struct Target {
    path: String,
    /// The persona trait, when that is what this is — its `status:` moves too.
    trait_status: Option<String>,
}

/// Find what the user named: an archive-relative path, an article slug, or a
/// trait id.
///
/// Matched through `slug::canonical`, like every other identity here. An
/// ambiguous name is reported rather than resolved: picking between an article
/// and a trait that share a name would silently record a verdict on the wrong
/// one, and a verdict is the last thing to guess about.
fn resolve(
    name: &str,
    traits: &[LoadedTrait],
    articles: &[wiki::LoadedArticle],
) -> io::Result<Target> {
    let wanted = slug::canonical(name);
    let mut matches: Vec<Target> = Vec::new();

    for t in traits {
        if t.rel_path == name || t.canonical_id() == wanted {
            matches.push(Target {
                path: t.rel_path.clone(),
                trait_status: Some(t.status().to_string()),
            });
        }
    }
    for a in articles {
        if a.rel_path() == name || a.canonical_slug() == wanted {
            matches.push(Target {
                path: a.rel_path().to_string(),
                trait_status: None,
            });
        }
    }

    match matches.len() {
        1 => Ok(matches.pop().expect("length checked")),
        0 => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "nothing called '{name}'. Give an archive-relative path, an \
                 article slug, or a trait id — `sentinel persona --json` and \
                 `sentinel search` list them."
            ),
        )),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "'{name}' is ambiguous — it matches {}. Name the path instead.",
                matches
                    .iter()
                    .map(|m| m.path.as_str())
                    .collect::<Vec<_>>()
                    .join(" and ")
            ),
        )),
    }
}

pub fn run(
    target: Option<&str>,
    verdict: Option<&str>,
    note: Option<&str>,
    by: Option<&str>,
) -> io::Result<i32> {
    let loaded = wiki::load_all()?;
    let persona_loaded = persona::load_all()?;

    let Some(target) = target else {
        return list(&loaded, &persona_loaded);
    };

    let Some(verdict) = verdict else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "no verdict given for '{target}'. Pass --approve, --reject, \
                 --request-changes, or --comment."
            ),
        ));
    };

    // Rewriting a document's durable standing, so the view it is decided from
    // has to be whole: an unreadable file could be the one this name really
    // meant, and a verdict recorded on the wrong document is worse than none.
    let articles = loaded.require_complete()?;
    let traits = persona_loaded.require_complete()?;
    let found = resolve(target, &traits, &articles)?;

    let entry = Entry {
        verdict: verdict.to_string(),
        by: reviewer(by)?,
        at: chrono::Local::now()
            .format(crate::core::frontmatter::DATE_FORMAT)
            .to_string(),
        note: note
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string),
    };

    let full = paths::archive_root().join(&found.path);
    let content = std::fs::read_to_string(&full)?;
    let mut updated = review::append(&content, &entry)?;

    // A trait carries its standing twice: `status:` is what a reader sees, and
    // the review list is the history behind it. Writing one without the other
    // is exactly the state `verdict-disagrees-with-status` reports.
    let mut new_status = None;
    if found.trait_status.is_some()
        && let Some(implied) = review::implied_status(verdict)
    {
        updated = set_trait_status(&updated, implied);
        new_status = Some(implied.to_string());
    }

    atomic::write(&full, &updated)?;
    crate::core::log::append(
        "review",
        &format!("{verdict} {} by {}", found.path, entry.by),
    )?;

    let recorded = Recorded {
        path: found.path.clone(),
        verdict: entry.verdict.clone(),
        by: entry.by.clone(),
        at: entry.at.clone(),
        note: entry.note.clone(),
        status: new_status.clone(),
    };
    if output::is_json() {
        output::emit("review", recorded)?;
        return Ok(0);
    }

    let tag = match verdict {
        "approved" => "approved".green(),
        "rejected" => "rejected".red(),
        "changes-requested" => "changes requested".yellow(),
        _ => "comment".normal(),
    };
    println!("{tag} — {} ({})", found.path.cyan(), entry.at.dimmed());
    if let Some(note) = &entry.note {
        println!("  {note}");
    }
    if let Some(status) = new_status {
        println!("  {}", format!("status is now `{status}`").dimmed());
    }
    Ok(0)
}

/// Rewrite a trait's `status:` line, or add one.
///
/// Textual and confined to the frontmatter block, like everything else that
/// edits these files by hand.
fn set_trait_status(content: &str, status: &str) -> String {
    let Some((start, end)) = crate::core::frontmatter::block_span(content) else {
        return content.to_string();
    };
    let yaml = &content[start..end];
    let mut out = String::with_capacity(yaml.len() + 24);
    let mut replaced = false;
    for line in yaml.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if !replaced && trimmed.starts_with("status:") && !trimmed.starts_with(char::is_whitespace)
        {
            out.push_str(&format!("status: {status}\n"));
            replaced = true;
            continue;
        }
        out.push_str(line);
    }
    if !replaced {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("status: {status}\n"));
    }
    format!("{}{out}{}", &content[..start], &content[end..])
}

/// What is waiting on the owner.
fn list(loaded: &wiki::Loaded, persona_loaded: &persona::Loaded) -> io::Result<i32> {
    let mut pending: Vec<Pending> = Vec::new();

    for t in &persona_loaded.traits {
        let standing = review::standing(&t.frontmatter.review);
        // A `proposed` trait is the agent's reading of a person that the
        // person has not seen. That is the queue this command exists for.
        let reason = match standing {
            Some(e) if e.verdict == "changes-requested" => "changes requested",
            Some(_) => continue,
            None if t.status() == "proposed" => "unconfirmed reading of the author",
            None => continue,
        };
        pending.push(Pending {
            path: t.rel_path.clone(),
            id: t.id(),
            kind: "trait",
            title: t.frontmatter.claim.clone().unwrap_or_else(|| t.id()),
            reason: reason.to_string(),
            note: standing.and_then(|e| e.note.clone()),
        });
    }

    for a in &loaded.articles {
        let fm = &a.article.frontmatter;
        let standing = review::standing(&fm.review);
        // Two ways an article gets here. `changes-requested` is anything the
        // owner sent back. The other is generated work with no approval —
        // which `export` will refuse to publish, so leaving it out of this
        // queue would mean the one thing that *needs* an answer is the one
        // thing this command does not mention.
        let reason = match standing {
            Some(e) if e.verdict == "changes-requested" => "changes requested",
            _ if fm.is_extrapolated() && !review::is_approved(&fm.review) => match standing {
                Some(e) if e.verdict == "rejected" => "rejected, still in the wiki",
                _ => "written by the clone, unapproved — cannot be published",
            },
            _ => continue,
        };
        pending.push(Pending {
            path: a.rel_path().to_string(),
            id: a.slug(),
            kind: "article",
            title: a.title().to_string(),
            reason: reason.to_string(),
            note: standing.and_then(|e| e.note.clone()),
        });
    }

    pending.sort_by(|a, b| a.path.cmp(&b.path));
    let mut unreadable = loaded.unreadable.clone();
    unreadable.extend(persona_loaded.unreadable.iter().cloned());
    unreadable.sort_by(|a, b| a.path.cmp(&b.path));

    let queue = Queue {
        count: pending.len(),
        pending,
        unreadable,
    };

    if output::is_json() {
        output::emit("review", queue)?;
        return Ok(0);
    }

    if queue.count == 0 {
        println!("{}", "Nothing is waiting on you.".bold());
    } else {
        println!("{} — {} item(s)", "Waiting on you".bold(), queue.count);
        for item in &queue.pending {
            println!(
                "\n  {} {}",
                format!("[{}]", item.kind).dimmed(),
                item.id.cyan()
            );
            println!("      {}", item.title);
            println!("      {} · {}", item.reason.yellow(), item.path.dimmed());
            if let Some(note) = &item.note {
                println!("      {}", format!("note: {note}").dimmed());
            }
        }
        println!(
            "\n  {}",
            "sentinel review <id> --approve | --reject --note \"...\"".dimmed()
        );
    }

    wiki::warn_partial(&queue.unreadable, "this queue may be short");
    Ok(0)
}
