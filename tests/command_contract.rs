//! Every subcommand must be classified, and the classification must hold.
//!
//! Two hardcoded lists governed this and nothing checked them against each
//! other: `main.rs` decides which commands take the archive lock, and
//! `correctness.rs` asserted which commands leave the archive untouched.
//! `init` and `log` appeared in neither — safe by construction, but by
//! accident rather than decision, and a new command would have joined them
//! silently.
//!
//! This enumerates from `--help` rather than from a list I maintain, per the
//! rule that a test should enumerate from the source of truth. Adding a
//! subcommand now fails here until someone says which kind it is.

mod common;

use common::{Archive, stdout};
use std::collections::BTreeSet;
use std::process::Command;

/// What a command is allowed to do to the archive.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// Must leave every file byte-identical.
    Query,
    /// Changes the archive under `meta/.lock`.
    Mutating,
    /// Changes the archive without the lock, because it is safe without one.
    /// Each of these needs a reason, given below.
    MutatingUnlocked,
}

/// The classification. Asserted against `--help`, so it cannot drift.
const COMMANDS: &[(&str, Kind)] = &[
    // Queries.
    ("config", Kind::Query),
    ("schema", Kind::Query),
    ("status", Kind::Query),
    ("next", Kind::Query),
    ("uncompiled", Kind::Query),
    ("persona", Kind::Query),
    ("lint", Kind::Query),
    ("search", Kind::Query),
    ("graph", Kind::Query),
    // Read-modify-write on the manifest; serialised.
    ("ingest", Kind::Mutating),
    ("ingest-repo", Kind::Mutating),
    ("sync", Kind::Mutating),
    ("index", Kind::Mutating),
    ("mv", Kind::Mutating),
    ("rm", Kind::Mutating),
    // Writes a tree outside the archive and appends to the log. Locked because
    // it reads the whole wiki to decide what goes in it.
    ("export", Kind::Mutating),
    // `init` only ever creates files that do not exist, so two of them racing
    // write identical content and neither can clobber the other's work. It
    // also runs before `meta/` exists, which is where the lock would live.
    ("init", Kind::MutatingUnlocked),
    // `log` appends with O_APPEND, which is atomic for writes this small, and
    // is called by the other commands while they already hold the lock.
    ("log", Kind::MutatingUnlocked),
];

fn subcommands() -> BTreeSet<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    let block = help
        .split("Commands:")
        .nth(1)
        .and_then(|s| s.split("Options:").next())
        .expect("--help must list commands");
    block
        .lines()
        .filter(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .filter(|c| *c != "help")
        .map(str::to_string)
        .collect()
}

#[test]
fn every_subcommand_is_classified() {
    let real = subcommands();
    let classified: BTreeSet<String> = COMMANDS.iter().map(|(c, _)| c.to_string()).collect();

    let unclassified: Vec<_> = real.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "new subcommand(s) {unclassified:?} — say whether they mutate the \
         archive and whether they need the lock, then add them to COMMANDS"
    );

    let stale: Vec<_> = classified.difference(&real).collect();
    assert!(
        stale.is_empty(),
        "COMMANDS lists commands that no longer exist: {stale:?}"
    );
}

/// Every file in the archive, so "untouched" can be asserted precisely.
fn snapshot(a: &Archive) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![a.root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.push((path.display().to_string(), bytes));
            }
        }
    }
    out.sort();
    out
}

/// A populated archive, so the query commands have something to report on.
fn populated() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "source text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/art.md",
        &common::article("Art", "philosophy", &["raw/philosophy/src.md"]),
    );
    a.run(&["index"]);
    a
}

/// Arguments that make each query command runnable.
fn invocation(command: &str) -> Vec<&str> {
    match command {
        "search" => vec!["search", "source"],
        other => vec![other],
    }
}

#[test]
fn every_query_command_leaves_the_archive_byte_identical() {
    let a = populated();
    let before = snapshot(&a);

    for (command, kind) in COMMANDS {
        if *kind != Kind::Query {
            continue;
        }
        // Both renderings, since only one of them was ever checked by hand.
        a.output(&invocation(command));
        let mut json = invocation(command);
        json.push("--json");
        a.output(&json);

        assert_eq!(
            before,
            snapshot(&a),
            "`sentinel {command}` modified the archive"
        );
    }
}

#[test]
fn unlocked_mutating_commands_are_safe_run_concurrently() {
    // They are exempt from the lock by argument, not by oversight. The
    // argument has to hold.
    let a = populated();

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_sentinel"));
            cmd.env("SENTINEL_ARCHIVE", &a.root);
            cmd.env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml");
            cmd.args(["log", "concurrent", &format!("entry {i}")]);
            cmd.spawn().unwrap()
        })
        .collect();
    for mut h in handles {
        assert!(h.wait().unwrap().success());
    }

    let log = a.read("meta/log.md");
    assert_eq!(
        (0..8)
            .filter(|i| log.contains(&format!("entry {i}")))
            .count(),
        8,
        "concurrent appends lost an entry:\n{log}"
    );

    // And re-running init concurrently must not damage an existing archive.
    let before = snapshot(&a);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_sentinel"));
            cmd.env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml");
            cmd.args(["init", &a.root.display().to_string()]);
            cmd.spawn().unwrap()
        })
        .collect();
    for mut h in handles {
        h.wait().unwrap();
    }
    assert_eq!(before, snapshot(&a), "concurrent init damaged the archive");
}

#[test]
#[cfg(unix)]
fn output_larger_than_a_pipe_buffer_survives_an_early_reader() {
    // Rust ignores SIGPIPE at startup, so writing to a pipe whose reader has
    // gone away returns EPIPE and `println!` panics. `sentinel graph | head`
    // crashed with a backtrace instead of stopping quietly — and only once the
    // output exceeded the 64 KB pipe buffer, so it was invisible on a small
    // archive and reproducible on a real one.
    use std::io::Read;
    use std::process::Stdio;

    let a = Archive::new();
    a.write("raw/philosophy/src.md", "x");
    a.run(&["sync"]);
    for i in 0..400 {
        a.write(
            &format!("wiki/philosophy/a{i:04}.md"),
            &common::article(&format!("A{i}"), "philosophy", &["raw/philosophy/src.md"])
                .replace("Body.", &format!("See [[a{:04}]].", (i + 1) % 400)),
        );
    }
    a.run(&["index"]);

    for args in [vec!["graph"], vec!["graph", "--json"]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_sentinel"))
            .env("SENTINEL_ARCHIVE", &a.root)
            .env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        // Read one small chunk, then drop the pipe — what `head` does.
        let mut stdout = child.stdout.take().unwrap();
        let mut buf = [0u8; 64];
        let _ = stdout.read(&mut buf);
        drop(stdout);

        let out = child.wait_with_output().unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            !err.contains("Broken pipe") && !err.contains("panicked"),
            "`sentinel {}` panicked when its reader went away:\n{err}",
            args.join(" ")
        );
    }
}

#[test]
fn init_stubs_exactly_the_indexes_that_index_regenerates() {
    // Two hardcoded filename lists in different files — `init` stubs them,
    // `index` writes them — with nothing comparing the two. A file in one list
    // and not the other is either a stub nothing ever updates or a generated
    // file that appears from nowhere on first index.
    //
    // Compared by observation rather than by a third list maintained here.
    let a = Archive::new();
    let listing = |a: &Archive| -> BTreeSet<String> {
        std::fs::read_dir(a.path("index"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    };

    let stubbed = listing(&a);
    assert!(
        !stubbed.is_empty(),
        "init should stub the generated indexes"
    );

    // Remove them all, then let `index` recreate whatever it owns.
    for name in &stubbed {
        std::fs::remove_file(a.path(&format!("index/{name}"))).unwrap();
    }
    a.write("raw/philosophy/s.md", "x");
    a.run(&["sync"]);
    a.run(&["index"]);

    assert_eq!(
        stubbed,
        listing(&a),
        "the set `init` stubs and the set `index` regenerates disagree"
    );
}

#[test]
fn every_command_that_accepts_json_actually_emits_it() {
    // `--json` is a *global* flag, so it is accepted by every subcommand — and
    // four of them ignored it and printed human text: `init`, `ingest`, `sync`,
    // and `index`. A flag that parses and does nothing is worse than one that
    // does not exist, because a caller has no way to tell.
    //
    // Enumerated from `--help`, so a new subcommand fails here until it either
    // emits JSON or is listed below with a reason.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    let scratch = a.path("scratch.md");
    std::fs::write(&scratch, "x").unwrap();

    // Commands needing arguments, and the ones that legitimately produce none.
    let invocation = |c: &str| -> Option<Vec<String>> {
        let s = |v: &[&str]| Some(v.iter().map(|x| x.to_string()).collect());
        match c {
            // Refuses by design; its error path is covered elsewhere.
            "ingest-repo" => None,
            "init" => s(&["init", &a.root.display().to_string()]),
            "ingest" => Some(vec![
                "ingest".into(),
                scratch.display().to_string(),
                "-d".into(),
                "philosophy".into(),
            ]),
            "search" => s(&["search", "text"]),
            "log" => s(&["log", "probe", "detail"]),
            "mv" | "rm" => None, // exercised in their own suites
            other => s(&[other]),
        }
    };

    for command in subcommands() {
        let Some(args) = invocation(&command) else {
            continue;
        };
        let mut with_json: Vec<&str> = args.iter().map(String::as_str).collect();
        with_json.push("--json");

        let out = a.output(&with_json);
        let text = stdout(&out);
        assert!(
            serde_json::from_str::<serde_json::Value>(&text).is_ok(),
            "`sentinel {} --json` did not emit JSON:\n{}",
            args.join(" "),
            text.lines().take(3).collect::<Vec<_>>().join("\n")
        );
    }
}
