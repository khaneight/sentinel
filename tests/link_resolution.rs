//! Wikilink targets resolve by canonical form, and ingest can name its output.
//!
//! Both defects were found by dogfooding — ingesting this repository's own
//! documentation into an archive and running the loop by hand. Neither showed
//! up in 135 tests against archives built by a fixture that only ever wrote
//! byte-exact slugs and uniquely-named files.

mod common;

use common::{Archive, article};

fn with_body(title: &str, body: &str) -> String {
    article(title, "philosophy", &["raw/philosophy/src.md"]).replace("Body.", body)
}

fn archive_with_source() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "notes");
    a.run(&["sync"]);
    a
}

#[test]
fn a_link_spelled_differently_still_finds_its_article() {
    // `compile-loop.md` exists. Before this, only the byte-exact spelling
    // resolved; the other two were reported as broken links to an article
    // sitting right there.
    let a = archive_with_source();
    a.write(
        "wiki/philosophy/compile-loop.md",
        &with_body("Compile Loop", "Leaf."),
    );
    a.write(
        "wiki/philosophy/refers.md",
        &with_body(
            "Refers",
            "See [[Compile-Loop]], [[compile loop]], [[compile-loop]], [[Compile Loop]].",
        ),
    );

    let v = a.json(&["lint"]);
    let broken: Vec<&serde_json::Value> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["rule"] == "broken-link")
        .collect();
    assert!(
        broken.is_empty(),
        "links to an existing article reported as broken: {broken:?}"
    );
}

#[test]
fn demand_for_one_concept_does_not_fragment_across_spellings() {
    // The serious consequence. Three spellings used to produce three wanted
    // entries with one referrer each, so a genuinely popular gap could rank
    // below a rarely-mentioned one.
    let a = archive_with_source();
    for (i, spelling) in ["Derived-State", "derived state", "derived-state"]
        .iter()
        .enumerate()
    {
        a.write(
            &format!("wiki/philosophy/a{i}.md"),
            &with_body(&format!("A{i}"), &format!("See [[{spelling}]].")),
        );
    }
    // A rival concept named consistently, by two articles.
    for i in 0..2 {
        a.write(
            &format!("wiki/philosophy/b{i}.md"),
            &with_body(&format!("B{i}"), "See [[rival]]."),
        );
    }

    let v = a.json(&["next"]);
    assert_eq!(v["action"], "write", "{v}");
    assert_eq!(
        v["targets"][0]["id"], "derived-state",
        "three articles want derived-state and two want rival; fragmentation \
         used to hide that:\n{v}"
    );
    assert!(
        v["targets"][0]["detail"].as_str().unwrap().contains('3'),
        "{}",
        v["targets"][0]["detail"]
    );
}

#[test]
fn next_does_not_recommend_writing_an_article_that_already_exists() {
    // The dangerous case: `/sentinel-grow` acting on this would research and
    // write a duplicate of an article already in the wiki.
    let a = archive_with_source();
    a.write(
        "wiki/philosophy/free-will.md",
        &with_body("Free Will", "Leaf."),
    );
    a.write(
        "wiki/philosophy/refers.md",
        &with_body("Refers", "See [[Free Will]] and [[Free-Will]]."),
    );

    let v = a.json(&["next"]);

    // Both spellings resolve, so there is nothing wanted at all — `write` must
    // not even appear in the backlog.
    let write_backlog = v["backlog"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["action"] == "write");
    assert!(
        write_backlog.is_none(),
        "an existing article was counted as an unwritten gap: {v}"
    );
    assert_ne!(v["action"], "write", "{v}");
}

#[test]
fn plurals_are_left_alone() {
    // Case and separators are safe to fold. Stemming is not — a wrong merge
    // silently collapses two real concepts.
    let a = archive_with_source();
    a.write("wiki/philosophy/state.md", &with_body("State", "Leaf."));
    a.write(
        "wiki/philosophy/refers.md",
        &with_body("Refers", "See [[states]]."),
    );

    let v = a.json(&["lint"]);
    let messages: Vec<&str> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["message"].as_str())
        .collect();
    assert!(
        messages.iter().any(|m| m.contains("[[states]]")),
        "a plural must not silently resolve to the singular: {messages:?}"
    );
}

#[test]
fn a_case_only_filename_collision_is_reported() {
    // Two files whose stems differ only in case are as ambiguous to a wikilink
    // as two identical stems.
    let a = archive_with_source();
    a.write("wiki/philosophy/ethics.md", &with_body("Ethics", "x"));
    a.write("wiki/coding/Ethics.md", &with_body("Ethics", "x"));

    let v = a.json(&["lint"]);
    let rules: Vec<&str> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["rule"].as_str())
        .collect();
    assert!(rules.contains(&"duplicate-slug"), "{v}");
}

#[test]
fn graph_node_accepts_any_spelling() {
    let a = archive_with_source();
    a.write(
        "wiki/philosophy/compile-loop.md",
        &with_body("Compile Loop", "See [[other]]."),
    );
    a.write("wiki/philosophy/other.md", &with_body("Other", "Leaf."));
    a.run(&["index"]);

    let v = a.json(&["graph", "--node", "Compile Loop"]);
    assert_eq!(v["unknown"], false, "{v}");
    assert!(v["node_count"].as_u64().unwrap() >= 2, "{v}");
}

// ---------------------------------------------------------------------------
// ingest naming
// ---------------------------------------------------------------------------

#[test]
fn sources_sharing_a_basename_can_all_be_ingested() {
    // Repeated basenames are the norm in real corpora — SKILL.md, README.md,
    // index.md, chapter-1.md under per-book directories. Ingesting the second
    // one failed outright, with no flag available to resolve it.
    let a = Archive::new();
    let tmp = tempfile::tempdir().unwrap();
    for dir in ["one", "two", "three"] {
        let d = tmp.path().join(dir);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("SKILL.md"), format!("contents of {dir}")).unwrap();
    }

    for dir in ["one", "two", "three"] {
        let src = tmp.path().join(dir).join("SKILL.md");
        a.run(&[
            "ingest",
            &src.display().to_string(),
            "-d",
            "coding",
            "-t",
            &format!("Skill {dir}"),
        ]);
    }

    for dir in ["one", "two", "three"] {
        let expected = format!("raw/coding/skill-{dir}.md");
        assert!(
            a.path(&expected).is_file(),
            "expected {expected} to exist; ingest derives the name from --title"
        );
    }
}

#[test]
fn the_as_flag_names_the_destination_explicitly() {
    let a = Archive::new();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("SKILL.md");
    std::fs::write(&src, "x").unwrap();

    a.run(&[
        "ingest",
        &src.display().to_string(),
        "-d",
        "coding",
        "--as",
        "my-chosen-name.md",
    ]);

    assert!(a.path("raw/coding/my-chosen-name.md").is_file());
}

#[test]
fn a_genuine_collision_names_the_remedy() {
    let a = Archive::new();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("notes.md");
    std::fs::write(&src, "x").unwrap();

    a.run(&["ingest", &src.display().to_string(), "-d", "coding"]);
    let output = a.output(&["ingest", &src.display().to_string(), "-d", "coding"]);

    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("--as"),
        "the error must say how to fix it:\n{err}"
    );
}

#[test]
fn ingest_without_a_title_still_uses_the_source_filename() {
    // Unchanged behaviour for the common case; only titled ingests derive a name.
    let a = Archive::new();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("meditations.md");
    std::fs::write(&src, "x").unwrap();

    a.run(&["ingest", &src.display().to_string(), "-d", "philosophy"]);
    assert!(a.path("raw/philosophy/meditations.md").is_file());
}

#[test]
fn one_gap_spelled_two_ways_is_one_finding_per_article() {
    // After canonicalisation the demand folds correctly, but lint still listed
    // each spelling — so an agent working the list would research the same
    // missing article twice.
    let a = archive_with_source();
    for slug in ["a", "b"] {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &with_body(slug, "Refers to [[Missing-Thing]] and [[missing thing]]."),
        );
    }

    let v = a.json(&["lint", "--rule", "broken-link"]);
    let findings = v["findings"].as_array().unwrap();

    assert_eq!(
        findings.len(),
        2,
        "one per article, not one per spelling:\n{v}"
    );
    let message = findings[0]["message"].as_str().unwrap();
    assert!(message.contains("[[missing-thing]]"), "{message}");
    assert!(
        message.contains("[[Missing-Thing]]") && message.contains("[[missing thing]]"),
        "the spellings used are still worth reporting:\n{message}"
    );
}

#[test]
fn distinct_gaps_remain_distinct_findings() {
    let a = archive_with_source();
    a.write(
        "wiki/philosophy/a.md",
        &with_body("A", "See [[alpha]] and [[beta]]."),
    );

    let v = a.json(&["lint", "--rule", "broken-link"]);
    assert_eq!(v["findings"].as_array().unwrap().len(), 2, "{v}");
}
