//! The command sequences the skills publish, actually run.
//!
//! `tests/skills.rs` checks that a skill only names commands that exist. That
//! catches a deleted subcommand and nothing else: a sequence can be composed
//! entirely of real commands and still fail, because the flags are wrong, the
//! order is wrong, or a step refuses in a state the previous step created.
//!
//! Every skill's fenced blocks are extracted and executed against a fixture
//! archive, in the order the skill gives them. Placeholders a person fills in
//! are substituted with working values; anything left unsubstituted is reported
//! rather than skipped silently, so this cannot quietly cover nothing.

mod common;

use common::Archive;
use std::path::{Path, PathBuf};

/// A `sentinel …` line from a fenced block in a skill.
struct Command {
    skill: String,
    line: usize,
    raw: String,
}

fn skill_commands() -> Vec<Command> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let mut out = Vec::new();
    let mut skills: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path().join("SKILL.md"))
        .filter(|p| p.is_file())
        .collect();
    skills.sort();

    for path in skills {
        let skill = path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        let mut in_block = false;
        for (i, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("```") {
                in_block = !in_block;
                continue;
            }
            if in_block && line.trim().starts_with("sentinel ") {
                out.push(Command {
                    skill: skill.clone(),
                    line: i + 1,
                    raw: line.trim().to_string(),
                });
            }
        }
    }
    assert!(out.len() >= 20, "expected the skills to publish commands");
    out
}

/// Split a command line, honouring the double quotes the skills use.
fn tokenize(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Fill in the placeholders a person would.
///
/// Returns `None` when something is left that this harness cannot supply, so
/// the caller can report it instead of pretending the command was covered.
fn substitute(token: &str, source: &Path) -> Option<String> {
    let filled = match token {
        // The fixture holds `wiki/philosophy/virtue.md`, so a slug or a topic
        // both resolve to something the command can actually act on.
        "<key terms>" | "<topic>" | "<slug>" => "virtue".to_string(),
        "<path-to-notes>" => source.display().to_string(),
        "{domain}" => "philosophy".to_string(),
        t if t.starts_with("\"Research:") || t.starts_with("Research:") => {
            "Research: Virtue".to_string()
        }
        // A free-text log detail; any string is valid.
        t if t.contains('{') && !t.starts_with('-') => "a detail".to_string(),
        t => t.to_string(),
    };
    if filled.contains('<') || filled.contains('{') {
        return None;
    }
    Some(filled)
}

/// An archive with enough in it that every published sequence has work to do.
fn fixture() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/meditations.md", "On virtue and control.");
    a.write("raw/philosophy/stranded.md", "nothing cites this yet");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/virtue.md",
        "---\ntitle: Virtue\ndomain: philosophy\norigin: researched\nstatus: draft\n\
         tags: [ethics]\nsources: [raw/philosophy/meditations.md]\n---\n\n\
         Virtue is the only good. See [[unwritten-concept]].\n",
    );
    a.run(&["index"]);
    a
}

#[test]
fn every_command_a_skill_publishes_runs_against_a_real_archive() {
    // A sequence can name only real commands and still be wrong — bad flags,
    // wrong order, or a step that refuses in the state the last one left.
    let a = fixture();
    let source = a.path("notes-to-ingest.md");
    std::fs::write(&source, "Notes on virtue.\n").unwrap();

    let mut ran = 0;
    let mut unsupported = Vec::new();

    for cmd in skill_commands() {
        let tokens = tokenize(&cmd.raw);
        let args: Option<Vec<String>> =
            tokens[1..].iter().map(|t| substitute(t, &source)).collect();
        let Some(args) = args else {
            unsupported.push(format!("{}:{} {}", cmd.skill, cmd.line, cmd.raw));
            continue;
        };
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();

        // `sentinel review` refuses to attribute a verdict to nobody, and a
        // skill that publishes it has to be runnable. Supplying the identity
        // here rather than in the skill keeps the published line the one a
        // person would actually type.
        let out = a
            .cmd(&argv)
            .env("SENTINEL_REVIEWER", "skill-flows")
            .output()
            .expect("the binary should run");
        let code = out.status.code().unwrap_or(-1);
        let stderr = common::stderr(&out);

        // clap also exits 2, for a usage error. Accepting 2 blanket-wise let a
        // bogus flag through as "ran and found problems" — checked, and it did:
        // `sentinel uncompiled --nonsense-flag` passed this test before.
        assert!(
            !stderr.contains("Usage:") && !stderr.contains("unexpected argument"),
            "{}:{} publishes `{}`, which is not valid usage:\n{stderr}",
            cmd.skill,
            cmd.line,
            cmd.raw
        );

        // Only `lint` legitimately reports findings. Everything else must
        // succeed outright.
        let findings_allowed = argv.first() == Some(&"lint");
        let acceptable = code == 0 || (findings_allowed && code == 2);
        assert!(
            acceptable,
            "{}:{} publishes `{}`, which exits {code}:\n{}\n{stderr}",
            cmd.skill,
            cmd.line,
            cmd.raw,
            common::stdout(&out)
        );
        ran += 1;
    }

    assert!(
        ran >= 20,
        "only {ran} commands were executed; the rest could not be filled in: \
         {unsupported:#?}"
    );
    assert!(
        unsupported.is_empty(),
        "these published commands could not be run, so nothing verifies them. \
         Either the harness needs to know the placeholder, or the skill should \
         show a runnable example:\n{unsupported:#?}"
    );
}

#[test]
fn the_compile_flow_runs_in_the_order_the_skill_gives_it() {
    // `/sentinel-compile` is a sequence, not a set: `index` before writing the
    // article produces nothing, and `lint` before `index` reports a graph that
    // predates the work.
    let a = fixture();

    assert_eq!(a.json(&["uncompiled"])["count"], 1, "a source to compile");

    a.write(
        "wiki/philosophy/stranded-notes.md",
        "---\ntitle: Stranded Notes\ndomain: philosophy\norigin: researched\n\
         status: draft\ntags: [t]\nsources: [raw/philosophy/stranded.md]\n---\n\n\
         Compiled from the stranded source. Relates to [[virtue]].\n",
    );

    assert_eq!(a.code(&["index"]), 0);
    assert_eq!(
        a.json(&["uncompiled"])["count"],
        0,
        "compiling should empty the queue the skill told the agent to work"
    );
    assert!(
        matches!(a.code(&["lint"]), 0 | 2),
        "lint should run, not fail"
    );
}

#[test]
fn the_research_flow_records_provenance_the_way_the_skill_says() {
    // The skill was filing research trails with `sync`, which registers
    // `origin: authored`. It now says `ingest -o researched`, and this is the
    // assertion that keeps it honest.
    let a = fixture();
    let notes = a.path("research-notes.md");
    std::fs::write(&notes, "Findings, with sources.\n").unwrap();

    assert_eq!(
        a.code(&[
            "ingest",
            &notes.display().to_string(),
            "-d",
            "philosophy",
            "-o",
            "researched",
            "-t",
            "Research: Virtue",
        ]),
        0
    );

    let manifest: serde_json::Value = serde_json::from_str(&a.read("meta/manifest.json")).unwrap();
    let origin = manifest["entries"]["raw/philosophy/research-virtue.md"]["origin"]
        .as_str()
        .expect("the ingested trail should be registered");
    assert_eq!(
        origin, "researched",
        "the research flow must not record its own output as the user's writing"
    );
}

#[test]
fn the_grow_loop_can_reach_every_rung_it_documents() {
    // `--action` is how `/sentinel-grow` schedules across categories rather than
    // following strict priority. A rung it documents but cannot request would be
    // advice the loop cannot take.
    let a = fixture();
    let ladder: Vec<String> = a.json(&["schema"])["next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["action"].as_str().unwrap().to_string())
        .collect();

    for rung in &ladder {
        let v = a.json(&["next", "--action", rung]);
        assert_eq!(
            v["requested"], true,
            "`next --action {rung}` should mark the response as requested:\n{v}"
        );
        assert!(
            v["backlog"].is_array(),
            "every response carries the backlog the loop schedules from:\n{v}"
        );
    }
}

#[test]
fn the_loop_terminates_on_an_archive_with_nothing_left_to_do() {
    // `/sentinel-grow`'s first stop condition is `action: "none"`. An archive
    // that satisfies every rung must actually report it, or the loop runs to
    // its budget on work that does not exist.
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "text");
    a.run(&["sync"]);
    for (slug, other) in [("alpha", "beta"), ("beta", "alpha")] {
        a.write(
            &format!("wiki/philosophy/{slug}.md"),
            &format!(
                "---\ntitle: {slug}\ndomain: philosophy\norigin: authored\n\
                 status: stable\ntags: [t]\nsources: [raw/philosophy/s.md]\n\
                 persona:\n  - plain\n---\n\n\
                 Links to [[{other}]] and [[voiced]].\n"
            ),
        );
    }
    // The `learn` rung counts documents registered as the author's own writing
    // that nothing has read for voice. A "complete" archive has to satisfy that
    // rung too, or this test asserts the loop stops while work remains.
    a.write(
        "persona/plain.md",
        &common::trait_citing("plain", &["raw/philosophy/s.md"]),
    );
    // And expressed: an affirmed trait nothing has written from is `extend`
    // work, so an archive that has not written from it is not complete.
    a.write(
        "wiki/philosophy/voiced.md",
        "---\ntitle: Voiced\ndomain: philosophy\norigin: extrapolated\n\
         status: stable\ntags: [t]\npersona:\n  - plain\n---\n\n\
         Links to [[alpha]].\n",
    );
    a.run(&["index"]);

    let v = a.json(&["next"]);
    assert_eq!(
        v["action"], "none",
        "a complete archive must report the loop's stop condition:\n{v}"
    );
    assert_eq!(a.code(&["lint"]), 0, "{}", a.run(&["lint"]));
}
