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

- `src/main.rs` — clap entry; resolves the archive root and takes the lock
- `src/core/paths.rs` — root resolution, derived paths, user config
- `src/core/manifest.rs` — raw-document manifest, hashing, save conflicts
- `src/core/compilation.rs` — derives raw → wiki from `sources:`; `SourceIndex`
  is the one matcher for a cited path
- `src/core/wiki.rs` — the one loader for wiki articles
- `src/core/persona.rs` — `persona/` traits: the cited model of the author
- `src/core/review.rs` — verdicts, and the only writer of `review:`
- `src/core/frontmatter.rs` — parsing, generic over both document schemas;
  `ORIGINS`/`STATUSES`
- `src/core/links.rs` — wikilink extraction, demand ranking, link graph
- `src/core/slug.rs` — canonical form used for **all** identity comparison
- `src/core/lint.rs` — rules (`analyze`), rule registry, severities
- `src/core/{atomic,lock,output,text,log,history}.rs` — writes, mutual
  exclusion, JSON/exit-code contract, truncation, activity log, progress series
- `src/commands/` — one module per subcommand
- `tests/` — integration tests driving the compiled binary

## Invariants

Each of these was a bug. Breaking one reintroduces it.

**Identity.** Every comparison between a wikilink and an article goes through
`slug::canonical` — NFKC, lowercased, separator runs collapsed, invisible format
characters dropped. Use `canonical_slug()`, never `slug()`, which is for display.
Plurals and the Turkish dotted `İ` are deliberately *not* folded.

**The citation chain, corpus to output.** The repair for a broken link is never
to supply the missing part. A trait must cite `evidence:` resolving to an
`authored`/`hybrid` document — research records what they read, not what they
think. An article names the `persona:` traits it was written through: absent on
`extrapolated` work is an error, elsewhere `unvoiced-article` warns. Cited
traits must resolve and be *affirmed* — a rejected one is an error, unconfirmed
a warning. `INGESTABLE_ORIGINS` is a strict subset of `ORIGINS`, so `raw/` can
never hold extrapolated work, or the archive could learn a person from its own
output. [`docs/clone.md`](docs/clone.md).

**`raw/` is published one document at a time.** `export --with-sources` copies
only what `sentinel sources` marked `publish: true`, default false — nothing
about a file says whether its owner may publish it. A published article's
`sources:` names only what a reader can open; a withheld filename is often the
private part.

**The owner's word.** `review:` entries are the owner's verdicts and `sentinel
review` is their only writer — no skill invokes it, because an agent that can
approve its own work has a permission system in name only. Entries append; the
operative one is the latest that *decided* something, and a `comment` decides
nothing. A verdict is never attributed to a default: with no `--by`,
`SENTINEL_REVIEWER` or `USER` it refuses. A trait carries its standing twice —
`status:` is what a reader sees, `review:` the history — and
`verdict-disagrees-with-status` reports them drifting.

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

**Exclusivity.** Anything that reads shared state, changes it and writes it back
takes `core::lock::ArchiveLock` (`meta/.lock`); queries take no lock. Every
subcommand is classified in `tests/command_contract.rs`, exemptions with
reasons.

**Honesty about limits.** Any capped list publishes its true total —
`ref_count`, `target_count`, `result_count`, `entry_count`, and the header of
`index/_recent.md`. A truncated list that does not say so reads as complete.

**One source of truth.** Where one list must match another, derive it rather
than keeping a second copy — five bugs got through guards that checked only the
case in front of them. The enum constants (`ORIGINS`/`STATUSES`,
`persona::{KINDS, CONFIDENCES, STATUSES, REQUIRED, EVIDENCE_ORIGINS}`,
`review::{VERDICTS, DECISIONS}`) back the lint rules, `sentinel schema`, and
`ingest`'s validation at once. `schema::FIELDS` is asserted against the
serialised struct and generates the template; `Action::LADDER` generates the
published ladder and its numbering; `lint::RULES` and `analyze` must agree in
both directions; the subcommand classification comes from `--help`; `init`'s
stubs from what `index` regenerates. `EVIDENCE_ORIGINS` is asserted a *strict*
subset of `ORIGINS` — if every origin counts as evidence the safeguard checks
nothing. Nothing `init` writes may assert a fact the tool will not maintain.

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

**`sentinel next`** — priority `fix-errors` → `learn` → `compile` → `write` →
`connect` → `extend` → `review`, from `Action::LADDER`, the one ordering:
`schema`'s published list and numbering derive from it. It ranks; it does not
schedule. `--action <name>` reaches any category, `backlog` counts them all, and
`progress` reports what the archive *contains* — measure loop progress by that,
never by backlog size.
**Every rung must move a counter** — `errors`, `unmined`, `uncompiled`,
`wiki_articles`, `orphans`, `unexpressed`, `drafts`, in ladder order — or a
correct iteration reads as no progress and halts the loop. A new rung needs a
counter, and must only fire for archives that opted into it, or `next` never
says "nothing outstanding" again.
**Nothing the user must answer is a rung.** The agent cannot approve its own
work, so `awaiting_approval` and `unconfirmed_traits` are things to stop on:
with traits proposed and none affirmed, `next` returns `none` and points at
`sentinel review`, because everything below `learn` is the clone writing and an
unconfirmed reading of somebody is what the verdict prevents.

**`sentinel schema`** — the published frontmatter contract, domains (read from
disk), lint rules, and the `next` ladder. Skills read this instead of restating
it.

## Commands with non-obvious behaviour

Behaviour a reader would not guess. Everything else is in `--help`, and the
reasoning behind each of these is in [`docs/design-notes.md`](docs/design-notes.md).

- **`mv`** rewrites every `sources:` citation, matched the way the compile loop
  does. **`rm`** refuses when anything cites the target and points at `mv`;
  `--force` reports each citation it orphans. Both edit text inside the
  frontmatter block only.
- **`search`** ranks title 1000 / slug 500 / tag 200 / body line 1. **`graph`**
  bare dumps the whole topology. **`log`** reads with no arguments, appends with
  them.
- **`export`** writes the publishable subset, rewriting `[[links]]` to
  unpublished pages as plain text. It renders no HTML, refuses to write under
  `wiki/`, `raw/` or `index/`, and — publishing not being recoverable by
  re-running — refuses on a partial view. **An `extrapolated` article publishes
  only when its latest verdict is `approved`**; no `--status` opens that gate,
  and the *exporter* appends the attribution notice, because an agent that
  composes its own disclosure can leave it out.
  `--data`/`--ui` emit the front-end bundle. `LAYERS` (source material, persona,
  the clone's work) and each node's `layer` are published, so a page cannot
  invent its own account of the archive. **Affirmed traits are nodes**;
  `proposed` ones are absent, as from `persona`. Never `raw/` paths.
  **Every edge points outward** with an `EDGE_KINDS` `role`; a test asserts each
  joins the layers it names. Only `authorship` (`distils`, `writes`) is ancestry
  and walked transitively — `links` is a `reference`, one hop, and following it
  as ancestry made a trait claim work it had not written; `grounds` (`sources:`)
  is a `citation`, never a way in, because the clone wrote the article through
  the persona, not the document. `--ui` also writes `ui/index.html`
  (`include_str!`d, so page and bundle cannot drift) beside its own
  `bundle.json`. [`docs/publishing.md`](docs/publishing.md) has the workflow;
  `meta/progress.jsonl` — one snapshot per `index` **that changed something** —
  is the growth series it carries.
- **`index`** regenerates every `index/` page, the link graph and the manifest's
  compilation mapping. `_dashboard.md` is the human page — from
  `next::recommend`, `status::summarize`, `lint::analyze` and `schema`, never a
  second definition, and capped so it cannot become `_master.md`. Agents use
  `sentinel next --json` instead. `paths::DEFAULT_DOMAINS` is only what `init`
  creates; live domains come from disk.

## Skills

`skills/{name}/SKILL.md`, prefixed `sentinel-`, symlinked into the archive's
`.claude/skills`. Flat, not `/sentinel <verb>` — a skill loads whole, so a
dispatcher costs every caller all of them. A section serving a `next` rung
names it, and references between skills are by section title, never step
number. `sentinel-grow` runs the maintenance loop; `ask`, `clone`, `compile`,
`research`, `improve` do one job each.

`tests/skill_flows.rs` executes every `sentinel …` line in a skill's fenced
blocks against a real archive — naming a command that exists is not the same as
publishing a sequence that runs. `tests/skills.rs` enforces the rest: that every
command named exists, that every mutating command and every lint rule is
reachable from some skill, that each skill defines empty-`$ARGUMENTS` behaviour
and defers to `sentinel schema`, and that any skill invoking a command which can
refuse says what to do about it.

**A new failure mode in the CLI is a change to the skills — and to the
`CLAUDE.md` `init` writes into the archive**, which is the same kind of
document and drifted the same way. `tests/skills.rs` guards both, including
that the archive file stays small enough to be per-session context.

## Testing

- Integration tests drive the compiled binary against temp archives and scrub
  `SENTINEL_ARCHIVE`/`SENTINEL_CONFIG` so they cannot read the developer's own.
- **`tests/onboarding.rs` tests journeys, not commands**, over a recorded
  transcript. Two defects it exists for were invisible one command at a time:
  `next` telling a brand-new archive "Nothing outstanding", and `connect`
  asking a one-article archive for an incoming link forever. Add a step when a
  change alters what a user sees *in sequence*.
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
