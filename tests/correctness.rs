//! Regression coverage for defects that corrupted state or crashed outright.

mod common;

use common::{Archive, stdout};

#[test]
fn search_survives_long_multibyte_lines() {
    let a = Archive::new();
    // Byte 100 of this line lands inside a multibyte character. The previous
    // implementation sliced `&line[..100]` and panicked.
    let line = "—".repeat(200);
    a.write(
        "wiki/philosophy/dashes.md",
        &format!("---\ntitle: Dashes\n---\n\nvirtue {line}\n"),
    );

    let out = a.run(&["search", "virtue"]);
    assert!(out.contains("dashes.md"), "{out}");
}

#[test]
fn lint_names_invalid_yaml_instead_of_inventing_missing_fields() {
    let a = Archive::new();
    a.write(
        "wiki/philosophy/broken.md",
        "---\ntitle: [unterminated\n---\n\nBody\n",
    );

    let out = stdout(&a.output(&["lint"]));

    assert!(out.contains("invalid frontmatter"), "{out}");
    assert!(
        !out.contains("missing 'title'"),
        "a parse failure must not be reported as five absent fields:\n{out}"
    );
}

#[test]
fn lint_flags_slugs_that_collide_across_domains() {
    let a = Archive::new();
    let body = |domain| common::article("Ethics", domain, &["raw/x.md"]);
    a.write("wiki/philosophy/ethics.md", &body("philosophy"));
    a.write("wiki/coding/ethics.md", &body("coding"));

    let out = stdout(&a.output(&["lint"]));

    assert!(out.contains("duplicate slug 'ethics'"), "{out}");
    assert!(out.contains("wiki/philosophy/ethics.md"), "{out}");
    assert!(out.contains("wiki/coding/ethics.md"), "{out}");
}

#[test]
fn sync_drops_entries_whose_source_file_is_gone() {
    let a = Archive::new();
    let doc = a.write("raw/philosophy/meditations.md", "notes");

    a.run(&["sync"]);
    assert!(a.read("meta/manifest.json").contains("meditations.md"));

    std::fs::remove_file(&doc).unwrap();
    a.run(&["sync"]);

    let manifest = a.read("meta/manifest.json");
    assert!(
        !manifest.contains("meditations.md"),
        "a deleted source must not stay 'uncompiled' forever:\n{manifest}"
    );
}

#[test]
fn sync_ignores_hidden_files() {
    let a = Archive::new();
    a.write("raw/philosophy/.DS_Store", "junk");
    a.write("raw/philosophy/real.md", "notes");

    a.run(&["sync"]);

    let manifest = a.read("meta/manifest.json");
    assert!(manifest.contains("real.md"), "{manifest}");
    assert!(!manifest.contains("DS_Store"), "{manifest}");
}

#[test]
fn sync_dry_run_writes_nothing() {
    let a = Archive::new();
    a.write("raw/philosophy/new.md", "notes");
    let before = a.read("meta/manifest.json");

    let out = a.run(&["sync", "--dry-run"]);
    assert!(out.contains("Dry run"), "{out}");

    assert_eq!(before, a.read("meta/manifest.json"));
}

#[test]
fn index_handles_articles_with_unicode_titles() {
    let a = Archive::new();
    a.write(
        "wiki/philosophy/ethika.md",
        "---\ntitle: Ἠθικά — “virtue”\ndomain: philosophy\n---\n\nSee [[stoicism]].\n",
    );

    a.run(&["index"]);

    let master = a.read("index/_master.md");
    assert!(master.contains("Ἠθικά — “virtue”"), "{master}");
}

#[test]
fn an_unimplemented_command_fails_instead_of_silently_succeeding() {
    // `ingest-repo` printed an apology and exited 0. It is listed in --help, so
    // an agent picking from the available commands would run it, see success,
    // and continue as though a codebase had been ingested.
    let a = Archive::new();
    let output = a.output(&["ingest-repo", "/some/repo"]);

    assert_eq!(output.status.code(), Some(1));
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("not implemented"), "{err}");
    assert!(
        err.contains("sentinel ingest"),
        "it must name the thing to do instead:\n{err}"
    );
}

#[test]
fn an_unimplemented_command_reports_json_when_json_was_requested() {
    let a = Archive::new();
    let output = a.output(&["ingest-repo", "/some/repo", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8_lossy(&output.stderr);
    let v: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}):\n{text}"));
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not implemented")
    );
}
