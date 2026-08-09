//! `meta/link-graph.json` is a cache that only `index` refreshes, so it can
//! disagree with what is on disk. Every command that reads it has to say so.
//!
//! Before this, none of them did in JSON. The human output of `graph` warned;
//! `status` reported `orphan_pages: 0`; `next` dropped `connect` out of the
//! backlog entirely — the exact outcome the comment above its corrupt-graph
//! branch says that branch exists to prevent.

mod common;

use common::Archive;

/// An archive with one article and a built graph.
fn indexed() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/first.md",
        "---\ntitle: First\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources: [raw/philosophy/src.md]\n---\n\nSee [[second]].\n",
    );
    a.run(&["index"]);
    a
}

/// Add an article without reindexing.
fn add_unindexed(a: &Archive, slug: &str) {
    a.write(
        &format!("wiki/philosophy/{slug}.md"),
        &format!(
            "---\ntitle: {slug}\ndomain: philosophy\norigin: authored\ntags: [t]\n\
             sources: [raw/philosophy/src.md]\n---\n\nRefers to [[first]].\n"
        ),
    );
}

/// Commands that read the saved graph, derived from the source rather than
/// from the three that happened to be found.
///
/// The same defect was in all three, phrased differently in each. A list
/// written by hand here would have been a list of the ones already fixed.
fn graph_reading_commands() -> Vec<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    let mut found: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .filter(|e| {
            std::fs::read_to_string(e.path())
                .map(|t| t.contains("LinkGraph::load()"))
                .unwrap_or(false)
        })
        .map(|e| {
            e.path()
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .replace('_', "-")
        })
        .collect();
    found.sort();
    assert!(
        found.len() >= 3,
        "expected graph readers to be discoverable: {found:?}"
    );
    found
}

/// The staleness note, wherever a given command puts it.
fn stale_note(v: &serde_json::Value) -> Option<String> {
    v.get("stale")
        .or_else(|| v.get("link_graph_stale"))
        .or_else(|| v.get("progress").and_then(|p| p.get("link_graph_stale")))
        .and_then(|x| x.as_str())
        .map(str::to_string)
}

#[test]
fn every_command_that_reads_the_graph_discloses_that_it_is_out_of_date() {
    let a = indexed();
    add_unindexed(&a, "second");

    for command in graph_reading_commands() {
        let v = a.json(&[&command]);
        let note = stale_note(&v).unwrap_or_else(|| {
            panic!("`sentinel {command} --json` does not disclose a stale graph:\n{v:#}")
        });
        assert!(
            note.contains("sentinel index"),
            "`{command}` says the graph is stale but not what to do:\n{note}"
        );
    }
}

#[test]
fn every_command_that_reads_the_graph_discloses_that_none_was_ever_built() {
    // Distinct from staleness. "No graph has been built" is why every article
    // looks unlinked; "out of date" means the numbers are real but old.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/first.md",
        "---\ntitle: First\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources: [raw/philosophy/src.md]\n---\n\nSee [[second]].\n",
    );

    for command in graph_reading_commands() {
        let v = a.json(&[&command]);
        let note = stale_note(&v).unwrap_or_else(|| {
            panic!("`sentinel {command} --json` hides an unbuilt graph:\n{v:#}")
        });
        assert!(
            note.contains("No link graph has been built"),
            "`{command}` must distinguish never-built from out-of-date:\n{note}"
        );
    }
}

#[test]
fn a_current_graph_raises_no_alarm() {
    // A warning that is always on is a warning nobody reads.
    let a = indexed();
    for command in graph_reading_commands() {
        let v = a.json(&[&command]);
        assert_eq!(
            stale_note(&v),
            None,
            "`{command}` claims a freshly built graph is stale:\n{v:#}"
        );
    }
}

#[test]
fn a_deleted_article_also_makes_the_graph_stale() {
    // Written against additions alone, the check would have passed while a
    // graph still listing deleted articles reported itself current.
    let a = indexed();
    std::fs::remove_file(a.path("wiki/philosophy/first.md")).unwrap();

    let note = stale_note(&a.json(&["graph"])).expect("removal must count as staleness");
    assert!(note.contains("removed"), "{note}");
}

#[test]
fn an_article_written_since_the_last_index_is_not_reported_as_nonexistent() {
    // `unknown` meant "absent from the graph", so an article sitting on disk
    // with outgoing links came back `unknown: true` — indistinguishable from a
    // slug nobody has ever written.
    // Not `second`: `first.md` forward-declares `[[second]]`, so that slug is
    // already a graph node as a link target. This one is nowhere in the graph.
    let a = indexed();
    add_unindexed(&a, "newcomer");

    let v = a.json(&["graph", "--node", "newcomer"]);
    assert_eq!(v["unknown"], false, "the article exists on disk:\n{v:#}");
    assert_eq!(v["in_graph"], false, "but the graph predates it:\n{v:#}");

    let missing = a.json(&["graph", "--node", "never-written"]);
    assert_eq!(
        missing["unknown"], true,
        "a slug with no article must stay unknown:\n{missing:#}"
    );
}

#[test]
fn next_still_offers_connect_work_when_the_graph_is_unbuilt() {
    // The failure this closes: with no graph, `orphan_articles` returned an
    // empty list and no note, so `connect` silently vanished from the backlog
    // and `next` reported an archive with nothing to do.
    let a = Archive::new();
    a.write("raw/philosophy/src.md", "text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/lonely.md",
        "---\ntitle: Lonely\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources: [raw/philosophy/src.md]\n---\n\nNo links here.\n",
    );

    let v = a.json(&["next"]);
    let note = v["progress"]["link_graph_stale"]
        .as_str()
        .expect("next must disclose the unbuilt graph rather than report no work");
    assert!(note.contains("sentinel index"), "{note}");
}

#[test]
fn the_human_output_says_it_too() {
    let a = indexed();
    add_unindexed(&a, "second");

    for command in graph_reading_commands() {
        let out = a.run(&[&command]);
        assert!(
            out.contains("out of date"),
            "`sentinel {command}` (human) hides the stale graph:\n{out}"
        );
    }
}
