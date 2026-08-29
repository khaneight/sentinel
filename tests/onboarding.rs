//! The first hour with sentinel, tested as a sequence rather than as commands.
//!
//! Everything here was reachable by a new user and several things were wrong in
//! ways no per-command test could see. `sentinel next` on a fresh archive said
//! "✓ Nothing outstanding" — correct about the data, wrong about the situation,
//! and the first sentence anybody reads. `connect` asked a one-article archive
//! for an incoming link, which no amount of work provides.
//!
//! Each test is a path a person actually takes. A failure prints the whole
//! session, because the defect is usually in the sequence, not the step.

mod common;

use common::journey::Journey;

#[test]
fn a_user_with_no_archive_is_told_how_to_get_one() {
    // The very first command, run from a directory with no archive above it.
    let (mut j, _) = Journey::uninitialized();

    for args in [vec!["status"], vec!["next"], vec!["lint"]] {
        let step = j.run_unrooted(&args);
        let (code, text, label) = (step.code, step.output(), step.label());
        assert_ne!(code, 0, "should refuse without an archive");
        assert!(
            text.contains("sentinel init"),
            "`{label}` must name the command that fixes this:\n{text}"
        );
        assert!(
            text.contains("--archive") && text.contains("SENTINEL_ARCHIVE"),
            "it must list every way to point at an archive:\n{text}"
        );
    }
    j.assert_no_dead_ends();
}

#[test]
fn init_leaves_an_archive_that_every_command_can_read() {
    // A layout that `init` creates but no command understands would be a broken
    // first five minutes with nothing to indicate why.
    let mut j = Journey::new();
    for rel in common::journey::expected_layout() {
        assert!(
            common::journey::exists(&j.archive.root, rel),
            "init did not create {rel}"
        );
    }

    for args in [
        vec!["status"],
        vec!["next"],
        vec!["lint"],
        vec!["schema"],
        vec!["config"],
        vec!["uncompiled"],
        vec!["graph"],
        vec!["log"],
    ] {
        let step = j.run(&args);
        let (code, text, label) = (step.code, step.output(), step.label());
        assert_eq!(
            code, 0,
            "`{label}` fails on a freshly initialised archive:\n{text}"
        );
    }
    j.assert_no_dead_ends();
}

#[test]
fn an_empty_archive_is_told_it_has_not_started_rather_than_finished() {
    // This is the one that matters most, because it is the first thing a new
    // user reads after `init`. "Nothing outstanding" means "you are done".
    let mut j = Journey::new();
    let text = j.run(&["next"]).output();

    assert!(
        !text.contains("Nothing outstanding"),
        "a brand-new archive was told its work was complete:\n{text}"
    );
    assert!(
        text.contains("sentinel ingest"),
        "it must name the command that begins the loop:\n{text}"
    );
    assert!(
        !text.contains('✓'),
        "a tick reads as success on an archive nobody has used yet:\n{text}"
    );
}

#[test]
fn the_first_source_produces_a_recommendation_that_can_be_followed() {
    let mut j = Journey::new();
    let notes = j.write_scratch("notes.md", "Notes on Stoic ethics.\n");

    let step = j.run(&[
        "ingest",
        &notes.display().to_string(),
        "-d",
        "philosophy",
        "-o",
        "researched",
        "-t",
        "My Notes",
    ]);
    let (code, text) = (step.code, step.output());
    assert_eq!(code, 0, "{text}");

    let v = j.archive.json(&["next"]);
    assert_eq!(v["action"], "compile", "{v}");
    assert!(
        v["suggested_command"]
            .as_str()
            .is_some_and(|c| !c.is_empty()),
        "a recommendation with no command to run is a dead end:\n{v}"
    );
    j.assert_recommendation_is_achievable();
    j.assert_no_dead_ends();
}

#[test]
fn the_template_produces_an_article_that_lints_clean() {
    // A new user copies `templates/wiki-article.md` and fills it in. If the
    // result does not pass `lint`, the first thing they ever write is broken
    // and the tool taught them to write it.
    let mut j = Journey::new();
    let notes = j.write_scratch("notes.md", "Source text.\n");
    j.run(&[
        "ingest",
        &notes.display().to_string(),
        "-d",
        "philosophy",
        "-t",
        "Notes",
    ]);

    let template = j.archive.read("templates/wiki-article.md");
    let filled = template
        .replace("title:", "title: Stoicism")
        .replace("domain:", "domain: philosophy")
        .replace("tags: []", "tags: [ethics]")
        .replace("sources: []", "sources: [raw/philosophy/notes.md]");
    j.archive
        .write("wiki/philosophy/stoicism.md", &format!("{filled}\nBody.\n"));

    let step = j.run(&["lint"]);
    let (code, text) = (step.code, step.output());
    assert_eq!(
        code, 0,
        "an article built from the shipped template does not lint clean:\n{text}"
    );
}

#[test]
fn a_one_article_archive_is_never_asked_to_do_the_impossible() {
    // `connect` wants an incoming link. With one article there is nothing to
    // link from, so the same target came back on every run — which is also
    // `sentinel-grow`'s "same target twice" stop condition, triggered by the
    // tool's own advice.
    let mut j = Journey::new();
    let notes = j.write_scratch("notes.md", "Source.\n");
    j.run(&[
        "ingest",
        &notes.display().to_string(),
        "-d",
        "philosophy",
        "-t",
        "Notes",
    ]);
    j.write_article(
        "only",
        &[
            ("title", "Only"),
            ("domain", "philosophy"),
            ("origin", "authored"),
            ("tags", "[t]"),
            ("sources", "[raw/philosophy/notes.md]"),
        ],
        "The single article.",
    );
    j.run(&["index"]);

    let mut seen = Vec::new();
    for _ in 0..3 {
        let v = j.archive.json(&["next"]);
        seen.push(v["action"].as_str().unwrap_or("?").to_string());
    }
    assert!(
        !seen.contains(&"connect".to_string()),
        "recommended `connect` on a one-article archive: {seen:?}{}",
        j.transcript()
    );
    j.assert_recommendation_is_achievable();
}

#[test]
fn two_articles_restore_connect_as_real_work() {
    // The guard above must not silence a genuine orphan.
    let mut j = Journey::new();
    let notes = j.write_scratch("notes.md", "Source.\n");
    j.run(&[
        "ingest",
        &notes.display().to_string(),
        "-d",
        "philosophy",
        "-t",
        "Notes",
    ]);
    for slug in ["first", "second"] {
        j.write_article(
            slug,
            &[
                ("title", slug),
                ("domain", "philosophy"),
                ("origin", "authored"),
                ("tags", "[t]"),
                ("sources", "[raw/philosophy/notes.md]"),
            ],
            "Body with no links.",
        );
    }
    // `learn` outranks `connect`, and the ingested source is registered as the
    // author's own writing. This test is about orphans, so it satisfies the
    // rung above rather than asserting past it.
    common::mine_corpus(&j.archive);
    j.run(&["index"]);

    let v = j.archive.json(&["next"]);
    assert_eq!(
        v["action"], "connect",
        "two unlinked articles are a real connect task:\n{v}"
    );
}

#[test]
fn the_whole_loop_runs_from_empty_to_published() {
    // The journey the tool exists for, in one test. Each assertion is about the
    // *transition*: that acting on what `next` said changes what it says.
    let mut j = Journey::new();
    let notes = j.write_scratch("notes.md", "Stoic sources.\n");

    assert_eq!(j.archive.json(&["next"])["action"], "none", "starts empty");

    j.run(&[
        "ingest",
        &notes.display().to_string(),
        "-d",
        "philosophy",
        "-o",
        "researched",
        "-t",
        "Sources",
    ]);
    assert_eq!(
        j.archive.json(&["next"])["action"],
        "compile",
        "ingesting a source should ask for it to be compiled"
    );

    j.write_article(
        "ethics",
        &[
            ("title", "Ethics"),
            ("domain", "philosophy"),
            ("origin", "researched"),
            ("status", "stable"),
            ("tags", "[t]"),
            ("sources", "[raw/philosophy/sources.md]"),
        ],
        "Rests on [[control]].",
    );
    j.run(&["index"]);
    assert_eq!(
        j.archive.json(&["next"])["action"],
        "write",
        "an article that names a concept it has not written should ask for it"
    );

    j.write_article(
        "control",
        &[
            ("title", "Control"),
            ("domain", "philosophy"),
            ("origin", "researched"),
            ("status", "stable"),
            ("tags", "[t]"),
            ("sources", "[raw/philosophy/sources.md]"),
        ],
        "Some things are up to us. See [[ethics]].",
    );
    j.run(&["index"]);
    let lint_code = j.run(&["lint"]).code;
    assert_eq!(lint_code, 0, "{}", j.transcript());
    assert_eq!(
        j.archive.json(&["next"])["action"],
        "none",
        "a complete archive has nothing outstanding{}",
        j.transcript()
    );

    let site = j.scratch("site");
    let step = j.run(&["export", "--out", &site.display().to_string()]);
    let (code, text) = (step.code, step.output());
    assert_eq!(code, 0, "{text}");
    assert_eq!(
        std::fs::read_dir(site.join("philosophy")).unwrap().count(),
        2,
        "both stable articles should publish{}",
        j.transcript()
    );

    j.assert_no_dead_ends();
}

#[test]
fn every_command_a_new_user_meets_survives_an_empty_archive() {
    // Derived from `--help`, so a subcommand added later is covered here on the
    // day it ships rather than the day somebody remembers.
    let mut j = Journey::new();
    let help = j.archive.run(&["--help"]);
    let subcommands: Vec<String> = help
        .split("Commands:")
        .nth(1)
        .and_then(|s| s.split("Options:").next())
        .expect("--help lists commands")
        .lines()
        .filter(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();

    // Commands that need arguments, or that are documented to refuse.
    let skip = [
        "help",
        "init",
        "ingest",
        "ingest-repo",
        "mv",
        "rm",
        "search",
    ];

    for command in &subcommands {
        if skip.contains(&command.as_str()) {
            continue;
        }
        let step = j.run(&[command, "--json"]);
        let (code, text, stdout) = (step.code, step.output(), step.stdout.clone());
        assert_eq!(
            code, 0,
            "`sentinel {command} --json` fails on an empty archive:\n{text}"
        );
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
        assert!(
            parsed.is_ok(),
            "`sentinel {command} --json` emitted no JSON on an empty archive:\n{stdout}"
        );
    }
}

// ---------------------------------------------------------------------------
// The compile journey
// ---------------------------------------------------------------------------

/// An archive with three ingested sources and nothing compiled.
fn three_sources() -> Journey {
    let mut j = Journey::new();
    for name in ["epictetus", "marcus", "seneca"] {
        let f = j.write_scratch(&format!("{name}.txt"), "Discusses virtue and control.\n");
        j.run(&[
            "ingest",
            &f.display().to_string(),
            "-d",
            "philosophy",
            "-o",
            "researched",
            "-t",
            name,
        ]);
    }
    j
}

#[test]
fn compiling_each_source_advances_the_queue_rather_than_repeating() {
    // The compile rung must hand out a different source each time. Repeating
    // one would be the same stuck-recommendation failure as `connect` on a
    // one-article archive, and `sentinel-grow` halts on a repeated target.
    let mut j = three_sources();
    let mut compiled = Vec::new();

    for _ in 0..3 {
        let v = j.archive.json(&["next"]);
        assert_eq!(v["action"], "compile", "{v}");
        let target = v["targets"][0]["id"].as_str().unwrap().to_string();
        assert!(
            !compiled.contains(&target),
            "`compile` offered {target} twice; the queue is not advancing: {compiled:?}{}",
            j.transcript()
        );
        let slug = target.rsplit('/').next().unwrap().replace(".txt", "");
        j.write_article(
            &slug,
            &[
                ("title", &slug),
                ("domain", "philosophy"),
                ("origin", "researched"),
                ("tags", "[t]"),
                ("sources", &format!("[{target}]")),
            ],
            "On virtue.",
        );
        j.run(&["index"]);
        compiled.push(target);
    }

    assert_ne!(
        j.archive.json(&["next"])["action"],
        "compile",
        "every source is compiled and the queue should have moved on{}",
        j.transcript()
    );
}

#[test]
fn a_mistyped_citation_says_what_was_probably_meant() {
    // This is where the compile loop stalls, and "matches no raw document" left
    // the reader to guess whether they mistyped, forgot to ingest, or used the
    // wrong path form.
    let j = three_sources();

    for (cite, expected) in [
        ("raw/philosophy/senca.txt", true),  // typo
        ("raw/philosophy/Seneca.txt", true), // wrong case
        ("Seneca", true),                    // from memory, no extension
        ("raw/philosophy/nothing-like-it.md", false),
    ] {
        j.write_article(
            "probe",
            &[
                ("title", "Probe"),
                ("domain", "philosophy"),
                ("origin", "authored"),
                ("tags", "[t]"),
                ("sources", &format!("[{cite}]")),
            ],
            "Body.",
        );
        let report = j.archive.json(&["lint"]);
        let message = report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["rule"] == "unresolved-source")
            .map(|f| f["message"].as_str().unwrap().to_string())
            .unwrap_or_else(|| panic!("`{cite}` should not resolve"));

        if expected {
            assert!(
                message.contains("Did you mean 'raw/philosophy/seneca.txt'?"),
                "`{cite}` should suggest the near match:\n{message}"
            );
        } else {
            assert!(
                !message.contains("Did you mean"),
                "`{cite}` resembles nothing; a wrong guess is worse than none:\n{message}"
            );
            assert!(
                message.contains("sentinel uncompiled"),
                "with no suggestion it must say how to list what exists:\n{message}"
            );
        }
        std::fs::remove_file(j.archive.path("wiki/philosophy/probe.md")).unwrap();
    }
}

#[test]
fn the_three_path_forms_a_person_writes_all_resolve() {
    // `sources:` is written by hand. These are the forms that appear in
    // practice and all three are unambiguous.
    let j = three_sources();
    for cite in [
        "raw/philosophy/seneca.txt",
        "philosophy/seneca.txt",
        "seneca.txt",
        "./raw/philosophy/seneca.txt",
    ] {
        j.write_article(
            "probe",
            &[
                ("title", "Probe"),
                ("domain", "philosophy"),
                ("origin", "authored"),
                ("tags", "[t]"),
                ("sources", &format!("[{cite}]")),
            ],
            "Body.",
        );
        let unresolved = j.archive.json(&["lint"])["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["rule"] == "unresolved-source");
        assert!(!unresolved, "`{cite}` should resolve");
        std::fs::remove_file(j.archive.path("wiki/philosophy/probe.md")).unwrap();
    }
}

#[test]
fn a_corpus_is_read_before_it_is_compiled_and_stops_for_a_verdict() {
    // The sequence the tool exists for, asserted as a sequence: hand over a
    // corpus, see what the archive made of you, say whether it read you right,
    // and only then does it write anything. Each step is only correct because
    // of the one before it, which is what `tests/onboarding.rs` is for.
    let mut j = Journey::new();
    let notes = j.write_scratch("essays.md", "Start from a case you can hold.\n");
    j.run(&[
        "ingest",
        &notes.display().to_string(),
        "-d",
        "philosophy",
        "-o",
        "authored",
        "-t",
        "Essays",
    ]);

    assert_eq!(
        j.archive.json(&["next"])["action"],
        "learn",
        "a fresh corpus is read for its author before it is summarised{}",
        j.transcript()
    );

    j.archive.write(
        "persona/from-cases.md",
        "---\nid: from-cases\nkind: pattern\nclaim: Starts from a case.\n\
         confidence: high\nstatus: proposed\nevidence:\n  - raw/philosophy/essays.md\n---\n\n\
         \"Start from a case you can hold.\"\n",
    );
    j.run(&["index"]);

    let v = j.archive.json(&["next"]);
    assert_eq!(
        v["action"],
        "none",
        "with the reading unconfirmed the archive waits{}",
        j.transcript()
    );
    assert_eq!(v["suggested_command"], "sentinel review");
    let human = j.archive.run(&["next"]);
    assert!(
        !human.contains('✓'),
        "waiting on a verdict is not completion:\n{human}"
    );

    let mut cmd = j.archive.cmd(&["review", "from-cases", "--approve"]);
    cmd.env("SENTINEL_REVIEWER", "author");
    assert!(cmd.output().unwrap().status.success());

    assert_eq!(
        j.archive.json(&["next"])["action"],
        "compile",
        "and only once it is answered does the clone start writing{}",
        j.transcript()
    );
}
