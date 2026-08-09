//! `sentinel mv` — moving a raw document without breaking provenance.
//!
//! Reorganising `raw/` is inevitable in a real archive. Doing it by hand turns
//! every citation of the old path into an `unresolved-source` error: loud, but
//! repaired entirely by hand and easy to do incompletely.

mod common;

use common::{Archive, article};

fn with_sources(title: &str, sources: &[&str]) -> String {
    article(title, "philosophy", sources)
}

/// One source cited by three articles, plus one that does not cite it.
fn archive() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "text");
    a.write("raw/philosophy/other.md", "text");
    a.run(&["sync"]);
    for i in 0..3 {
        a.write(
            &format!("wiki/philosophy/cites-{i}.md"),
            &with_sources(&format!("Cites {i}"), &["raw/philosophy/meditations.md"]),
        );
    }
    a.write(
        "wiki/philosophy/unrelated.md",
        &with_sources("Unrelated", &["raw/philosophy/other.md"]),
    );
    a
}

#[test]
fn a_rename_repoints_every_citation() {
    let a = archive();
    assert_eq!(a.code(&["lint"]), 0, "precondition: archive is clean");

    a.run(&["mv", "raw/philosophy/meditations.md", "marcus.md"]);

    assert!(a.path("raw/philosophy/marcus.md").is_file());
    assert!(!a.path("raw/philosophy/meditations.md").exists());
    for i in 0..3 {
        let text = a.read(&format!("wiki/philosophy/cites-{i}.md"));
        assert!(text.contains("raw/philosophy/marcus.md"), "{text}");
        assert!(!text.contains("meditations.md"), "{text}");
    }
    assert_eq!(
        a.code(&["lint"]),
        0,
        "a move must not leave the archive in error"
    );
}

#[test]
fn the_manifest_follows_the_file() {
    let a = archive();
    a.run(&["mv", "raw/philosophy/meditations.md", "marcus.md"]);

    let manifest = a.read("meta/manifest.json");
    assert!(manifest.contains("raw/philosophy/marcus.md"), "{manifest}");
    assert!(!manifest.contains("meditations.md"), "{manifest}");
}

#[test]
fn moving_across_domains_updates_the_recorded_domain() {
    let a = archive();
    a.run(&[
        "mv",
        "raw/philosophy/meditations.md",
        "raw/research/meditations.md",
    ]);

    let v: serde_json::Value = serde_json::from_str(&a.read("meta/manifest.json")).unwrap();
    let entry = &v["entries"]["raw/research/meditations.md"];
    assert_eq!(entry["domain"], "research", "{entry}");
}

#[test]
fn articles_that_do_not_cite_it_are_untouched() {
    let a = archive();
    let before = a.read("wiki/philosophy/unrelated.md");
    a.run(&["mv", "raw/philosophy/meditations.md", "marcus.md"]);
    assert_eq!(before, a.read("wiki/philosophy/unrelated.md"));
}

#[test]
fn frontmatter_formatting_is_preserved() {
    // Round-tripping through serde would reorder keys and strip comments from a
    // file the user may also edit by hand, so the edit is textual and scoped to
    // the frontmatter block.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/fancy.md",
        "---\n\
         # a comment the user wrote\n\
         status: draft\n\
         title: Fancy\n\
         sources:\n  - raw/philosophy/src.md\n\
         domain: philosophy\n\
         origin: authored\n\
         tags: [t]\n\
         ---\n\n\
         Body with raw/philosophy/src.md mentioned in prose.\n",
    );

    a.run(&["mv", "raw/philosophy/src.md", "renamed.md"]);
    let text = a.read("wiki/philosophy/fancy.md");

    assert!(text.contains("# a comment the user wrote"), "{text}");
    assert!(
        text.find("status:").unwrap() < text.find("title:").unwrap(),
        "key order must survive:\n{text}"
    );
    assert!(text.contains("- raw/philosophy/renamed.md"), "{text}");
    assert!(
        text.contains("Body with raw/philosophy/src.md mentioned in prose."),
        "the body is not the citation list and must not be rewritten:\n{text}"
    );
}

#[test]
fn citations_written_in_other_shapes_are_repointed_too() {
    // Citations are hand-written by an LLM, so they arrive in several forms.
    // `mv` resolves them the same way the compile loop does.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    for (i, spelling) in ["./raw/philosophy/src.md", "philosophy/src.md", "src.md"]
        .iter()
        .enumerate()
    {
        a.write(
            &format!("wiki/philosophy/c{i}.md"),
            &with_sources(&format!("C{i}"), &[spelling]),
        );
    }

    a.run(&["mv", "raw/philosophy/src.md", "renamed.md"]);

    for i in 0..3 {
        let text = a.read(&format!("wiki/philosophy/c{i}.md"));
        assert!(
            text.contains("raw/philosophy/renamed.md"),
            "citation {i} not repointed:\n{text}"
        );
    }
    assert_eq!(a.code(&["lint"]), 0);
}

#[test]
fn a_bare_filename_identifies_the_source() {
    let a = archive();
    a.run(&["mv", "meditations.md", "marcus.md"]);
    assert!(a.path("raw/philosophy/marcus.md").is_file());
}

#[test]
fn dry_run_writes_nothing() {
    let a = archive();
    let before = a.read("wiki/philosophy/cites-0.md");
    let manifest = a.read("meta/manifest.json");

    let out = a.run(&[
        "mv",
        "raw/philosophy/meditations.md",
        "marcus.md",
        "--dry-run",
    ]);

    assert!(out.contains("Would move"), "{out}");
    assert!(out.contains("cites-0.md"), "{out}");
    assert!(a.path("raw/philosophy/meditations.md").is_file());
    assert!(!a.path("raw/philosophy/marcus.md").exists());
    assert_eq!(before, a.read("wiki/philosophy/cites-0.md"));
    assert_eq!(manifest, a.read("meta/manifest.json"));
}

#[test]
fn an_unknown_source_is_refused_with_guidance() {
    let a = archive();
    let output = a.output(&["mv", "raw/philosophy/nope.md", "x.md"]);

    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("No raw document matches"), "{err}");
    assert!(
        err.contains("uncompiled"),
        "must say how to find one:\n{err}"
    );
}

#[test]
fn an_occupied_destination_is_refused() {
    let a = archive();
    let output = a.output(&["mv", "raw/philosophy/meditations.md", "other.md"]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already exists"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(a.path("raw/philosophy/meditations.md").is_file());
}

#[test]
fn a_destination_outside_raw_is_refused() {
    // raw/ is the provenance floor; moving a source out of it would orphan
    // every article compiled from it with no way to repair the link.
    let a = archive();
    let output = a.output(&["mv", "raw/philosophy/meditations.md", "wiki/oops.md"]);

    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("under raw/"), "{err}");
    assert!(a.path("raw/philosophy/meditations.md").is_file());
}

#[test]
fn moving_a_source_nothing_cites_is_fine() {
    let a = archive();
    let out = a.run(&["mv", "raw/philosophy/other.md", "renamed.md"]);
    assert!(a.path("raw/philosophy/renamed.md").is_file());
    assert!(out.contains("Moved"), "{out}");
}

#[test]
fn json_reports_what_moved_and_what_was_repointed() {
    let a = archive();
    let v = a.json(&["mv", "raw/philosophy/meditations.md", "marcus.md"]);

    assert_eq!(v["command"], "mv");
    assert_eq!(v["from"], "raw/philosophy/meditations.md");
    assert_eq!(v["to"], "raw/philosophy/marcus.md");
    assert_eq!(v["updated_articles"].as_array().unwrap().len(), 3);
    assert_eq!(v["dry_run"], false);
}
