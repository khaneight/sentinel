//! `sentinel next` — the recommendation surface.
//!
//! The priority ladder is editorial judgement encoded in the CLI, so it is
//! pinned here explicitly. If the ordering changes, these tests should fail and
//! force the change to be deliberate.

mod common;

use common::{Archive, article};

/// An article with a body, so wikilinks can be planted in it.
fn article_with(title: &str, sources: &[&str], body: &str) -> String {
    article(title, "philosophy", sources).replace("Body.", body)
}

#[test]
fn an_empty_archive_has_nothing_to_do() {
    let a = Archive::new();
    let v = a.json(&["next"]);

    assert_eq!(v["action"], "none");
    assert_eq!(v["backlog"].as_array().unwrap().len(), 0);
    assert!(v["suggested_command"].is_null(), "{v}");
}

#[test]
fn an_uncompiled_source_is_the_next_thing_to_compile() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);

    let v = a.json(&["next"]);
    assert_eq!(v["action"], "compile");
    assert_eq!(v["targets"][0]["id"], "raw/philosophy/meditations.md");
    assert_eq!(
        v["suggested_command"],
        "/sentinel-compile raw/philosophy/meditations.md"
    );
}

#[test]
fn errors_outrank_everything_else() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    // Malformed frontmatter: an error. There is also an uncompiled source.
    a.write("wiki/philosophy/bad.md", "---\ntitle: [unterminated\n---\n");

    let v = a.json(&["next"]);
    assert_eq!(
        v["action"], "fix-errors",
        "a malformed archive makes every later judgement unreliable:\n{v}"
    );

    // The compile work is still reported, just not recommended first.
    let backlog: Vec<&str> = v["backlog"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert!(backlog.contains(&"compile"), "{v}");
}

#[test]
fn once_sources_are_compiled_the_wiki_names_its_own_next_article() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/stoicism.md",
        &article_with(
            "Stoicism",
            &["raw/philosophy/meditations.md"],
            "Related: [[virtue]] and [[ataraxia]].",
        ),
    );
    a.write(
        "wiki/philosophy/logos.md",
        &article_with(
            "Logos",
            &["raw/philosophy/meditations.md"],
            "See [[virtue]] and [[stoicism]].",
        ),
    );

    let v = a.json(&["next"]);
    assert_eq!(v["action"], "write");
    assert_eq!(
        v["targets"][0]["id"], "virtue",
        "the most-linked unwritten concept is the most wanted:\n{v}"
    );
    assert_eq!(v["targets"][1]["id"], "ataraxia");
    assert_eq!(v["suggested_command"], "/sentinel-research virtue");
}

#[test]
fn orphans_come_after_gaps() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    // Two articles, fully compiled, no dangling links, neither linked to.
    for slug in ["alpha", "beta"] {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &article_with(slug, &["raw/philosophy/meditations.md"], "No links here."),
        );
    }
    a.run(&["index"]);

    let v = a.json(&["next"]);
    assert_eq!(v["action"], "connect", "{v}");
    assert_eq!(v["targets"].as_array().unwrap().len(), 2);
}

#[test]
fn a_stalled_draft_is_surfaced_last() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    // Linked to each other, so neither is an orphan; no dangling links.
    a.write(
        "wiki/philosophy/alpha.md",
        &article_with("Alpha", &["raw/philosophy/meditations.md"], "See [[beta]].")
            .replace("updated: 2026-01-01", "updated: 2020-01-01"),
    );
    a.write(
        "wiki/philosophy/beta.md",
        &article_with("Beta", &["raw/philosophy/meditations.md"], "See [[alpha]].")
            .replace("updated: 2026-01-01", "updated: 2020-01-01"),
    );
    a.run(&["index"]);

    let v = a.json(&["next"]);
    assert_eq!(v["action"], "review", "{v}");
    assert!(
        v["reason"].as_str().unwrap().contains("draft"),
        "{}",
        v["reason"]
    );
}

#[test]
fn backlog_reports_every_category_so_a_caller_can_disagree() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.write("raw/philosophy/stranded.md", "nothing cites this");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/stoicism.md",
        &article_with(
            "Stoicism",
            &["raw/philosophy/meditations.md"],
            "See [[virtue]].",
        ),
    );
    a.run(&["index"]);

    let v = a.json(&["next"]);
    let backlog: Vec<(&str, u64)> = v["backlog"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["action"].as_str().unwrap(), e["count"].as_u64().unwrap()))
        .collect();

    assert!(backlog.contains(&("compile", 1)), "{v}");
    assert!(backlog.contains(&("write", 1)), "{v}");
    // Priority order is preserved in the backlog listing.
    let order: Vec<&str> = backlog.iter().map(|(a, _)| *a).collect();
    let compile_at = order.iter().position(|a| *a == "compile").unwrap();
    let write_at = order.iter().position(|a| *a == "write").unwrap();
    assert!(compile_at < write_at, "{v}");
}

#[test]
fn human_output_names_the_command_to_run() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);

    let out = a.run(&["next"]);
    assert!(out.contains("compile"), "{out}");
    assert!(out.contains("/sentinel-compile"), "{out}");
}

#[test]
fn next_works_before_index_has_ever_run() {
    // An agent's first action on a fresh archive should not require knowing to
    // run `index` first.
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);

    let v = a.json(&["next"]);
    assert_eq!(v["action"], "compile", "{v}");
}
