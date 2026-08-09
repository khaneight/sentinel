//! `sentinel schema` — the published contract.
//!
//! The point of this command is that skills and agents stop restating the
//! schema in prose. These tests check the published contract actually matches
//! what the rest of the tool enforces, because a schema that lies is worse than
//! no schema at all.

mod common;

use common::Archive;

#[test]
fn schema_publishes_the_required_frontmatter_fields() {
    let a = Archive::new();
    let v = a.json(&["schema"]);

    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["command"], "schema");

    let fields = v["frontmatter"].as_array().unwrap();
    let required: Vec<&str> = fields
        .iter()
        .filter(|f| f["required"] == true)
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(required, ["title", "domain", "origin"]);
}

#[test]
fn enum_fields_publish_their_accepted_values() {
    let a = Archive::new();
    let v = a.json(&["schema"]);
    let field = |name: &str| {
        v["frontmatter"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap_or_else(|| panic!("no field {name}"))
            .clone()
    };

    assert_eq!(
        field("origin")["values"].as_array().unwrap().len(),
        3,
        "origin must publish authored/researched/hybrid"
    );
    assert_eq!(field("status")["values"].as_array().unwrap().len(), 3);
}

#[test]
fn published_enum_values_are_the_ones_lint_actually_accepts() {
    // The failure this prevents: schema advertising a value the linter rejects,
    // so an agent following the published contract produces a lint error.
    let a = Archive::new();
    let v = a.json(&["schema"]);

    let origins: Vec<String> = v["frontmatter"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "origin")
        .unwrap()["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();

    a.write("raw/philosophy/src.md", "notes");
    a.run(&["sync"]);
    for (i, origin) in origins.iter().enumerate() {
        a.write(
            &format!("wiki/philosophy/a{i}.md"),
            &format!(
                "---\ntitle: A{i}\ndomain: philosophy\norigin: {origin}\ntags: [t]\nsources: [raw/philosophy/src.md]\n---\n\nBody.\n"
            ),
        );
    }

    let lint = a.json(&["lint"]);
    let bad: Vec<&serde_json::Value> = lint["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["rule"] == "invalid-origin")
        .collect();
    assert!(
        bad.is_empty(),
        "schema advertises an origin the linter rejects: {bad:?}"
    );
}

#[test]
fn every_lint_rule_is_published_with_its_severity() {
    let a = Archive::new();
    let v = a.json(&["schema"]);

    let rules = v["lint_rules"].as_array().unwrap();
    assert!(!rules.is_empty());
    for rule in rules {
        assert!(rule["rule"].is_string(), "{rule}");
        assert!(
            rule["severity"] == "error" || rule["severity"] == "warning",
            "{rule}"
        );
        assert!(
            rule["description"].as_str().is_some_and(|d| d.len() > 10),
            "every rule needs a description an agent can act on: {rule}"
        );
    }
}

#[test]
fn domains_are_reported_from_disk_not_from_a_constant() {
    // The bug this closes: /sentinel-compile documented five domains where
    // DEFAULT_DOMAINS had three, and nothing could tell which was true.
    let a = Archive::new();
    a.write("wiki/anthropology/kinship.md", "---\ntitle: Kinship\n---\n");

    let v = a.json(&["schema"]);
    let present: Vec<&str> = v["domains"]["present"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();

    assert!(
        present.contains(&"anthropology"),
        "a domain created by hand must be reported: {present:?}"
    );
    assert!(present.contains(&"philosophy"), "{present:?}");

    let default: Vec<&str> = v["domains"]["default"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();
    assert_eq!(default, ["philosophy", "coding", "research"]);
}

#[test]
fn the_next_priority_ladder_is_published() {
    let a = Archive::new();
    let v = a.json(&["schema"]);

    let actions: Vec<&str> = v["next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        ["fix-errors", "compile", "write", "connect", "review"]
    );
}

#[test]
fn schema_works_without_an_initialized_archive_layout() {
    // An agent may run `schema` before anything exists, to learn the contract.
    let a = Archive::new();
    std::fs::remove_dir_all(a.path("wiki")).unwrap();

    let v = a.json(&["schema"]);
    assert!(v["frontmatter"].as_array().unwrap().len() > 5, "{v}");
}

#[test]
fn human_output_is_readable_and_names_the_enums() {
    let a = Archive::new();
    let out = a.run(&["schema"]);

    assert!(out.contains("Wiki Article Frontmatter"), "{out}");
    assert!(out.contains("authored | researched | hybrid"), "{out}");
    assert!(out.contains("Lint Rules"), "{out}");
}

// ---------------------------------------------------------------------------
// init scaffolding
// ---------------------------------------------------------------------------

#[test]
fn init_writes_the_archive_claude_md_the_readme_promises() {
    let a = Archive::new();
    let text = a.read("CLAUDE.md");

    assert!(text.contains("sentinel next"), "{text}");
    assert!(
        text.contains("sources:"),
        "the conventions must explain what closes the compile loop"
    );
    assert!(
        text.contains("_master.md"),
        "it must warn against slurping the master index into context"
    );
}

#[test]
fn init_does_not_clobber_an_edited_claude_md() {
    let a = Archive::new();
    a.write("CLAUDE.md", "my own conventions");
    a.run(&["init", &a.root.display().to_string()]);

    assert_eq!(a.read("CLAUDE.md"), "my own conventions");
}
