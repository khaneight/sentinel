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

// ---------------------------------------------------------------------------
// Whole-entry rewriting
//
// The first implementation replaced the citation as a substring anywhere in the
// frontmatter block. Renaming `a.md` in an article that also cited `data.md`
// produced `datraw/philosophy/alpha.md` — which then resolved *by basename* to
// the renamed document, so lint reported the archive clean while one article's
// provenance silently pointed at the wrong source.
// ---------------------------------------------------------------------------

fn two_sources_one_a_substring_of_the_other() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/a.md", "one");
    a.write("raw/philosophy/data.md", "two");
    a.run(&["sync"]);
    a
}

#[test]
fn renaming_a_source_does_not_corrupt_a_neighbouring_citation() {
    let a = two_sources_one_a_substring_of_the_other();
    a.write(
        "wiki/philosophy/both.md",
        "---\ntitle: Both\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources:\n  - a.md\n  - data.md\nstatus: draft\n---\n\nBody.\n",
    );

    a.run(&["mv", "a.md", "alpha.md"]);
    let text = a.read("wiki/philosophy/both.md");

    assert!(text.contains("- raw/philosophy/alpha.md"), "{text}");
    assert!(
        text.contains("- data.md"),
        "the neighbouring citation was rewritten:\n{text}"
    );
    assert!(!text.contains("datraw"), "substring corruption:\n{text}");
    assert_eq!(a.code(&["lint"]), 0);
}

#[test]
fn a_corrupted_citation_would_not_have_been_caught_by_lint() {
    // Why the bug mattered: the corrupted path still resolved, by basename, to
    // the renamed document. Provenance pointed at the wrong source and nothing
    // reported it. This asserts the *correct* mapping, which is the only way to
    // detect that failure — an error count cannot.
    let a = two_sources_one_a_substring_of_the_other();
    a.write(
        "wiki/philosophy/both.md",
        "---\ntitle: Both\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources:\n  - a.md\n  - data.md\nstatus: draft\n---\n\nBody.\n",
    );
    a.run(&["mv", "a.md", "alpha.md"]);
    a.run(&["index"]);

    let manifest: serde_json::Value = serde_json::from_str(&a.read("meta/manifest.json")).unwrap();
    let cited_by = |raw: &str| -> Vec<String> {
        manifest["entries"][raw]["wiki_articles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        cited_by("raw/philosophy/alpha.md"),
        ["wiki/philosophy/both.md"]
    );
    assert_eq!(
        cited_by("raw/philosophy/data.md"),
        ["wiki/philosophy/both.md"],
        "data.md must still be cited; corruption silently unlinked it"
    );
}

#[test]
fn the_inline_list_form_is_rewritten_entry_by_entry() {
    let a = two_sources_one_a_substring_of_the_other();
    a.write(
        "wiki/philosophy/inline.md",
        "---\ntitle: Inline\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources: [\"a.md\", data.md]\nstatus: draft\n---\n\nBody.\n",
    );

    a.run(&["mv", "a.md", "alpha.md"]);
    let text = a.read("wiki/philosophy/inline.md");

    assert!(
        text.contains("sources: [\"raw/philosophy/alpha.md\", data.md]"),
        "quoting and the untouched neighbour must both survive:\n{text}"
    );
}

#[test]
fn a_quoted_citation_keeps_its_quotes() {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "x");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/quoted.md",
        "---\ntitle: Quoted\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources:\n  - \"raw/philosophy/src.md\"\nstatus: draft\n---\n\nBody.\n",
    );

    a.run(&["mv", "raw/philosophy/src.md", "renamed.md"]);
    let text = a.read("wiki/philosophy/quoted.md");
    assert!(text.contains("- \"raw/philosophy/renamed.md\""), "{text}");
}

#[test]
fn a_matching_string_outside_the_sources_list_is_left_alone() {
    // Only citation entries are rewritten — not a title, a tag, or prose that
    // happens to contain the same text.
    let a = Archive::new();
    a.write("raw/philosophy/notes.md", "x");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/tricky.md",
        "---\ntitle: About notes.md\ndomain: philosophy\norigin: authored\n\
         tags: [t]\nsources:\n  - notes.md\nstatus: draft\n---\n\n\
         The file notes.md is discussed here.\n",
    );

    a.run(&["mv", "notes.md", "renamed.md"]);
    let text = a.read("wiki/philosophy/tricky.md");

    assert!(text.contains("title: About notes.md"), "{text}");
    assert!(
        text.contains("The file notes.md is discussed here."),
        "{text}"
    );
    assert!(text.contains("- raw/philosophy/renamed.md"), "{text}");
}

#[cfg(unix)]
fn set_mode(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
#[cfg(unix)]
fn mv_refuses_when_an_article_cannot_be_read() {
    // `mv` rewrites the articles it can see and moves the file regardless. An
    // article missed here keeps a citation to a path that no longer exists —
    // and `mv` reported "(no articles cited it)", which was simply untrue.
    let a = archive();
    let hidden = a.path("wiki/philosophy/cites-0.md");
    set_mode(&hidden, 0o000);

    let output = a.output(&["mv", "raw/philosophy/meditations.md", "marcus.md"]);
    let code = output.status.code();
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    set_mode(&hidden, 0o644);

    assert_eq!(code, Some(1), "mv reported success on a partial view");
    assert!(err.contains("could not be read"), "{err}");
    assert!(
        a.path("raw/philosophy/meditations.md").is_file(),
        "the source must not move when its citations cannot all be repointed"
    );
    assert!(!a.path("raw/philosophy/marcus.md").exists());
}

#[test]
#[cfg(unix)]
fn mv_does_not_claim_nothing_cited_a_source_it_could_not_check() {
    let a = archive();
    let hidden = a.path("wiki/philosophy/cites-0.md");
    set_mode(&hidden, 0o000);
    let output = a.output(&["mv", "raw/philosophy/meditations.md", "marcus.md"]);
    set_mode(&hidden, 0o644);

    let out = String::from_utf8_lossy(&output.stdout);
    assert!(
        !out.contains("no articles cited it"),
        "a claim about citations must not be made from an incomplete read:\n{out}"
    );
}
