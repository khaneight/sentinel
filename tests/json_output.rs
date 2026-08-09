//! The `--json` contract.
//!
//! These are deliberately strict about field names and shapes. The whole point
//! of a machine-readable surface is that a consumer can rely on it, so a change
//! that renames or drops a field should fail here and force a conscious
//! decision about `schema_version` rather than silently break every caller.

mod common;

use common::{Archive, article, stdout};

/// Every payload identifies itself the same way.
fn assert_envelope(value: &serde_json::Value, command: &str) {
    assert_eq!(value["schema_version"], 1, "{value}");
    assert_eq!(value["command"], command, "{value}");
    assert!(
        value["archive"].as_str().is_some_and(|s| !s.is_empty()),
        "every payload must name the archive it describes:\n{value}"
    );
    // The one exception is `config` reporting that no archive resolved; it has
    // its own assertions in `config_json_diagnoses_a_failure_it_cannot_resolve`.
}

fn populated() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "On the shortness of life.");
    a.write("raw/philosophy/stranded.md", "nothing cites this");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/stoicism.md",
        &article("Stoicism", "philosophy", &["raw/philosophy/meditations.md"]),
    );
    a.run(&["index"]);
    a
}

#[test]
fn status_reports_every_counter() {
    let a = populated();
    let v = a.json(&["status"]);
    assert_envelope(&v, "status");

    assert_eq!(v["raw_documents"], 2);
    assert_eq!(v["wiki_articles"], 1);
    assert_eq!(v["uncompiled"], 1);
    assert_eq!(v["unresolved_sources"], 0);
    for key in ["orphan_pages", "raw_domains", "wiki_domains"] {
        assert!(v[key].is_number(), "missing {key}:\n{v}");
    }
}

#[test]
fn lint_findings_carry_severity_and_a_stable_rule_id() {
    let a = Archive::new();
    a.write(
        "wiki/philosophy/bad.md",
        "---\ntitle: Bad\ndomain: philosophy\norigin: nonsense\ntags: [t]\nsources: []\n---\n\nSee [[nowhere]].\n",
    );

    let v = a.json(&["lint"]);
    assert_envelope(&v, "lint");

    let findings = v["findings"].as_array().expect("findings array");
    let rules: Vec<&str> = findings.iter().filter_map(|f| f["rule"].as_str()).collect();
    assert!(rules.contains(&"invalid-origin"), "{v}");
    assert!(rules.contains(&"broken-link"), "{v}");
    assert!(rules.contains(&"missing-sources"), "{v}");

    // Counts must agree with the array, or a consumer that trusts the summary
    // and a consumer that counts the array will disagree.
    let errors = findings.iter().filter(|f| f["severity"] == "error").count();
    let warnings = findings
        .iter()
        .filter(|f| f["severity"] == "warning")
        .count();
    assert_eq!(v["errors"], errors);
    assert_eq!(v["warnings"], warnings);

    for finding in findings {
        assert!(
            finding["severity"] == "error" || finding["severity"] == "warning",
            "unexpected severity in {finding}"
        );
        assert!(finding["message"].is_string(), "{finding}");
    }
}

#[test]
fn a_broken_link_is_a_warning_and_a_bad_origin_is_an_error() {
    // This split is what makes the exit code usable: forward-declared
    // wikilinks are the documented workflow, so they must not fail a lint.
    let a = Archive::new();
    a.write(
        "wiki/philosophy/links.md",
        &article("Links", "philosophy", &["raw/x.md"]).replace("Body.", "See [[nowhere]]."),
    );

    let v = a.json(&["lint"]);
    let severity_of = |rule: &str| -> String {
        v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["rule"] == rule)
            .unwrap_or_else(|| panic!("no {rule} finding in {v}"))["severity"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(severity_of("broken-link"), "warning");
    assert_eq!(severity_of("unresolved-source"), "error");
}

#[test]
fn uncompiled_lists_full_records() {
    let a = populated();
    let v = a.json(&["uncompiled"]);
    assert_envelope(&v, "uncompiled");

    assert_eq!(v["count"], 1);
    let doc = &v["documents"][0];
    assert_eq!(doc["raw_path"], "raw/philosophy/stranded.md");
    for key in ["title", "domain", "origin", "source_type", "ingested_at"] {
        assert!(doc[key].is_string(), "missing {key}:\n{doc}");
    }
}

#[test]
fn search_results_carry_titles_and_line_numbers() {
    let a = populated();
    let v = a.json(&["search", "Body"]);
    assert_envelope(&v, "search");

    assert_eq!(v["query"], "Body");
    assert_eq!(v["result_count"], 1);
    let result = &v["results"][0];
    assert_eq!(result["path"], "wiki/philosophy/stoicism.md");
    assert_eq!(result["slug"], "stoicism");
    assert_eq!(result["title"], "Stoicism");
    assert!(result["matches"][0]["line"].is_number(), "{result}");
    assert!(result["matches"][0]["text"].is_string(), "{result}");
}

#[test]
fn search_with_no_results_is_an_empty_list_not_an_error() {
    let a = populated();
    let v = a.json(&["search", "zzzznotpresent"]);
    assert_eq!(v["result_count"], 0);
    assert_eq!(v["results"].as_array().unwrap().len(), 0);
}

#[test]
fn graph_exposes_both_directions_and_orphans() {
    let a = populated();
    let v = a.json(&["graph"]);
    assert_envelope(&v, "graph");

    assert!(v["forward"].is_object(), "{v}");
    assert!(v["backlinks"].is_object(), "{v}");
    assert!(v["orphans"].is_array(), "{v}");
    assert!(v["node_count"].is_number(), "{v}");
    assert!(v["edge_count"].is_number(), "{v}");
}

#[test]
fn config_reports_the_resolution_rule_as_a_stable_token() {
    let a = populated();
    let v = a.json(&["config"]);
    assert_envelope(&v, "config");

    assert_eq!(v["resolved"], true);
    assert_eq!(v["resolved_via"], "env");
    assert_eq!(v["initialized"], true);
    assert!(v["inputs"]["env_archive"].is_string(), "{v}");
    assert_eq!(v["directories"].as_array().unwrap().len(), 5);
}

#[test]
fn failures_are_json_too_when_json_was_requested() {
    let tmp = tempfile::tempdir().unwrap();
    let output = common::bare()
        .args(["status", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8_lossy(&output.stderr);
    let v: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("error output was not JSON ({e}):\n{text}"));
    assert_eq!(v["schema_version"], 1);
    assert!(
        v["error"]["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty())
    );
}

#[test]
fn json_output_carries_no_ansi_escapes() {
    let a = populated();
    for args in [
        vec!["status"],
        vec!["lint"],
        vec!["uncompiled"],
        vec!["graph"],
        vec!["config"],
        vec!["search", "Body"],
    ] {
        let mut with_json = args.clone();
        with_json.push("--json");
        let text = stdout(&a.output(&with_json));
        assert!(
            !text.contains('\u{1b}'),
            "`sentinel {}` leaked terminal escapes into JSON",
            args.join(" ")
        );
    }
}

// ---------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------

#[test]
fn a_healthy_archive_lints_clean() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/stoicism.md",
        &article("Stoicism", "philosophy", &["raw/philosophy/meditations.md"]),
    );

    assert_eq!(a.code(&["lint"]), 0);
    assert_eq!(a.code(&["lint", "--strict"]), 0);
}

#[test]
fn warnings_alone_do_not_fail_a_lint() {
    // An archive mid-workflow: a source not yet compiled, a wikilink pointing
    // at an article not yet written. Both are normal; neither is a failure.
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);

    assert_eq!(a.code(&["lint"]), 0, "an uncompiled source is not an error");
    assert_eq!(
        a.code(&["lint", "--strict"]),
        2,
        "--strict is what makes warnings fail"
    );
}

#[test]
fn errors_exit_two_so_they_are_distinguishable_from_a_crash() {
    let a = Archive::new();
    a.write("wiki/philosophy/bad.md", "---\ntitle: [unterminated\n---\n");

    assert_eq!(
        a.code(&["lint"]),
        2,
        "exit 2 means 'ran and found problems'; exit 1 means 'failed'"
    );
}

#[test]
fn a_missing_archive_exits_one() {
    let tmp = tempfile::tempdir().unwrap();
    let output = common::bare()
        .arg("status")
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// `config` is the command run when nothing else works
// ---------------------------------------------------------------------------

/// Every way archive resolution can fail, and what `config --json` owes each.
///
/// The failure cases are the whole reason this command exists — the successful
/// one is answerable by any other command's envelope.
#[test]
fn config_json_diagnoses_a_failure_it_cannot_resolve() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let cases: [(&str, Option<&str>); 3] = [
        ("malformed config", Some("archive = \n")),
        ("config naming a missing key", Some("something_else = 1\n")),
        ("nothing set anywhere", None),
    ];

    for (name, contents) in cases {
        match contents {
            Some(text) => std::fs::write(&config_path, text).unwrap(),
            None => {
                let _ = std::fs::remove_file(&config_path);
            }
        }

        let out = std::process::Command::new(env!("CARGO_BIN_EXE_sentinel"))
            .args(["config", "--json"])
            // An empty directory with no archive above it, so discovery fails.
            .current_dir(dir.path())
            .env_remove("SENTINEL_ARCHIVE")
            .env("SENTINEL_CONFIG", &config_path)
            .output()
            .unwrap();

        let text = stdout(&out);
        let v: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{name}: not JSON ({e}):\n{text}"));

        assert_eq!(v["schema_version"], 1, "{name}: {v}");
        assert_eq!(v["command"], "config", "{name}: {v}");
        assert_eq!(v["resolved"], false, "{name}: {v}");
        assert!(
            v["error"].as_str().is_some_and(|s| !s.is_empty()),
            "{name}: must say why:\n{v}"
        );
        // The point of the fix: the inputs are knowable without a root, and
        // they are what tells a caller which rule to correct.
        assert!(
            v["inputs"].is_object(),
            "{name}: must report which inputs were set, not just a message:\n{v}"
        );
        assert!(
            v["inputs"].get("env_archive").is_some(),
            "{name}: inputs must enumerate every resolution rule:\n{v}"
        );
        assert!(
            !out.status.success(),
            "{name}: reporting a failure is not succeeding at resolution"
        );
    }
}

#[test]
fn only_config_may_omit_the_archive_from_its_envelope() {
    // The envelope contract says every payload names its archive. Making the
    // field optional to accommodate one command would quietly weaken that for
    // all of them, so this pins the exception to the case that earned it.
    let a = populated();
    for args in [
        vec!["status"],
        vec!["lint"],
        vec!["next"],
        vec!["search", "stoicism"],
        vec!["graph"],
        vec!["config"],
    ] {
        let v = a.json(&args);
        assert!(
            v["archive"].as_str().is_some_and(|s| !s.is_empty()),
            "`sentinel {}` dropped the archive from a resolved payload:\n{v}",
            args.join(" ")
        );
    }
}
