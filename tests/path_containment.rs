//! Nothing the tool writes may land outside the archive root.
//!
//! `sentinel ingest -d /tmp/x` used to write to `/tmp/x`, because `Path::join`
//! discards its base when the argument is absolute. `-d ../..` wrote above the
//! root. `mv`'s destination check was `starts_with("raw/")`, a string test that
//! `raw/../../x.md` satisfies. All three exited 0 and recorded the traversal in
//! the manifest verbatim.

mod common;

use common::Archive;
use std::path::Path;

/// Every way a caller can name a destination, and an escape for each.
///
/// One table rather than one test per command: the three surfaces had the same
/// defect and two different half-checks between them, and a fix aimed at
/// whichever one I noticed first would have left the others open.
const ESCAPES: &[(&str, &[&str])] = &[
    (
        "ingest -d absolute",
        &["ingest", "SRC", "-d", "/tmp/sentinel-escape-probe"],
    ),
    ("ingest -d traversal", &["ingest", "SRC", "-d", "../.."]),
    ("ingest -d nested", &["ingest", "SRC", "-d", "a/b"]),
    (
        "ingest --as traversal",
        &["ingest", "SRC", "-d", "philosophy", "--as", "../../p.md"],
    ),
    (
        "ingest --as absolute",
        &["ingest", "SRC", "-d", "philosophy", "--as", "/tmp/p.md"],
    ),
    (
        "mv to traversal",
        &["mv", "raw/philosophy/doc.md", "raw/../../esc.md"],
    ),
    (
        "mv to absolute",
        &["mv", "raw/philosophy/doc.md", "/tmp/esc.md"],
    ),
    ("mv to dotdot", &["mv", "raw/philosophy/doc.md", ".."]),
];

fn seeded() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/doc.md", "content");
    a.run(&["sync"]);
    a
}

/// Files under `dir`, so a test can prove nothing new appeared.
fn snapshot(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.display().to_string());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn no_argument_can_make_the_tool_write_outside_the_archive() {
    for (name, args) in ESCAPES {
        let a = seeded();
        // The archive lives inside a tempdir; watching the parent catches an
        // escape of one level, which is all any of these needed.
        let outside = a.root.parent().unwrap().to_path_buf();
        let before = snapshot(&outside);

        let src = a.path("source.md");
        std::fs::write(&src, "x").unwrap();
        let args: Vec<String> = args
            .iter()
            .map(|s| {
                if *s == "SRC" {
                    src.display().to_string()
                } else {
                    s.to_string()
                }
            })
            .collect();
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();

        let out = a.output(&argv);
        assert!(
            !out.status.success(),
            "{name}: succeeded; it must refuse\nstdout: {}",
            common::stdout(&out)
        );

        // The parent contains the archive, so anything inside it is ordinary
        // work and only paths outside the root count as an escape.
        let inside = format!("{}/", a.root.display());
        let after = snapshot(&outside);
        let new: Vec<&String> = after
            .iter()
            .filter(|f| !before.contains(f) && !f.starts_with(&inside))
            .collect();
        assert!(new.is_empty(), "{name}: wrote outside the archive: {new:?}");
    }
}

#[test]
fn the_refusal_says_what_was_wrong_with_the_path() {
    // "invalid argument" would leave a caller guessing which of two arguments
    // it meant, and an agent retrying the same thing.
    let a = seeded();
    let src = a.path("source.md");
    std::fs::write(&src, "x").unwrap();

    let out = a.output(&["ingest", &src.display().to_string(), "-d", "../.."]);
    let err = common::stderr(&out);
    assert!(err.contains("domain"), "must name the argument:\n{err}");
    assert!(
        err.contains("separator") || err.contains("outside") || err.contains("single plain name"),
        "must say what is wrong with it:\n{err}"
    );
}

#[test]
fn legitimate_paths_are_not_caught_by_any_of_this() {
    // A containment check that also blocks ordinary work is a worse bug than
    // the one it fixes, because it stops the loop rather than corrupting it.
    let a = seeded();
    let src = a.path("source.md");
    std::fs::write(&src, "x").unwrap();

    assert_eq!(
        a.code(&[
            "ingest",
            &src.display().to_string(),
            "-d",
            "philosophy",
            "-t",
            "Good"
        ]),
        0
    );
    assert_eq!(
        a.code(&["mv", "raw/philosophy/doc.md", "raw/coding/doc.md"]),
        0
    );
    assert_eq!(a.code(&["mv", "raw/coding/doc.md", "renamed.md"]), 0);
    // An interior `..` that stays inside the archive is legal and resolves.
    assert_eq!(
        a.code(&[
            "mv",
            "raw/coding/renamed.md",
            "raw/coding/../philosophy/x.md"
        ]),
        0
    );
    common::assert_exists(&a.path("raw/philosophy/x.md"));
}

#[test]
fn a_manifest_poisoned_by_an_older_build_is_not_acted_on() {
    // Validating new input alone would leave every archive already carrying a
    // traversal exploitable: `rm` deletes `root.join(key)`.
    let a = seeded();
    let outside = a.root.parent().unwrap().join("victim.md");
    std::fs::write(&outside, "must survive").unwrap();

    let manifest_path = a.path("meta/manifest.json");
    let text = std::fs::read_to_string(&manifest_path).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    let entry = manifest["entries"]["raw/philosophy/doc.md"].clone();
    manifest["entries"]["raw/../../victim.md"] = entry;
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();

    assert_ne!(
        a.code(&["rm", "raw/../../victim.md", "--force"]),
        0,
        "rm acted on a traversal from the manifest"
    );
    assert!(outside.exists(), "rm deleted a file outside the archive");

    assert_ne!(
        a.code(&["mv", "raw/../../victim.md", "raw/philosophy/v.md"]),
        0,
        "mv acted on a traversal from the manifest"
    );
    assert!(outside.exists(), "mv moved a file outside the archive");
}

#[test]
fn every_filesystem_mutation_under_the_root_goes_through_the_check() {
    // The three surfaces were found by trying them. This is the guard that does
    // not depend on my having thought of a fourth: a new `root.join(...)` fed
    // to a mutating call fails here until it is routed or justified.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    let mutators = ["remove_file(", "rename(", "fs::copy(", "create_dir_all("];

    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (i, line) in text.lines().enumerate() {
            let mutating = mutators.iter().any(|m| line.contains(m));
            if mutating && line.contains("root.join(") {
                offenders.push(format!(
                    "{}:{}: {}",
                    path.file_name().unwrap().to_string_lossy(),
                    i + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these mutate a path joined straight onto the archive root; use \
         `paths::resolve_in_archive` so a traversal cannot reach the \
         filesystem:\n  {}",
        offenders.join("\n  ")
    );
}
