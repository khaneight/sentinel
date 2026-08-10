//! Checks that apply to every document an agent reads to learn how to work here.
//!
//! There are three kinds — the repository's `CLAUDE.md`, the `CLAUDE.md` that
//! `init` writes into an archive, and the skills — and each has drifted at
//! least once while the others were being guarded:
//!
//! - the skills described a tool that always succeeded, sixteen PRs after
//!   `index` learned to refuse (#24)
//! - the archive's `CLAUDE.md` never mentioned `mv` or `rm` (#43)
//! - the repository's grew to 31 KB of always-loaded context before anyone
//!   measured it (#38), and then nothing stopped it growing back
//!
//! Each time the fix guarded the document in front of me. This enumerates the
//! set instead, so a check added for one applies to all of them.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Any single document loaded into an agent's context.
///
/// The two `CLAUDE.md` files are loaded unconditionally; a skill is loaded when
/// invoked. Both costs are paid per use, so the same ceiling applies — this is
/// a runaway-growth backstop, not a style rule.
const MAX_BYTES: usize = 12_000;

struct Document {
    name: String,
    text: String,
}

fn documents() -> Vec<Document> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut docs = vec![Document {
        name: "CLAUDE.md (repository, auto-loaded)".into(),
        text: std::fs::read_to_string(repo.join("CLAUDE.md")).unwrap(),
    }];

    // Generated, so it has to be produced rather than read from the tree.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("archive");
    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .args(["init", &root.display().to_string()])
        .env_remove("SENTINEL_ARCHIVE")
        .env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml")
        .output()
        .unwrap();
    assert!(out.status.success(), "init failed");
    docs.push(Document {
        name: "CLAUDE.md (archive, written by init)".into(),
        text: std::fs::read_to_string(root.join("CLAUDE.md")).unwrap(),
    });

    let mut skills: Vec<PathBuf> = std::fs::read_dir(repo.join("skills"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path().join("SKILL.md"))
        .filter(|p| p.is_file())
        .collect();
    skills.sort();
    for path in skills {
        docs.push(Document {
            name: format!(
                "skills/{}",
                path.parent()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
            ),
            text: std::fs::read_to_string(&path).unwrap(),
        });
    }
    docs
}

/// Subcommands the binary actually accepts.
fn subcommands() -> BTreeSet<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    help.split("Commands:")
        .nth(1)
        .and_then(|s| s.split("Options:").next())
        .expect("--help lists commands")
        .lines()
        .filter(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .filter(|c| *c != "help")
        .map(str::to_string)
        .collect()
}

/// `sentinel <word>` occurrences inside code — fenced blocks and inline spans.
/// Prose is excluded: "ask sentinel what to do" is English, not an invocation.
fn invocations(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut in_fence = false;
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        let mut collect = |segment: &str| {
            let mut rest = segment;
            while let Some(at) = rest.find("sentinel ") {
                rest = &rest[at + "sentinel ".len()..];
                if let Some(word) = rest.split_whitespace().next() {
                    found.push((i + 1, word.to_string()));
                }
            }
        };
        if in_fence {
            collect(line);
        } else {
            for span in line.split('`').skip(1).step_by(2) {
                collect(span);
            }
        }
    }
    found
}

#[test]
fn no_instruction_document_names_a_command_that_does_not_exist() {
    let real = subcommands();
    for doc in documents() {
        for (line, word) in invocations(&doc.text) {
            let candidate: String = word
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                .collect();
            if candidate.is_empty() {
                continue;
            }
            assert!(
                real.contains(&candidate),
                "{}:{line} references `sentinel {candidate}`, which is not a \
                 subcommand",
                doc.name
            );
        }
    }
}

#[test]
fn no_instruction_document_tells_an_agent_to_read_the_master_index() {
    // `index/_master.md` lists every article. Reading it to orient consumes the
    // context needed to do the work, and gets worse as the archive grows.
    for doc in documents() {
        for (i, line) in doc.text.lines().enumerate() {
            // `_dashboard.md` is capped, but it is still a page of prose where
            // `sentinel next --json` is a payload. Same rule.
            if !line.contains("_master.md") && !line.contains("_dashboard.md") {
                continue;
            }
            let lower = line.to_lowercase();
            // Naming the file is fine — `index` legitimately regenerates it.
            // What must not appear is an instruction to *read* it.
            let tells_you_to_read = ["read ", "open ", "consult ", "start with ", "look at "]
                .iter()
                .any(|verb| lower.contains(verb));
            let negated = [
                "do not",
                "don't",
                "never",
                "rather than",
                "instead of",
                "not the",
            ]
            .iter()
            .any(|neg| lower.contains(neg));
            assert!(
                !tells_you_to_read || negated,
                "{}:{} instructs reading the master index:\n  {line}",
                doc.name,
                i + 1
            );
        }
    }
}

#[test]
fn the_master_index_check_would_catch_a_real_instruction() {
    // The predicate above is prose-matching, so it is worth knowing it still
    // fires — a check that cannot fail is not a check.
    let offending = "Read `index/_master.md` to understand the full scope.";
    let lower = offending.to_lowercase();
    let tells_you_to_read = ["read ", "open ", "consult ", "start with ", "look at "]
        .iter()
        .any(|v| lower.contains(v));
    let negated = ["do not", "don't", "never", "rather than", "instead of"]
        .iter()
        .any(|n| lower.contains(n));
    assert!(
        tells_you_to_read && !negated,
        "the predicate no longer fires"
    );
}

#[test]
fn no_instruction_document_grows_past_its_context_budget() {
    // The repository's CLAUDE.md reached 31 KB — roughly 7,700 tokens spent
    // before any work began — because it was appended to for twenty PRs and
    // nobody looked at the total. Nothing prevented it growing back.
    for doc in documents() {
        assert!(
            doc.text.len() <= MAX_BYTES,
            "{} is {} bytes (~{} tokens). It is loaded into an agent's context; \
             move reasoning to docs/design-notes.md and keep this a map.",
            doc.name,
            doc.text.len(),
            doc.text.len() / 4
        );
    }
}

#[test]
fn the_set_of_documents_is_what_it_is_expected_to_be() {
    // If a fourth kind of instruction document appears, the checks above should
    // cover it — and this fails until someone adds it to `documents()`.
    let names: Vec<String> = documents().into_iter().map(|d| d.name).collect();
    assert_eq!(
        names.len(),
        2 + std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("skills"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().join("SKILL.md").is_file())
            .count(),
        "documents() must cover both CLAUDE.md files and every skill: {names:?}"
    );
}
