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
    common::mine_corpus(&a);
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
    common::mine_corpus(&a);
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
    common::mine_corpus(&a);
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

// ---------------------------------------------------------------------------
// --action: scheduling across categories
// ---------------------------------------------------------------------------

/// Several uncompiled sources plus a real gap — the shape a real ingest takes.
fn ingested_corpus() -> Archive {
    let a = Archive::new();
    for i in 0..8 {
        a.write(&format!("raw/philosophy/source-{i}.md"), "text");
    }
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/compiled.md",
        &article_with(
            "Compiled",
            &["raw/philosophy/source-0.md"],
            "See [[a-gap]].",
        ),
    );
    a
}

#[test]
fn a_large_ingest_makes_compile_dominate_the_recommendation() {
    // Not a defect — it is why `--action` has to exist. Pinning it so the
    // reason for `--action` stays visible if the ladder is ever retuned.
    let a = ingested_corpus();
    let v = a.json(&["next"]);
    assert_eq!(v["action"], "compile", "{v}");
    let counts: Vec<(&str, u64)> = v["backlog"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["action"].as_str().unwrap(), b["count"].as_u64().unwrap()))
        .collect();
    assert!(counts.contains(&("compile", 7)), "{v}");
    assert!(counts.contains(&("write", 1)), "{v}");
}

#[test]
fn action_reaches_a_category_priority_would_starve() {
    let a = ingested_corpus();
    let v = a.json(&["next", "--action", "write"]);

    assert_eq!(v["action"], "write");
    assert_eq!(v["targets"][0]["id"], "a-gap");
    assert_eq!(
        v["requested"], true,
        "a scheduling choice must be distinguishable from sentinel's advice"
    );
}

#[test]
fn action_still_reports_the_whole_backlog() {
    // So one call is enough to schedule the next step too.
    let a = ingested_corpus();
    let v = a.json(&["next", "--action", "write"]);
    let actions: Vec<&str> = v["backlog"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["action"].as_str().unwrap())
        .collect();
    assert!(actions.contains(&"compile"), "{v}");
}

#[test]
fn asking_for_an_empty_category_is_not_an_error() {
    let a = ingested_corpus();
    let v = a.json(&["next", "--action", "fix-errors"]);

    assert_eq!(v["action"], "none");
    assert_eq!(v["requested"], true);
    assert_eq!(v["targets"].as_array().unwrap().len(), 0);
    assert_eq!(a.code(&["next", "--action", "fix-errors"]), 0);
}

#[test]
fn an_unknown_action_is_rejected_with_the_valid_ones_named() {
    let a = ingested_corpus();
    let output = a.output(&["next", "--action", "nonsense"]);

    assert_eq!(output.status.code(), Some(1));
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("write"), "must list the valid actions:\n{err}");
}

#[test]
fn the_recommendation_is_not_marked_as_requested() {
    let a = ingested_corpus();
    let v = a.json(&["next"]);
    assert!(
        v["requested"].is_null(),
        "the field is omitted unless the caller asked: {v}"
    );
}

// ---------------------------------------------------------------------------
// write targets must be actionable without a second call
// ---------------------------------------------------------------------------

/// One gap wanted by several articles, spelled two ways.
fn archive_with_a_popular_gap() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    for (i, spelling) in ["prohairesis", "Prohairesis", "prohairesis"]
        .iter()
        .enumerate()
    {
        a.write(
            &format!("wiki/philosophy/ref-{i}.md"),
            &article_with(
                &format!("Ref {i}"),
                &["raw/philosophy/src.md"],
                &format!("See [[{spelling}]]."),
            ),
        );
    }
    a
}

#[test]
fn a_write_target_names_the_articles_that_want_it() {
    // `/sentinel-grow` tells the agent to read a gap's referrers before writing
    // it — they define what the concept means in this archive rather than in
    // general. A bare count cannot be read.
    let a = archive_with_a_popular_gap();
    let v = a.json(&["next", "--action", "write"]);
    let target = &v["targets"][0];

    assert_eq!(target["id"], "prohairesis");
    assert_eq!(target["ref_count"], 3);
    let refs: Vec<&str> = target["refs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert_eq!(refs.len(), 3, "{target}");
    assert!(refs.contains(&"wiki/philosophy/ref-0.md"), "{target}");
}

#[test]
fn the_spellings_used_are_surfaced() {
    // Tells the writer what the file will be called, and flags naming drift.
    let a = archive_with_a_popular_gap();
    let v = a.json(&["next", "--action", "write"]);
    let variants: Vec<&str> = v["targets"][0]["variants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert_eq!(variants, ["Prohairesis"], "{v}");
}

#[test]
fn a_truncated_referrer_list_says_it_is_truncated() {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    for i in 0..9 {
        a.write(
            &format!("wiki/philosophy/ref-{i}.md"),
            &article_with(
                &format!("Ref {i}"),
                &["raw/philosophy/src.md"],
                "See [[popular]].",
            ),
        );
    }

    let v = a.json(&["next", "--action", "write"]);
    let target = &v["targets"][0];
    assert_eq!(target["ref_count"], 9, "the true total must be exact");
    assert_eq!(
        target["refs"].as_array().unwrap().len(),
        5,
        "the sample is capped"
    );

    let out = a.run(&["next", "--action", "write"]);
    assert!(
        out.contains("and 4 more"),
        "silent truncation reads as complete:\n{out}"
    );
}

#[test]
fn targets_that_have_no_referrers_omit_the_fields_entirely() {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "notes");
    a.run(&["sync"]);

    let v = a.json(&["next"]);
    assert_eq!(v["action"], "compile");
    let target = &v["targets"][0];
    assert!(target["refs"].is_null(), "{target}");
    assert!(target["ref_count"].is_null(), "{target}");
    assert!(target["variants"].is_null(), "{target}");
}

// ---------------------------------------------------------------------------
// progress: the archive advancing, not the queue shrinking
// ---------------------------------------------------------------------------

#[test]
fn next_reports_what_the_archive_contains() {
    let a = Archive::new();
    a.write("raw/philosophy/one.md", "text");
    a.write("raw/philosophy/two.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/art.md",
        &article_with("Art", &["raw/philosophy/one.md"], "See [[gap]]."),
    );

    let p = &a.json(&["next"])["progress"];
    assert_eq!(p["wiki_articles"], 1);
    assert_eq!(p["raw_documents"], 2);
    assert_eq!(p["uncompiled"], 1);
    assert_eq!(p["errors"], 0);
}

#[test]
fn a_generative_article_advances_progress_even_as_the_backlog_grows() {
    // The defect this pins: `/sentinel-grow` used to stop when the total
    // backlog failed to shrink. An article that fills one gap and opens three
    // is the loop working — halting there would end the run right after its
    // most productive step.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/seed.md",
        &article_with("Seed", &["raw/philosophy/src.md"], "See [[wanted]]."),
    );

    let before = a.json(&["next"]);
    let backlog_before: u64 = before["backlog"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_u64().unwrap())
        .sum();

    // Fill the gap with an article that legitimately raises three more.
    a.write(
        "wiki/philosophy/wanted.md",
        &article_with(
            "Wanted",
            &["raw/philosophy/src.md"],
            "Opens [[alpha]], [[beta]] and [[gamma]].",
        ),
    );

    let after = a.json(&["next"]);
    let backlog_after: u64 = after["backlog"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_u64().unwrap())
        .sum();

    assert!(
        backlog_after > backlog_before,
        "precondition: the backlog grew ({backlog_before} → {backlog_after})"
    );
    assert!(
        after["progress"]["wiki_articles"].as_u64().unwrap()
            > before["progress"]["wiki_articles"].as_u64().unwrap(),
        "progress must show the archive advanced even though the queue grew"
    );
}

#[test]
fn progress_is_present_on_an_action_query_too() {
    // The loop decides and measures from one call.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);

    let v = a.json(&["next", "--action", "compile"]);
    assert!(v["progress"]["uncompiled"].is_number(), "{v}");
}

#[test]
fn a_truncated_target_list_declares_its_true_length() {
    // `targets` is capped at MAX_TARGETS. #12 added `ref_count` on the
    // principle that a truncated list which does not say so reads as complete;
    // the same list-of-targets in the same file did not follow it.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    let gaps: String = (0..8).map(|i| format!("[[gap-{i}]] ")).collect();
    a.write(
        "wiki/philosophy/seed.md",
        &article_with("Seed", &["raw/philosophy/src.md"], &gaps),
    );

    let v = a.json(&["next", "--action", "write"]);
    assert_eq!(v["target_count"], 8, "the true total must be exact:\n{v}");
    assert_eq!(
        v["targets"].as_array().unwrap().len(),
        5,
        "the sample is capped"
    );

    let out = a.run(&["next", "--action", "write"]);
    assert!(
        out.contains("and 3 more"),
        "silent truncation reads as complete:\n{out}"
    );
}

#[test]
fn an_untruncated_target_list_says_nothing_extra() {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/seed.md",
        &article_with("Seed", &["raw/philosophy/src.md"], "[[only-gap]]"),
    );

    let v = a.json(&["next", "--action", "write"]);
    assert_eq!(v["target_count"], 1);
    assert!(!a.run(&["next", "--action", "write"]).contains("more"));
}

// ---------------------------------------------------------------------------
// The terminal state
//
// Driving a real corpus to completion — 8 sources, 26 articles, one connected
// graph — showed the loop terminating cleanly and also showed the tool saying
// "Nothing outstanding" over an archive in which every single article was
// still a draft. True of the mechanical work, misleading about the whole.
// ---------------------------------------------------------------------------

fn completed_archive(status: &str) -> Archive {
    // Updated today, so the drafts are current rather than stale — otherwise
    // `next` correctly recommends `review` and this is not the terminal state.
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    for (slug, other) in [("alpha", "beta"), ("beta", "alpha")] {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &article_with(
                slug,
                &["raw/philosophy/src.md"],
                &format!("See [[{other}]]."),
            )
            .replace("status: draft", &format!("status: {status}"))
            .replace("updated: 2026-01-01", &format!("updated: {today}")),
        );
    }
    // A terminal archive has to satisfy every rung, `learn` included — the
    // source is registered as the author's own writing, and nothing had read
    // it. Without this the fixture is not "completed", it is one rung short.
    common::mine_corpus(&a);
    a.run(&["index"]);
    a
}

#[test]
fn a_finished_archive_reports_nothing_outstanding() {
    let a = completed_archive("stable");
    let v = a.json(&["next"]);

    assert_eq!(v["action"], "none");
    assert_eq!(v["backlog"].as_array().unwrap().len(), 0);
    assert!(v["suggested_command"].is_null());
    assert_eq!(a.code(&["next"]), 0);
}

#[test]
fn an_all_draft_archive_says_so_rather_than_implying_completion() {
    let a = completed_archive("draft");
    let v = a.json(&["next"]);

    assert_eq!(v["action"], "none", "there is no mechanical work left");
    let reason = v["reason"].as_str().unwrap();
    assert!(
        reason.contains("still `draft`"),
        "an archive nobody has reviewed must not read as finished:\n{reason}"
    );
}

#[test]
fn a_reviewed_archive_gets_no_such_note() {
    let a = completed_archive("stable");
    // Not merely "draft": the base message legitimately contains "no draft has
    // stalled", so the assertion has to name the note itself.
    assert!(
        !a.json(&["next"])["reason"]
            .as_str()
            .unwrap()
            .contains("still `draft`")
    );
}

#[test]
fn status_reports_the_maturity_breakdown() {
    let a = completed_archive("draft");
    let v = a.json(&["status"]);
    assert_eq!(v["maturity"]["draft"], 2, "{v}");
    assert!(
        a.run(&["status"]).contains("2 draft"),
        "{}",
        a.run(&["status"])
    );
}

// ---------------------------------------------------------------------------
// Every recommendable action must be able to register as progress
//
// `/sentinel-grow` stops when nothing in `progress` moved. The counters chosen
// covered fix-errors, compile, and write — the three actions in front of me
// when I wrote them. `connect` adds a link to an existing article and `review`
// promotes a draft; neither changes an article count, an uncompiled count, or
// an error count, so a correct iteration of either registered as no progress
// and halted the loop claiming non-convergence.
// ---------------------------------------------------------------------------

fn counters(a: &Archive) -> serde_json::Value {
    a.json(&["next"])["progress"].clone()
}

/// Any counter that moved in the direction that means "work happened".
fn advanced(before: &serde_json::Value, after: &serde_json::Value) -> bool {
    let up = |k: &str| after[k].as_u64() > before[k].as_u64();
    let down = |k: &str| after[k].as_u64() < before[k].as_u64();
    up("wiki_articles") || down("uncompiled") || down("errors") || down("orphans") || down("drafts")
}

#[test]
fn connecting_an_orphan_registers_as_progress() {
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "x");
    a.run(&["sync"]);
    common::mine_corpus(&a);
    a.write(
        "wiki/philosophy/alpha.md",
        &article_with("Alpha", &["raw/philosophy/s.md"], "See [[beta]]."),
    );
    a.write(
        "wiki/philosophy/beta.md",
        &article_with("Beta", &["raw/philosophy/s.md"], "Leaf."),
    );
    a.write(
        "wiki/philosophy/gamma.md",
        &article_with("Gamma", &["raw/philosophy/s.md"], "Nothing links here."),
    );
    a.run(&["index"]);

    let before = counters(&a);
    assert_eq!(a.json(&["next"])["action"], "connect", "precondition");

    // The recommended work: link the orphan from somewhere real.
    a.write(
        "wiki/philosophy/beta.md",
        &article_with(
            "Beta",
            &["raw/philosophy/s.md"],
            "Leaf. Related: [[gamma]].",
        ),
    );
    a.run(&["index"]);

    let after = counters(&a);
    assert!(
        advanced(&before, &after),
        "a correct `connect` iteration must not read as no progress:\n{before}\n{after}"
    );
}

#[test]
fn promoting_a_draft_registers_as_progress() {
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "x");
    a.run(&["sync"]);
    for (slug, other) in [("alpha", "beta"), ("beta", "alpha")] {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &article_with(slug, &["raw/philosophy/s.md"], &format!("See [[{other}]].")),
        );
    }
    a.run(&["index"]);

    let before = counters(&a);
    assert!(before["drafts"].as_u64().unwrap() >= 2, "{before}");

    a.write(
        "wiki/philosophy/alpha.md",
        &article_with("alpha", &["raw/philosophy/s.md"], "See [[beta]].")
            .replace("status: draft", "status: stable"),
    );
    a.run(&["index"]);

    assert!(advanced(&before, &counters(&a)), "review must register");
}

#[test]
fn progress_reports_a_counter_for_every_action_in_the_ladder() {
    // Derived from the published ladder rather than a list maintained here, so
    // a new action fails until someone says which counter it moves.
    let a = Archive::new();
    let actions: Vec<String> = a.json(&["schema"])["next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["action"].as_str().unwrap().to_string())
        .collect();

    let progress = counters(&a);
    let covered_by = |action: &str| -> &'static str {
        match action {
            "fix-errors" => "errors",
            "compile" => "uncompiled",
            "learn" => "unmined",
            "write" => "wiki_articles",
            "connect" => "orphans",
            "review" => "drafts",
            other => panic!(
                "action `{other}` is in the ladder but no progress counter is \
                 declared for it — `/sentinel-grow` would halt after doing it"
            ),
        }
    };
    for action in &actions {
        let counter = covered_by(action);
        assert!(
            progress[counter].is_number(),
            "`{action}` maps to progress counter `{counter}`, which is absent:\n{progress}"
        );
    }
}
