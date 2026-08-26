//! A command that reads a layer must say when it could not read all of it.
//!
//! The rule is already written down: `wiki::load_all` returns
//! `Loaded { articles, unreadable }` and never silently skips; a command that
//! rewrites durable state calls `require_complete()`, and one that only reads
//! "may proceed but must disclose the partial view".
//!
//! `index`, `mv` and `rm` refuse. `lint` and `status` disclose. `next`,
//! `uncompiled` and `search` did neither — they took `.articles` and dropped
//! the rest, so every count they published described whatever happened to be
//! legible, with nothing saying so.
//!
//! `persona/` has the same contract and its own loader, so "reads the wiki" is
//! no longer the same set as "proceeds on a partial view". Each command is
//! checked against the layers it actually loads: warning `persona` about an
//! unreadable *article* would be a disclosure that is not true, which is its
//! own kind of dishonesty.

mod common;

use common::Archive;
use std::path::Path;

/// A readable layer of the archive, and the file this test hides in it.
///
/// Named so the enumeration below can say *which* partial view a command owes
/// a disclosure about, rather than assuming there is only one.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Layer {
    Wiki,
    Persona,
}

impl Layer {
    /// The call that loads it. Read out of each command's source, so a command
    /// is checked against the layers it genuinely touches.
    fn loader(self) -> &'static str {
        match self {
            Layer::Wiki => "wiki::load_all(",
            Layer::Persona => "persona::load_all(",
        }
    }

    fn hidden_file(self) -> &'static str {
        match self {
            Layer::Wiki => "wiki/philosophy/compiled.md",
            Layer::Persona => "persona/hidden.md",
        }
    }

    const ALL: &'static [Layer] = &[Layer::Wiki, Layer::Persona];
}

/// An archive whose only article compiles its only source, and which holds one
/// persona trait cited to it.
fn archive() -> Archive {
    let a = Archive::new();
    a.write("raw/philosophy/s.md", "source text");
    a.run(&["sync"]);
    a.write(
        "wiki/philosophy/compiled.md",
        "---\ntitle: Compiled\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         sources: [raw/philosophy/s.md]\n---\n\nAbout virtue and courage.\n",
    );
    a.write(
        "persona/hidden.md",
        "---\nid: hidden\nkind: style\nclaim: Writes plainly.\n\
         evidence: [raw/philosophy/s.md]\n---\n\nThe source reads plainly.\n",
    );
    a.run(&["index"]);
    a
}

fn make_unreadable(a: &Archive) {
    for layer in Layer::ALL {
        let path = a.path(layer.hidden_file());
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o000);
        }
        #[cfg(not(unix))]
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms).unwrap();

        // Running as root defeats mode 0o000, and every assertion below would
        // then pass while testing nothing. Fail loudly instead of agreeing.
        assert!(
            std::fs::read_to_string(&path).is_err(),
            "the fixture is still readable, so this test proves nothing — \
             it is probably running as root"
        );
    }
}

/// `unreadable`, wherever a given command puts it.
fn disclosed(v: &serde_json::Value) -> usize {
    v.get("unreadable")
        .or_else(|| v.get("progress").and_then(|p| p.get("unreadable")))
        .and_then(|u| u.as_array())
        .map(Vec::len)
        .unwrap_or(0)
}

/// Read commands that load the wiki without demanding a complete view.
///
/// Derived from the source: a command calling `load_all` and not
/// `require_complete` is one that proceeds on a partial view, and every one of
/// those owes the caller a disclosure. Enumerating by hand here would have
/// produced a list of the three I had already fixed.
fn read_commands() -> Vec<(String, usize)> {
    // Intersected with `--help`, because `src/commands/` also holds modules
    // that are not subcommands — `dashboard` renders a page for `index` and has
    // no CLI surface of its own to disclose anything through.
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    let subcommands: Vec<String> = help
        .split("Commands:")
        .nth(1)
        .and_then(|s| s.split("Options:").next())
        .expect("--help lists commands")
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    let mut found: Vec<(String, usize)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path()).unwrap_or_default();
            if text.contains("require_complete()") {
                return None; // refuses instead of disclosing
            }
            // How many layers it proceeds on a partial view of. That is exactly
            // how many unreadable files it owes a disclosure about, so a
            // command that loads both and mentions one still fails here.
            let layers = Layer::ALL
                .iter()
                .filter(|l| text.contains(l.loader()))
                .count();
            if layers == 0 {
                return None;
            }
            let name = e
                .path()
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .replace('_', "-");
            Some((name, layers))
        })
        .filter(|(name, _)| subcommands.contains(name))
        .collect();
    found.sort();
    assert!(
        found.len() >= 5,
        "expected the read commands to be discoverable: {found:?}"
    );
    assert!(
        found.iter().any(|(_, layers)| *layers > 1),
        "no command loads both layers, so the multi-layer case is untested: {found:?}"
    );
    found
}

/// Arguments each command needs to run at all.
fn invocation(command: &str) -> Vec<String> {
    let mut argv = vec![command.to_string()];
    if command == "search" {
        argv.push("virtue".into());
    }
    argv.push("--json".into());
    argv
}

#[test]
fn every_read_command_discloses_a_partial_view_in_json() {
    let a = archive();
    make_unreadable(&a);

    for (command, layers) in read_commands() {
        let argv = invocation(&command);
        let args: Vec<&str> = argv.iter().map(String::as_str).collect();
        let out = a.output(&args);
        let text = common::stdout(&out);
        let v: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("`sentinel {command} --json` emitted no JSON: {e}\n{text}"));

        assert_eq!(
            disclosed(&v),
            layers,
            "`sentinel {command} --json` reads {layers} layer(s) but discloses \
             {} unreadable file(s):\n{v:#}",
            disclosed(&v)
        );
    }
}

#[test]
fn every_read_command_discloses_it_in_human_output_too() {
    // The JSON is the agent's path and the text is the user's. A tool that
    // tells one of them and not the other has picked a side.
    let a = archive();
    make_unreadable(&a);

    for (command, _) in read_commands() {
        let argv: Vec<String> = invocation(&command)
            .into_iter()
            .filter(|x| x != "--json")
            .collect();
        let args: Vec<&str> = argv.iter().map(String::as_str).collect();
        let out = common::stdout(&a.output(&args));
        assert!(
            out.contains("could not be read"),
            "`sentinel {command}` (human) hides the partial view:\n{out}"
        );
    }
}

#[test]
fn a_complete_view_raises_no_warning() {
    let a = archive();
    for (command, _) in read_commands() {
        let argv = invocation(&command);
        let args: Vec<&str> = argv.iter().map(String::as_str).collect();
        let v: serde_json::Value = serde_json::from_str(&common::stdout(&a.output(&args))).unwrap();
        assert_eq!(
            disclosed(&v),
            0,
            "`{command}` reports unreadable files in a healthy archive:\n{v:#}"
        );
    }
}

#[test]
fn an_unreadable_article_makes_its_source_look_uncompiled() {
    // This is the consequence that costs something. An article that cannot be
    // read cites nothing, so the source it compiles reappears in the compile
    // queue — and the loop is told to write an article that already exists.
    let a = archive();
    assert_eq!(a.json(&["uncompiled"])["count"], 0);

    make_unreadable(&a);
    let v = a.json(&["uncompiled"]);
    assert_eq!(v["count"], 1, "expected the false positive to be present");
    assert_eq!(
        disclosed(&v),
        1,
        "the false positive is tolerable; not saying why is not:\n{v:#}"
    );
}

#[test]
fn next_ranks_a_partial_archive_but_says_that_it_did() {
    // `next` is the command the loop asks for direction, and it took
    // `.articles` off a `Loaded` and discarded the rest.
    let a = archive();
    make_unreadable(&a);

    let v = a.json(&["next"]);
    assert_eq!(
        disclosed(&v),
        Layer::ALL.len(),
        "the whole ladder was ranked without a file nobody was told about. \
         `next` reads every layer, so it owes a disclosure for each:\n{v:#}"
    );
    assert_eq!(
        v["progress"]["wiki_articles"], 0,
        "the count should reflect what was actually readable"
    );
}

#[test]
fn search_does_not_report_zero_results_from_a_file_it_could_not_open() {
    // `wiki.rs` names this case exactly: "0 results from an unreadable file is
    // as misleading as a wrong answer."
    let a = archive();
    assert_eq!(a.json(&["search", "virtue"])["result_count"], 1);

    make_unreadable(&a);
    let v = a.json(&["search", "virtue"]);
    assert_eq!(v["result_count"], 0);
    assert_eq!(disclosed(&v), 1, "silent zero:\n{v:#}");
}
