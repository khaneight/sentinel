//! End-to-end coverage for locating the archive.
//!
//! These drive the real binary because archive resolution is precisely the
//! layer that unit tests stub out — the bug this suite exists to prevent is a
//! hardcoded path that only works on one machine.

use std::path::Path;
use std::process::{Command, Output};

fn sentinel() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sentinel"));
    // Inherited configuration would make these tests depend on the developer's
    // machine, which is the exact failure mode under test.
    cmd.env_remove("SENTINEL_ARCHIVE");
    cmd.env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml");
    cmd
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn init_archive(root: &Path) -> Output {
    let output = sentinel()
        .args(["init", &root.display().to_string()])
        .output()
        .expect("failed to run sentinel");
    assert!(output.status.success(), "init failed: {}", stderr(&output));
    output
}

#[test]
fn init_creates_the_archive_at_an_explicit_path() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("archive");
    init_archive(&root);

    for dir in ["raw", "wiki", "index", "meta", "templates"] {
        assert!(root.join(dir).is_dir(), "missing {dir}/");
    }
    assert!(root.join("meta/manifest.json").is_file());
    assert!(root.join("meta/link-graph.json").is_file());
    assert!(root.join("SUMMARY.md").is_file());
}

#[test]
fn init_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("archive");
    init_archive(&root);
    std::fs::write(root.join("wiki/keep-me.md"), "not clobbered").unwrap();
    init_archive(&root);

    assert_eq!(
        std::fs::read_to_string(root.join("wiki/keep-me.md")).unwrap(),
        "not clobbered"
    );
}

#[test]
fn init_refuses_to_scatter_an_archive_across_a_populated_directory() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("important.txt"), "mine").unwrap();

    let output = sentinel()
        .arg("init")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(stderr(&output).contains("Refusing"), "{}", stderr(&output));
    assert!(!tmp.path().join("wiki").exists());
}

#[test]
fn init_falls_back_to_an_empty_working_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let output = sentinel()
        .arg("init")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(tmp.path().join("meta/manifest.json").is_file());
}

#[test]
fn the_env_var_selects_the_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("archive");
    init_archive(&root);

    let output = sentinel()
        .arg("status")
        .env("SENTINEL_ARCHIVE", &root)
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("Archive Status"));
}

#[test]
fn the_archive_flag_outranks_the_env_var() {
    let tmp = tempfile::tempdir().unwrap();
    let wanted = tmp.path().join("wanted");
    init_archive(&wanted);

    let output = sentinel()
        .args(["--archive", &wanted.display().to_string(), "config"])
        .env("SENTINEL_ARCHIVE", tmp.path().join("ignored"))
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains(&wanted.display().to_string()), "{out}");
    assert!(out.contains("--archive flag"), "{out}");
}

#[test]
fn commands_discover_the_archive_from_a_nested_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("archive");
    init_archive(&root);
    let nested = root.join("wiki/philosophy");

    let output = sentinel()
        .arg("status")
        .current_dir(&nested)
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("Archive Status"));
}

#[test]
fn a_missing_archive_produces_actionable_guidance() {
    let tmp = tempfile::tempdir().unwrap();

    let output = sentinel()
        .arg("status")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("--archive"), "{err}");
    assert!(err.contains("SENTINEL_ARCHIVE"), "{err}");
    assert!(err.contains("sentinel init"), "{err}");
}

#[test]
fn config_reports_a_missing_archive_instead_of_crashing_unhelpfully() {
    let tmp = tempfile::tempdir().unwrap();

    let output = sentinel()
        .arg("config")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let out = stdout(&output);
    assert!(out.contains("Sentinel Configuration"), "{out}");
    assert!(out.contains("No archive resolved"), "{out}");
}

#[test]
fn set_default_records_the_archive_in_the_config_file() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("archive");
    let config = tmp.path().join("config.toml");

    let output = sentinel()
        .args(["init", &root.display().to_string(), "--set-default"])
        .env("SENTINEL_CONFIG", &config)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));

    let written = std::fs::read_to_string(&config).unwrap();
    assert!(written.contains(&root.display().to_string()), "{written}");

    // A later command run from an unrelated directory picks the default up.
    let elsewhere = tempfile::tempdir().unwrap();
    let output = sentinel()
        .arg("config")
        .env("SENTINEL_CONFIG", &config)
        .current_dir(elsewhere.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("config file"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn the_full_bookkeeping_loop_runs_against_a_fresh_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("archive");
    init_archive(&root);

    let source = tmp.path().join("meditations.md");
    std::fs::write(&source, "# Meditations\n\nOn the shortness of life.\n").unwrap();

    for args in [
        vec!["ingest", &source.display().to_string(), "-d", "philosophy"],
        vec!["sync"],
        vec!["index"],
        vec!["lint"],
        vec!["uncompiled"],
        vec!["graph"],
        vec!["search", "shortness"],
        vec!["status"],
    ] {
        let output = sentinel()
            .args(&args)
            .env("SENTINEL_ARCHIVE", &root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`sentinel {}` failed: {}",
            args.join(" "),
            stderr(&output)
        );
    }

    assert!(root.join("raw/philosophy/meditations.md").is_file());
    let manifest = std::fs::read_to_string(root.join("meta/manifest.json")).unwrap();
    assert!(
        manifest.contains("raw/philosophy/meditations.md"),
        "{manifest}"
    );
}

#[test]
fn the_readme_first_run_works_verbatim() {
    // The Usage section used to open with a bare `sentinel init`, which refuses
    // in any non-empty directory — contradicting the README's own "Where your
    // archive lives" section and failing for anyone who runs it in their home
    // directory or a project. This pins the corrected sequence.
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("archive");
    let config = tmp.path().join("config.toml");
    std::fs::write(tmp.path().join("essay.md"), "# Essay\n\nOn virtue.\n").unwrap();

    // 1. sentinel init <PATH> --set-default
    let init = sentinel()
        .args(["init", &archive.display().to_string(), "--set-default"])
        .env("SENTINEL_CONFIG", &config)
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(init.status.success(), "{}", stderr(&init));

    // 2. Every later command finds it from an unrelated directory, with no
    //    --archive and no SENTINEL_ARCHIVE — which is what --set-default buys.
    let elsewhere = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let out = sentinel()
            .args(args)
            .env("SENTINEL_CONFIG", &config)
            .current_dir(elsewhere.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "`sentinel {}` failed: {}",
            args.join(" "),
            stderr(&out)
        );
        stdout(&out)
    };

    run(&[
        "ingest",
        &tmp.path().join("essay.md").display().to_string(),
        "-d",
        "philosophy",
    ]);
    run(&["sync"]);
    run(&["status"]);
    let next = run(&["next"]);
    assert!(next.contains("compile"), "{next}");
}
