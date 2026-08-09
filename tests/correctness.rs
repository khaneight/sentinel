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

#[test]
#[cfg(unix)]
fn an_unreadable_directory_hides_articles_and_must_be_reported() {
    // The #17 fix covered unreadable files. A directory that cannot be
    // traversed hides every article inside it just as effectively, and the
    // walk error was being dropped — so those articles vanished with nothing
    // to indicate the listing was short.
    let (a, _) = archive_with_one_article();
    std::fs::create_dir_all(a.path("wiki/locked")).unwrap();
    a.write(
        "wiki/locked/hidden.md",
        &common::article("Hidden", "philosophy", &["raw/philosophy/src.md"]),
    );
    make_unreadable(&a.path("wiki/locked"));

    let status = a.json(&["status"]);
    let index = a.output(&["index"]);
    make_readable(&a.path("wiki/locked"));

    assert_eq!(
        status["unreadable"].as_array().unwrap().len(),
        1,
        "a directory that could not be traversed must be reported:\n{status}"
    );
    assert_eq!(
        index.status.code(),
        Some(1),
        "index must not rebuild while a directory is unreadable"
    );
}

#[test]
#[cfg(unix)]
fn a_readable_archive_reports_nothing_unreadable() {
    // Guard against the reporting becoming noisy on healthy archives.
    let (a, _) = archive_with_one_article();
    let v = a.json(&["status"]);
    assert!(v["unreadable"].is_null(), "{v}");
    assert_eq!(a.code(&["index"]), 0);
}

// ---------------------------------------------------------------------------
// A corrupt link graph is not an empty one
//
// `LinkGraph::load()` returns Ok(default) when the file is absent — no index
// has run yet — and Err when it exists but cannot be parsed. Treating both as
// "empty" made `status` report a confident "Orphan pages: 0" from an
// unparseable file, and silently dropped `connect` from `next`'s backlog.
// ---------------------------------------------------------------------------

fn archive_with_orphans() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "src");
    a.run(&["sync"]);
    for slug in ["a", "b"] {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &common::article(slug, "philosophy", &["raw/philosophy/s.md"]),
        );
    }
    a.run(&["index"]);
    a
}

#[test]
fn status_does_not_report_zero_orphans_from_an_unparseable_graph() {
    let a = archive_with_orphans();
    assert_eq!(a.json(&["status"])["orphan_pages"], 2, "precondition");

    a.write("meta/link-graph.json", "not json at all");
    let v = a.json(&["status"]);

    assert!(
        v["link_graph_error"].is_string(),
        "a zero derived from an unreadable file must say so:\n{v}"
    );
    assert!(a.run(&["status"]).contains("could not be read"));
}

#[test]
fn next_does_not_silently_drop_connect_when_the_graph_is_corrupt() {
    let a = archive_with_orphans();
    a.write("meta/link-graph.json", "not json at all");

    let v = a.json(&["next"]);
    assert!(
        v["progress"]["link_graph_error"].is_string(),
        "`connect` is absent because nothing could be counted, not because \
         there is nothing to do:\n{v}"
    );
}

#[test]
fn an_absent_graph_is_legitimately_empty_and_reports_nothing() {
    // Distinguishing the two cases is the whole point; a fresh archive that has
    // never been indexed must not warn.
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "src");
    a.run(&["sync"]);
    std::fs::remove_file(a.path("meta/link-graph.json")).unwrap();

    let v = a.json(&["status"]);
    assert!(v["link_graph_error"].is_null(), "{v}");
    assert!(a.json(&["next"])["progress"]["link_graph_error"].is_null());
}

// ---------------------------------------------------------------------------
// Durable state is written atomically
//
// `fs::write` truncates before writing. An interruption in between leaves the
// file truncated — and for meta/manifest.json that is unrecoverable: it holds
// `origin` and `ingested_at`, which #16 established cannot be derived from
// disk, and a truncated manifest makes every command fail to parse it.
// ---------------------------------------------------------------------------

fn temp_files_in(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.contains(".tmp"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_truncated_manifest_is_the_state_atomic_writes_prevent() {
    // Establishes the stake rather than the mechanism: this is what the archive
    // looks like if a write is interrupted, and why it must not be reachable.
    let a = Archive::new();
    let src = a.path("scratch.md");
    std::fs::write(&src, "x").unwrap();
    a.run(&[
        "ingest",
        &src.display().to_string(),
        "-d",
        "research",
        "-o",
        "researched",
    ]);

    a.write("meta/manifest.json", "{\"entries\":{\"raw/research/scr");

    let output = a.output(&["status"]);
    assert!(!output.status.success());
    assert!(
        !a.output(&["sync"]).status.success(),
        "a torn manifest is not self-healing; every command fails on it"
    );
}

#[test]
fn commands_that_rewrite_state_leave_no_temp_files() {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/art.md",
        &common::article("Art", "philosophy", &["raw/philosophy/src.md"]),
    );
    a.run(&["index"]);
    a.run(&["mv", "raw/philosophy/src.md", "renamed.md"]);

    for dir in ["meta", "index", "wiki/philosophy", "raw/philosophy"] {
        assert!(
            temp_files_in(&a.path(dir)).is_empty(),
            "{dir} has leftover temp files: {:?}",
            temp_files_in(&a.path(dir))
        );
    }
}

#[test]
fn the_manifest_survives_a_full_write_cycle_intact() {
    // End-to-end shape check: the atomic path must produce a manifest that
    // still parses and still carries the unrecoverable fields.
    let a = Archive::new();
    let src = a.path("scratch.md");
    std::fs::write(&src, "x").unwrap();
    a.run(&[
        "ingest",
        &src.display().to_string(),
        "-d",
        "research",
        "-o",
        "researched",
        "-t",
        "Kept",
    ]);
    a.run(&["sync"]);
    a.run(&["index"]);

    let m: serde_json::Value = serde_json::from_str(&a.read("meta/manifest.json"))
        .expect("manifest must still parse after every command that rewrites it");
    let entry = &m["entries"]["raw/research/kept.md"];
    assert_eq!(entry["origin"], "researched", "{entry}");
    assert!(entry["ingested_at"].is_string(), "{entry}");
}

// ---------------------------------------------------------------------------
// Read-only commands must not modify the archive
//
// The archive lives in git — the README recommends it. `sentinel lint`
// appended to meta/log.md on every run, so a validation command left a dirty
// working tree and could not be used to check whether the tree was clean.
// `/sentinel-grow` runs lint every iteration, which buried the entries
// recording real changes under "0 error(s), 0 warning(s)".
// ---------------------------------------------------------------------------

fn indexed_archive() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "x");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/one.md",
        &common::article("One", "philosophy", &["raw/philosophy/s.md"]),
    );
    a.run(&["index"]);
    a
}

/// Every file's contents, so "nothing was touched" can be asserted precisely.
fn snapshot(a: &Archive) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dir in ["meta", "index", "wiki/philosophy", "raw/philosophy"] {
        if let Ok(entries) = std::fs::read_dir(a.path(dir)) {
            for e in entries
                .filter_map(Result::ok)
                .filter(|e| e.path().is_file())
            {
                let name = format!("{dir}/{}", e.file_name().to_string_lossy());
                out.push((name, std::fs::read_to_string(e.path()).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn lint_does_not_modify_the_archive() {
    let a = indexed_archive();
    let before = snapshot(&a);

    for _ in 0..5 {
        a.run(&["lint"]);
        a.run(&["lint", "--summary"]);
        a.json(&["lint"]);
    }

    assert_eq!(before, snapshot(&a), "a query must leave no trace");
}

#[test]
fn other_query_commands_do_not_modify_the_archive() {
    let a = indexed_archive();
    let before = snapshot(&a);

    for args in [
        vec!["status"],
        vec!["next"],
        vec!["uncompiled"],
        vec!["graph"],
        vec!["schema"],
        vec!["search", "one"],
        vec!["config"],
    ] {
        a.run(&args);
    }

    assert_eq!(before, snapshot(&a));
}

#[test]
fn a_rebuild_that_changes_nothing_writes_nothing() {
    // Generated output is deterministic, so a no-op index should leave every
    // file — and every mtime — alone.
    let a = indexed_archive();
    let before = snapshot(&a);
    let mtimes: Vec<_> = ["index/_master.md", "meta/link-graph.json"]
        .iter()
        .map(|f| std::fs::metadata(a.path(f)).unwrap().modified().unwrap())
        .collect();

    std::thread::sleep(std::time::Duration::from_millis(20));
    for _ in 0..3 {
        a.run(&["index"]);
    }

    assert_eq!(before, snapshot(&a));
    for (i, f) in ["index/_master.md", "meta/link-graph.json"]
        .iter()
        .enumerate()
    {
        assert_eq!(
            std::fs::metadata(a.path(f)).unwrap().modified().unwrap(),
            mtimes[i],
            "{f} was rewritten with identical contents"
        );
    }
}

#[test]
fn a_rebuild_that_changes_something_still_writes_and_logs() {
    // The no-op path must not turn into a no-work path.
    let a = indexed_archive();
    let entries_before = a.read("meta/log.md").matches("## [").count();

    a.write(
        "wiki/philosophy/two.md",
        &common::article("Two", "philosophy", &["raw/philosophy/s.md"]),
    );
    a.run(&["index"]);

    assert!(
        a.read("index/_master.md").contains("Two"),
        "the index must update"
    );
    assert_eq!(
        a.read("meta/log.md").matches("## [").count(),
        entries_before + 1,
        "a real change must still be recorded"
    );
}
