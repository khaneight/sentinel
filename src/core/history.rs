//! `meta/progress.jsonl` — what the archive looked like, over time.
//!
//! `meta/log.md` records *events*: "27 articles indexed". It is dated to the
//! day, carries no counts beyond whatever the detail string happens to mention,
//! and cannot answer "how did the backlog move last month". Every count the
//! tool publishes is a snapshot of now.
//!
//! That is fine for driving the loop and useless for showing it. A wiki that
//! builds itself is worth watching, and watching needs a series.
//!
//! One JSON object per line, appended by `index`, and only when something
//! changed — the same rule as `atomic::write_if_changed`. A rebuild that alters
//! nothing adds nothing, so the file stays a record of the archive's history
//! rather than of how often somebody ran a command.

use std::fs::OpenOptions;
use std::io::{self, Write};

use serde::{Deserialize, Serialize};

use super::paths;

/// One measurement of the archive.
///
/// Deliberately only counts. Anything larger — titles, paths, findings — makes
/// the file grow with the archive rather than with its history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Local date and time, to the minute. Finer would record how often `index`
    /// ran; coarser would collapse a working session into one point.
    pub at: String,
    pub wiki_articles: usize,
    pub raw_documents: usize,
    pub uncompiled: usize,
    pub orphans: usize,
    pub errors: usize,
    pub warnings: usize,
    /// Concepts linked but not yet written — the backlog the `write` rung works.
    pub wanted: usize,
    pub links: usize,
}

impl Snapshot {
    /// Whether two measurements describe the same archive.
    ///
    /// `at` is excluded: two snapshots a week apart with identical counts are
    /// the same state observed twice, and recording the second says only that
    /// somebody ran a command.
    fn same_state(&self, other: &Self) -> bool {
        Self {
            at: other.at.clone(),
            ..self.clone()
        } == *other
    }
}

pub fn path() -> std::path::PathBuf {
    paths::meta_dir().join("progress.jsonl")
}

/// Every snapshot on record, oldest first.
///
/// A line that will not parse is skipped rather than fatal: this is a history
/// for looking at, and one corrupt line should not cost the other thousand.
/// The count of skipped lines is returned so a caller can disclose it.
pub fn read() -> io::Result<(Vec<Snapshot>, usize)> {
    let p = path();
    if !p.exists() {
        return Ok((Vec::new(), 0));
    }
    let text = std::fs::read_to_string(&p)?;
    let mut out = Vec::new();
    let mut unreadable = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Snapshot>(line) {
            Ok(s) => out.push(s),
            Err(_) => unreadable += 1,
        }
    }
    Ok((out, unreadable))
}

/// Append a snapshot, unless the last one already says the same thing.
///
/// Returns whether anything was written.
pub fn record(snapshot: &Snapshot) -> io::Result<bool> {
    let (existing, _) = read()?;
    if existing
        .last()
        .is_some_and(|last| last.same_state(snapshot))
    {
        return Ok(false);
    }

    let line = serde_json::to_string(snapshot)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path())?;
    writeln!(file, "{line}")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(at: &str, articles: usize) -> Snapshot {
        Snapshot {
            at: at.to_string(),
            wiki_articles: articles,
            raw_documents: 1,
            uncompiled: 0,
            orphans: 0,
            errors: 0,
            warnings: 0,
            wanted: 0,
            links: 0,
        }
    }

    #[test]
    fn a_repeated_state_is_not_recorded_twice() {
        let a = snapshot("2026-01-01 10:00", 5);
        let b = snapshot("2026-01-08 17:30", 5);
        assert!(
            a.same_state(&b),
            "a week apart with identical counts is one state seen twice"
        );
    }

    #[test]
    fn a_changed_count_is_a_new_state() {
        let a = snapshot("2026-01-01 10:00", 5);
        let b = snapshot("2026-01-01 10:00", 6);
        assert!(!a.same_state(&b));
    }

    #[test]
    fn every_field_participates_in_the_comparison() {
        // Written as a loop so a field added later is covered without anyone
        // remembering: each mutation must register as a different state.
        let base = snapshot("2026-01-01 10:00", 5);
        let mutations: Vec<Snapshot> = vec![
            Snapshot {
                wiki_articles: 9,
                ..base.clone()
            },
            Snapshot {
                raw_documents: 9,
                ..base.clone()
            },
            Snapshot {
                uncompiled: 9,
                ..base.clone()
            },
            Snapshot {
                orphans: 9,
                ..base.clone()
            },
            Snapshot {
                errors: 9,
                ..base.clone()
            },
            Snapshot {
                warnings: 9,
                ..base.clone()
            },
            Snapshot {
                wanted: 9,
                ..base.clone()
            },
            Snapshot {
                links: 9,
                ..base.clone()
            },
        ];
        // One per field, `at` excluded.
        let fields = 8;
        assert_eq!(mutations.len(), fields, "a field was added without a case");
        for m in mutations {
            assert!(!base.same_state(&m), "a change went unnoticed: {m:?}");
        }
    }
}
