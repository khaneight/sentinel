//! Bounds and relevance on the commands an agent queries most.
//!
//! These exist because of a scale test, not a hunch. On a generated archive of
//! 423 articles and 140 sources:
//!
//! | command                | before  | after |
//! |------------------------|---------|-------|
//! | `search <common> --json` | 467 KB | 11 KB |
//! | `graph --json`           |  65 KB |  1.8 KB with `--node` |
//! | `lint --json`            |  50 KB |  0.3 KB with `--summary` |
//!
//! and `search virtue` ranked a note that mentions the word twice above the
//! article actually titled "Virtue".

mod common;

use common::{Archive, article};

/// An archive where one article is *about* a term and many merely mention it.
fn archive_mentioning(term: &str, mentions: usize) -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "notes");
    a.run(&["sync"]);

    a.write(
        &format!("wiki/philosophy/{term}.md"),
        &article(term, "philosophy", &["raw/philosophy/src.md"])
            .replace("Body.", "The article about the concept itself."),
    );
    for i in 0..mentions {
        a.write(
            &format!("wiki/philosophy/note-{i:03}.md"),
            &article(
                &format!("Note {i}"),
                "philosophy",
                &["raw/philosophy/src.md"],
            )
            .replace(
                "Body.",
                &format!("Passing mention of {term}. And again: {term}. And {term}."),
            ),
        );
    }
    a
}

#[test]
fn the_article_about_a_term_outranks_articles_that_mention_it() {
    let a = archive_mentioning("virtue", 12);
    let v = a.json(&["search", "virtue"]);

    assert_eq!(
        v["results"][0]["slug"], "virtue",
        "raw match count ranked passing mentions above the article itself:\n{v}"
    );
    let top = v["results"][0]["score"].as_u64().unwrap();
    let next = v["results"][1]["score"].as_u64().unwrap();
    assert!(
        top > next * 10,
        "a title match must dominate body mentions, not edge them out: {top} vs {next}"
    );
}

#[test]
fn a_tag_match_ranks_above_a_body_mention() {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "notes");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/tagged.md",
        &article("Tagged", "philosophy", &["raw/philosophy/src.md"])
            .replace("tags: [t]", "tags: [epistemology]"),
    );
    a.write(
        "wiki/philosophy/mentioner.md",
        &article("Mentioner", "philosophy", &["raw/philosophy/src.md"])
            .replace("Body.", "epistemology epistemology epistemology"),
    );

    let v = a.json(&["search", "epistemology"]);
    assert_eq!(v["results"][0]["slug"], "tagged", "{v}");
}

#[test]
fn search_is_bounded_by_default_and_says_so() {
    let a = archive_mentioning("virtue", 40);
    let v = a.json(&["search", "virtue"]);

    assert_eq!(v["result_count"], 41, "counts must cover every match");
    assert_eq!(v["returned"], 20, "default limit");
    assert_eq!(v["truncated"], true);
    assert_eq!(v["results"].as_array().unwrap().len(), 20);
}

#[test]
fn search_limit_is_adjustable() {
    let a = archive_mentioning("virtue", 40);
    let v = a.json(&["search", "virtue", "--limit", "5"]);

    assert_eq!(v["returned"], 5);
    assert_eq!(
        v["result_count"], 41,
        "limit narrows output, never the count"
    );
}

#[test]
fn match_lines_are_capped_per_file() {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "notes");
    a.run(&["sync"]);
    let body = (0..30)
        .map(|i| format!("line {i} mentions virtue"))
        .collect::<Vec<_>>()
        .join("\n");
    a.write(
        "wiki/philosophy/many.md",
        &article("Many", "philosophy", &["raw/philosophy/src.md"]).replace("Body.", &body),
    );

    let v = a.json(&["search", "virtue"]);
    let result = &v["results"][0];
    assert_eq!(
        result["match_count"], 30,
        "the true count is still reported"
    );
    assert_eq!(
        result["matches"].as_array().unwrap().len(),
        3,
        "but only a few excerpts are shipped"
    );
}

#[test]
fn long_lines_are_excerpted_rather_than_shipped_whole() {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "notes");
    a.run(&["sync"]);
    let long = format!("virtue {}", "x".repeat(5000));
    a.write(
        "wiki/philosophy/long.md",
        &article("Long", "philosophy", &["raw/philosophy/src.md"]).replace("Body.", &long),
    );

    let v = a.json(&["search", "virtue"]);
    let text = v["results"][0]["matches"][0]["text"].as_str().unwrap();
    assert!(
        text.chars().count() < 300,
        "a 5000-character line came back whole: {} chars",
        text.chars().count()
    );
}

// ---------------------------------------------------------------------------
// graph neighbourhoods
// ---------------------------------------------------------------------------

/// a → b → c, plus an unrelated island.
fn linked_archive() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "notes");
    a.run(&["sync"]);
    let write = |slug: &str, body: &str| {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &article(slug, "philosophy", &["raw/philosophy/src.md"]).replace("Body.", body),
        );
    };
    write("a", "See [[b]].");
    write("b", "See [[c]].");
    write("c", "Leaf.");
    write("island", "Nothing here.");
    a.run(&["index"]);
    a
}

// ---------------------------------------------------------------------------
// search reads prose, not metadata
// ---------------------------------------------------------------------------

#[test]
fn an_article_is_not_a_match_for_a_word_that_appears_only_in_its_citation() {
    // `sources:` holds paths. Scanning the frontmatter as body text made those
    // paths searchable, so an article about the weather that cited
    // `kant-on-duty.md` was the sole result for `search kant` — and quoted the
    // YAML line back as its evidence.
    let a = Archive::new();
    a.write("raw/philosophy/kant-on-duty.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/weather.md",
        "---\ntitle: Weather\ndomain: philosophy\norigin: researched\n\
         tags: [meteorology]\nsources: [raw/philosophy/kant-on-duty.md]\n---\n\n\
         Rain falls. Nothing here concerns moral philosophy.\n",
    );

    let v = a.json(&["search", "kant"]);
    assert_eq!(
        v["result_count"], 0,
        "an article that never mentions Kant matched `search kant`: {}",
        v["results"]
    );
}

#[test]
fn no_frontmatter_key_is_searchable_as_body_text() {
    // Written against `sources:` alone, this check would have passed while
    // `origin:`, `status:` and `domain:` stayed searchable — searching a domain
    // name returned every article in that domain. Enumerate the keys from the
    // published contract instead of from the one that broke.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/only.md",
        "---\ntitle: Only\ndomain: philosophy\norigin: researched\nstatus: draft\n\
         tags: [alpha]\nsources: [raw/philosophy/src.md]\n---\n\nBody prose.\n",
    );

    let fields = a.json(&["schema"]);
    let keys: Vec<String> = fields["frontmatter"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap().to_string())
        .collect();
    assert!(keys.len() >= 5, "{keys:?}");

    for key in &keys {
        // The key name itself never appears in the prose, so any hit is the
        // YAML being read as body text.
        let v = a.json(&["search", key]);
        for result in v["results"].as_array().unwrap() {
            for m in result["matches"].as_array().unwrap() {
                let text = m["text"].as_str().unwrap();
                assert!(
                    !text.starts_with(&format!("{key}:")),
                    "`search {key}` quoted the frontmatter line `{text}`"
                );
            }
        }
    }
}

#[test]
fn an_excerpt_cites_the_line_number_it_actually_occupies() {
    // Numbering the body from 1 would be worse than not reporting a line at
    // all: it points at real text that is not the text quoted.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    let body_line = "The distinctive claim is stated here.";
    a.write(
        "wiki/philosophy/note.md",
        &format!(
            "---\ntitle: Note\ndomain: philosophy\norigin: authored\ntags: [t]\n\
             sources: [raw/philosophy/src.md]\n---\n\n{body_line}\n"
        ),
    );

    let v = a.json(&["search", "distinctive"]);
    let line = v["results"][0]["matches"][0]["line"].as_u64().unwrap() as usize;
    let actual = a.read("wiki/philosophy/note.md");
    assert_eq!(
        actual.lines().nth(line - 1),
        Some(body_line),
        "excerpt cites line {line}, which holds something else"
    );
}

#[test]
fn a_domain_name_that_matches_no_prose_says_where_the_answer_is() {
    // Reading frontmatter as body meant `search philosophy` returned every
    // article in the domain. It now returns none, which is accurate — but bare
    // silence reads as "the archive has no philosophy", so it has to point
    // somewhere. Canonical comparison, so `Philosophy` works too.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/one.md",
        &common::article("One", "philosophy", &["raw/philosophy/src.md"]),
    );

    for query in ["philosophy", "Philosophy"] {
        let v = a.json(&["search", query]);
        assert_eq!(v["result_count"], 0, "{v}");
        assert_eq!(v["domain"], "philosophy", "no hint for `{query}`: {v}");
    }

    let human = a.run(&["search", "philosophy"]);
    assert!(human.contains("_by-domain.md"), "{human}");

    // A query that is simply absent must not be dressed up as a near miss.
    let miss = a.json(&["search", "zzzabsent"]);
    assert!(miss.get("domain").is_none(), "{miss}");
}

#[test]
fn an_empty_query_is_refused_rather_than_matching_everything() {
    // An empty needle is a substring of every line, so this returned the whole
    // archive ranked by file length, with scores in the thousands. It reads as
    // a relevance ranking. It is a measure of how long the documents are.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/one.md",
        &common::article("One", "philosophy", &["raw/philosophy/src.md"]),
    );

    for query in ["", "   "] {
        let out = a.output(&["search", query]);
        assert!(
            !out.status.success(),
            "`search {query:?}` succeeded; stdout:\n{}",
            common::stdout(&out)
        );
        let err = common::stderr(&out);
        assert!(
            err.contains("index/_by-domain.md") || err.contains("sentinel index"),
            "the refusal must say how to list the archive instead:\n{err}"
        );
    }
}

#[test]
fn a_neighbourhood_is_scoped_to_the_requested_depth() {
    let a = linked_archive();

    let d1 = a.json(&["graph", "--node", "b", "--depth", "1"]);
    let slugs: Vec<&str> = d1["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, ["b", "a", "c"], "depth 1 is both directions: {d1}");
    assert!(!slugs.contains(&"island"), "{d1}");
}

#[test]
fn depth_two_reaches_further() {
    let a = linked_archive();
    let d2 = a.json(&["graph", "--node", "a", "--depth", "2"]);
    let slugs: Vec<&str> = d2["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["slug"].as_str().unwrap())
        .collect();
    assert!(slugs.contains(&"c"), "a → b → c is two hops: {d2}");
    assert!(!slugs.contains(&"island"));
}

#[test]
fn a_neighbourhood_is_far_smaller_than_the_whole_graph() {
    // The property that matters: the answer is the size of the answer, not the
    // size of the archive.
    let a = linked_archive();
    for i in 0..60 {
        a.write(
            &format!("wiki/philosophy/filler-{i:03}.md"),
            &article(
                &format!("Filler {i}"),
                "philosophy",
                &["raw/philosophy/src.md"],
            )
            .replace("Body.", "See [[island]]."),
        );
    }
    a.run(&["index"]);

    let whole = a.run(&["graph", "--json"]).len();
    let scoped = a.run(&["graph", "--node", "b", "--json"]).len();
    assert!(
        scoped * 5 < whole,
        "neighbourhood {scoped} bytes vs whole graph {whole} bytes — not scoped enough"
    );
}

#[test]
fn an_unknown_node_is_reported_rather_than_returning_empty_silence() {
    let a = linked_archive();
    let v = a.json(&["graph", "--node", "never-written"]);

    assert_eq!(v["unknown"], true, "{v}");
    assert_eq!(v["node_count"], 1, "just the requested node itself");
}

// ---------------------------------------------------------------------------
// lint summary and filtering
// ---------------------------------------------------------------------------

#[test]
fn lint_summary_reports_counts_without_the_findings() {
    let a = Archive::new();
    for i in 0..15 {
        a.write(
            &format!("wiki/philosophy/broken-{i:02}.md"),
            &article(&format!("B{i}"), "philosophy", &["raw/nope.md"]),
        );
    }

    let full = a.json(&["lint"]);
    let summary = a.json(&["lint", "--summary"]);

    assert!(full["findings"].is_array());
    assert!(
        summary["findings"].is_null(),
        "summary must omit the findings array entirely"
    );
    assert_eq!(summary["errors"], full["errors"], "counts must agree");
    assert_eq!(
        summary["by_rule"]["unresolved-source"]["count"], 15,
        "{summary}"
    );
}

#[test]
fn a_rule_filter_narrows_the_listing_but_not_the_counts() {
    // A filter that changed the counts could make a broken archive look healthy.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "notes");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/bad.md",
        &article("Bad", "philosophy", &["raw/nope.md"]).replace("Body.", "See [[nowhere]]."),
    );

    let all = a.json(&["lint"]);
    let filtered = a.json(&["lint", "--rule", "broken-link"]);

    assert_eq!(filtered["errors"], all["errors"], "counts must not change");
    assert_eq!(filtered["warnings"], all["warnings"]);

    let rules: Vec<&str> = filtered["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule"].as_str().unwrap())
        .collect();
    assert!(!rules.is_empty());
    assert!(rules.iter().all(|r| *r == "broken-link"), "{filtered}");
}

#[test]
fn a_rule_filter_does_not_change_the_exit_code() {
    let a = Archive::new();
    a.write(
        "wiki/philosophy/bad.md",
        &article("Bad", "philosophy", &["raw/nope.md"]),
    );

    assert_eq!(a.code(&["lint"]), 2, "unresolved-source is an error");
    assert_eq!(
        a.code(&["lint", "--rule", "missing-tags"]),
        2,
        "filtering to a clean rule must not mask the error"
    );
}

#[test]
fn by_rule_is_present_even_in_full_output() {
    let a = Archive::new();
    a.write(
        "wiki/philosophy/bad.md",
        &article("Bad", "philosophy", &["raw/nope.md"]),
    );
    let v = a.json(&["lint"]);
    assert!(v["by_rule"].is_object(), "{v}");
}

#[test]
fn an_unknown_lint_rule_is_rejected_rather_than_reported_clean() {
    // `--rule brokenlink` for `broken-link` returned zero findings and exit 0,
    // indistinguishable from the rule having nothing to report.
    // `/sentinel-improve` tells an agent to work the rules one at a time, so a
    // typo silently reported a clean archive.
    let a = Archive::new();
    a.write(
        "wiki/philosophy/bad.md",
        &article("Bad", "philosophy", &["raw/nope.md"]),
    );

    let output = a.output(&["lint", "--rule", "brokenlink"]);
    assert!(!output.status.success(), "a typo must not read as clean");
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("Unknown lint rule"), "{err}");
    assert!(
        err.contains("broken-link"),
        "must name the valid rules:\n{err}"
    );
    assert!(err.contains("sentinel schema"), "{err}");
}

#[test]
fn every_published_rule_is_accepted_by_the_filter() {
    // Derived from what `sentinel schema` publishes, so a rule added to the
    // registry cannot be rejected by the flag that filters on it.
    let a = Archive::new();
    let schema = a.json(&["schema"]);
    let rules: Vec<String> = schema["lint_rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["rule"].as_str().unwrap().to_string())
        .collect();
    assert!(rules.len() >= 10, "{rules:?}");

    for rule in &rules {
        let code = a.code(&["lint", "--rule", rule]);
        assert!(
            code == 0 || code == 2,
            "`lint --rule {rule}` was rejected, but `sentinel schema` publishes it"
        );
    }
}

#[test]
fn a_valid_rule_with_no_matches_is_still_success() {
    // The distinction the fix rests on: "this rule is clean" and "there is no
    // such rule" must not look the same, and the first must keep working.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "x");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/good.md",
        &article("Good", "philosophy", &["raw/philosophy/src.md"]),
    );

    let v = a.json(&["lint", "--rule", "duplicate-slug"]);
    assert_eq!(v["findings"].as_array().unwrap().len(), 0);
    assert_eq!(a.code(&["lint", "--rule", "duplicate-slug"]), 0);
}
