//! Regression coverage for defects that corrupted state or crashed outright.

use std::path::Path;
use std::process::{Command, Output};

fn sentinel(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sentinel"));
    cmd.env("SENTINEL_ARCHIVE", root);
    cmd.env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml");
    cmd
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn archive() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("archive");
    let output = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .args(["init", &root.display().to_string()])
        .env_remove("SENTINEL_ARCHIVE")
        .env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml")
        .output()
        .unwrap();
    assert!(output.status.success());
    (tmp, root)
}

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
fn search_survives_long_multibyte_lines() {
    let (_tmp, root) = archive();
    // Byte 100 of this line lands inside a multibyte character. The previous
    // implementation sliced `&line[..100]` and panicked.
    let line = "—".repeat(200);
    write(
        &root.join("wiki/philosophy/dashes.md"),
        &format!("---\ntitle: Dashes\n---\n\nvirtue {line}\n"),
    );

    let output = sentinel(&root).args(["search", "virtue"]).output().unwrap();

    assert!(
        output.status.success(),
        "search panicked: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout(&output).contains("dashes.md"));
}

#[test]
fn lint_names_invalid_yaml_instead_of_inventing_missing_fields() {
    let (_tmp, root) = archive();
    write(
        &root.join("wiki/philosophy/broken.md"),
        "---\ntitle: [unterminated\n---\n\nBody\n",
    );

    let output = sentinel(&root).arg("lint").output().unwrap();
    let out = stdout(&output);

    assert!(out.contains("invalid frontmatter"), "{out}");
    assert!(
        !out.contains("missing 'title'"),
        "a parse failure must not be reported as five absent fields:\n{out}"
    );
}

#[test]
fn lint_flags_slugs_that_collide_across_domains() {
    let (_tmp, root) = archive();
    let body = |domain| {
        format!(
            "---\ntitle: Ethics\ndomain: {domain}\norigin: authored\ntags: [x]\nsources: [raw/x.md]\n---\n\nBody\n"
        )
    };
    write(&root.join("wiki/philosophy/ethics.md"), &body("philosophy"));
    write(&root.join("wiki/coding/ethics.md"), &body("coding"));

    let output = sentinel(&root).arg("lint").output().unwrap();
    let out = stdout(&output);

    assert!(out.contains("duplicate slug 'ethics'"), "{out}");
    assert!(out.contains("wiki/philosophy/ethics.md"), "{out}");
    assert!(out.contains("wiki/coding/ethics.md"), "{out}");
}

#[test]
fn sync_drops_entries_whose_source_file_is_gone() {
    let (_tmp, root) = archive();
    let doc = root.join("raw/philosophy/meditations.md");
    write(&doc, "notes");

    assert!(
        sentinel(&root)
            .arg("sync")
            .output()
            .unwrap()
            .status
            .success()
    );
    let manifest = std::fs::read_to_string(root.join("meta/manifest.json")).unwrap();
    assert!(manifest.contains("meditations.md"));

    std::fs::remove_file(&doc).unwrap();
    let output = sentinel(&root).arg("sync").output().unwrap();
    assert!(output.status.success());

    let manifest = std::fs::read_to_string(root.join("meta/manifest.json")).unwrap();
    assert!(
        !manifest.contains("meditations.md"),
        "a deleted source must not stay 'uncompiled' forever:\n{manifest}"
    );
}

#[test]
fn sync_ignores_hidden_files() {
    let (_tmp, root) = archive();
    write(&root.join("raw/philosophy/.DS_Store"), "junk");
    write(&root.join("raw/philosophy/real.md"), "notes");

    assert!(
        sentinel(&root)
            .arg("sync")
            .output()
            .unwrap()
            .status
            .success()
    );

    let manifest = std::fs::read_to_string(root.join("meta/manifest.json")).unwrap();
    assert!(manifest.contains("real.md"), "{manifest}");
    assert!(!manifest.contains("DS_Store"), "{manifest}");
}

#[test]
fn sync_dry_run_writes_nothing() {
    let (_tmp, root) = archive();
    write(&root.join("raw/philosophy/new.md"), "notes");
    let before = std::fs::read_to_string(root.join("meta/manifest.json")).unwrap();

    let output = sentinel(&root)
        .args(["sync", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(stdout(&output).contains("Dry run"), "{}", stdout(&output));

    let after = std::fs::read_to_string(root.join("meta/manifest.json")).unwrap();
    assert_eq!(before, after);
}

#[test]
fn index_handles_articles_with_unicode_titles() {
    let (_tmp, root) = archive();
    write(
        &root.join("wiki/philosophy/ethika.md"),
        "---\ntitle: Ἠθικά — “virtue”\ndomain: philosophy\n---\n\nSee [[stoicism]].\n",
    );

    let output = sentinel(&root).arg("index").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let master = std::fs::read_to_string(root.join("index/_master.md")).unwrap();
    assert!(master.contains("Ἠθικά — “virtue”"), "{master}");
}
