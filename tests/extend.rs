//! `origin: extrapolated` — the clone's own work, and what makes it traceable.
//!
//! This is the one thing in the archive that puts words in a real person's
//! mouth. The tests here are the attribution chain: generated prose names the
//! traits it was written from, those traits resolve, the author affirmed them,
//! and nothing counts as approved until they said so.

mod common;

use common::Archive;

fn trait_file(id: &str, status: &str) -> String {
    format!(
        "---\nid: {id}\nkind: belief\nclaim: The author holds {id}.\n\
         confidence: high\nstatus: {status}\nevidence:\n  - raw/philosophy/mine.md\n---\n\n\
         Their writing says so.\n"
    )
}

fn extrapolated(title: &str, traits: &[&str]) -> String {
    let cited = traits
        .iter()
        .map(|t| format!("  - {t}\n"))
        .collect::<String>();
    format!(
        "---\ntitle: {title}\ndomain: philosophy\norigin: extrapolated\ntags: [t]\n\
         persona:\n{cited}status: draft\n---\n\nAn argument that follows from the above.\n"
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
    a.write("persona/held.md", &trait_file("held", "affirmed"));
    a.write(
        "wiki/philosophy/compiled.md",
        "---\ntitle: Compiled\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources: [raw/philosophy/mine.md]\n---\n\nSee [[held-thing]].\n",
    );
    a.write(
        "wiki/philosophy/held-thing.md",
        "---\ntitle: Held Thing\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources: [raw/philosophy/mine.md]\n---\n\nSee [[compiled]].\n",
    );
    a.run(&["index"]);
    a
}

fn rules(a: &Archive) -> Vec<String> {
    a.json(&["lint"])["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_raw_document_can_never_be_extrapolated() {
    // `raw/` is the provenance floor. A generated file sitting in it could
    // later be cited as evidence for what its supposed author believes, which
    // is the archive learning a person from its own output.
    let a = archive();
    let f = a.path("generated.md");
    std::fs::write(&f, "Machine prose.\n").unwrap();
    let out = a.output(&[
        "ingest",
        &f.display().to_string(),
        "-d",
        "philosophy",
        "-o",
        "extrapolated",
    ]);
    assert_ne!(out.status.code(), Some(0), "ingest must refuse it");
    let stderr = common::stderr(&out);
    assert!(
        !stderr.contains("extrapolated,") && stderr.contains("authored"),
        "the error should list only the ingestable origins: {stderr}"
    );
}

#[test]
fn generated_prose_that_names_no_trait_is_an_error() {
    // The attribution rule. Without it the clone can write anything in
    // somebody's voice and point at nothing.
    let a = archive();
    a.write(
        "wiki/philosophy/orphaned.md",
        &extrapolated("Orphaned", &[]).replace("persona:\n", "persona: []\n"),
    );
    assert!(rules(&a).contains(&"unattributed-extrapolation".to_string()));
    assert_eq!(a.code(&["lint"]), 2);
}

#[test]
fn an_extrapolated_article_is_not_nagged_about_missing_sources() {
    // It is not compiled from anything, so "its raw document will stay
    // uncompiled" names a document that does not exist.
    let a = archive();
    a.write("wiki/philosophy/new.md", &extrapolated("New", &["held"]));
    let findings = rules(&a);
    assert!(
        !findings.contains(&"missing-sources".to_string()),
        "{:#}",
        a.json(&["lint"])
    );
    assert_eq!(a.code(&["lint"]), 0, "{}", a.run(&["lint"]));
}

#[test]
fn writing_from_a_rejected_trait_is_an_error() {
    // Their "no" is on the file. This is the whole point of recording it.
    let a = archive();
    a.write("persona/refused.md", &trait_file("refused", "rejected"));
    a.write("wiki/philosophy/new.md", &extrapolated("New", &["refused"]));
    assert!(rules(&a).contains(&"wrote-from-rejected".to_string()));
    assert_eq!(a.code(&["lint"]), 2);
}

#[test]
fn writing_from_an_unconfirmed_trait_is_a_warning_not_an_error() {
    // The reading may well be right; nothing has confirmed it is theirs.
    // Unfinished, not malformed.
    let a = archive();
    a.write("persona/guessed.md", &trait_file("guessed", "proposed"));
    a.write("wiki/philosophy/new.md", &extrapolated("New", &["guessed"]));
    assert!(rules(&a).contains(&"wrote-from-unconfirmed".to_string()));
    assert_eq!(a.code(&["lint"]), 0, "{}", a.run(&["lint"]));
}

#[test]
fn an_attribution_naming_nothing_is_an_error() {
    let a = archive();
    a.write(
        "wiki/philosophy/new.md",
        &extrapolated("New", &["no-such-trait"]),
    );
    assert!(rules(&a).contains(&"unresolved-trait".to_string()));
    assert_eq!(a.code(&["lint"]), 2);
}

#[test]
fn a_trait_is_cited_the_way_a_wikilink_is() {
    let a = archive();
    a.write("wiki/philosophy/new.md", &extrapolated("New", &["Held"]));
    assert!(
        !rules(&a).contains(&"unresolved-trait".to_string()),
        "citation should canonicalise like every other identity:\n{}",
        a.run(&["lint"])
    );
}

// --- the rung --------------------------------------------------------------

#[test]
fn an_affirmed_trait_nothing_has_written_from_is_the_extend_rung() {
    let a = archive();
    let v = a.json(&["next"]);
    assert_eq!(v["action"], "extend", "{v}");
    assert_eq!(v["progress"]["unexpressed"], 1);
    assert_eq!(v["targets"][0]["id"], "held");
    assert_eq!(v["suggested_command"], "/sentinel-extend held");
}

#[test]
fn writing_the_article_moves_the_counter_the_loop_measures_by() {
    let a = archive();
    assert_eq!(a.json(&["next"])["progress"]["unexpressed"], 1);
    a.write("wiki/philosophy/new.md", &extrapolated("New", &["held"]));
    assert_eq!(
        a.json(&["next"])["progress"]["unexpressed"],
        0,
        "{}",
        a.run(&["next"])
    );
}

#[test]
fn only_affirmed_traits_are_ever_offered_to_write_from() {
    // A `proposed` trait is the agent's own reading. Offering it as a prompt
    // would let the clone bootstrap a voice out of its own guesses, and then
    // cite that voice as the author's.
    let a = Archive::new();
    let mine = a.path("mine.md");
    std::fs::write(&mine, "text\n").unwrap();
    a.run(&[
        "ingest",
        &mine.display().to_string(),
        "-d",
        "philosophy",
        "-o",
        "authored",
        "--as",
        "mine.md",
    ]);
    for (id, status) in [("guessed", "proposed"), ("refused", "rejected")] {
        a.write(&format!("persona/{id}.md"), &trait_file(id, status));
    }
    a.run(&["index"]);

    let v = a.json(&["next", "--action", "extend"]);
    assert_eq!(v["progress"]["unexpressed"], 0, "{v:#}");
    assert_eq!(v["action"], "none");
}

#[test]
fn extend_comes_after_connect_and_before_review() {
    // Generating on top of a disconnected archive compounds what is wrong
    // with it, so the maintenance rungs go first.
    let a = archive();
    a.write(
        "wiki/philosophy/lonely.md",
        "---\ntitle: Lonely\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources: [raw/philosophy/mine.md]\n---\n\nNothing links here.\n",
    );
    a.run(&["index"]);
    assert_eq!(
        a.json(&["next"])["action"],
        "connect",
        "an orphan outranks generating something new"
    );
}

#[test]
fn work_the_author_has_not_signed_is_reported_but_is_not_a_rung() {
    // The agent cannot approve its own work, so this is something for the loop
    // to stop on rather than something for it to do.
    let a = archive();
    a.write("wiki/philosophy/new.md", &extrapolated("New", &["held"]));
    let v = a.json(&["next"]);
    assert_eq!(v["progress"]["awaiting_approval"], 1, "{v:#}");
    let backlog: Vec<&str> = v["backlog"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert!(
        !backlog.contains(&"approve"),
        "approval is not work an agent can pick up:\n{v:#}"
    );

    let mut cmd = a.cmd(&["review", "new", "--approve"]);
    cmd.env("SENTINEL_REVIEWER", "khaneight");
    assert!(cmd.output().unwrap().status.success());
    assert_eq!(
        a.json(&["next"])["progress"]["awaiting_approval"],
        0,
        "a signed article is no longer waiting"
    );
}

#[test]
fn a_rejected_article_is_still_waiting_rather_than_finished() {
    // "Rejected" is not "dealt with" — the work is still unpublishable and
    // still sitting in the wiki. Counting it as settled would hide it.
    let a = archive();
    a.write("wiki/philosophy/new.md", &extrapolated("New", &["held"]));
    let mut cmd = a.cmd(&["review", "new", "--reject", "--note", "no"]);
    cmd.env("SENTINEL_REVIEWER", "khaneight");
    assert!(cmd.output().unwrap().status.success());
    assert_eq!(a.json(&["next"])["progress"]["awaiting_approval"], 1);
}

#[test]
fn unapproved_generated_work_is_what_the_review_queue_is_for() {
    // Found by running the workflow rather than by a test: the queue listed
    // unconfirmed traits and changes-requested items, and said "nothing is
    // waiting on you" while an unpublishable article sat in the wiki.
    let a = archive();
    a.write("wiki/philosophy/new.md", &extrapolated("New", &["held"]));
    a.run(&["index"]);

    let v = a.json(&["review"]);
    let item = v["pending"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "new")
        .unwrap_or_else(|| panic!("generated work must appear in the queue:\n{v:#}"));
    assert_eq!(item["kind"], "article");
    assert!(
        item["reason"].as_str().unwrap().contains("unapproved"),
        "the reason should say why it is waiting: {item:#}"
    );

    let mut cmd = a.cmd(&["review", "new", "--approve"]);
    cmd.env("SENTINEL_REVIEWER", "khaneight");
    assert!(cmd.output().unwrap().status.success());
    assert_eq!(
        a.json(&["review"])["count"],
        0,
        "and leave once it is signed"
    );
}

#[test]
fn a_rejected_article_stays_in_the_queue_until_it_is_gone() {
    // "Rejected" is not "handled": the article is still in the wiki, still
    // unpublishable, and still something the owner has to do something about.
    let a = archive();
    a.write("wiki/philosophy/new.md", &extrapolated("New", &["held"]));
    let mut cmd = a.cmd(&["review", "new", "--reject", "--note", "not mine"]);
    cmd.env("SENTINEL_REVIEWER", "khaneight");
    assert!(cmd.output().unwrap().status.success());

    let v = a.json(&["review"]);
    let item = v["pending"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "new")
        .unwrap_or_else(|| panic!("a rejected article is still outstanding:\n{v:#}"));
    assert!(item["reason"].as_str().unwrap().contains("rejected"));
    assert_eq!(item["note"], "not mine");
}

#[test]
fn an_ordinary_article_never_appears_in_the_queue() {
    // Only generated work needs signing. Compiled articles would swamp it.
    let a = archive();
    let v = a.json(&["review"]);
    let text = v.to_string();
    assert!(
        !text.contains("compiled.md") && !text.contains("held-thing.md"),
        "compiled articles do not need approval:\n{v:#}"
    );
}
