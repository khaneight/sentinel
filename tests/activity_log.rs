//! The activity log is written by six commands and was readable by none.
//!
//! `/sentinel-grow` calls it "the only durable record of what this loop did to
//! the archive", and the only way to consult it was to read the whole file —
//! which grows without bound. That is the context problem this tool spent
//! several PRs removing from `search`, `graph`, and `lint`.

mod common;

use common::Archive;

fn with_history(a: &Archive, n: usize) {
    for i in 0..n {
        a.run(&["log", "compile", &format!("entry {i}")]);
    }
}

#[test]
fn the_log_can_be_read_back() {
    let a = Archive::new();
    a.run(&["log", "compile", "wrote three articles"]);

    let out = a.run(&["log"]);
    assert!(out.contains("compile"), "{out}");
    assert!(out.contains("wrote three articles"), "{out}");
}

#[test]
fn entries_come_back_newest_first() {
    let a = Archive::new();
    a.run(&["log", "compile", "older"]);
    a.run(&["log", "research", "newer"]);

    let v = a.json(&["log"]);
    let entries = v["entries"].as_array().unwrap();
    assert_eq!(entries[0]["detail"], "newer", "{v}");
    assert_eq!(entries[1]["detail"], "older");
}

#[test]
fn reading_is_bounded_and_says_so() {
    let a = Archive::new();
    with_history(&a, 30);

    let v = a.json(&["log"]);
    assert!(v["entry_count"].as_u64().unwrap() >= 30);
    assert_eq!(v["returned"], 20, "default limit");
    assert_eq!(v["truncated"], true);
    assert!(a.run(&["log"]).contains("older entries not shown"));
}

#[test]
fn the_limit_is_adjustable_and_the_count_is_not() {
    let a = Archive::new();
    with_history(&a, 30);

    let v = a.json(&["log", "--limit", "3"]);
    assert_eq!(v["returned"], 3);
    assert!(
        v["entry_count"].as_u64().unwrap() >= 30,
        "a limit narrows output, not the count"
    );
}

#[test]
fn a_short_log_reports_no_truncation() {
    let a = Archive::new();
    a.run(&["log", "compile", "one"]);

    let v = a.json(&["log"]);
    assert_eq!(v["truncated"], false, "{v}");
    assert!(!a.run(&["log"]).contains("not shown"));
}

#[test]
fn a_multi_line_detail_stays_one_parseable_entry() {
    // The file documents itself as parseable with `grep "^## \["`. A detail
    // containing a newline produced continuation lines that grep silently
    // drops, so the recorded text and the text any documented reader sees
    // would differ.
    let a = Archive::new();
    a.run(&["log", "compile", "wrote:\n  - a.md\n  - b.md"]);

    let raw = a.read("meta/log.md");
    let entry_lines = raw.lines().filter(|l| l.starts_with("## [")).count();
    let orphaned = raw
        .lines()
        .filter(|l| l.contains("- a.md") && !l.starts_with("## ["))
        .count();
    assert_eq!(
        orphaned, 0,
        "detail spilled onto a line grep will not see:\n{raw}"
    );

    let v = a.json(&["log"]);
    let detail = v["entries"][0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("a.md") && detail.contains("b.md"),
        "{detail}"
    );
    assert_eq!(entry_lines, v["entry_count"].as_u64().unwrap() as usize);
}

#[test]
fn appending_still_works_and_reports_what_it_wrote() {
    let a = Archive::new();
    let out = a.run(&["log", "research", "stoic ethics"]);
    assert!(out.contains("Logged:"), "{out}");
    assert!(out.contains("stoic ethics"), "{out}");
}

#[test]
fn an_empty_log_reads_cleanly() {
    let a = Archive::new();
    // `init` records one entry, so clear it to reach the genuinely empty case.
    a.write("meta/log.md", "# Activity Log\n\n");
    assert!(a.run(&["log"]).contains("No activity recorded yet"));
    assert_eq!(a.json(&["log"])["entry_count"], 0);
}
