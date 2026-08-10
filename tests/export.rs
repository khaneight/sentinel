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
        vec!["wiki/philosophy/alpha.md", "wiki/philosophy/beta.md"],
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

    let alpha = std::fs::read_to_string(out.join("wiki/philosophy/alpha.md")).unwrap();
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
    let text = std::fs::read_to_string(out.join("wiki/philosophy/published.md")).unwrap();

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
        vec!["wiki/philosophy/secret.md"],
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
