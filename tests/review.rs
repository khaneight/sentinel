//! `sentinel review` — the archive owner's answer, and the only writer of it.
//!
//! Every other command is the tool's opinion. This one is the user's, so the
//! things worth testing are: that it cannot be attributed to nobody, that it
//! appends rather than overwrites, that it survives a rebuild, and that a
//! rejection stays rejected.

mod common;

use common::Archive;

fn trait_file(id: &str, status: &str) -> String {
    format!(
        "---\nid: {id}\nkind: belief\nclaim: The author holds something.\n\
         confidence: medium\nstatus: {status}\nevidence:\n  - raw/philosophy/mine.md\n---\n\n\
         They write, in so many words, that they hold it.\n"
    )
}

fn archive() -> Archive {
    let a = Archive::new();
    let mine = a.path("mine.md");
    std::fs::write(&mine, "What I actually think.\n").unwrap();
    assert_eq!(
        a.code(&[
            "ingest",
            &mine.display().to_string(),
            "-d",
            "philosophy",
            "-o",
            "authored",
            "--as",
            "mine.md",
        ]),
        0
    );
    a.write("persona/holds.md", &trait_file("holds", "proposed"));
    a.write(
        "wiki/philosophy/essay.md",
        "---\ntitle: Essay\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources: [raw/philosophy/mine.md]\n---\n\nProse.\n",
    );
    a.run(&["index"]);
    a
}

/// Run with a reviewer identity, since the command refuses without one.
fn review(a: &Archive, args: &[&str]) -> std::process::Output {
    let mut cmd = a.cmd(args);
    cmd.env("SENTINEL_REVIEWER", "khaneight");
    cmd.output().unwrap()
}

fn frontmatter(a: &Archive, path: &str) -> serde_yaml::Value {
    let text = a.read(path);
    let yaml = text
        .strip_prefix("---\n")
        .and_then(|r| r.split_once("\n---"))
        .expect("a frontmatter block")
        .0;
    serde_yaml::from_str(yaml).expect("valid YAML")
}

#[test]
fn a_verdict_nobody_signed_is_refused() {
    // The whole mechanism exists so that a person, identifiably, said a thing.
    // Defaulting `by` to a placeholder would produce records that look like
    // somebody agreed.
    let a = archive();
    let out = a
        .cmd(&["review", "holds", "--approve"])
        .env_remove("SENTINEL_REVIEWER")
        .env_remove("USER")
        .output()
        .unwrap();
    assert_ne!(out.status.code(), Some(0));
    let stderr = common::stderr(&out);
    assert!(
        stderr.contains("--by"),
        "the refusal must say how: {stderr}"
    );
}

#[test]
fn approving_a_trait_records_the_verdict_and_moves_its_visible_status() {
    // A trait carries its standing twice: `status:` is what a reader sees, the
    // review list is the history behind it. Writing one without the other is
    // exactly what `verdict-disagrees-with-status` reports.
    let a = archive();
    let out = review(
        &a,
        &["review", "holds", "--approve", "--note", "yes, that's me"],
    );
    assert_eq!(out.status.code(), Some(0), "{}", common::stderr(&out));

    let fm = frontmatter(&a, "persona/holds.md");
    assert_eq!(fm["status"].as_str(), Some("affirmed"));
    let entries = fm["review"].as_sequence().expect("a review list");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["verdict"].as_str(), Some("approved"));
    assert_eq!(entries[0]["by"].as_str(), Some("khaneight"));
    assert_eq!(entries[0]["note"].as_str(), Some("yes, that's me"));

    assert_eq!(a.code(&["lint"]), 0, "{}", a.run(&["lint"]));
    assert_eq!(
        a.json(&["persona", "--affirmed"])["traits"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "an approved trait is one the clone may write from"
    );
}

#[test]
fn a_rejection_is_durable_and_the_body_survives() {
    // A "no" that evaporates next iteration is not a permission system. The
    // file stays on disk carrying the rejection, and so does the reasoning
    // somebody wrote underneath it.
    let a = archive();
    let out = review(
        &a,
        &["review", "holds", "--reject", "--note", "not what I think"],
    );
    assert_eq!(out.status.code(), Some(0), "{}", common::stderr(&out));

    assert!(
        a.read("persona/holds.md")
            .contains("They write, in so many words"),
        "the body must survive the edit"
    );
    let fm = frontmatter(&a, "persona/holds.md");
    assert_eq!(fm["status"].as_str(), Some("rejected"));

    // And it survives a rebuild, which is the point of storing it in the file.
    a.run(&["index"]);
    let fm = frontmatter(&a, "persona/holds.md");
    assert_eq!(fm["review"].as_sequence().unwrap().len(), 1);
    assert_eq!(a.json(&["persona"])["by_status"]["rejected"], 1);
}

#[test]
fn verdicts_append_so_the_history_is_kept() {
    let a = archive();
    review(
        &a,
        &[
            "review",
            "holds",
            "--request-changes",
            "--note",
            "too broad",
        ],
    );
    review(&a, &["review", "holds", "--approve"]);

    let entries = frontmatter(&a, "persona/holds.md")["review"]
        .as_sequence()
        .unwrap()
        .clone();
    assert_eq!(
        entries.len(),
        2,
        "a verdict must not overwrite the last one"
    );
    assert_eq!(entries[0]["verdict"].as_str(), Some("changes-requested"));
    assert_eq!(entries[0]["note"].as_str(), Some("too broad"));
    assert_eq!(entries[1]["verdict"].as_str(), Some("approved"));
    assert_eq!(
        frontmatter(&a, "persona/holds.md")["status"].as_str(),
        Some("affirmed"),
        "the latest decision is the operative one"
    );
}

#[test]
fn a_comment_leaves_standing_alone() {
    let a = archive();
    review(&a, &["review", "holds", "--approve"]);
    review(
        &a,
        &["review", "holds", "--comment", "--note", "still good"],
    );

    let fm = frontmatter(&a, "persona/holds.md");
    assert_eq!(
        fm["status"].as_str(),
        Some("affirmed"),
        "a remark on approved work must not un-approve it"
    );
    assert_eq!(fm["review"].as_sequence().unwrap().len(), 2);
    assert_eq!(a.code(&["lint"]), 0, "{}", a.run(&["lint"]));
}

#[test]
fn an_article_takes_a_verdict_without_gaining_a_trait_status() {
    // Articles have `status` too, but it means maturity. Approval is a
    // separate axis, and moving `draft` to `affirmed` would be nonsense.
    let a = archive();
    let out = review(&a, &["review", "essay", "--approve"]);
    assert_eq!(out.status.code(), Some(0), "{}", common::stderr(&out));
    let fm = frontmatter(&a, "wiki/philosophy/essay.md");
    assert_eq!(fm["review"].as_sequence().unwrap().len(), 1);
    assert!(
        fm["status"].is_null() || fm["status"].as_str() != Some("affirmed"),
        "an article's status is maturity, not approval: {fm:?}"
    );
    assert_eq!(a.code(&["lint"]), 0, "{}", a.run(&["lint"]));
}

#[test]
fn the_pending_queue_is_what_needs_an_answer_and_nothing_else() {
    let a = archive();
    let v = a.json(&["review"]);
    assert_eq!(v["count"], 1, "{v:#}");
    assert_eq!(v["pending"][0]["id"], "holds");
    assert_eq!(v["pending"][0]["kind"], "trait");
    assert!(
        !v.to_string().contains("wiki/philosophy/essay.md"),
        "an article nobody has asked about is not pending:\n{v:#}"
    );

    review(&a, &["review", "holds", "--approve"]);
    let v = a.json(&["review"]);
    assert_eq!(v["count"], 0, "an answered trait leaves the queue:\n{v:#}");
    assert!(a.run(&["review"]).contains("Nothing is waiting on you"));
}

#[test]
fn requesting_changes_puts_an_article_back_in_the_queue_with_the_note() {
    let a = archive();
    review(
        &a,
        &[
            "review",
            "essay",
            "--request-changes",
            "--note",
            "third section is weak",
        ],
    );
    let v = a.json(&["review"]);
    let item = v["pending"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["kind"] == "article")
        .expect("changes-requested reopens an article");
    assert_eq!(item["note"], "third section is weak");
}

#[test]
fn an_unknown_target_is_an_error_that_says_where_to_look() {
    let a = archive();
    let out = review(&a, &["review", "nothing-by-this-name", "--approve"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(common::stderr(&out).contains("sentinel persona"));
}

#[test]
fn an_ambiguous_name_is_refused_rather_than_resolved() {
    // Recording a verdict on the wrong document is worse than recording none.
    let a = archive();
    a.write("persona/essay.md", &trait_file("essay", "proposed"));
    let out = review(&a, &["review", "essay", "--approve"]);
    assert_ne!(out.status.code(), Some(0));
    let stderr = common::stderr(&out);
    assert!(
        stderr.contains("persona/essay.md") && stderr.contains("wiki/philosophy/essay.md"),
        "the refusal must name both: {stderr}"
    );
}

#[test]
fn a_target_with_no_verdict_is_an_error_not_a_silent_listing() {
    let a = archive();
    let out = review(&a, &["review", "holds"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(common::stderr(&out).contains("--approve"));
}

#[test]
fn recording_a_verdict_lands_in_the_activity_log() {
    // `meta/log.md` records what changed the archive. This changes it.
    let a = archive();
    review(&a, &["review", "holds", "--approve"]);
    let entries = a.json(&["log"]);
    let text = entries.to_string();
    assert!(
        text.contains("review") && text.contains("persona/holds.md"),
        "the log should say who decided what:\n{text}"
    );
}

#[test]
fn a_hand_edited_status_that_contradicts_its_own_history_is_reported() {
    let a = archive();
    review(&a, &["review", "holds", "--approve"]);
    let text = a
        .read("persona/holds.md")
        .replace("status: affirmed", "status: proposed");
    a.write("persona/holds.md", &text);

    let findings = a.json(&["lint"]);
    let hit = findings["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["rule"] == "verdict-disagrees-with-status")
        .expect("the visible standing and the recorded one must agree");
    assert_eq!(hit["path"], "persona/holds.md");
    assert_eq!(a.code(&["lint"]), 2);
}

#[test]
fn a_verdict_is_not_recorded_from_a_partial_view() {
    // Resolving a name against a wiki it could only half read could match the
    // wrong document, or miss the ambiguity that should have stopped it.
    let a = archive();
    let hidden = a.path("wiki/philosophy/essay.md");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hidden, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read_to_string(&hidden).is_ok() {
            return; // running as root
        }
        let out = review(&a, &["review", "holds", "--approve"]);
        assert_ne!(out.status.code(), Some(0), "must refuse on a partial view");
        assert!(common::stderr(&out).contains("essay.md"));
        std::fs::set_permissions(&hidden, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
}
