# Sentinel

CLI tooling for a personal knowledge base that an LLM builds and maintains.

Most work with LLMs and documents is stateless: upload, ask, discard. Sentinel keeps the result. You curate raw sources and ask questions; the agent compiles them into an interconnected wiki of markdown files, and the knowledge compounds instead of being re-derived every time.

You never write the wiki yourself. Sentinel is the CLI underneath — it tracks provenance, finds the gaps, and tells the agent what is most worth doing next.

Built for [Claude Code](https://docs.anthropic.com/en/docs/claude-code) and [Obsidian](https://obsidian.md), but the archive is just markdown in a git repo.

## Install

```bash
cargo install --path .          # puts `sentinel` in ~/.cargo/bin
sentinel init ~/Documents/archive --set-default
ln -s /path/to/sentinel/skills ~/Documents/archive/.claude/skills
```

## Quick start

```bash
sentinel ingest paper.md -d philosophy -o researched -t "Some Paper"
sentinel next                   # → compile
```

Then, from an agent in the archive:

```
/sentinel-grow                  # work the backlog until it empties
```

`sentinel next` is the centre of the tool. It reads the whole archive and names the single most valuable thing to do, in priority order:

**fix errors → compile uncompiled sources → write concepts the wiki links but hasn't covered → connect orphans → revisit stalled drafts**

```console
$ sentinel next
Next: write
  2 concept(s) linked but not yet written; 'virtue' is referenced by 2 article(s)

  • virtue — referenced by 2 articles
  • ataraxia — referenced by wiki/philosophy/stoicism.md

  run: /sentinel-research virtue
```

The third rung is where the wiki feeds itself. Writing `[[virtue]]` in an article when no such page exists tells the archive what it is missing; `next` ranks those gaps by how many articles ask for each. Filling one usually creates new links, which name the next gap.

It converges rather than running away. On a real corpus, filling five top-ranked gaps took the outstanding count from 8 to 3 and orphans to zero — early articles name concepts that don't exist yet, later ones mostly link to pages already written.

## Skills

Symlinked into the archive's `.claude/skills`, these are where the work happens. They all read the archive through `--json` and take the frontmatter contract from `sentinel schema` rather than restating it, so they keep working as the archive outgrows reading its own index.

| | |
|---|---|
| `/sentinel-grow [n]` | Runs the loop: ask `next`, do it, re-check. **Bounded** — 3 iterations by default, stopping early when the backlog empties, a pass makes no progress, or something needs your judgement. Never touches `raw/`, never deletes an article, never stubs a page to silence a warning. Every iteration lands in `meta/log.md`. |
| `/sentinel-clone [doc]` | Read a document the user wrote and record how they write and what they hold as cited `persona/` traits. Every claim carries `evidence:` from their own writing; nothing is marked `affirmed` except by them. |
| `/sentinel-extend [trait]` | Write a new `origin: extrapolated` article from traits the user has **affirmed** — their thinking extended, not summarised. Marked as the machine's, attributed to the traits it rests on, and unpublishable until they approve it. |
| `/sentinel-compile` | Turn raw documents into wiki articles. One source may touch a dozen pages. Preserves the author's voice for `authored` material. |
| `/sentinel-research <topic>` | Research via web search, file the trail under `raw/` as `origin: researched`, then compile it. |
| `/sentinel-ask <question>` | Answer from the wiki with citations. Offers to file the answer back **only** when it found a connection no article records — otherwise a knowledge base fills with restatements of what it already knew. |
| `/sentinel-improve` | Fix errors, work warnings that represent real loss, connect orphans, revisit stale drafts. |

## CLI

```bash
sentinel next                 # what to do next  (--json, --action <rung>)
sentinel status               # counts and health
sentinel schema               # frontmatter contract, domains, lint rules
sentinel persona              # the archive's model of its author  (--kind, --affirmed)
sentinel lint                 # validate frontmatter, links, manifest  (--summary, --strict, --rule X)
sentinel index                # rebuild indexes, link graph, dashboard
sentinel search "query"       # ranked, top 20  (--limit, --matches)
sentinel graph --node X       # one article's neighbourhood  (--depth N)
sentinel uncompiled           # raw docs no article cites yet
sentinel ingest F -d D        # register a source  (-o origin, -t title, --as name)
sentinel sync                 # register files dropped into raw/ by hand
sentinel mv old new           # move a source, repointing every citation
sentinel rm target            # delete a source, refusing if articles cite it
sentinel sources              # raw docs and which may be published  (--publish, --private)
sentinel review               # what needs your verdict  (<target> --approve|--reject)
sentinel export --out DIR     # the publishable subset  (--flat, --clean, --data, --with-sources)
sentinel log op "detail"      # append to the activity log; bare `log` reads it
sentinel config               # which archive am I pointed at, and why?
```

## Where the archive lives

Resolved in this order, first match wins:

| | |
|---|---|
| 1 | `--archive <PATH>` |
| 2 | `SENTINEL_ARCHIVE` |
| 3 | `archive = "…"` in `~/.config/sentinel/config.toml` (what `--set-default` writes) |
| 4 | nearest parent directory containing `meta/manifest.json` |

Rule 4 means sentinel works from anywhere inside the archive, the way git does. No match is an error listing the fixes, never a guess. `sentinel config` reports which rule applied.

```
archive/
  raw/          source documents — immutable, never modified by sentinel
  wiki/         compiled articles — the agent owns this layer
  index/        generated: _master, _by-domain, _recent, _orphans, _uncompiled, _dashboard
  meta/         manifest.json, link-graph.json, log.md
  templates/    article templates, generated from the frontmatter contract
```

`index/_dashboard.md` is the one page to read: the current recommendation, every backlog rung, health by rule, progress, recent activity, and what the agent is instructed to do. Regenerated by `sentinel index`, and `sentinel status` tells you when it has fallen behind.

## Article format

```yaml
---
title: Article Title
domain: philosophy
origin: authored | researched | hybrid
tags: [topic, other]
sources:
  - raw/philosophy/source-file.md
related:
  - "[[related-article]]"
created: 2025-01-15
updated: 2025-01-15
status: draft | review | stable
---
```

`sources:` closes the loop. A raw document counts as compiled once some article cites it, which makes `sentinel uncompiled` a work queue that empties as the wiki grows. `origin` records whether the content is the user's own writing (`authored`), gathered by research (`researched`), or both (`hybrid`) — it cannot be recovered from the file later, so `ingest -o` is worth getting right.

## Publishing

```bash
sentinel export --out ./content --flat --clean
```

Writes only articles whose `status` qualifies (`stable` by default), rewrites links to unpublished articles as plain text so the output has no dead ends, and never copies `raw/` or `meta/`.

Anything the clone wrote (`origin: extrapolated`) publishes **only once you have approved it** — `sentinel review <slug> --approve` — and `export` appends an attribution notice to it that no flag suppresses. Finished is not the same as signed. It renders no HTML — feed it to [Quartz](https://quartz.jzhao.xyz) or any generator that understands wikilinks.

`--data <file>` also emits a JSON bundle — published nodes and edges, plus the growth history from `meta/progress.jsonl` — for a front end to render.

[`docs/publishing.md`](docs/publishing.md) has the verified Quartz setup and self-hosting options.

## Scripting

Every read command takes `--json`. Each payload carries `schema_version`, the `command`, and the `archive` it describes. Errors are JSON too, on stderr, so there is one thing to parse.

| Exit | Meaning |
|---|---|
| 0 | success |
| 1 | the command failed |
| 2 | it ran and found problems |

`lint` exits 2 on **errors** — malformed frontmatter, a missing required field, an invalid `origin`/`status`/date, colliding slugs, a `sources:` entry matching nothing, a manifest entry with no file. Warnings alone exit 0: an archive with uncompiled sources and forward-declared `[[wikilinks]]` is healthy mid-workflow, not broken. `--strict` fails on warnings too.

```bash
sentinel lint --json | jq -r '.findings[] | select(.severity=="error") | "\(.rule)\t\(.path)"'
```

## Design

Three layers, following the [LLM Wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) pattern: **raw sources** you curate and sentinel never modifies; **the wiki** the agent owns entirely; and **the schema**, published by `sentinel schema` and generated from the code so it cannot drift from what the tool enforces.

[`CLAUDE.md`](CLAUDE.md) states the invariants; [`docs/design-notes.md`](docs/design-notes.md) explains what went wrong to produce each one.

## License

MIT
