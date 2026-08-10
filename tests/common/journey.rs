//! A harness for testing sequences of commands as a user experiences them.
//!
//! The rest of the suite tests commands. This tests *journeys*: what happens
//! across a run, in order, including what the output told you to do next.
//!
//! Two things it exists to catch that per-command tests structurally cannot:
//!
//! - **Dead ends.** A command can be individually correct and still leave the
//!   user with no idea what to do. `next` on a fresh archive said "✓ Nothing
//!   outstanding" — right about the data, wrong about the situation, and the
//!   first thing a new user sees.
//! - **Unreachable advice.** `connect` recommended adding an incoming link to
//!   the only article in the archive, forever. Every individual assertion about
//!   `connect` passed; the loop was still broken.
//!
//! A `Journey` records every step and asserts over the transcript, so a failure
//! prints the whole session rather than one exit code.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use super::Archive;

pub struct Step {
    pub args: Vec<String>,
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Step {
    /// Everything the user saw, wherever it was printed.
    pub fn output(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    pub fn label(&self) -> String {
        format!("sentinel {}", self.args.join(" "))
    }
}

pub struct Journey {
    pub archive: Archive,
    pub steps: Vec<Step>,
    /// Scratch space outside the archive, for files to ingest and sites to
    /// export. Kept alive for the journey's lifetime.
    workspace: tempfile::TempDir,
}

impl Journey {
    pub fn new() -> Self {
        Self {
            archive: Archive::new(),
            steps: Vec::new(),
            workspace: tempfile::tempdir().unwrap(),
        }
    }

    /// An archive directory that `init` has never touched, for testing the very
    /// first command a user runs.
    pub fn uninitialized() -> (Self, PathBuf) {
        let j = Self::new();
        let fresh = j.workspace.path().join("fresh");
        (j, fresh)
    }

    pub fn scratch(&self, name: &str) -> PathBuf {
        self.workspace.path().join(name)
    }

    pub fn write_scratch(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.scratch(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// Run a command, recording it. Returns the step for immediate assertions.
    pub fn run(&mut self, args: &[&str]) -> &Step {
        let out = self.archive.output(args);
        self.steps.push(Step {
            args: args.iter().map(|s| s.to_string()).collect(),
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        });
        self.steps.last().unwrap()
    }

    /// Run against a directory that has no archive, from a working directory
    /// with no archive above it — the state a new user is actually in.
    pub fn run_unrooted(&mut self, args: &[&str]) -> &Step {
        let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
            .args(args)
            .current_dir(self.workspace.path())
            .env_remove("SENTINEL_ARCHIVE")
            .env("SENTINEL_CONFIG", self.workspace.path().join("no-config"))
            .output()
            .unwrap();
        self.steps.push(Step {
            args: args.iter().map(|s| s.to_string()).collect(),
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        });
        self.steps.last().unwrap()
    }

    pub fn write_article(&self, slug: &str, frontmatter: &[(&str, &str)], body: &str) {
        let mut text = String::from("---\n");
        for (k, v) in frontmatter {
            text.push_str(&format!("{k}: {v}\n"));
        }
        text.push_str("---\n\n");
        text.push_str(body);
        text.push('\n');
        self.archive
            .write(&format!("wiki/philosophy/{slug}.md"), &text);
    }

    /// The whole session, for a failure message worth reading.
    pub fn transcript(&self) -> String {
        let mut out = String::from("\n--- journey ---\n");
        for step in &self.steps {
            out.push_str(&format!("\n$ {}   (exit {})\n", step.label(), step.code));
            for line in step.output().lines().take(12) {
                out.push_str(&format!("  {line}\n"));
            }
        }
        out
    }

    /// Every step must have told the user something they can act on.
    ///
    /// The rule is not "never fail" — refusing is often correct. It is that a
    /// step which fails, or which reports nothing to do, has to name a command,
    /// a flag, or a file. An exit code alone is a dead end.
    pub fn assert_no_dead_ends(&self) {
        for step in &self.steps {
            let text = step.output();
            let says_nothing_to_do = text.contains("Nothing outstanding")
                || text.contains("No results")
                || text.contains("Nothing exported")
                || text.contains("already in sync");
            if step.code == 0 && !says_nothing_to_do {
                continue;
            }
            // "Actionable" means it names something the user can run — a real
            // subcommand or a skill. The first version of this accepted any
            // output containing `--` or `.md`, which almost everything does;
            // it could not fail, and a guard that cannot fail is worse than
            // none because it reads as coverage.
            let names_a_command = real_subcommands()
                .iter()
                .any(|c| text.contains(&format!("sentinel {c}")));
            let names_a_skill = text.contains("/sentinel-");
            let actionable = names_a_command || names_a_skill;
            assert!(
                actionable,
                "`{}` (exit {}) named no command to run next:\n{}\n{}",
                step.label(),
                step.code,
                text,
                self.transcript()
            );
        }
    }

    /// A recommendation the user cannot act on is worse than none.
    ///
    /// Runs `next` repeatedly with no work done between calls. Repetition is
    /// expected — the point is that a *stuck* recommendation is one the archive
    /// cannot satisfy, and `sentinel-grow` treats a repeated target as a stop
    /// condition. This asserts the tool never emits one at a size where the
    /// task is structurally impossible.
    pub fn assert_recommendation_is_achievable(&mut self) {
        let v = self.archive.json(&["next"]);
        let action = v["action"].as_str().unwrap_or("none").to_string();
        if action == "none" {
            return;
        }
        let articles = v["progress"]["wiki_articles"].as_u64().unwrap_or(0);
        assert!(
            !(action == "connect" && articles < 2),
            "`connect` on a {articles}-article archive asks for an incoming \
             link with nothing to link from — a task no amount of work \
             completes:\n{}",
            self.transcript()
        );
    }
}

/// Assert a step succeeded, printing the journey if not.
#[macro_export]
macro_rules! assert_step_ok {
    ($journey:expr, $args:expr) => {{
        let code = $journey.run($args).code;
        assert_eq!(
            code,
            0,
            "`sentinel {}` failed{}",
            $args.join(" "),
            $journey.transcript()
        );
    }};
}

/// Subcommands the binary accepts, so "names a command" means a real one.
pub fn real_subcommands() -> Vec<String> {
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

/// Files an archive is expected to hold, for onboarding assertions.
pub fn expected_layout() -> Vec<&'static str> {
    vec![
        "raw",
        "wiki",
        "index",
        "meta",
        "templates",
        "meta/manifest.json",
        "templates/wiki-article.md",
        "CLAUDE.md",
        "SUMMARY.md",
    ]
}

pub fn exists(root: &Path, rel: &str) -> bool {
    root.join(rel).exists()
}
