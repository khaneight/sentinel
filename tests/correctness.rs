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

// ---------------------------------------------------------------------------
// sync must not destroy metadata that cannot be recovered from disk
//
// A file renamed by hand looks to sync like a deletion plus an addition. Being
// treated as such reset `origin` to "authored" — so a `researched` document
// silently became the user's own writing, which is the one distinction the
// whole archive is organised around.
// ---------------------------------------------------------------------------

fn ingested_research(a: &Archive) -> std::path::PathBuf {
    let tmp = a.path("scratch-source.md");
    std::fs::write(&tmp, "AI-gathered notes.").unwrap();
    a.run(&[
        "ingest",
        &tmp.display().to_string(),
        "-d",
        "research",
        "-o",
        "researched",
        "-t",
        "Research: Stoic Ethics",
    ]);
    a.path("raw/research/research-stoic-ethics.md")
}

fn entry(a: &Archive, rel: &str) -> serde_json::Value {
    let m: serde_json::Value = serde_json::from_str(&a.read("meta/manifest.json")).unwrap();
    m["entries"][rel].clone()
}

#[test]
fn a_hand_renamed_source_keeps_its_provenance() {
    let a = Archive::new();
    let original = ingested_research(&a);
    std::fs::rename(&original, a.path("raw/research/stoic-ethics-notes.md")).unwrap();

    a.run(&["sync"]);

    let e = entry(&a, "raw/research/stoic-ethics-notes.md");
    assert_eq!(
        e["origin"], "researched",
        "AI-gathered research was relabelled as the user's own writing:\n{e}"
    );
    assert_eq!(e["title"], "Research: Stoic Ethics", "{e}");
    assert!(
        entry(&a, "raw/research/research-stoic-ethics.md").is_null(),
        "the old entry must be gone"
    );
}

#[test]
fn a_move_between_domains_keeps_provenance_and_updates_the_domain() {
    let a = Archive::new();
    let original = ingested_research(&a);
    std::fs::create_dir_all(a.path("raw/philosophy")).unwrap();
    std::fs::rename(&original, a.path("raw/philosophy/moved.md")).unwrap();

    a.run(&["sync"]);

    let e = entry(&a, "raw/philosophy/moved.md");
    assert_eq!(e["origin"], "researched", "{e}");
    assert_eq!(e["domain"], "philosophy", "{e}");
}

#[test]
fn a_genuine_deletion_still_prunes_and_says_what_it_discards() {
    let a = Archive::new();
    let original = ingested_research(&a);
    std::fs::remove_file(&original).unwrap();

    let out = a.run(&["sync"]);

    assert!(
        entry(&a, "raw/research/research-stoic-ethics.md").is_null(),
        "a deleted source must not linger"
    );
    assert!(
        out.contains("origin: researched"),
        "pruning discards unrecoverable metadata and must say so:\n{out}"
    );
}

#[test]
fn two_files_swapping_names_are_not_confused() {
    let a = Archive::new();
    a.write("raw/philosophy/one.md", "content one");
    a.write("raw/philosophy/two.md", "content two");
    a.run(&["sync"]);

    std::fs::rename(
        a.path("raw/philosophy/one.md"),
        a.path("raw/philosophy/tmp"),
    )
    .unwrap();
    std::fs::rename(
        a.path("raw/philosophy/two.md"),
        a.path("raw/philosophy/one.md"),
    )
    .unwrap();
    std::fs::rename(
        a.path("raw/philosophy/tmp"),
        a.path("raw/philosophy/two.md"),
    )
    .unwrap();
    a.run(&["sync"]);

    // Content decides identity, so the entries follow the bytes.
    assert_eq!(a.read("raw/philosophy/one.md"), "content two");
    assert_eq!(entry(&a, "raw/philosophy/one.md")["origin"], "authored");
    assert_eq!(entry(&a, "raw/philosophy/two.md")["origin"], "authored");
}

#[test]
fn sync_backfills_hashes_so_older_archives_are_protected_too() {
    // Entries written before the field existed cannot be matched on a rename.
    // Backfilling on the next sync means the fix is not limited to documents
    // ingested from now on.
    let a = Archive::new();
    ingested_research(&a);

    // Simulate a manifest written by an earlier version.
    let mut m: serde_json::Value = serde_json::from_str(&a.read("meta/manifest.json")).unwrap();
    m["entries"]["raw/research/research-stoic-ethics.md"]
        .as_object_mut()
        .unwrap()
        .remove("content_hash");
    a.write(
        "meta/manifest.json",
        &serde_json::to_string_pretty(&m).unwrap(),
    );

    a.run(&["sync"]);
    assert!(
        entry(&a, "raw/research/research-stoic-ethics.md")["content_hash"].is_string(),
        "sync must record a hash for entries that lack one"
    );

    std::fs::rename(
        a.path("raw/research/research-stoic-ethics.md"),
        a.path("raw/research/renamed.md"),
    )
    .unwrap();
    a.run(&["sync"]);
    assert_eq!(entry(&a, "raw/research/renamed.md")["origin"], "researched");
}

#[test]
fn dry_run_reports_moves_separately_from_adds_and_removes() {
    let a = Archive::new();
    let original = ingested_research(&a);
    std::fs::rename(&original, a.path("raw/research/renamed.md")).unwrap();
    let before = a.read("meta/manifest.json");

    let out = a.run(&["sync", "--dry-run"]);

    assert!(out.contains("1 moved"), "{out}");
    assert!(
        out.contains("0 to add"),
        "a move is not an addition:\n{out}"
    );
    assert_eq!(before, a.read("meta/manifest.json"));
}

// ---------------------------------------------------------------------------
// A partial view must not overwrite durable state
//
// `wiki::load_all` skipped unreadable files silently. `index` then rebuilt from
// whatever it could read — reporting "Index rebuilt. Articles indexed: 0" and
// exit 0 while wiping the manifest's compilation mapping and blanking every
// generated index, because one file was briefly locked.
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn make_unreadable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
}

#[cfg(unix)]
fn make_readable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
}

#[cfg(unix)]
fn archive_with_one_article() -> (Archive, std::path::PathBuf) {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "source text");
    a.run(&["sync"]);
    let article = a.write(
        "wiki/philosophy/art.md",
        &common::article("Article", "philosophy", &["raw/philosophy/src.md"]),
    );
    a.run(&["index"]);
    (a, article)
}

#[test]
#[cfg(unix)]
fn index_refuses_to_rebuild_from_a_partial_view() {
    let (a, article) = archive_with_one_article();
    make_unreadable(&article);

    let output = a.output(&["index"]);
    let code = output.status.code();
    make_readable(&article);

    assert_eq!(code, Some(1), "index reported success on a partial view");
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("could not be read"), "{err}");
    assert!(
        err.contains("wiki/philosophy/art.md"),
        "must name the file:\n{err}"
    );
}

#[test]
#[cfg(unix)]
fn a_refused_rebuild_leaves_the_previous_state_intact() {
    let (a, article) = archive_with_one_article();
    let manifest_before = a.read("meta/manifest.json");
    let master_before = a.read("index/_master.md");

    make_unreadable(&article);
    let _ = a.output(&["index"]);
    make_readable(&article);

    assert_eq!(
        manifest_before,
        a.read("meta/manifest.json"),
        "the compilation mapping was rewritten from an incomplete view"
    );
    assert_eq!(
        master_before,
        a.read("index/_master.md"),
        "the generated index was blanked"
    );
}

#[test]
#[cfg(unix)]
fn status_says_when_its_counts_exclude_something() {
    let (a, article) = archive_with_one_article();
    make_unreadable(&article);

    let v = a.json(&["status"]);
    let human = a.run(&["status"]);
    make_readable(&article);

    assert_eq!(v["wiki_articles"], 0, "the count is genuinely partial");
    assert_eq!(
        v["unreadable"].as_array().unwrap().len(),
        1,
        "and must say so, or the zero reads as a fact:\n{v}"
    );
    assert!(human.contains("could not be read"), "{human}");
}

#[test]
#[cfg(unix)]
fn lint_does_not_report_a_partial_archive_as_clean() {
    let (a, article) = archive_with_one_article();
    make_unreadable(&article);

    let v = a.json(&["lint"]);
    let human = a.run(&["lint"]);
    make_readable(&article);

    assert_eq!(
        v["unreadable"].as_array().unwrap().len(),
        1,
        "a clean result over a partial view is not a clean archive:\n{v}"
    );
    assert!(human.contains("were not linted"), "{human}");
}

#[test]
#[cfg(unix)]
fn once_readable_again_index_succeeds_unchanged() {
    let (a, article) = archive_with_one_article();
    let before = a.read("index/_master.md");

    make_unreadable(&article);
    let _ = a.output(&["index"]);
    make_readable(&article);

    a.run(&["index"]);
    assert_eq!(before, a.read("index/_master.md"));
}
