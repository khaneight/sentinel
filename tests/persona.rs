//! `persona/` — the archive's cited model of its author.
//!
//! These are the safeguards from `docs/clone.md`, exercised against the real
//! binary. They matter more than ordinary lint coverage: the layer holds claims
//! about a person, and a rule that quietly stops firing means the archive can
//! assert anything it likes about them.

mod common;

use common::Archive;

/// A trait file, written the way an agent would.
fn trait_file(id: &str, evidence: &[&str]) -> String {
    let cited = evidence
        .iter()
        .map(|e| format!("  - {e}\n"))
        .collect::<String>();
    format!(
        "---\nid: {id}\nkind: belief\nclaim: The author holds something.\n\
         confidence: medium\nstatus: proposed\nevidence:\n{cited}---\n\n\
         They write, in so many words, that they hold it.\n"
    )
}

/// An archive with one authored source and one researched one.
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
    let theirs = a.path("theirs.md");
    std::fs::write(&theirs, "What somebody else thinks.\n").unwrap();
    assert_eq!(
        a.code(&[
            "ingest",
            &theirs.display().to_string(),
            "-d",
            "philosophy",
            "-o",
            "researched",
            "--as",
            "theirs.md",
        ]),
        0
    );
    a
}

/// Findings for one rule, from the whole-archive lint.
fn findings_for(a: &Archive, rule: &str) -> Vec<String> {
    let v = a.json(&["lint"]);
    v["findings"]
        .as_array()
        .expect("lint publishes findings")
        .iter()
        .filter(|f| f["rule"] == rule)
        .map(|f| f["path"].as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn init_creates_the_layer_and_a_template_generated_from_the_contract() {
    let a = Archive::new();
    assert!(a.path("persona").is_dir(), "init must create persona/");

    let template = a.read("templates/persona-trait.md");
    let fields = a.json(&["schema"]);
    let published = fields["persona"].as_array().expect("schema publishes it");
    assert!(!published.is_empty());
    for field in published {
        let name = field["name"].as_str().unwrap();
        assert!(
            template.contains(&format!("{name}:")),
            "the template omits `{name}`, which schema publishes:\n{template}"
        );
    }
}

#[test]
fn a_trait_citing_the_authors_own_writing_lints_clean() {
    let a = archive();
    a.write(
        "persona/grounded.md",
        &trait_file("grounded", &["raw/philosophy/mine.md"]),
    );
    assert_eq!(
        a.code(&["lint"]),
        0,
        "a well-formed trait should not trip anything:\n{}",
        a.run(&["lint"])
    );
}

#[test]
fn an_uncited_claim_about_a_person_is_an_error() {
    // Safeguard 1. Without this the archive can assert anything about its
    // author and point at nothing, and a profile nobody can audit is a profile
    // nobody can correct.
    let a = archive();
    a.write("persona/invented.md", &trait_file("invented", &[]));
    assert_eq!(
        findings_for(&a, "uncited-claim"),
        vec!["persona/invented.md"],
        "{}",
        a.run(&["lint"])
    );
    assert_eq!(a.code(&["lint"]), 2, "an uncited claim must fail the lint");
}

#[test]
fn a_belief_inferred_from_research_is_an_error() {
    // Safeguard 2. `theirs.md` resolves perfectly well — it is registered, it
    // is on disk, and `unresolved-evidence` has nothing to say about it. What
    // is wrong is that it records what the author *read*.
    let a = archive();
    a.write(
        "persona/borrowed.md",
        &trait_file("borrowed", &["raw/philosophy/theirs.md"]),
    );
    assert_eq!(
        findings_for(&a, "inferred-from-research"),
        vec!["persona/borrowed.md"],
        "{}",
        a.run(&["lint"])
    );
    assert!(
        findings_for(&a, "unresolved-evidence").is_empty(),
        "the path resolves; the problem is what it is, not where it is"
    );
    assert_eq!(a.code(&["lint"]), 2);
}

#[test]
fn evidence_is_matched_the_way_sources_are() {
    // A citation written as a bare filename resolves for `sources:`, so it must
    // resolve here too. Two matchers would mean a path that compiles an article
    // but does not count as evidence for a trait.
    let a = archive();
    a.write("persona/bare.md", &trait_file("bare", &["mine.md"]));
    assert!(
        findings_for(&a, "unresolved-evidence").is_empty(),
        "{}",
        a.run(&["lint"])
    );
    assert_eq!(a.code(&["lint"]), 0);
}

#[test]
fn evidence_naming_nothing_is_reported_with_the_nearest_match() {
    // A case near-miss, which is what the suggester is tuned for. It is
    // deliberately narrow — a transposition like `mien.md` is outside its range
    // and reports without a hint rather than guessing.
    let a = archive();
    a.write(
        "persona/typo.md",
        &trait_file("typo", &["raw/philosophy/Mine.md"]),
    );
    let v = a.json(&["lint"]);
    let finding = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["rule"] == "unresolved-evidence")
        .expect("a citation matching nothing must be reported");
    let message = finding["message"].as_str().unwrap();
    assert!(
        message.contains("mine.md"),
        "a dead citation should name what it probably meant: {message}"
    );
}

#[test]
fn two_traits_cannot_share_an_id() {
    let a = archive();
    for file in ["first", "second"] {
        a.write(
            &format!("persona/{file}.md"),
            &trait_file("shared", &["raw/philosophy/mine.md"]),
        );
    }
    let v = a.json(&["lint"]);
    let dup = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["rule"] == "duplicate-trait-id")
        .unwrap_or_else(|| panic!("two traits sharing an id must be reported"));
    let message = dup["message"].as_str().unwrap();
    assert!(
        message.contains("persona/first.md") && message.contains("persona/second.md"),
        "the finding must name both files: {message}"
    );
}

#[test]
fn an_id_is_matched_the_way_a_wikilink_is() {
    // `slug::canonical`, like every other identity comparison in the archive.
    // Two ids differing only in case or separator are one id.
    let a = archive();
    a.write(
        "persona/first.md",
        &trait_file("Argues From Cases", &["raw/philosophy/mine.md"]),
    );
    a.write(
        "persona/second.md",
        &trait_file("argues-from-cases", &["raw/philosophy/mine.md"]),
    );
    assert_eq!(
        findings_for(&a, "duplicate-trait-id").len(),
        1,
        "{}",
        a.run(&["lint"])
    );
}

#[test]
fn a_trait_that_cites_but_explains_nothing_is_a_warning_not_an_error() {
    let a = archive();
    a.write(
        "persona/terse.md",
        "---\nid: terse\nkind: style\nclaim: c\nevidence: [raw/philosophy/mine.md]\n---\n",
    );
    assert_eq!(
        findings_for(&a, "missing-reasoning"),
        vec!["persona/terse.md"]
    );
    assert_eq!(
        a.code(&["lint"]),
        0,
        "unfinished is not malformed:\n{}",
        a.run(&["lint"])
    );
}

#[test]
fn an_archive_predating_the_layer_still_works() {
    // `persona/` is absent from every archive created before it existed. A
    // missing directory is an empty profile, not a failure — read commands have
    // to keep working, and `index` has to keep rebuilding.
    let a = archive();
    std::fs::remove_dir_all(a.path("persona")).unwrap();
    assert_eq!(a.code(&["lint"]), 0, "{}", a.run(&["lint"]));
    assert_eq!(a.code(&["index"]), 0, "{}", a.run(&["index"]));
    assert_eq!(a.code(&["next"]), 0);
    assert_eq!(a.code(&["status"]), 0);
}

#[test]
fn schema_publishes_the_persona_contract_with_its_enums() {
    let a = Archive::new();
    let v = a.json(&["schema"]);
    let by_name = |name: &str| -> serde_json::Value {
        v["persona"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap_or_else(|| panic!("schema omits `{name}`"))
            .clone()
    };
    assert_eq!(
        by_name("kind")["values"],
        serde_json::json!(["style", "principle", "belief", "pattern"])
    );
    assert_eq!(
        by_name("status")["values"],
        serde_json::json!(["proposed", "affirmed", "rejected"])
    );
    for required in ["id", "kind", "claim"] {
        assert_eq!(by_name(required)["required"], true);
    }

    // The layout must name the directory, or nothing tells an agent it exists.
    let layout = v["layout"].as_array().unwrap();
    assert!(
        layout.iter().any(|d| d["name"] == "persona/"),
        "schema's layout omits persona/"
    );
}

#[test]
fn a_trait_that_cannot_be_read_stops_index_rather_than_shrinking_the_profile() {
    // The same rule the wiki has, and for a sharper reason: a trait that could
    // not be read is indistinguishable from one that was never written, so a
    // rebuild on a partial view concludes the author holds less than they do.
    let a = archive();
    a.write(
        "persona/kept.md",
        &trait_file("kept", &["raw/philosophy/mine.md"]),
    );
    let unreadable = a.write(
        "persona/hidden.md",
        &trait_file("hidden", &["raw/philosophy/mine.md"]),
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read_to_string(&unreadable).is_ok() {
            return; // running as root; the premise does not hold
        }

        let out = a.output(&["index"]);
        assert_ne!(out.status.code(), Some(0), "index must refuse");
        let stderr = common::stderr(&out);
        assert!(
            stderr.contains("persona/hidden.md"),
            "the refusal must name the file: {stderr}"
        );

        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
}

// --- `sentinel persona` ----------------------------------------------------

#[test]
fn an_empty_profile_says_the_clone_has_nothing_to_write_from() {
    // "0 traits" read as a healthy archive with nothing to report. It is the
    // opposite: the archive holds no model of its author at all.
    let a = archive();
    let out = a.run(&["persona"]);
    assert!(
        out.contains("no model of its author"),
        "an empty profile must say what is missing:\n{out}"
    );
    let v = a.json(&["persona"]);
    assert_eq!(v["count"], 0);
    assert_eq!(
        v["coverage"]["eligible"], 1,
        "one authored source is registered; the researched one is not eligible"
    );
}

#[test]
fn research_is_not_eligible_evidence_so_it_is_not_counted_as_corpus() {
    // The archive holds two raw documents. Only the authored one can support a
    // claim about its author, so only that one belongs in the coverage
    // denominator — counting both would report the profile as half-read when
    // it is fully read.
    let a = archive();
    a.write(
        "persona/grounded.md",
        &trait_file("grounded", &["raw/philosophy/mine.md"]),
    );
    let v = a.json(&["persona"]);
    assert_eq!(v["coverage"]["eligible"], 1);
    assert_eq!(v["coverage"]["mined"], 1);
    assert_eq!(v["coverage"]["unmined_count"], 0);
}

#[test]
fn a_rejected_trait_does_not_count_its_evidence_as_read() {
    // The author disagreed with what was read out of the document. That means
    // the document has not been read *correctly*, so it stays in the queue —
    // otherwise one rejected trait retires a source permanently.
    let a = archive();
    a.write(
        "persona/wrong.md",
        &trait_file("wrong", &["raw/philosophy/mine.md"])
            .replace("status: proposed", "status: rejected"),
    );
    let v = a.json(&["persona"]);
    assert_eq!(v["count"], 1);
    assert_eq!(v["by_status"]["rejected"], 1);
    assert_eq!(
        v["coverage"]["unmined_count"], 1,
        "a rejected reading must not retire the source it came from:\n{v:#}"
    );
    assert_eq!(v["coverage"]["unmined"][0], "raw/philosophy/mine.md");
}

#[test]
fn a_filter_narrows_what_is_listed_and_never_what_is_counted() {
    // The same rule `lint --rule` follows. A `--kind belief` view that also
    // reported "1 trait" would make a profile look like it holds one thing.
    let a = archive();
    for (id, kind) in [("a", "style"), ("b", "belief")] {
        a.write(
            &format!("persona/{id}.md"),
            &trait_file(id, &["raw/philosophy/mine.md"])
                .replace("kind: belief", &format!("kind: {kind}")),
        );
    }
    let all = a.json(&["persona"]);
    assert_eq!(all["count"], 2);

    let filtered = a.json(&["persona", "--kind", "belief"]);
    assert_eq!(
        filtered["count"], 2,
        "the count describes the profile, not the filter:\n{filtered:#}"
    );
    let listed = filtered["traits"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["kind"], "belief");
}

#[test]
fn an_unknown_kind_is_an_error_not_an_empty_profile() {
    // `--rule` and `--action` both validate, for the reason this does: a typo
    // that returns nothing is indistinguishable from a category that is empty,
    // and the caller acts on "you hold no beliefs".
    let a = archive();
    let out = a.output(&["persona", "--kind", "beleif"]);
    assert_ne!(out.status.code(), Some(0), "a typo must not read as clean");
    let stderr = common::stderr(&out);
    assert!(
        stderr.contains("belief"),
        "the error should name the real values: {stderr}"
    );
}

#[test]
fn affirmed_shows_only_what_the_author_has_agreed_to() {
    // This is the set the clone may write from. An unset status is `proposed`,
    // so a trait nobody has looked at must not appear here.
    let a = archive();
    a.write(
        "persona/unset.md",
        "---\nid: unset\nkind: style\nclaim: c\n\
         evidence: [raw/philosophy/mine.md]\n---\n\nbody\n",
    );
    a.write(
        "persona/agreed.md",
        &trait_file("agreed", &["raw/philosophy/mine.md"])
            .replace("status: proposed", "status: affirmed"),
    );

    let v = a.json(&["persona", "--affirmed"]);
    assert_eq!(v["count"], 2, "counts still describe the whole profile");
    let listed = v["traits"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "an unset status is not agreement:\n{v:#}");
    assert_eq!(listed[0]["id"], "agreed");
    assert_eq!(v["by_status"]["proposed"], 1);
    assert_eq!(v["by_status"]["affirmed"], 1);
}

#[test]
fn a_capped_unmined_list_publishes_its_true_total() {
    // The archive's rule for every capped list. A short list that does not say
    // it is short reads as the whole corpus already being read.
    let a = archive();
    for i in 0..15 {
        let f = a.path(&format!("src{i}.md"));
        std::fs::write(&f, format!("Essay {i}.\n")).unwrap();
        assert_eq!(
            a.code(&[
                "ingest",
                &f.display().to_string(),
                "-d",
                "philosophy",
                "-o",
                "authored",
                "--as",
                &format!("src{i}.md"),
            ]),
            0
        );
    }
    let v = a.json(&["persona"]);
    assert_eq!(v["coverage"]["unmined_count"], 16);
    let sample = v["coverage"]["unmined"].as_array().unwrap();
    assert!(
        sample.len() < 16,
        "the sample should be capped, or this proves nothing"
    );

    let out = a.run(&["persona"]);
    assert!(
        out.contains(&format!("and {} more", 16 - sample.len())),
        "human output must say the list is short:\n{out}"
    );
}

#[test]
fn an_uncategorised_trait_still_appears_in_the_profile() {
    // Grouping by the published kinds means anything outside them prints
    // nowhere. `lint` reports it, but the one command meant to show the whole
    // profile would be the one place it is invisible.
    let a = archive();
    a.write(
        "persona/odd.md",
        &trait_file("odd", &["raw/philosophy/mine.md"]).replace("kind: belief", "kind: nonsense"),
    );
    let out = a.run(&["persona"]);
    assert!(
        out.contains("persona/odd.md"),
        "a trait with an invalid kind must not vanish:\n{out}"
    );
}
