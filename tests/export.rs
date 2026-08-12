//! `sentinel export` — what leaves the archive.
//!
//! Publishing is the one operation whose mistakes are not recoverable by
//! re-running it: a draft that went out has been read, and a licence question
//! answered wrongly stays answered. So the checks here are mostly about what
//! the command *refuses* to do.

mod common;

use common::Archive;

/// Two stable articles that link to each other and to an unpublished draft.
fn archive() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "source");
    a.run(&["sync"]);
    let article = |title: &str, status: &str, body: &str| {
        format!(
            "---\ntitle: {title}\ndomain: philosophy\norigin: authored\nstatus: {status}\n\
             tags: [t]\nsources: [raw/philosophy/s.md]\n---\n\n{body}\n"
        )
    };
    a.write(
        "wiki/philosophy/alpha.md",
        &article("Alpha", "stable", "Links to [[beta]] and to [[secret]]."),
    );
    a.write(
        "wiki/philosophy/beta.md",
        &article(
            "Beta",
            "stable",
            "Back to [[alpha]]. Also [[never-written]].",
        ),
    );
    a.write(
        "wiki/philosophy/secret.md",
        &article("Secret", "draft", "Not finished."),
    );
    a.run(&["index"]);
    a
}

fn exported(a: &Archive) -> Vec<String> {
    let dir = a.path("out");
    let mut found = Vec::new();
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                found.push(p.strip_prefix(&dir).unwrap().display().to_string());
            }
        }
    }
    found.sort();
    found
}

#[test]
fn only_stable_articles_are_published_by_default() {
    // `status` exists to mark what is not finished. Defaulting to anything
    // looser would make the field decorative at the one moment it matters.
    let a = archive();
    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string()]);

    assert_eq!(
        exported(&a),
        vec!["philosophy/alpha.md", "philosophy/beta.md"],
        "a draft was published"
    );
}

#[test]
fn the_export_contains_no_link_a_reader_cannot_follow() {
    // This is the property publishing needs and the archive deliberately does
    // not have. `broken-link` is a warning here because the compile loop names
    // concepts before writing them — that is the growth signal. On a website it
    // is a dead end.
    let a = archive();
    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string()]);

    let published: std::collections::HashSet<String> = exported(&a)
        .iter()
        .filter_map(|p| {
            std::path::Path::new(p)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
        })
        .collect();

    for rel in exported(&a) {
        let text = std::fs::read_to_string(out.join(&rel)).unwrap();
        let mut rest = text.as_str();
        while let Some(i) = rest.find("[[") {
            let Some(j) = rest[i..].find("]]") else { break };
            let target = &rest[i + 2..i + j];
            let target = target.split('|').next().unwrap();
            assert!(
                published.contains(target),
                "{rel} links to `{target}`, which was not published"
            );
            rest = &rest[i + j + 2..];
        }
    }
}

#[test]
fn a_defused_link_keeps_the_words_it_was_written_with() {
    // Deleting the link text would change what the sentence says. The concept
    // was mentioned deliberately; only the link is dropped.
    let a = archive();
    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string()]);

    let alpha = std::fs::read_to_string(out.join("philosophy/alpha.md")).unwrap();
    assert!(
        alpha.contains("and to Secret."),
        "the prose lost its words rather than its link:\n{alpha}"
    );
    assert!(
        alpha.contains("[[beta]]"),
        "a link to a published article was defused anyway:\n{alpha}"
    );
}

#[test]
fn a_defused_link_reads_as_prose_not_as_a_filename() {
    // `[[dichotomy-of-control]]` left as "dichotomy-of-control" puts a slug in
    // a sentence a reader is meant to read. The archive knows the article's
    // title even when it is not published.
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "source");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/published.md",
        "---\ntitle: Published\ndomain: philosophy\norigin: authored\nstatus: stable\n\
         tags: [t]\nsources: [raw/philosophy/s.md]\n---\n\n\
         Rests on [[held-back]], and on [[never-written]], and on [[held-back|its own words]].\n",
    );
    a.write(
        "wiki/philosophy/held-back.md",
        "---\ntitle: Dichotomy of Control\ndomain: philosophy\norigin: authored\n\
         status: draft\ntags: [t]\nsources: [raw/philosophy/s.md]\n---\n\nDraft.\n",
    );
    a.run(&["index"]);

    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string()]);
    let text = std::fs::read_to_string(out.join("philosophy/published.md")).unwrap();

    assert!(
        text.contains("Rests on Dichotomy of Control,"),
        "an unpublished article should be named by its title:\n{text}"
    );
    assert!(
        text.contains("its own words"),
        "an explicit [[slug|Label]] was written for the reader; keep it:\n{text}"
    );
    assert!(
        text.contains("on never-written,"),
        "a concept with no article has no title to use; keep the target:\n{text}"
    );
    assert!(!text.contains("[["), "no links should survive:\n{text}");
}

#[test]
fn what_was_held_back_is_reported_with_a_true_total() {
    let a = archive();
    let out = a.path("out");
    let v = a.json(&["export", "--out", &out.display().to_string()]);

    assert_eq!(v["published"], 2, "{v}");
    assert_eq!(v["excluded_count"], 1, "{v}");
    assert_eq!(v["links_defused"], 2, "{v}");
    assert!(
        v["excluded"][0]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("draft")),
        "the reason must be actionable:\n{v}"
    );
}

#[test]
fn dry_run_writes_nothing() {
    let a = archive();
    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string(), "--dry-run"]);
    assert!(!out.exists(), "--dry-run created {}", out.display());
}

#[test]
fn it_refuses_to_export_into_a_directory_index_rebuilds() {
    // An export under wiki/ would be walked as article content on the next
    // rebuild: the published copy becomes part of the archive, and then part of
    // the next export.
    let a = archive();
    for dir in ["wiki/pub", "raw/pub", "index/pub"] {
        let out = a.path(dir);
        let result = a.output(&["export", "--out", &out.display().to_string()]);
        assert!(
            !result.status.success(),
            "exported into {dir}, which `index` walks"
        );
        assert!(
            common::stderr(&result).contains("Refusing to export"),
            "{}",
            common::stderr(&result)
        );
    }
}

#[test]
fn status_selection_is_explicit_and_overridable() {
    let a = archive();
    let out = a.path("out");
    a.run(&[
        "export",
        "--out",
        &out.display().to_string(),
        "--status",
        "draft",
    ]);
    assert_eq!(
        exported(&a),
        vec!["philosophy/secret.md"],
        "--status did not select what was asked for"
    );
}

#[test]
fn include_drafts_publishes_everything_readable() {
    let a = archive();
    let out = a.path("out");
    a.run(&[
        "export",
        "--out",
        &out.display().to_string(),
        "--include-drafts",
    ]);
    assert_eq!(exported(&a).len(), 3, "{:?}", exported(&a));
}

#[test]
fn an_archive_with_nothing_publishable_says_so_rather_than_writing_an_empty_site() {
    // The corpus this was built against is 27 articles and every one is a
    // draft. Silently producing an empty directory would look like success.
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "x");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/one.md",
        "---\ntitle: One\ndomain: philosophy\norigin: authored\nstatus: draft\n\
         tags: [t]\nsources: [raw/philosophy/s.md]\n---\n\nBody.\n",
    );

    let out = a.path("out");
    let text = a.run(&["export", "--out", &out.display().to_string()]);
    assert!(text.contains("Nothing exported"), "{text}");
    assert!(
        text.contains("stable"),
        "it must say what would qualify:\n{text}"
    );
}

#[cfg(unix)]
#[test]
fn it_refuses_on_a_partial_view() {
    // Publishing less than the archive holds is indistinguishable, to a reader,
    // from an article that was never written.
    let a = archive();
    let path = a.path("wiki/philosophy/alpha.md");
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o000);
    }
    std::fs::set_permissions(&path, perms).unwrap();
    let blocked = std::fs::read_to_string(&path).is_err();

    let out = a.path("out");
    let result = a.output(&["export", "--out", &out.display().to_string()]);

    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o644);
    }
    std::fs::set_permissions(&path, perms).unwrap();

    assert!(blocked, "fixture still readable — probably running as root");
    assert!(!result.status.success(), "exported from a partial view");
    assert!(!out.exists(), "wrote a partial site before refusing");
}

#[test]
fn an_unpublished_article_is_reported_as_still_readable() {
    // Export is not incremental. An article unpublished since the last run sits
    // in the destination, still served — and the likeliest reason to unpublish
    // something is that it should not be public. Silence here is the dangerous
    // direction of wrong.
    let a = archive();
    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string()]);
    assert!(out.join("philosophy/beta.md").exists());

    let beta = a.path("wiki/philosophy/beta.md");
    let text = std::fs::read_to_string(&beta).unwrap();
    std::fs::write(&beta, text.replace("status: stable", "status: draft")).unwrap();
    a.run(&["index"]);

    let v = a.json(&["export", "--out", &out.display().to_string()]);
    assert_eq!(v["published"], 1, "{v}");
    assert_eq!(
        v["stale"][0], "philosophy/beta.md",
        "the unpublished article was not reported:\n{v}"
    );
    assert_eq!(v["stale_removed"], false, "nothing should be deleted:\n{v}");
    assert!(
        out.join("philosophy/beta.md").exists(),
        "export deleted a file without being asked"
    );

    let human = a.run(&["export", "--out", &out.display().to_string()]);
    assert!(
        human.contains("--clean"),
        "it must say how to fix it:\n{human}"
    );
}

#[test]
fn clean_removes_exactly_the_stale_files() {
    let a = archive();
    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string()]);

    let beta = a.path("wiki/philosophy/beta.md");
    let text = std::fs::read_to_string(&beta).unwrap();
    std::fs::write(&beta, text.replace("status: stable", "status: draft")).unwrap();
    a.run(&["index"]);

    a.run(&["export", "--out", &out.display().to_string(), "--clean"]);
    assert!(
        !out.join("philosophy/beta.md").exists(),
        "--clean left the stale file"
    );
    assert!(
        out.join("philosophy/alpha.md").exists(),
        "--clean removed a file that is still published"
    );
}

#[test]
fn dry_run_does_not_clean_either() {
    // `--dry-run` means nothing happens. A flag combination that deletes during
    // a rehearsal would be the worst possible surprise.
    let a = archive();
    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string()]);

    let beta = a.path("wiki/philosophy/beta.md");
    let text = std::fs::read_to_string(&beta).unwrap();
    std::fs::write(&beta, text.replace("status: stable", "status: draft")).unwrap();
    a.run(&["index"]);

    a.run(&[
        "export",
        "--out",
        &out.display().to_string(),
        "--clean",
        "--dry-run",
    ]);
    assert!(
        out.join("philosophy/beta.md").exists(),
        "--dry-run --clean deleted a file"
    );
}

#[test]
fn a_status_list_that_names_nothing_is_refused() {
    // `--status ""` selected nothing and reported "Publishable statuses: ." —
    // indistinguishable from an archive where nothing happens to qualify.
    let a = archive();
    let out = a.path("out");
    let result = a.output(&[
        "export",
        "--out",
        &out.display().to_string(),
        "--status",
        "",
    ]);
    assert!(!result.status.success(), "empty --status was accepted");
    let err = common::stderr(&result);
    assert!(
        err.contains("stable"),
        "it must name the valid values:\n{err}"
    );
}

#[test]
fn the_two_status_selectors_cannot_both_be_given() {
    // `--status` silently won and `--include-drafts` did nothing. A flag that
    // parses and is ignored is worse than one that does not exist.
    let a = archive();
    let out = a.path("out");
    let result = a.output(&[
        "export",
        "--out",
        &out.display().to_string(),
        "--status",
        "stable",
        "--include-drafts",
    ]);
    assert!(!result.status.success(), "both selectors were accepted");
}

#[test]
fn the_export_drops_the_wiki_prefix_but_keeps_the_domain() {
    // A site generator turns directories into URL segments. `wiki/` is how the
    // archive separates articles from raw sources; once only articles are being
    // published it means nothing, and it appeared in every URL. The domain does
    // group the site the way it groups the wiki, so it stays.
    let a = archive();
    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string()]);

    assert!(
        out.join("philosophy/alpha.md").exists(),
        "{:?}",
        exported(&a)
    );
    assert!(
        !out.join("wiki").exists(),
        "the wiki/ prefix would be a segment in every URL"
    );
}

#[test]
fn flat_drops_the_domain_too() {
    let a = archive();
    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string(), "--flat"]);

    assert_eq!(
        exported(&a),
        vec!["alpha.md", "beta.md"],
        "--flat should write at the top level"
    );
}

#[test]
fn flat_refuses_rather_than_overwriting_a_collision() {
    // `duplicate-slug` is a lint error, so this should not arise — but "should
    // not arise" is how an export silently loses an article.
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "text");
    a.run(&["sync"]);
    for domain in ["philosophy", "coding"] {
        a.write(
            &format!("wiki/{domain}/ethics.md"),
            &format!(
                "---\ntitle: {domain} ethics\ndomain: {domain}\norigin: authored\n\
                 status: stable\ntags: [t]\nsources: [raw/philosophy/s.md]\n---\n\nBody.\n"
            ),
        );
    }

    let out = a.path("out");
    let result = a.output(&["export", "--out", &out.display().to_string(), "--flat"]);
    assert!(!result.status.success(), "a collision was written anyway");
    let err = common::stderr(&result);
    assert!(
        err.contains("--flat"),
        "the refusal must name the cause:\n{err}"
    );

    // Without --flat the two are distinct and both publish.
    assert_eq!(a.code(&["export", "--out", &out.display().to_string()]), 0);
    assert_eq!(exported(&a).len(), 2);
}

#[test]
fn stale_detection_still_works_with_the_new_shape() {
    // Stale files are compared against the paths this export *would* write, so
    // reshaping them is exactly the change that could break it.
    let a = archive();
    let out = a.path("out");
    a.run(&["export", "--out", &out.display().to_string()]);

    let beta = a.path("wiki/philosophy/beta.md");
    let text = std::fs::read_to_string(&beta).unwrap();
    std::fs::write(&beta, text.replace("status: stable", "status: draft")).unwrap();
    a.run(&["index"]);

    let v = a.json(&["export", "--out", &out.display().to_string()]);
    assert_eq!(v["stale"][0], "philosophy/beta.md", "{v}");
}

// ---------------------------------------------------------------------------
// The data bundle a front end consumes
// ---------------------------------------------------------------------------

#[test]
fn the_bundle_describes_only_what_was_published() {
    // The graph is served beside the site. An edge to an article nobody can
    // open is not a connection a reader can follow, and a node for a draft
    // would leak its existence.
    let a = archive();
    let out = a.path("out");
    let data = a.path("data.json");
    a.run(&[
        "export",
        "--out",
        &out.display().to_string(),
        "--data",
        &data.display().to_string(),
    ]);

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&data).unwrap()).unwrap();
    let slugs: Vec<&str> = v["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, vec!["alpha", "beta"], "a draft reached the bundle");

    for e in v["edges"].as_array().unwrap() {
        for end in ["from", "to"] {
            assert!(
                slugs.contains(&e[end].as_str().unwrap()),
                "edge {end} points outside the published set: {e}"
            );
        }
    }
    assert!(v["nodes"][0]["inbound"].is_number(), "{v}");
}

#[test]
fn the_bundle_carries_the_progress_history() {
    let a = archive();
    a.run(&["index"]);
    let out = a.path("out");
    let data = a.path("data.json");
    a.run(&[
        "export",
        "--out",
        &out.display().to_string(),
        "--data",
        &data.display().to_string(),
    ]);

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&data).unwrap()).unwrap();
    let progress = v["progress"].as_array().unwrap();
    assert!(
        !progress.is_empty(),
        "index should have recorded a snapshot"
    );
    for field in ["at", "wiki_articles", "links", "wanted", "orphans"] {
        assert!(progress[0].get(field).is_some(), "missing {field}: {v}");
    }
    assert_eq!(v["unreadable_snapshots"], 0);
}

#[test]
fn no_bundle_is_written_without_the_flag_or_during_a_dry_run() {
    let a = archive();
    let out = a.path("out");
    let data = a.path("data.json");

    a.run(&["export", "--out", &out.display().to_string()]);
    assert!(!data.exists(), "a bundle appeared without --data");

    a.run(&[
        "export",
        "--out",
        &out.display().to_string(),
        "--data",
        &data.display().to_string(),
        "--dry-run",
    ]);
    assert!(!data.exists(), "--dry-run wrote a bundle");
}

#[test]
fn a_rebuild_that_changes_nothing_adds_no_history() {
    // Otherwise the file records how often `index` ran rather than what the
    // archive did — the same rule as `write_if_changed`.
    let a = archive();
    a.run(&["index"]);
    let path = a.path("meta/progress.jsonl");
    let lines = |p: &std::path::Path| std::fs::read_to_string(p).unwrap().lines().count();
    let before = lines(&path);

    a.run(&["index"]);
    a.run(&["index"]);
    assert_eq!(
        lines(&path),
        before,
        "an unchanged rebuild added a snapshot"
    );

    a.write(
        "wiki/philosophy/late.md",
        "---\ntitle: Late\ndomain: philosophy\norigin: authored\nstatus: stable\n\
         tags: [t]\nsources: [raw/philosophy/s.md]\n---\n\nBody.\n",
    );
    a.run(&["index"]);
    assert_eq!(lines(&path), before + 1, "a real change was not recorded");
}
