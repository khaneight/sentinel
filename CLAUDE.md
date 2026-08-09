# Sentinel

Rust CLI for managing a personal knowledge base. Operates on an archive directory containing raw source documents, compiled wiki articles with YAML frontmatter, Obsidian-style `[[wikilinks]]`, and auto-generated indexes.

## Build

```
cargo build
cargo test
cargo clippy
cargo fmt --check
```

## Architecture

- `src/main.rs` — clap-derive CLI entry point, defines all subcommands
- `src/core/paths.rs` — archive root resolution, derived path helpers, user config file
- `src/core/manifest.rs` — JSON manifest tracking raw documents and compilation status
- `src/core/compilation.rs` — derives the raw → wiki mapping from article `sources:` frontmatter
- `src/core/wiki.rs` — shared loader for wiki articles
- `src/core/frontmatter.rs` — YAML frontmatter parsing/rendering for wiki articles
- `src/core/links.rs` — wikilink extraction and link graph (forward + backlinks)
- `src/core/lint.rs` — lint rules (`analyze`), finding type, severity model, stable ordering
- `src/core/output.rs` — output format switch, JSON envelope, exit-code constants
- `src/core/text.rs` — display helpers (character-safe truncation)
- `src/commands/` — one module per CLI subcommand: init, config, ingest, ingest-repo, sync, status, next, uncompiled, index, lint, search, graph
- `tests/` — integration tests that drive the compiled binary against temporary archives

## Skills

Claude Code skill definitions live in `skills/{skill-name}/SKILL.md`. Each skill has YAML frontmatter with `name`, `description`, and `user-invocable: true`.

Skills are prefixed `sentinel-` to avoid namespace collisions:
- `sentinel-ask` — query the knowledge base
- `sentinel-compile` — compile raw docs into wiki articles
- `sentinel-research` — research a topic and add findings
- `sentinel-improve` — health check and quality improvement

The archive's `.claude/skills` is a symlink to this repo's `skills/` directory, so changes here are immediately available in the archive context.

## Archive resolution

Sentinel never assumes where the archive lives. `src/core/paths.rs::resolve` picks a root in this order:

1. `--archive <PATH>` (global flag; `sentinel init <PATH>` is an equivalent spelling)
2. `SENTINEL_ARCHIVE`
3. `archive = "..."` in `~/.config/sentinel/config.toml` (override the file with `SENTINEL_CONFIG`)
4. the nearest ancestor of the working directory containing `meta/manifest.json`

Only `init` may fall back to the working directory, and only when that directory is empty. Everything else errors with the four options above rather than guessing. `sentinel config` prints the resolved root and which rule produced it.

Commands reach the root through `paths::archive_root()`, which reads a `OnceLock` installed by `main` before dispatch. Path resolution logic lives in the pure `paths::resolve` function so it can be unit tested without touching the process environment.

## The compile loop

A raw document is "compiled" when at least one wiki article names it in `sources:`. That mapping is **derived, never recorded**: `core::compilation::Compilation::derive` inverts every article's `sources:` list against the manifest, and `uncompiled`, `status`, and `lint` all call it live. A stale index therefore cannot produce a wrong answer.

`sentinel index` additionally writes the mapping into `ManifestEntry.wiki_articles` and generates `index/_uncompiled.md`. Both are published projections for external readers (Obsidian, scripts); nothing in sentinel reads them back to make a decision.

Citations are matched leniently — `raw/d/x.md`, `./raw/d/x.md`, `/raw/d/x.md`, `d/x.md`, `[[raw/d/x.md]]`, and a bare `x.md` all resolve — because an agent writes them by hand into YAML. A bare filename matching two raw documents is reported as unresolved rather than guessed.

## `sentinel next`

Reads the whole archive and recommends one action. The priority ladder encodes editorial judgement about what is most worth doing, so it is documented here to be argued with, and pinned by `tests/next.rs` so changing it has to be deliberate:

| Priority | Action | Trigger |
|---|---|---|
| 1 | `fix-errors` | any lint **error** — every later judgement would be made on data the errors call into question |
| 2 | `compile` | raw documents no wiki article cites — knowledge already in hand |
| 3 | `write` | `[[wikilinks]]` with no article behind them, ranked by how many distinct articles ask for each |
| 4 | `connect` | articles with no incoming links |
| 5 | `review` | drafts whose `updated` date is over 30 days old |
| — | `none` | nothing outstanding |

Priority 3 is the self-generating part: an unresolved wikilink is existing knowledge naming what it wants next, and `links::wanted` ranks those by demand. `/sentinel-research <slug>` on the top entry is a loop that grows the wiki from its own gaps.

`next` is a recommendation, not a constraint — `backlog` reports the count for every category, so a caller that disagrees with the ordering has the same data without a second invocation. The rules come from `core::lint::analyze`, shared with `sentinel lint`, so the two can never drift.

## Output contract

Every read command takes the global `--json` flag and emits one object with a common envelope:

```json
{ "schema_version": 1, "command": "status", "archive": "/path/to/archive", ... }
```

`SCHEMA_VERSION` lives in `src/core/output.rs`. **Bump it on any breaking change to a payload shape** — renaming or removing a field, changing a type, changing what a field means. Adding an optional field is not breaking. `tests/json_output.rs` asserts the field names, so a breaking change fails the build rather than silently breaking consumers.

Errors are JSON too when `--json` was requested, on stderr: `{"schema_version":1,"error":{"message":"..."}}`. Otherwise a consumer needs a second, prose-shaped parser for the unhappy path.

Exit codes:

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | the command failed |
| 2 | the command ran and found problems (`output::EXIT_FINDINGS`) |

Separating 1 from 2 is what lets a caller tell "your archive has issues" from "sentinel is broken".

### Lint severities

`error` means the archive is malformed — unparseable, ambiguous, or claiming something untrue. `warning` means work that is not finished yet. The split decides the exit code, so it is not cosmetic: a broken `[[wikilink]]` is a **warning** because the compile workflow deliberately links concepts before their articles exist, and a lint that failed on that would be one nobody could gate on.

| Rule | Severity |
|---|---|
| `invalid-frontmatter`, `missing-field`, `invalid-origin`, `invalid-status`, `duplicate-slug`, `unresolved-source` | error |
| `broken-link`, `missing-tags`, `missing-sources`, `uncompiled-source` | warning |

`sentinel lint` exits 2 on any error; `--strict` also fails on warnings. Rule ids are stable, so output can be filtered or grouped without matching on prose.

## Known Limitations

- `ingest-repo` command is a stub (not yet implemented)
- Wikilink slugs are bare filename stems, so two articles with the same stem in different domains collide in the link graph. `sentinel lint` reports the collision; it does not resolve it.
- `meta/log.md` is append-only — commands that mutate state (init, ingest, sync, index, lint) auto-append entries
- No CI/CD pipeline
