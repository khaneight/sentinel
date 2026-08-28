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

    let origin_field = field("origin");
    let origins: Vec<&str> = origin_field["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert_eq!(
        origins,
        ["authored", "researched", "hybrid", "extrapolated"],
        "origin must publish every value an article may carry"
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
fn every_command_that_takes_an_origin_accepts_every_published_value() {
    // The earlier version of this checked only the linter. `ingest` kept its
    // own hardcoded list and rejected `hybrid` while `schema` advertised it —
    // so an agent following the published contract got an error.
    let a = Archive::new();
    let v = a.json(&["schema"]);
    // The ingestable set, published separately: an article may be
    // `extrapolated`, and a raw document may never be, so one list would have
    // to be wrong for one of them.
    let origins: Vec<String> = v["ingest_origins"]
        .as_array()
        .expect("schema publishes what `ingest -o` accepts")
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    assert!(origins.len() >= 3, "{origins:?}");

    let article_origins: Vec<&str> = v["frontmatter"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "origin")
        .unwrap()["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    for origin in &origins {
        assert!(
            article_origins.contains(&origin.as_str()),
            "`{origin}` can be ingested but is not a legal article origin"
        );
    }
    assert!(
        origins.len() < article_origins.len(),
        "if every article origin can be ingested, `raw/` can hold generated work"
    );

    for origin in &origins {
        let src = a.path(&format!("src-{origin}.md"));
        std::fs::write(&src, "x").unwrap();
        let output = a.output(&[
            "ingest",
            &src.display().to_string(),
            "-d",
            "philosophy",
            "-o",
            origin,
            "-t",
            &format!("Doc {origin}"),
        ]);
        assert!(
            output.status.success(),
            "`sentinel ingest -o {origin}` was rejected, but `sentinel schema` \
             advertises it:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn an_unknown_origin_names_the_accepted_values() {
    let a = Archive::new();
    let src = a.path("src.md");
    std::fs::write(&src, "x").unwrap();
    let output = a.output(&[
        "ingest",
        &src.display().to_string(),
        "-d",
        "philosophy",
        "-o",
        "nonsense",
    ]);

    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("hybrid"), "must list what is accepted:\n{err}");
    assert!(err.contains("sentinel schema"), "{err}");
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
        [
            "fix-errors",
            "learn",
            "compile",
            "write",
            "connect",
            "extend",
            "review"
        ]
    );

    // The numbering is derived from the ladder, not written beside it.
    let priorities: Vec<u64> = v["next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["priority"].as_u64().unwrap())
        .collect();
    assert_eq!(priorities, (1..=actions.len() as u64).collect::<Vec<_>>());
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

// ---------------------------------------------------------------------------
// Artifacts `init` creates must not assert facts nothing maintains
// ---------------------------------------------------------------------------

#[test]
fn the_article_template_matches_the_published_contract() {
    // The template was a fourth hand-written copy of the frontmatter fields,
    // alongside the lint rule, the schema output, and `ingest`'s validation.
    // Three of those four had already drifted apart.
    let a = Archive::new();
    let template = a.read("templates/wiki-article.md");
    let v = a.json(&["schema"]);

    for field in v["frontmatter"].as_array().unwrap() {
        let name = field["name"].as_str().unwrap();
        assert!(
            template.contains(&format!("{name}:")),
            "template omits `{name}`, which `sentinel schema` publishes:\n{template}"
        );
    }

    let keys: Vec<&str> = template
        .lines()
        .filter(|l| !l.starts_with("---") && !l.trim().is_empty())
        .filter_map(|l| l.split(':').next())
        .collect();
    assert_eq!(
        keys.len(),
        v["frontmatter"].as_array().unwrap().len(),
        "template has keys the contract does not: {keys:?}"
    );
}

#[test]
fn a_template_article_passes_lint_once_filled_in() {
    // The template is only useful if what it produces is valid.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);

    let filled = a
        .read("templates/wiki-article.md")
        .replace("title:", "title: Filled")
        .replace("domain:", "domain: philosophy")
        .replace("tags: []", "tags: [t]")
        .replace("sources: []", "sources: [raw/philosophy/src.md]");
    a.write("wiki/philosophy/filled.md", &format!("{filled}\nBody.\n"));

    assert_eq!(a.code(&["lint"]), 0, "{}", a.run(&["lint"]));
}

#[test]
fn the_front_page_does_not_list_domains_it_will_not_update() {
    // SUMMARY.md named three domains with descriptions, written once at `init`
    // and never revisited — so an archive that grew a fourth had a front page
    // quietly disagreeing with it.
    let a = Archive::new();
    a.write("wiki/anthropology/kinship.md", "---\ntitle: Kinship\n---\n");

    let summary = a.read("SUMMARY.md");
    let schema = a.json(&["schema"]);
    let present: Vec<&str> = schema["domains"]["present"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();
    assert!(present.contains(&"anthropology"), "{present:?}");

    assert!(
        !summary.contains("**Philosophy**"),
        "the front page asserts a domain list it does not maintain:\n{summary}"
    );
    assert!(
        summary.contains("_by-domain.md") || summary.contains("sentinel schema"),
        "it should point at the live answer instead:\n{summary}"
    );
}
