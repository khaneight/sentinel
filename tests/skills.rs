//! Structural checks on the shipped skills.
//!
//! Skills are prompts, so most of their quality is unverifiable. These cover
//! the parts that are not: that they declare the metadata the harness needs,
//! that they only tell an agent to run commands that exist, and that they do
//! not reintroduce the two habits that made the previous versions fragile —
//! restating the schema in prose, and slurping the master index into context.

mod common;

use common::Archive;
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
fn readme_documents_every_implemented_command() {
    // The README went stale on `sentinel export` because nothing compared it
    // with the CLI. Enumerated from `--help`, so the next command added is
    // documented on the day it ships rather than the day somebody notices.
    let readme =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md")).unwrap();

    // Commands a reader has no use for, with the reason.
    const OMITTED: &[(&str, &str)] = &[
        ("help", "clap builtin"),
        ("ingest-repo", "unimplemented; exits non-zero with guidance"),
    ];
    let omitted: BTreeSet<&str> = OMITTED.iter().map(|(c, _)| *c).collect();

    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    let subcommands: Vec<String> = help
        .split("Commands:")
        .nth(1)
        .and_then(|s| s.split("Options:").next())
        .expect("--help lists commands")
        .lines()
        .filter(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();
    assert!(subcommands.len() >= 10, "{subcommands:?}");

    for command in &subcommands {
        if omitted.contains(command.as_str()) {
            continue;
        }
        assert!(
            readme.contains(&format!("sentinel {command}")),
            "README.md does not document `sentinel {command}`. Document it, or \
             add it to OMITTED with a reason."
        );
    }
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

#[test]
fn skills_that_rebuild_say_what_to_do_when_the_rebuild_refuses() {
    // `index`, `mv`, and `rm` refuse on a partial view (#17, #18, #20). That
    // refusal only protects the archive if the agent driving it knows not to
    // retry or route around it — and those behaviours were added sixteen PRs
    // after the skills were written, without the skills being told.
    for (dir, path, text) in skill_files() {
        let rebuilds = command_invocations(&text).iter().any(|(_, cmd)| {
            cmd.starts_with("index") || cmd.starts_with("mv") || cmd.starts_with("rm")
        });
        if !rebuilds {
            continue;
        }
        assert!(
            text.contains("could not be read"),
            "{} runs a command that can refuse on a partial view but never says \
             what to do about it ({dir})",
            path.display()
        );
    }
}

#[test]
fn every_command_is_either_reachable_from_a_skill_or_deliberately_not() {
    // A command no skill mentions is one an agent will never choose. `rm`
    // shipped in #20 and no skill knew it existed.
    //
    // The list used to be written here by hand, which meant it covered the
    // commands somebody had remembered. Enumerating from `--help` instead: a
    // new subcommand fails this test until it is either taught to a skill or
    // named below with a reason it should not be.
    const NOT_FOR_AGENTS: &[(&str, &str)] = &[
        (
            "init",
            "creates the archive an agent is already working inside",
        ),
        (
            "config",
            "diagnoses the caller's own setup, not the archive",
        ),
        ("ingest-repo", "unimplemented; exits non-zero with guidance"),
        (
            "export",
            "publishing is the user's decision. An agent that can publish can \
             publish a draft, and that is not recoverable by re-running it.",
        ),
        (
            "review",
            "records the archive owner's verdict. An agent that can approve its \
             own work has a permission system in name only.",
        ),
        ("help", "clap builtin"),
    ];

    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    let subcommands: Vec<String> = help
        .split("Commands:")
        .nth(1)
        .and_then(|s| s.split("Options:").next())
        .expect("--help lists commands")
        .lines()
        .filter(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();
    assert!(subcommands.len() >= 10, "{subcommands:?}");

    let all: String = skill_files().iter().map(|(_, _, t)| t.clone()).collect();
    let exempt: BTreeSet<&str> = NOT_FOR_AGENTS.iter().map(|(c, _)| *c).collect();

    for command in &subcommands {
        if exempt.contains(command.as_str()) {
            continue;
        }
        assert!(
            all.contains(&format!("sentinel {command}")),
            "no skill mentions `sentinel {command}`, so an agent will never \
             reach for it. Teach a skill, or add it to NOT_FOR_AGENTS with a \
             reason."
        );
    }
}

#[test]
fn the_archive_conventions_keep_pace_with_the_cli() {
    // `init` writes a CLAUDE.md into every archive. It is the orientation
    // document for an agent working *in* the archive — the counterpart to the
    // skills — and it drifted the same way they did: `mv` and `rm` shipped
    // without it mentioning them, and the refusal behaviour from #17/#18/#20
    // was added to all five skills and not to this file.
    //
    // Same rule as the skills: a command no instruction mentions is one an
    // agent will never reach for.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("archive");
    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .args(["init", &root.display().to_string()])
        .env_remove("SENTINEL_ARCHIVE")
        .env("SENTINEL_CONFIG", "/nonexistent/sentinel/config.toml")
        .output()
        .unwrap();
    assert!(out.status.success());

    let raw = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap();
    // Wrapped prose: compare on collapsed whitespace, not on line breaks.
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");

    for phrase in [
        "sentinel mv",
        "sentinel rm",
        "sentinel next --action",
        "sentinel log --json",
        "could not be read",
        "index/_master.md",
    ] {
        assert!(
            text.contains(phrase),
            "the archive's CLAUDE.md does not mention `{phrase}`:\n{raw}"
        );
    }

    // It is loaded every session in the archive, so it must stay small — the
    // mistake the repo's own CLAUDE.md made by growing to 31 KB.
    assert!(
        raw.len() < 6_000,
        "archive CLAUDE.md is {} bytes; it is per-session context, keep it a \
         map rather than a manual",
        raw.len()
    );
}

#[test]
fn every_lint_rule_has_a_repair_instruction_somewhere_in_the_skills() {
    // Adding `invalid-date` to `lint::RULES` updated `sentinel schema`
    // automatically and the skills not at all. An agent hitting a rule with no
    // published repair has to invent one, against an archive it is editing.
    //
    // Enumerated from the published contract, so the next rule fails here
    // rather than shipping undocumented.
    let a = Archive::new();
    let rules: Vec<String> = a.json(&["schema"])["lint_rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["rule"].as_str().unwrap().to_string())
        .collect();
    assert!(rules.len() >= 10, "{rules:?}");

    let skills: String = skill_files()
        .into_iter()
        .map(|(_, _, text)| text)
        .collect::<Vec<_>>()
        .join("\n");

    let missing: Vec<&String> = rules
        .iter()
        .filter(|r| !skills.contains(&format!("`{r}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "these lint rules have no repair instruction in any skill: {missing:?}"
    );
}

#[test]
fn every_command_the_cli_suggests_lands_somewhere_a_skill_can_act_on() {
    // `next` prints a `suggested_command` and an agent runs it verbatim. Two of
    // them named a skill and then a phrase that appeared nowhere in it:
    // `/sentinel-improve connect orphan pages` and `/sentinel-improve promote
    // stale drafts`. The skill existed; the instruction inside it had to be
    // hunted for, across a step holding five unrelated tasks.
    //
    // Every rung is driven, from the ladder `sentinel schema` publishes, so a
    // rung added later is covered without anyone remembering.
    let a = Archive::new();
    let ladder: Vec<String> = a.json(&["schema"])["next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["action"].as_str().unwrap().to_string())
        .collect();
    assert!(ladder.len() >= 5, "{ladder:?}");

    // An archive with work outstanding in every category.
    a.write("raw/philosophy/cited.md", "text");
    a.write("raw/philosophy/stranded.md", "nothing cites this");
    a.run(&["sync"]);
    for (slug, extra) in [
        ("alpha", "status: draft\nupdated: 2020-01-01\n"),
        ("beta", ""),
    ] {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &format!(
                "---\ntitle: {slug}\ndomain: philosophy\norigin: authored\n{extra}\
                 tags: [t]\nsources: [raw/philosophy/cited.md]\n---\n\nSee [[unwritten]].\n"
            ),
        );
    }
    a.run(&["index"]);

    let skills: Vec<(String, String)> = skill_files()
        .into_iter()
        .map(|(name, _, text)| (name, text.to_lowercase()))
        .collect();

    let mut checked = 0;
    for rung in &ladder {
        let v = a.json(&["next", "--action", rung]);
        let Some(command) = v["suggested_command"].as_str() else {
            continue;
        };
        let Some(rest) = command.strip_prefix('/') else {
            // `sentinel lint` and friends are CLI commands, covered elsewhere.
            continue;
        };
        checked += 1;

        let (skill_name, phrase) = rest.split_once(' ').unwrap_or((rest, ""));
        let (_, body) = skills
            .iter()
            .find(|(name, _)| name == skill_name)
            .unwrap_or_else(|| panic!("`{rung}` suggests `/{skill_name}`, which is not a skill"));

        // A bare `/skill target` is a target, not a phrase to find. Only check
        // the ones that name a section.
        let names_a_section = !phrase.is_empty() && !phrase.contains('/');
        if names_a_section {
            assert!(
                body.contains(&phrase.to_lowercase()),
                "`{rung}` suggests `{command}`, but `/{skill_name}` contains no \
                 section matching \"{phrase}\" — the agent has to hunt for it"
            );
        }
    }
    assert!(checked >= 3, "only {checked} rungs suggested a skill");
}

#[test]
fn no_skill_points_into_another_by_step_number() {
    // `/sentinel-grow` said "Follow `/sentinel-improve` step 2" and "step 4".
    // Renumbering the target silently redirects the caller, and step 4 already
    // held five unrelated tasks, so "step 4" meant one bullet in five. Sections
    // are referenced by name now.
    for (name, _, text) in skill_files() {
        for (i, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            if !lower.contains("/sentinel-") {
                continue;
            }
            assert!(
                !lower.contains("step 1")
                    && !lower.contains("step 2")
                    && !lower.contains("step 3")
                    && !lower.contains("step 4")
                    && !lower.contains("step 5"),
                "{name}:{} points into another skill by step number, which \
                 renumbering breaks silently:\n  {line}",
                i + 1
            );
        }
    }
}

#[test]
fn every_rung_of_the_ladder_is_named_by_some_skill() {
    // `review` was the only rung `/sentinel-grow` handled inline while every
    // other one delegated — so the loop and the CLI gave different instructions
    // for the same work, and nothing noticed.
    let a = Archive::new();
    let ladder: Vec<String> = a.json(&["schema"])["next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["action"].as_str().unwrap().to_string())
        .collect();

    let all: String = skill_files()
        .iter()
        .map(|(_, _, t)| t.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");

    for rung in &ladder {
        assert!(
            all.contains(&format!("`{rung}`")),
            "no skill names the `{rung}` rung, so an agent reaching it has no \
             published instruction"
        );
    }
}
