# Sentinel

Rust CLI for a personal knowledge base: an archive of raw source documents, wiki
articles with YAML frontmatter and `[[wikilinks]]`, and generated indexes.

This file is loaded into every session. It states the rules; the reasoning
behind them — what went wrong, what was measured — is in
[`docs/design-notes.md`](docs/design-notes.md), which is worth reading before
changing any invariant here.

## Build

```
cargo build
cargo test --locked --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

CI (`.github/workflows/ci.yml`) runs these on Linux and macOS for every push and
PR. **Read its result** — a local run on one filesystem is not the same
evidence. To exercise the case-sensitive path on macOS, see *Running the
case-sensitive tests* in [`docs/design-notes.md`](docs/design-notes.md).

## Architecture

- `src/main.rs` — clap entry point; resolves the archive root and acquires the
  lock before dispatch
- `src/core/paths.rs` — archive root resolution, derived paths, user config
- `src/core/manifest.rs` — raw-document manifest, content hashing, save conflicts
- `src/core/compilation.rs` — derives the raw → wiki mapping from `sources:`
- `src/core/wiki.rs` — the one loader for wiki articles
- `src/core/persona.rs` — `persona/` traits: the cited model of the author
- `src/core/frontmatter.rs` — frontmatter parsing (generic over the two
  document schemas); `ORIGINS`/`STATUSES`
- `src/core/links.rs` — wikilink extraction, demand ranking, link graph
- `src/core/slug.rs` — canonical form used for **all** wikilink resolution
- `src/core/lint.rs` — rules (`analyze`), rule registry, severities
- `src/core/{atomic,lock,output,text,log}.rs` — writes, mutual exclusion,
  JSON/exit-code contract, truncation, activity log
- `src/commands/` — one module per subcommand
- `tests/` — integration tests driving the compiled binary

## Invariants

Each of these was a bug. Breaking one reintroduces it.

**Identity.** Every comparison between a wikilink and an article goes through
`slug::canonical` — NFKC, lowercased, separator runs collapsed, invisible format
characters dropped. Use `canonical_slug()`, never `slug()`, which is for display.
Plurals and the Turkish dotted `İ` are deliberately *not* folded.

**Nothing uncited about a person.** `persona/` holds claims about the author. A
trait with no `evidence:` is a lint **error**, and every entry must resolve to a
raw document whose `origin` is `authored` or `hybrid` — research records what
they read, not what they think. The repair for either is never to supply the
missing part. [`docs/clone.md`](docs/clone.md).

**Derivation.** A raw document is compiled when some article names it in
`sources:`. That mapping is derived live by `compilation::Compilation`, never
read back from the manifest — the manifest copy is a published projection.

**Completeness.** `wiki::load_all` returns `Loaded { articles, unreadable }` and
never silently skips. A command that *rewrites* durable state calls
`require_complete()`; a command that only *reads* may proceed but must disclose
the partial view. Walk errors count, not just file reads.

**Queries.** A read-only command must leave the archive byte-identical.
`meta/log.md` records what changed the archive, not what looked at it. A rewrite
producing identical bytes writes nothing (`atomic::write_if_changed`).

**Durability.** Everything persistent goes through `atomic::write` — temp
sibling, `sync_all`, `rename`. Never `fs::write`.

**Exclusivity.** `main` takes `core::lock::ArchiveLock` (`meta/.lock`) for `ingest`,
`ingest-repo`, `sync`, `index`, `mv`, `rm`. Queries take no lock. `init` and
`log` are exempt with reasons recorded in `tests/command_contract.rs`, which
requires every subcommand to be classified.

**Honesty about limits.** Any capped list publishes its true total —
`ref_count`, `target_count`, `result_count`, `entry_count`, and the header of
`index/_recent.md`. A truncated list that does not say so reads as complete.

**Derived sets.** Where one list must match another, derive it rather than
maintaining a second copy: `schema::FIELDS` against the serialised
`Frontmatter`, the `next` ladder against its progress counters, `lint::RULES`
against `analyze`, the subcommand classification against `--help`, `init`'s
index stubs against what `index` regenerates. Four bugs got through guards that
checked only the case in front of them.

**One source of truth.** `ORIGINS`/`STATUSES` back the lint rule, `sentinel
schema`, and `ingest`'s validation. `persona::{KINDS, CONFIDENCES, STATUSES,
REQUIRED, EVIDENCE_ORIGINS}` back theirs the same way, and `EVIDENCE_ORIGINS`
is asserted to be a strict subset of `ORIGINS` — if every origin counts as
evidence the safeguard checks nothing. `lint::RULES` and `lint::analyze` are
asserted to agree in both directions. `templates/wiki-article.md` is generated
from `schema::FIELDS`. Nothing `init` writes may assert a fact the tool will not
maintain.

**Pipes.** `main` restores default `SIGPIPE` before anything else.

## Contracts

**Archive resolution** — `--archive`, then `SENTINEL_ARCHIVE`, then `archive =`
in `~/.config/sentinel/config.toml`, then the nearest ancestor containing
`meta/manifest.json`. No match is an error, not a default. Only `init` may fall
back to the working directory, and only when it is empty. `sentinel config`
reports which rule produced the current root, and diagnoses failures rather than
dying on them.

**Output** — `--json` is global, so **every** command must honour it, mutating ones included; a flag that parses and does nothing is worse than one that does not exist. Every command takes `--json` and emits
`{schema_version, command, archive, …}`. Errors are JSON too when JSON was
asked for. Bump `SCHEMA_VERSION` (`core/output.rs`) on any breaking payload
change; adding an optional field is not breaking.

**Exit codes** — 0 success, 1 the command failed, 2 it ran and found problems
(`output::EXIT_FINDINGS`).

**Lint severity** — `error` means the archive is malformed; `warning` means work
is unfinished. `lint` exits 2 on errors only; `--strict` also fails on warnings.
`--rule` is validated against `lint::RULES`: an unknown rule is an error, not
an empty result, so a typo cannot read as a clean rule.
A `broken-link` is a **warning**: the compile workflow forward-declares links
deliberately, so an archive full of them is healthy.

**`sentinel next`** — priority `fix-errors` → `compile` → `write` → `connect` →
`review`. It ranks; it does not schedule.
**Every action in the ladder must move a progress counter** — `fix-errors`→`errors`,
`compile`→`uncompiled`, `write`→`wiki_articles`, `connect`→`orphans`,
`review`→`drafts` — or a correct iteration of it reads as no progress and halts
the loop. A new action needs a counter. `--action <name>` reaches any
category, `backlog` counts them all, and `progress` reports what the archive
*contains*. Measure loop progress by `progress`, never by backlog size.

**`sentinel schema`** — the published frontmatter contract, domains (read from
disk), lint rules, and the `next` ladder. Skills read this instead of restating
it.

## Commands with non-obvious behaviour

`sentinel mv <from> <to>` moves a raw document and rewrites every `sources:`
citation, matching them the way the compile loop does. `sentinel rm <target>`
refuses when anything cites the target and points at `mv`; `--force` proceeds
and reports each citation it orphans. Both are textual, frontmatter-only edits —
see [`docs/design-notes.md`](docs/design-notes.md).

`sentinel search` ranks title 1000 / slug 500 / tag 200 / body line 1, top
`--limit` (20). `sentinel graph --node <slug>` returns a neighbourhood; bare, it
dumps the whole topology for humans. `sentinel log` reads with no arguments and
appends with them.

`sentinel export` writes the publishable subset: articles whose `status`
qualifies (`stable` by default), with `[[links]]` to anything unpublished
rewritten as plain text so the output has no dead ends. It renders no HTML. It
refuses to write under `wiki/`, `raw/`, or `index/`, which `index` walks, and —
since publishing is not recoverable by re-running — refuses on a partial view;
`--dry-run` reports instead, `--clean` removes what a previous export left.
`--data <file>` emits a JSON bundle for a front end, including the growth
history in `meta/progress.jsonl`: one snapshot per `index` **that changed
something**, so it records the archive rather than how often a command ran.
[`docs/publishing.md`](docs/publishing.md) has the workflow and the path layout.

`sentinel index` regenerates `_master.md`, `_by-domain.md`, `_recent.md`,
`_orphans.md`, `_uncompiled.md`, `_dashboard.md`, the link graph, and the
manifest's compilation mapping. `_dashboard.md` is the human page — generated
from `next::recommend`, `status::summarize`, `lint::analyze` and `schema`,
never a second definition, and capped so it cannot become `_master.md`. Agents
use `sentinel next --json` instead; same facts, less context.
`paths::DEFAULT_DOMAINS` is only what `init` creates; live domains come from
disk.

## Skills

`skills/{name}/SKILL.md`, prefixed `sentinel-`, symlinked into the archive's
`.claude/skills`. Flat, not `/sentinel <verb>`: a skill loads whole, so a
dispatcher would cost every caller all five (31 KB) — see
[`docs/design-notes.md`](docs/design-notes.md).
A section serving a `next` rung names it, and references between skills are by
section title, never step number. `sentinel-grow` runs the maintenance loop; `ask`, `compile`,
`research`, `improve` do one job each.

`tests/skill_flows.rs` executes every `sentinel …` line in a skill's fenced
blocks against a real archive — naming a command that exists is not the same as
publishing a sequence that runs. `tests/skills.rs` enforces the rest: frontmatter, `name` matching the directory,
that every `sentinel <cmd>` named exists, that none send an agent to
`index/_master.md`, that each defines empty-`$ARGUMENTS` behaviour and defers to
`sentinel schema`, that every mutating command and every lint rule is reachable
from some skill, and that any skill invoking a command which can refuse says
what to do about it.

**A new failure mode in the CLI is a change to the skills — and to the
`CLAUDE.md` `init` writes into the archive**, which is the same kind of
document and drifted the same way. `tests/skills.rs` guards both, including
that the archive file stays small enough to be per-session context.

## Testing

- Integration tests drive the compiled binary against temp archives and scrub
  `SENTINEL_ARCHIVE`/`SENTINEL_CONFIG` so they cannot read the developer's own.
- **`tests/onboarding.rs` tests journeys, not commands.** `tests/common/journey.rs`
  records a whole session and asserts over the transcript. Two defects it exists
  for were invisible one command at a time: `next` telling a brand-new archive
  "Nothing outstanding", and `connect` asking a one-article archive for an
  incoming link forever. Add a step when a change alters what a user sees *in
  sequence*.
- **When a test enumerates, enumerate from the source of truth** — from
  `--help`, from `schema`, from `RULES` — not from the case in front of you.
  Three bugs got through guards that checked only the site being written.
- **For a fix to a measured defect, the acceptance criterion is the
  measurement**, not a test that exercises the new code.

## Known limitations

- Wikilink slugs are filename stems, so two articles whose stems canonicalise
  the same collide in the link graph. `lint` reports it; nothing resolves it.
- `ingest-repo` is not implemented; it exits non-zero with guidance.
- `meta/log.md` is append-only and never pruned.
