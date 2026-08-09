# Sentinel

Rust CLI for managing a personal knowledge base. Operates on an archive directory containing raw source documents, compiled wiki articles with YAML frontmatter, Obsidian-style `[[wikilinks]]`, and auto-generated indexes.

## Build

```
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

CI (`.github/workflows/ci.yml`) runs exactly these on every push and PR. The tree is warning-free, and clippy runs with `-D warnings` to keep it that way. Tests run on both Linux and macOS — the defect that made this project unusable was a hardcoded `/home/...` path, and a Linux-only matrix would not have caught it.

## Architecture

- `src/main.rs` — clap-derive CLI entry point, defines all subcommands
- `src/core/paths.rs` — archive root resolution, derived path helpers, user config file
- `src/core/manifest.rs` — JSON manifest tracking raw documents and compilation status
- `src/core/compilation.rs` — derives the raw → wiki mapping from article `sources:` frontmatter
- `src/core/wiki.rs` — shared loader for wiki articles
- `src/core/frontmatter.rs` — YAML frontmatter parsing/rendering for wiki articles
- `src/core/links.rs` — wikilink extraction, demand ranking, link graph (forward + backlinks)
- `src/core/slug.rs` — canonical form used for all wikilink resolution
- `src/core/lint.rs` — lint rules (`analyze`), finding type, severity model, stable ordering
- `src/core/output.rs` — output format switch, JSON envelope, exit-code constants
- `src/core/text.rs` — display helpers (character-safe truncation)
- `src/commands/` — one module per CLI subcommand: init, config, schema, ingest, ingest-repo, mv, sync, status, next, uncompiled, index, lint, search, graph
- `tests/` — integration tests that drive the compiled binary against temporary archives

## Skills

Claude Code skill definitions live in `skills/{skill-name}/SKILL.md`. Each skill has YAML frontmatter with `name`, `description`, and `user-invocable: true`.

Skills are prefixed `sentinel-` to avoid namespace collisions:
- `sentinel-grow` — run the self-maintenance loop (bounded; default 3 iterations)
- `sentinel-ask` — query the knowledge base
- `sentinel-compile` — compile raw docs into wiki articles
- `sentinel-research` — research a topic and add findings
- `sentinel-improve` — health check and quality improvement

`tests/skills.rs` enforces what can be enforced about a prompt: required frontmatter keys, `name:` matching the directory, that every `sentinel <cmd>` appearing in code actually exists, that none instruct reading `index/_master.md`, that each defines empty-`$ARGUMENTS` behaviour and defers to `sentinel schema`, and that `sentinel-grow` states its budget and stop conditions. Adding a skill means satisfying those.

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

`next` also returns `progress` — `wiki_articles`, `raw_documents`, `uncompiled`, `errors` — describing what the archive *contains*, alongside `backlog` describing what is left. A loop needs both, from the same call.

**Progress is the archive advancing, not the queue shrinking.** Measured on a real archive, two write iterations moved the total backlog 15 → 14 → 13. An article on a rich concept legitimately fills one gap and opens three, which grows the backlog while making the archive richer — so a loop that halts on "backlog did not shrink" stops right after its most generative work. `/sentinel-grow` therefore measures `progress`, not `backlog`.

A `write` target carries `refs` (a sample of the articles that want it, capped at 5), `ref_count` (the exact total), and `variants` (the spellings actually used). Those are what make the recommendation actionable in one call: the referring articles define what the concept means *in this archive*, and a bare count cannot be read. Any capped list must publish its true total alongside — a truncated list that does not say so reads as complete.

`next` ranks; it does not budget. The ladder is strict priority, which is right for a single question ("what is most valuable now?") and wrong for a loop: a real ingest of eight sources makes `compile` win eight times running, so a three-iteration budget never reaches `write` — the step the archive actually grows by. That was found by ingesting a real corpus, not by inspection.

Scheduling therefore belongs to the caller. `sentinel next --action <name>` returns any category's targets regardless of priority, marked `requested: true` so a consumer can tell a scheduling choice from sentinel's advice. `backlog` is always present, so one call is enough to plan the next step too. `/sentinel-grow` uses this: never the same action more than twice in a row while another category has work — except `fix-errors`, which is never deferred, because every later judgement is made on data the errors call into question.

The rules come from `core::lint::analyze`, shared with `sentinel lint`, so the two can never drift.

## `sentinel schema` — the published contract

Emits the frontmatter fields and which are required, the accepted `origin`/`status` values, the domains this archive actually has, the directory layout, every lint rule with its severity and description, and the `next` priority ladder.

It exists so skills and agent instructions stop restating the schema in prose. Prose drifts: `/sentinel-compile` documented five domains where `DEFAULT_DOMAINS` had three, and nothing could tell which was true. Anything published here is generated from the code.

Two invariants hold this together, both enforced by tests:

- `core::frontmatter::ORIGINS` and `STATUSES` are the single source for both the lint rule and the schema output, so the checker cannot reject a value the contract advertises.
- `core::lint::RULES` and `core::lint::analyze` are asserted to agree in **both** directions — every documented rule can actually fire, and every emitted rule is documented, with matching severities.

`domains.present` is read from disk (the union of `raw/` and `wiki/` subdirectories); `domains.default` is `DEFAULT_DOMAINS`. Reporting both is deliberate — an archive that has moved past the defaults should say so.

`sentinel init` also writes a `CLAUDE.md` into the archive. The README always described this file as "the schema" that you and the LLM co-evolve, but `init` never created one, so a fresh archive had no conventions at all. It is deliberately short and mostly pointers — `sentinel schema --json` is authoritative, and anything restated in that file is something that can go stale.

## Wikilink resolution

Every match between a wikilink and an article goes through `core::slug::canonical` — lowercase, and any run of non-alphanumerics collapsed to a single `-`. So `[[Compile Loop]]`, `[[Compile-Loop]]`, and `[[compile-loop]]` all resolve to `compile-loop.md`, which is also how Obsidian behaves.

This is not cosmetic. Before it, the same concept referenced three ways produced three separate entries in `links::wanted`, each with one referrer — so `sentinel next` could rank a rarely-mentioned gap above a popular one, and could recommend writing an article that already existed under a different capitalisation. `/sentinel-grow` acting on that would have created a duplicate.

Deliberately *not* folded: plurals and stemming. `derived-state` and `derived-states` stay distinct — merging needs a stemmer, and a wrong merge silently collapses two real concepts, which is worse than a missed one.

Anything that compares a link to an article must use `canonical_slug()`, never `slug()`. `slug()` is for display.

## Partial views must not overwrite durable state

`wiki::load_all` returns `Loaded { articles, unreadable }` and never silently skips a file it could not read.

The rule that follows: **a command that rewrites derived state calls `require_complete()`; a command that only reads may proceed, but must disclose that the view was partial.**

`index` overwrites five generated files and the manifest's compilation mapping. Rebuilding from a partial view deletes everything the missing files accounted for — and it did: one unreadable article made `index` print "Index rebuilt. Articles indexed: 0", exit 0, wipe the mapping and blank `_master.md`. It now refuses and names the files.

This covers directories as well as files. `markdown_files` returns walk errors rather than dropping them — a directory that cannot be traversed hides every article inside it just as effectively as an unreadable file, and dropping the error made those articles vanish with nothing to show the listing was short.

`mv` calls `require_complete()` for the same reason `index` does: it rewrites the articles it can see and moves the file regardless, so an article it could not read keeps a citation to a path that no longer exists.

The same distinction applies to `meta/link-graph.json`. `LinkGraph::load()` returns an empty graph when the file is **absent** — no `index` has run yet, which is legitimately empty — and an error when it exists but cannot be parsed. Collapsing the two reported a confident "Orphan pages: 0" derived from an unparseable file, and dropped `connect` from `next`'s backlog as though there were nothing to do. Both now disclose it via `link_graph_error`.

`status` and `lint` carry an `unreadable` list in their JSON and print it. A count of zero articles, or a clean lint, computed over files that could not be opened is not a fact about the archive — it is a fact about what was legible, and the difference has to be visible.

## What `sync` may and may not throw away

`sync` prunes manifest entries whose file is gone. Two of an entry's fields are **not derivable from disk** — `origin` and `ingested_at` — and `title` is only derivable as the filename stem. Re-registering a document therefore resets `origin` to `authored`, which silently relabels AI-gathered research as the user's own writing. That is the one distinction the whole archive is organised around.

A hand-renamed file looks exactly like a deletion plus an addition, so this was reachable by ordinary use. `ManifestEntry.content_hash` closes it: `sync` matches missing entries against new files by content and carries the record across as a move. Genuine deletions still prune, and now print what they discard.

The hash is `DefaultHasher` over the file's bytes — not cryptographic, and it does not need to be. It exists only to recognise the same content under a new name, and a collision would carry metadata between two byte-identical files, which is the same outcome either way.

`sync` backfills the hash on entries that lack one, so archives written before the field are protected from the next rename onward.

## Moving raw documents

`sentinel mv <from> <to>` moves a raw document and rewrites every `sources:` citation that pointed at it. Reorganising `raw/` is inevitable; doing it by hand turns each citation into an `unresolved-source` error, and the repair was manual and easy to do incompletely.

Three properties it has to hold:

- **Citations are matched the same way the compile loop matches them** — via `compilation::SourceIndex` — so `./raw/d/x.md`, `d/x.md`, and a bare `x.md` are all repointed, not just the exact spelling.
- **The edit is textual, scoped to the frontmatter block, and applied to whole citation entries.** Round-tripping through serde would reorder keys and strip comments from a file the user may also edit by hand, so `frontmatter::block_end` gives the boundary and the body is never rewritten. But substring replacement inside that block is not safe either: renaming `a.md` in an article that also cites `data.md` produced `datraw/.../alpha.md`, which still resolved *by basename* to the renamed file — so provenance pointed at the wrong source and lint reported clean. `repoint_sources` rewrites entries under `sources:` only, in both YAML list forms, preserving indentation and quoting.
- **Destinations must stay under `raw/`.** It is the provenance floor — moving a source out of it would orphan every article compiled from it with no way to repair the link.

`--dry-run` reports the move and every article that would be rewritten.

## Bounded output

Every agent-facing query is bounded, because the consumer has a context window. This was measured, not assumed — on a generated archive of 423 articles and 140 sources:

| command | before | after |
|---|---|---|
| `search <common-word> --json` | 467 KB (~117k tokens) | 11 KB |
| `graph --json` | 65 KB | 1.8 KB via `--node <slug>` |
| `lint --json` | 50 KB | 315 B via `--summary` |

- `search` returns the top `--limit` (default 20) results, capped at `--matches` (default 3) excerpts each, each excerpt truncated to 200 characters. `result_count` always reports the true total and `truncated` says whether the list was cut.
- `search` ranks by relevance, not raw match count: title 1000, slug 500, tag 200, body line 1. Before this, searching a common term ranked articles that mention it in passing above the article named for it.
- `graph --node <slug> --depth <n>` returns a BFS neighbourhood over both link directions. The bare form still dumps the whole topology, which is fine for a human and wrong for an agent — the skills use `--node`.
- `lint --summary` returns per-rule counts and omits the findings array; `lint --rule <id>` lists one rule. **A filter narrows what is listed, never what is counted** — counts and the exit code always describe the whole archive, so no flag can make a broken archive look healthy.

Any new query command should assume its output lands in a context window and be bounded by default.

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

- `ingest-repo` is not implemented. It exits non-zero with guidance rather than pretending to succeed.
- Wikilink slugs are bare filename stems, so two articles whose stems canonicalise the same collide in the link graph. `sentinel lint` reports the collision; it does not resolve it.
- `meta/log.md` is append-only — commands that mutate state (init, ingest, sync, index, lint) auto-append entries
