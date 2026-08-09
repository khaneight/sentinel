//! `sentinel rm` — deleting a raw document.
//!
//! Written adversarially from the start. The audit in #15–#19 found that every
//! mutating command in this tool shipped with tests proving the feature worked
//! and none asking what happened when it could not get what it needed. These
//! ask the second question first.

mod common;

use common::{Archive, article};

/// One source cited by two articles, and one cited by nothing.
fn archive() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/cited.md", "text");
    a.write("raw/philosophy/lonely.md", "text");
    a.run(&["sync"]);
    for i in 0..2 {
        a.write(
            &format!("wiki/philosophy/cites-{i}.md"),
            &article(
                &format!("Cites {i}"),
                "philosophy",
                &["raw/philosophy/cited.md"],
            ),
        );
    }
    a
}

// ---------------------------------------------------------------------------
// Refusing
// ---------------------------------------------------------------------------

#[test]
fn deleting_a_cited_source_is_refused_and_names_the_articles() {
    let a = archive();
    let output = a.output(&["rm", "raw/philosophy/cited.md"]);

    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("cited by 2 wiki article(s)"), "{err}");
    assert!(err.contains("wiki/philosophy/cites-0.md"), "{err}");
    assert!(err.contains("wiki/philosophy/cites-1.md"), "{err}");
    assert!(
        a.path("raw/philosophy/cited.md").is_file(),
        "a refusal must not have deleted anything"
    );
}

#[test]
fn the_refusal_points_at_mv_for_the_case_it_is_probably_hiding() {
    // Most attempts to delete a cited source are really a rename in disguise.
    let a = archive();
    let output = a.output(&["rm", "raw/philosophy/cited.md"]);
    let err = String::from_utf8_lossy(&output.stderr);

    assert!(err.contains("sentinel mv"), "{err}");
    assert!(
        err.contains("--force"),
        "and must say how to proceed:\n{err}"
    );
}

#[test]
fn an_uncited_source_is_removed_without_ceremony() {
    let a = archive();
    a.run(&["rm", "raw/philosophy/lonely.md"]);

    assert!(!a.path("raw/philosophy/lonely.md").exists());
    let manifest = a.read("meta/manifest.json");
    assert!(!manifest.contains("lonely.md"), "{manifest}");
    assert_eq!(a.code(&["lint"]), 0, "nothing should be broken");
}

#[test]
fn an_unknown_target_is_refused_with_guidance() {
    let a = archive();
    let output = a.output(&["rm", "raw/philosophy/nope.md"]);

    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("No raw document matches"), "{err}");
    assert!(err.contains("uncompiled"), "{err}");
}

// ---------------------------------------------------------------------------
// Forcing
// ---------------------------------------------------------------------------

#[test]
fn force_deletes_but_reports_every_citation_it_orphans() {
    let a = archive();
    let out = a.run(&["rm", "raw/philosophy/cited.md", "--force"]);

    assert!(!a.path("raw/philosophy/cited.md").exists());
    assert!(out.contains("wiki/philosophy/cites-0.md"), "{out}");
    assert!(
        out.contains("does not exist"),
        "the consequence must be stated, not just the action:\n{out}"
    );
}

#[test]
fn a_forced_delete_leaves_findable_damage() {
    // The articles are genuinely broken afterwards. That has to be visible to
    // lint rather than quietly absorbed.
    let a = archive();
    a.run(&["rm", "raw/philosophy/cited.md", "--force"]);

    assert_eq!(a.code(&["lint"]), 2);
    let v = a.json(&["lint"]);
    assert_eq!(v["by_rule"]["unresolved-source"]["count"], 2, "{v}");
}

// ---------------------------------------------------------------------------
// Partial views and previews
// ---------------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn rm_refuses_when_an_article_cannot_be_read() {
    // This command's whole value is telling the caller what they will break.
    // An article it could not read is a citation it would not have counted, so
    // the report would understate the damage.
    use std::os::unix::fs::PermissionsExt;
    let set = |p: &std::path::Path, m: u32| {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
    };

    let a = archive();
    let hidden = a.path("wiki/philosophy/cites-0.md");
    set(&hidden, 0o000);
    let output = a.output(&["rm", "raw/philosophy/cited.md", "--force"]);
    let code = output.status.code();
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    set(&hidden, 0o644);

    assert_eq!(code, Some(1), "rm proceeded on a partial view");
    assert!(err.contains("could not be read"), "{err}");
    assert!(
        a.path("raw/philosophy/cited.md").is_file(),
        "and must not have deleted anything"
    );
}

#[test]
fn dry_run_previews_the_damage_without_doing_it() {
    let a = archive();
    let out = a.run(&["rm", "raw/philosophy/cited.md", "--force", "--dry-run"]);

    assert!(out.contains("Would remove"), "{out}");
    assert!(out.contains("would lose its source"), "{out}");
    assert!(a.path("raw/philosophy/cited.md").is_file());
    assert!(a.read("meta/manifest.json").contains("cited.md"));
}

#[test]
fn dry_run_on_a_cited_source_still_refuses_without_force() {
    // A preview must not be a way to skip the guard rail.
    let a = archive();
    let output = a.output(&["rm", "raw/philosophy/cited.md", "--dry-run"]);
    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// Shape of the target argument
// ---------------------------------------------------------------------------

#[test]
fn any_spelling_a_citation_can_take_identifies_the_target() {
    for spelling in [
        "raw/philosophy/lonely.md",
        "./raw/philosophy/lonely.md",
        "philosophy/lonely.md",
        "lonely.md",
    ] {
        let a = archive();
        a.run(&["rm", spelling]);
        assert!(
            !a.path("raw/philosophy/lonely.md").exists(),
            "failed for {spelling}"
        );
    }
}

#[test]
fn json_reports_the_removal_and_what_it_orphaned() {
    let a = archive();
    let v = a.json(&["rm", "raw/philosophy/cited.md", "--force"]);

    assert_eq!(v["command"], "rm");
    assert_eq!(v["removed"], "raw/philosophy/cited.md");
    assert_eq!(v["orphaned_citations"].as_array().unwrap().len(), 2);
    assert_eq!(v["dry_run"], false);
}
