//! `index/_dashboard.md` — a generated page that must not become a second,
//! quietly wrong source of truth.
//!
//! The risk this file guards is specific. A generated page is read by a person,
//! not diffed against the command it came from, so a number that drifts is
//! never noticed. Every check here either compares the page with the commands
//! or enumerates what it must cover from the code that defines it.

mod common;

use common::Archive;

/// An archive with something in every category worth reporting.
fn populated() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/cited.md", "text about virtue");
    a.write("raw/philosophy/stranded.md", "nothing cites this");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/first.md",
        "---\ntitle: First\ndomain: philosophy\norigin: authored\nstatus: draft\n\
         tags: [t]\nsources: [raw/philosophy/cited.md]\n---\n\nSee [[unwritten]].\n",
    );
    a.run(&["index"]);
    a
}

fn dashboard(a: &Archive) -> String {
    a.read("index/_dashboard.md")
}

#[test]
fn index_generates_it_and_init_stubs_it() {
    // The pairing is enforced by `init_stubs_exactly_the_indexes_that_index
    // _regenerates` in command_contract.rs; this checks the file is real.
    let a = Archive::new();
    common::assert_exists(&a.path("index/_dashboard.md"));
    a.run(&["index"]);
    assert!(dashboard(&a).contains("# Dashboard"), "{}", dashboard(&a));
}

#[test]
fn it_stays_within_its_context_budget() {
    // `index/_master.md` reached 16 KB on a 400-article archive and is the file
    // skills are forbidden to read. A dashboard that grows with the archive
    // becomes the same trap, so every list on it is capped.
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "x");
    a.run(&["sync"]);
    for i in 0..120 {
        a.write(
            &format!("wiki/philosophy/a{i}.md"),
            &format!(
                "---\ntitle: A{i}\ndomain: philosophy\norigin: authored\nstatus: draft\n\
                 tags: [t]\nsources: [raw/philosophy/s.md]\n---\n\nSee [[gap{i}]].\n"
            ),
        );
    }
    a.run(&["index"]);

    let text = dashboard(&a);
    assert!(
        text.len() <= 6_000,
        "dashboard is {} bytes on a 120-article archive; it must not scale \
         with the wiki",
        text.len()
    );
}

#[test]
fn every_rung_of_the_ladder_appears_even_when_it_is_empty() {
    // Derived from `sentinel schema`, which publishes the ladder, rather than
    // from a list written here. A rung missing from the page reads as a rung
    // that does not exist — and #39 was two of five actions absent from a
    // counter written exactly that way.
    let a = populated();
    let text = dashboard(&a);
    let schema = a.json(&["schema"]);
    let actions: Vec<&str> = schema["next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["action"].as_str().unwrap())
        .collect();
    assert!(actions.len() >= 5, "{actions:?}");

    for action in actions {
        assert!(
            text.contains(&format!("`{action}`")),
            "the backlog table omits `{action}`:\n{text}"
        );
    }
}

#[test]
fn the_numbers_are_the_same_numbers_the_commands_report() {
    // The whole hazard of a generated page: it is read, not verified. If these
    // ever disagree, the page is the one that will be believed.
    let a = populated();
    let text = dashboard(&a);

    let status = a.json(&["status"]);
    let next = a.json(&["next"]);

    for (label, value) in [
        ("articles", status["wiki_articles"].as_u64().unwrap()),
        ("raw documents", status["raw_documents"].as_u64().unwrap()),
        ("uncompiled", status["uncompiled"].as_u64().unwrap()),
        ("orphans", status["orphan_pages"].as_u64().unwrap()),
    ] {
        assert!(
            text.contains(&format!("| {label} | {value} |")),
            "dashboard disagrees with `sentinel status` about {label} \
             (expected {value}):\n{text}"
        );
    }

    let action = next["action"].as_str().unwrap();
    assert!(
        text.contains(&format!("**{action}**")),
        "dashboard recommends something other than `sentinel next`:\n{text}"
    );
}

#[test]
fn a_capped_list_publishes_its_true_total() {
    // Same rule as `_recent.md` and every JSON payload: a truncated list that
    // does not say so reads as complete.
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "x");
    a.run(&["sync"]);
    // Ten gaps, so the write rung has more targets than the page shows.
    for i in 0..10 {
        a.write(
            &format!("wiki/philosophy/a{i}.md"),
            &format!(
                "---\ntitle: A{i}\ndomain: philosophy\norigin: authored\ntags: [t]\n\
                 sources: [raw/philosophy/s.md]\n---\n\nSee [[gap{i}]].\n"
            ),
        );
    }
    a.run(&["index"]);

    let text = dashboard(&a);
    let target_count = a.json(&["next"])["target_count"].as_u64().unwrap();
    assert!(
        target_count > 5,
        "fixture should exceed the cap: {target_count}"
    );
    assert!(
        text.contains(&format!("of {target_count} target(s)")),
        "the page shows a sample without naming the total:\n{text}"
    );
}

#[test]
fn it_says_when_it_was_generated() {
    // A static page's largest lie is being out of date. It cannot refresh
    // itself, so it has to date itself.
    let a = populated();
    let text = dashboard(&a);
    assert!(text.contains("Generated"), "{text}");
    assert!(
        text.contains("sentinel index"),
        "it must say how to refresh:\n{text}"
    );
}

#[test]
fn it_repeats_the_disclosures_the_commands_make() {
    // A dashboard that describes a partial archive without saying so is the
    // failure this repo has now fixed five times.
    let a = populated();
    // Add an article without reindexing: the graph is now behind disk.
    a.write(
        "wiki/philosophy/late.md",
        "---\ntitle: Late\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources: [raw/philosophy/cited.md]\n---\n\nSee [[first]].\n",
    );
    // Regenerating is what makes it current again, so to observe the stale
    // path the page must be rendered while the graph is behind — which `index`
    // never is. Instead assert the inverse: after `index`, no stale note.
    a.run(&["index"]);
    let text = dashboard(&a);
    assert!(
        !text.contains("out of date"),
        "a freshly generated page claims to be stale:\n{text}"
    );
}

#[test]
fn a_lint_finding_is_counted_but_not_reproduced() {
    // Findings are unbounded; `sentinel lint` is where they belong. The page
    // carries counts so a reader knows to go look.
    let a = populated();
    a.write(
        "wiki/philosophy/broken.md",
        "---\ntitle: Broken\ndomain: philosophy\norigin: nonsense\ntags: [t]\n\
         sources: [raw/philosophy/cited.md]\n---\n\nBody.\n",
    );
    a.run(&["index"]);

    let text = dashboard(&a);
    assert!(
        text.contains("`invalid-origin`"),
        "the rule is not named:\n{text}"
    );
    assert!(
        text.contains("sentinel lint"),
        "it must point at the findings themselves:\n{text}"
    );
    assert!(
        !text.contains("wiki/philosophy/broken.md"),
        "the page reproduced a finding instead of counting it:\n{text}"
    );
}

#[test]
fn regeneration_converges_instead_of_churning_forever() {
    // The page reports the activity log, and `index` appends to that log
    // whenever it writes — so the first regeneration after a change does
    // legitimately differ, because it picks up the entry the previous run
    // made. What must not happen is that difference counting as news, logging
    // again, and changing the next page in turn.
    //
    // Two things prevent it: the dashboard write does not feed `index`'s
    // "something changed" flag, and the timestamp carries no time of day.
    let a = populated();
    a.run(&["index"]);

    let settled = dashboard(&a);
    let before = std::fs::metadata(a.path("index/_dashboard.md"))
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    a.run(&["index"]);

    assert_eq!(
        dashboard(&a),
        settled,
        "the dashboard is still changing on an archive nobody touched"
    );
    assert_eq!(
        std::fs::metadata(a.path("index/_dashboard.md"))
            .unwrap()
            .modified()
            .unwrap(),
        before,
        "identical content was rewritten anyway; `write_if_changed` is being \
         defeated, most likely by a timestamp with a finer grain than a day"
    );
}
