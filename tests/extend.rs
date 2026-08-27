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
    // `stable`, so the publishing tests below have something ordinary to
    // publish alongside the generated article.
    a.write(
        "wiki/philosophy/compiled.md",
        "---\ntitle: Compiled\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         status: stable\nsources: [raw/philosophy/mine.md]\n---\n\nSee [[held-thing]].\n",
    );
    a.write(
        "wiki/philosophy/held-thing.md",
        "---\ntitle: Held Thing\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         status: stable\nsources: [raw/philosophy/mine.md]\n---\n\nSee [[compiled]].\n",
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

// --- publishing ------------------------------------------------------------

fn approve(a: &Archive, slug: &str) {
    let mut cmd = a.cmd(&["review", slug, "--approve", "--note", "yes"]);
    cmd.env("SENTINEL_REVIEWER", "khaneight");
    assert!(cmd.output().unwrap().status.success());
}

/// An archive holding one finished, unapproved extrapolated article.
fn publishable() -> (Archive, tempfile::TempDir) {
    let a = archive();
    a.write(
        "wiki/philosophy/new.md",
        &extrapolated("New", &["held"]).replace("status: draft", "status: stable"),
    );
    a.run(&["index"]);
    (a, tempfile::tempdir().unwrap())
}

#[test]
fn finished_but_unsigned_generated_work_is_not_published() {
    // The gate. `stable` means finished; `approved` means the archive's owner
    // signed it, and only the second lets machine prose out under their name.
    let (a, out) = publishable();
    let dest = out.path().join("site");
    let v = a.json(&["export", "--out", &dest.display().to_string(), "--flat"]);
    assert_eq!(v["published"], 2, "the compiled articles still go:\n{v:#}");
    assert_eq!(v["held_for_approval"], 1, "{v:#}");
    assert!(
        !dest.join("new.md").exists(),
        "unsigned work must not reach the site"
    );

    let reason = v["excluded"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == "wiki/philosophy/new.md")
        .expect("it must be reported, not silently dropped")["reason"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(reason.contains("not approved"), "{reason}");
}

#[test]
fn no_status_flag_can_open_the_gate() {
    // A flag that could override this would make the gate advisory. Approval
    // is a different axis from maturity and `--status` only speaks to maturity.
    let (a, out) = publishable();
    let dest = out.path().join("site");
    for args in [
        vec!["export", "--out", "", "--flat", "--include-drafts"],
        vec![
            "export",
            "--out",
            "",
            "--flat",
            "--status",
            "draft,review,stable",
        ],
    ] {
        let d = dest.display().to_string();
        let argv: Vec<&str> = args
            .iter()
            .map(|x| if x.is_empty() { d.as_str() } else { *x })
            .collect();
        let v = a.json(&argv);
        assert_eq!(
            v["held_for_approval"], 1,
            "{argv:?} opened the gate:\n{v:#}"
        );
        assert!(!dest.join("new.md").exists());
    }
}

#[test]
fn approved_work_is_published_and_carries_a_notice_the_agent_did_not_write() {
    // The exporter writes the attribution, unconditionally. An agent that
    // composes its own disclosure is an agent that can leave it out.
    let (a, out) = publishable();
    approve(&a, "new");
    let dest = out.path().join("site");
    let v = a.json(&["export", "--out", &dest.display().to_string(), "--flat"]);
    assert_eq!(v["published"], 3, "{v:#}");
    assert_eq!(v["held_for_approval"], 0);

    let text = std::fs::read_to_string(dest.join("new.md")).unwrap();
    assert!(
        text.contains("Written by a language model"),
        "a reader must not take this for the author's own writing:\n{text}"
    );
    assert!(
        text.contains("The author holds held"),
        "the notice should name the claim it was written from:\n{text}"
    );
    assert!(
        text.contains("Approved by khaneight"),
        "and who signed it:\n{text}"
    );
}

#[test]
fn an_ordinary_article_gets_no_notice() {
    // The disclosure has to mean something. Attaching it to everything would
    // make it furniture.
    let (a, out) = publishable();
    let dest = out.path().join("site");
    a.run(&["export", "--out", &dest.display().to_string(), "--flat"]);
    let text = std::fs::read_to_string(dest.join("compiled.md")).unwrap();
    assert!(!text.contains("Written by a language model"), "{text}");
}

#[test]
fn the_bundle_marks_generated_work_and_publishes_only_affirmed_traits() {
    // A front end that renders the clone's work like the author's own misleads
    // its readers, and a bundle carrying `proposed` traits would put an
    // unconfirmed claim about a person in front of them.
    let (a, out) = publishable();
    approve(&a, "new");
    a.write("persona/guessed.md", &trait_file("guessed", "proposed"));
    // Something genuinely unfinished, so `unpublished` has a real value to
    // report rather than being asserted against a fully-published archive.
    a.write(
        "wiki/philosophy/half-written.md",
        "---\ntitle: Half Written\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         status: draft\nsources: [raw/philosophy/mine.md]\n---\n\nSee [[compiled]].\n",
    );
    a.run(&["index"]);

    let dest = out.path().join("site");
    let data = out.path().join("bundle.json");
    a.run(&[
        "export",
        "--out",
        &dest.display().to_string(),
        "--flat",
        "--data",
        &data.display().to_string(),
    ]);
    let bundle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&data).unwrap()).unwrap();

    let node = bundle["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["slug"] == "new")
        .expect("the approved article is in the bundle");
    assert_eq!(node["extrapolated"], true);

    let traits = bundle["persona"].as_array().unwrap();
    assert_eq!(traits.len(), 1, "affirmed only:\n{traits:#?}");
    assert_eq!(traits[0]["id"], "held");
    assert_eq!(
        traits[0]["expressed_in"],
        serde_json::json!(["new"]),
        "the bundle should link a claim to what was written from it"
    );
    assert!(
        traits[0].get("evidence").is_none(),
        "raw/ paths are not published, so citing them at readers is a dead end"
    );
    assert_eq!(traits[0]["evidence_count"], 1);

    assert_eq!(bundle["in_progress"]["unconfirmed_traits"], 1);
    assert_eq!(bundle["in_progress"]["awaiting_approval"], 0);
    assert_eq!(
        bundle["in_progress"]["unpublished"], 1,
        "the draft is in the archive and not on the site"
    );
}

#[test]
fn the_notice_reads_as_a_sentence() {
    // Found by reading the published file rather than by asserting on it: the
    // string was line-wrapped into the middle of a sentence, and joining
    // claims that already end in a full stop produced "generalising..".
    let (a, out) = publishable();
    approve(&a, "new");
    let dest = out.path().join("site");
    a.run(&["export", "--out", &dest.display().to_string(), "--flat"]);
    let text = std::fs::read_to_string(dest.join("new.md")).unwrap();
    let notice = text
        .lines()
        .find(|l| l.contains("Written by a language model"))
        .expect("the notice is there");
    assert!(!notice.contains("  "), "doubled spacing: {notice}");
    assert!(!notice.contains(".."), "doubled punctuation: {notice}");
}

#[test]
fn provenance_becomes_depth_in_the_bundle() {
    // What the showcase draws as distance from the author. Asserted here rather
    // than left to the page, so a front end cannot invent its own opinion about
    // whose work something is.
    let a = archive();
    for (slug, origin) in [
        ("mine", "authored"),
        ("ours", "hybrid"),
        ("theirs", "researched"),
    ] {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &format!(
                "---\ntitle: {slug}\ndomain: philosophy\norigin: {origin}\ntags: [t]\n\
                 status: stable\nsources: [raw/philosophy/mine.md]\n---\n\nProse.\n"
            ),
        );
    }
    a.write(
        "wiki/philosophy/machine.md",
        &extrapolated("Machine", &["held"]).replace("status: draft", "status: stable"),
    );
    a.run(&["index"]);
    approve(&a, "machine");

    let out = tempfile::tempdir().unwrap();
    let ui = out.path().join("ui");
    a.run(&[
        "export",
        "--out",
        &out.path().join("site").display().to_string(),
        "--flat",
        "--ui",
        &ui.display().to_string(),
    ]);
    let bundle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(ui.join("bundle.json")).unwrap()).unwrap();

    let layer = |slug: &str| -> u64 {
        bundle["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["slug"] == slug)
            .unwrap_or_else(|| panic!("{slug} is published"))["layer"]
            .as_u64()
            .unwrap()
    };
    assert_eq!(layer("mine"), 1, "their own thinking sits nearest");
    assert_eq!(layer("ours"), 2);
    assert_eq!(layer("theirs"), 3);
    assert_eq!(layer("machine"), 4, "the clone's work sits furthest out");
}

#[test]
fn the_bundle_carries_the_published_prose_not_the_source_file() {
    // The page reads from the bundle, so what it shows has to be what the site
    // shows — links already defused, attribution already appended. Rendering
    // the article as it sits in `wiki/` would put a live `[[wikilink]]` to an
    // unpublished page in front of a reader who cannot follow it.
    let a = archive();
    a.write(
        "wiki/philosophy/new.md",
        &extrapolated("New", &["held"])
            .replace("status: draft", "status: stable")
            .replace(
                "An argument that follows from the above.",
                "Follows from [[held-thing]] and from [[never-written]].",
            ),
    );
    a.run(&["index"]);
    approve(&a, "new");

    let out = tempfile::tempdir().unwrap();
    let ui = out.path().join("ui");
    a.run(&[
        "export",
        "--out",
        &out.path().join("site").display().to_string(),
        "--flat",
        "--ui",
        &ui.display().to_string(),
    ]);
    let bundle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(ui.join("bundle.json")).unwrap()).unwrap();
    let body = bundle["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["slug"] == "new")
        .unwrap()["body"]
        .as_str()
        .unwrap()
        .to_string();

    assert!(
        !body.starts_with("---"),
        "frontmatter must not be in the prose"
    );
    assert!(body.contains("[[held-thing]]"), "a published link survives");
    assert!(
        !body.contains("[[never-written]]"),
        "a link to an unpublished page must be defused here too:\n{body}"
    );
    assert!(
        body.contains("Written by a language model"),
        "the attribution travels with the prose:\n{body}"
    );
}
