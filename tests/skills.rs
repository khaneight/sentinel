//! Structural checks on the shipped skills.
//!
//! Skills are prompts, so most of their quality is unverifiable. These cover
//! the parts that are not: that they declare the metadata the harness needs,
//! that they only tell an agent to run commands that exist, and that they do
//! not reintroduce the two habits that made the previous versions fragile —
//! restating the schema in prose, and slurping the master index into context.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn skills_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("skills")
}

fn skill_files() -> Vec<(String, PathBuf, String)> {
    let mut skills: Vec<(String, PathBuf, String)> = std::fs::read_dir(skills_dir())
        .expect("skills/ must exist")
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let path = e.path().join("SKILL.md");
            let text = std::fs::read_to_string(&path).ok()?;
            Some((e.file_name().to_string_lossy().to_string(), path, text))
        })
        .collect();
    skills.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!skills.is_empty(), "no skills found");
    skills
}

/// The YAML frontmatter block, as raw text.
fn frontmatter(text: &str) -> &str {
    let body = text
        .strip_prefix("---\n")
        .expect("SKILL.md must open with a --- frontmatter fence");
    let end = body.find("\n---").expect("unterminated frontmatter");
    &body[..end]
}

/// Subcommands the binary actually accepts, parsed from --help.
fn real_subcommands() -> BTreeSet<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .arg("--help")
        .output()
        .expect("failed to run sentinel");
    let help = String::from_utf8_lossy(&output.stdout);

    let mut names = BTreeSet::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if in_commands {
            if line.trim().is_empty() || !line.starts_with("  ") {
                break;
            }
            if let Some(name) = line.split_whitespace().next() {
                names.insert(name.to_string());
            }
        }
    }
    assert!(names.contains("next"), "failed to parse --help: {names:?}");
    names
}

#[test]
fn every_skill_declares_the_metadata_the_harness_needs() {
    for (dir, path, text) in skill_files() {
        let fm = frontmatter(&text);
        for key in ["name:", "description:", "user-invocable:", "allowed-tools:"] {
            assert!(
                fm.contains(key),
                "{}: frontmatter is missing `{key}`",
                path.display()
            );
        }
        assert!(
            fm.contains(&format!("name: {dir}")),
            "{}: `name:` must match the directory name `{dir}`",
            path.display()
        );
        assert!(
            dir.starts_with("sentinel-"),
            "{dir}: skills are prefixed `sentinel-` to avoid namespace collisions"
        );
    }
}

#[test]
fn skills_only_reference_commands_that_exist() {
    // The failure this prevents: a skill telling an agent to run
    // `sentinel reconcile`, which it dutifully does, and which fails.
    let real = real_subcommands();

    for (_, path, text) in skill_files() {
        for (i, invocation) in command_invocations(&text) {
            let candidate: String = invocation
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                .collect();
            if candidate.is_empty() {
                continue;
            }
            assert!(
                real.contains(&candidate),
                "{}:{}: references `sentinel {candidate}`, which is not a subcommand",
                path.display(),
                i + 1
            );
        }
    }
}

/// Every `sentinel <word>` that appears as code — inside a fenced block or an
/// inline span — paired with its line number.
///
/// Prose is excluded deliberately: "ask sentinel what to do next" is English,
/// not an invocation, and flagging it would make this check unusable.
fn command_invocations(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut in_fence = false;

    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }

        let mut push_from = |segment: &str| {
            let mut rest = segment;
            while let Some(at) = rest.find("sentinel ") {
                rest = &rest[at + "sentinel ".len()..];
                if let Some(word) = rest.split_whitespace().next() {
                    found.push((i, word.to_string()));
                }
            }
        };

        if in_fence {
            push_from(line);
        } else {
            // Inline code spans only.
            for span in line.split('`').skip(1).step_by(2) {
                push_from(span);
            }
        }
    }
    found
}

#[test]
fn no_skill_tells_the_agent_to_read_the_master_index() {
    // `index/_master.md` lists every article. Reading it to orient consumes the
    // context an agent needs to do the work, and gets worse as the archive
    // grows — the tool becomes context-hostile exactly when it becomes useful.
    for (_, path, text) in skill_files() {
        for (i, line) in text.lines().enumerate() {
            let mentions = line.contains("_master.md");
            let warns_against = line.contains("Do not read") || line.contains("do not read");
            assert!(
                !mentions || warns_against,
                "{}:{}: instructs reading the master index.\n  {line}",
                path.display(),
                i + 1
            );
        }
    }
}

#[test]
fn skills_defer_to_sentinel_schema_for_the_contract() {
    // Every skill used to restate the frontmatter contract in prose, and the
    // prose drifted: one documented five domains where the code had three.
    // Pointing at the generated contract is what makes that class of bug
    // impossible, so it is checked rather than trusted.
    for (dir, path, text) in skill_files() {
        assert!(
            text.contains("sentinel schema"),
            "{}: must send the agent to `sentinel schema` rather than restating the contract",
            path.display()
        );
        let _ = dir;
    }
}

#[test]
fn skills_define_what_happens_with_no_arguments() {
    // `/sentinel-research` with no topic used to render as "You are
    // researching ****". Every skill has to say what an empty invocation does.
    for (_, path, text) in skill_files() {
        assert!(
            text.contains("$ARGUMENTS` is empty") || text.contains("$ARGUMENTS` for"),
            "{}: must define behaviour when $ARGUMENTS is empty",
            path.display()
        );
    }
}

#[test]
fn the_growth_loop_is_bounded() {
    // A loop that writes to the user's knowledge base must state a budget and
    // its stop conditions in the prompt itself.
    let text = std::fs::read_to_string(skills_dir().join("sentinel-grow/SKILL.md"))
        .expect("sentinel-grow must exist");

    assert!(
        text.contains("Default: 3 iterations"),
        "no stated default budget"
    );
    for condition in [
        "Budget exhausted",
        "Backlog empty",
        "No progress",
        "Same target twice",
    ] {
        assert!(
            text.contains(condition),
            "missing stop condition: {condition}"
        );
    }
    assert!(
        text.contains("Modify anything in `raw/`"),
        "must forbid touching immutable source documents"
    );
}

#[test]
fn readme_documents_every_shipped_skill() {
    let readme =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md")).unwrap();

    for (dir, _, _) in skill_files() {
        assert!(
            readme.contains(&format!("/{dir}")),
            "README.md does not document /{dir}"
        );
    }
}
