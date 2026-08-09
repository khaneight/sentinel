//! Concurrent commands must not lose each other's work.
//!
//! Every mutating command does load → modify → save on `meta/manifest.json`,
//! and nothing ordered them. Two running at once both read the same state and
//! the second save discarded the first's. Measured before the fix: ten
//! concurrent `ingest` calls left nine documents on disk with no manifest
//! entry — every one of them reporting success and exiting 0.
//!
//! The recovery path made it worse: `sync` re-registers an unlisted file as
//! `origin: authored`, which is the unrecoverable provenance loss from #16.

mod common;

use common::Archive;
use std::process::Command;

/// Run `count` ingests at once, returning (successes, manifest entry count).
fn concurrent_ingests(a: &Archive, count: usize) -> (usize, usize) {
    let dir = tempfile::tempdir().unwrap();
    let handles: Vec<_> = (0..count)
        .map(|i| {
            let src = dir.path().join(format!("doc{i}.md"));
            std::fs::write(&src, format!("document {i}")).unwrap();
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_sentinel"));
            cmd.env_remove("SENTINEL_ARCHIVE");
            cmd.env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml");
            cmd.env("SENTINEL_ARCHIVE", &a.root);
            cmd.args([
                "ingest",
                &src.display().to_string(),
                "-d",
                "research",
                "-o",
                "researched",
                "-t",
                &format!("Doc {i}"),
            ]);
            cmd.spawn().unwrap()
        })
        .collect();

    let successes = handles
        .into_iter()
        .filter(|_| true)
        .map(|mut h| h.wait().unwrap())
        .filter(|s| s.success())
        .count();

    let manifest: serde_json::Value = serde_json::from_str(&a.read("meta/manifest.json")).unwrap();
    (successes, manifest["entries"].as_object().unwrap().len())
}

#[test]
fn concurrent_ingests_do_not_lose_manifest_entries() {
    let a = Archive::new();
    let before = 0;

    let (successes, entries) = concurrent_ingests(&a, 8);

    assert!(successes > 0, "at least some should get through");
    assert_eq!(
        entries,
        before + successes,
        "every command that reported success must be recorded; \
         {} entries lost",
        before + successes - entries
    );
}

#[test]
fn a_command_that_fails_leaves_no_orphaned_file() {
    // A refused save must not leave the copied document behind: `sync` would
    // adopt it as `authored`, which is the #16 provenance loss.
    let a = Archive::new();
    let (successes, _) = concurrent_ingests(&a, 8);

    let on_disk = std::fs::read_dir(a.path("raw/research"))
        .map(|d| d.filter_map(Result::ok).count())
        .unwrap_or(0);
    assert_eq!(
        on_disk, successes,
        "files on disk must match successful commands"
    );
}

#[test]
fn every_ingested_document_keeps_its_provenance() {
    let a = Archive::new();
    concurrent_ingests(&a, 8);

    let manifest: serde_json::Value = serde_json::from_str(&a.read("meta/manifest.json")).unwrap();
    for (path, entry) in manifest["entries"].as_object().unwrap() {
        assert_eq!(entry["origin"], "researched", "{path} lost its origin");
    }
}

#[test]
fn the_lock_is_released_so_later_commands_are_unaffected() {
    let a = Archive::new();
    concurrent_ingests(&a, 4);

    assert!(
        !a.path("meta/.lock").exists(),
        "a leftover lock would wedge every future command"
    );
    a.run(&["status"]);
    a.run(&["sync"]);
}

#[test]
fn queries_are_not_serialised_against_each_other() {
    // Read-only commands take no lock, so they cannot be blocked by one
    // another or contend on a busy archive.
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "x");
    a.run(&["sync"]);

    let handles: Vec<_> = (0..6)
        .map(|_| {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_sentinel"));
            cmd.env("SENTINEL_ARCHIVE", &a.root);
            cmd.env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml");
            cmd.args(["status", "--json"]);
            cmd.spawn().unwrap()
        })
        .collect();
    for mut h in handles {
        assert!(h.wait().unwrap().success());
    }
}
