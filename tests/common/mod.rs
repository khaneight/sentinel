// Each integration test binary compiles this module separately and uses only
// the helpers it needs, so unused-warnings here are structural rather than real.
#![allow(dead_code)]

//! Shared harness for integration tests.
//!
//! Every helper scrubs `SENTINEL_ARCHIVE` and `SENTINEL_CONFIG` from the child
//! environment. Without that a test could silently pass by operating on the
//! developer's real archive instead of its own temp directory.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A throwaway archive. Deleted when dropped.
pub struct Archive {
    _tmp: tempfile::TempDir,
    pub root: PathBuf,
}

impl Archive {
    /// Create and initialize a fresh archive.
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("archive");
        let output = bare()
            .args(["init", &root.display().to_string()])
            .output()
            .expect("failed to run sentinel");
        assert!(
            output.status.success(),
            "init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self { _tmp: tmp, root }
    }

    /// A `sentinel` invocation pointed at this archive.
    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = bare();
        cmd.env("SENTINEL_ARCHIVE", &self.root);
        cmd.args(args);
        cmd
    }

    /// Run a command and require that it succeeds.
    pub fn run(&self, args: &[&str]) -> String {
        let output = self.cmd(args).output().unwrap();
        assert!(
            output.status.success(),
            "`sentinel {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Run a command without requiring success — for exit-code assertions.
    pub fn output(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    /// The exit code of a command.
    pub fn code(&self, args: &[&str]) -> i32 {
        self.output(args).status.code().unwrap_or(-1)
    }

    /// Run a command and parse its `--json` output.
    pub fn json(&self, args: &[&str]) -> serde_json::Value {
        let mut with_json = args.to_vec();
        with_json.push("--json");
        let output = self.output(&with_json);
        let text = stdout(&output);
        serde_json::from_str(&text).unwrap_or_else(|e| {
            panic!(
                "`sentinel {}` did not emit JSON ({e}):\n{text}",
                with_json.join(" ")
            )
        })
    }

    /// Write a file inside the archive, creating parent directories.
    pub fn write(&self, rel_path: &str, contents: &str) -> PathBuf {
        let path = self.root.join(rel_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// Read a file inside the archive.
    pub fn read(&self, rel_path: &str) -> String {
        std::fs::read_to_string(self.root.join(rel_path)).unwrap()
    }

    pub fn path(&self, rel_path: &str) -> PathBuf {
        self.root.join(rel_path)
    }
}

/// A `sentinel` invocation with no archive configured.
pub fn bare() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sentinel"));
    cmd.env_remove("SENTINEL_ARCHIVE");
    cmd.env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml");
    cmd
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A minimal well-formed wiki article citing `sources`.
pub fn article(title: &str, domain: &str, sources: &[&str]) -> String {
    let sources = sources
        .iter()
        .map(|s| format!("  - {s}\n"))
        .collect::<String>();
    format!(
        "---\ntitle: {title}\ndomain: {domain}\norigin: authored\ntags: [t]\nsources:\n{sources}created: 2026-01-01\nupdated: 2026-01-01\nstatus: draft\n---\n\nBody.\n"
    )
}

/// Assert a path exists, with a message naming it.
pub fn assert_exists(path: &Path) {
    assert!(path.exists(), "expected {} to exist", path.display());
}
