//! The raw → wiki compilation loop.
//!
//! Before this existed, nothing in the program ever populated
//! `ManifestEntry.wiki_articles`, so `uncompiled` listed every raw document
//! forever and the archive had no notion of progress. These tests pin the
//! state machine: a raw document leaves the queue when — and only when — a
//! wiki article cites it.

mod common;

use common::{Archive, article};

#[test]
fn a_raw_document_starts_uncompiled() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);

    let out = a.run(&["uncompiled"]);
    assert!(out.contains("raw/philosophy/meditations.md"), "{out}");
}

#[test]
fn citing_a_raw_document_takes_it_out_of_the_queue() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/stoicism.md",
        &article("Stoicism", "philosophy", &["raw/philosophy/meditations.md"]),
    );

    let out = a.run(&["uncompiled"]);
    assert!(
        out.contains("All raw documents have been compiled"),
        "a cited raw document must leave the queue:\n{out}"
    );
}

#[test]
fn the_queue_is_correct_without_running_index_first() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/stoicism.md",
        &article("Stoicism", "philosophy", &["raw/philosophy/meditations.md"]),
    );

    // No `sentinel index` in between. The mapping is derived on read, so a
    // stale index cannot produce a wrong answer.
    let status = a.run(&["status"]);
    assert!(status.contains("Uncompiled:      0"), "{status}");
    assert!(status.contains("Wiki articles:   1"), "{status}");
}

#[test]
fn index_publishes_the_mapping_into_the_manifest() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/stoicism.md",
        &article("Stoicism", "philosophy", &["raw/philosophy/meditations.md"]),
    );
    a.run(&["index"]);

    let manifest = a.read("meta/manifest.json");
    assert!(
        manifest.contains("wiki/philosophy/stoicism.md"),
        "index must write the derived mapping for external readers:\n{manifest}"
    );
}

#[test]
fn removing_a_citation_returns_the_document_to_the_queue() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    let wiki = "wiki/philosophy/stoicism.md";
    a.write(
        wiki,
        &article("Stoicism", "philosophy", &["raw/philosophy/meditations.md"]),
    );
    a.run(&["index"]);

    // The article is rewritten without its source citation.
    a.write(wiki, &article("Stoicism", "philosophy", &[]));
    a.run(&["index"]);

    let out = a.run(&["uncompiled"]);
    assert!(out.contains("meditations.md"), "{out}");

    let manifest = a.read("meta/manifest.json");
    assert!(
        !manifest.contains("wiki/philosophy/stoicism.md"),
        "a stale mapping must be cleared, not left behind:\n{manifest}"
    );
}

#[test]
fn one_raw_document_may_feed_several_articles() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    for slug in ["stoicism", "virtue"] {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &article(slug, "philosophy", &["raw/philosophy/meditations.md"]),
        );
    }
    a.run(&["index"]);

    let manifest = a.read("meta/manifest.json");
    assert!(
        manifest.contains("wiki/philosophy/stoicism.md"),
        "{manifest}"
    );
    assert!(manifest.contains("wiki/philosophy/virtue.md"), "{manifest}");
}

#[test]
fn lint_reports_a_citation_that_matches_no_raw_document() {
    let a = Archive::new();
    a.write(
        "wiki/philosophy/stoicism.md",
        &article(
            "Stoicism",
            "philosophy",
            &["raw/philosophy/never-existed.md"],
        ),
    );

    let out = a.run(&["lint"]);
    assert!(
        out.contains("matches no raw document"),
        "a dangling source citation is a silent hole in provenance:\n{out}"
    );
}

#[test]
fn an_ambiguous_bare_filename_is_reported_not_guessed() {
    let a = Archive::new();
    a.write("raw/philosophy/notes.md", "one");
    a.write("raw/coding/notes.md", "two");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/stoicism.md",
        &article("Stoicism", "philosophy", &["notes.md"]),
    );

    let out = a.run(&["lint"]);
    assert!(out.contains("matches no raw document"), "{out}");

    let uncompiled = a.run(&["uncompiled"]);
    assert!(
        uncompiled.contains("raw/philosophy/notes.md"),
        "{uncompiled}"
    );
    assert!(uncompiled.contains("raw/coding/notes.md"), "{uncompiled}");
}

#[test]
fn index_generates_the_uncompiled_work_queue() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    a.run(&["index"]);

    let queue = a.read("index/_uncompiled.md");
    assert!(queue.contains("raw/philosophy/meditations.md"), "{queue}");

    a.write(
        "wiki/philosophy/stoicism.md",
        &article("Stoicism", "philosophy", &["raw/philosophy/meditations.md"]),
    );
    a.run(&["index"]);

    let queue = a.read("index/_uncompiled.md");
    assert!(
        queue.contains("Every raw document has been compiled"),
        "{queue}"
    );
}

#[test]
fn generated_indexes_are_stable_across_runs() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    for slug in ["virtue", "stoicism", "ataraxia"] {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &article(slug, "philosophy", &["raw/philosophy/meditations.md"]),
        );
    }

    a.run(&["index"]);
    let first: Vec<String> = ["_master.md", "_by-domain.md", "_recent.md", "_orphans.md"]
        .iter()
        .map(|f| a.read(&format!("index/{f}")))
        .collect();

    a.run(&["index"]);
    let second: Vec<String> = ["_master.md", "_by-domain.md", "_recent.md", "_orphans.md"]
        .iter()
        .map(|f| a.read(&format!("index/{f}")))
        .collect();

    assert_eq!(
        first, second,
        "generated files must not churn between runs — they live in the user's git repo"
    );
}
