# Sentinel

Clone yourself from your corpus of work.

Point sentinel at what you have written. It reads you — how you argue, what you hold, the moves you make — and records each as a **cited** claim you can check, correct, or reject. It compiles your sources into an interconnected wiki. Then it writes new work that extends your thinking, in your voice, researched where it needs research, and holds it for your approval before anyone else sees it.

Most work with LLMs is stateless: upload, ask, discard. This keeps the result, and the result compounds.

You never write the wiki yourself. Sentinel is the CLI underneath — it tracks provenance, finds the gaps, and tells the agent what is most worth doing next.

Built for [Claude Code](https://docs.anthropic.com/en/docs/claude-code) and [Obsidian](https://obsidian.md), but the archive is just markdown in a git repo.

## What keeps it honest

A tool that writes in your voice, states your beliefs, and publishes under your name is an impersonation engine if it is careless. Six things stop that, and each is a lint error or a refusal rather than an instruction an agent can talk itself out of:

1. **No uncited claim about you.** A `persona/` trait with no `evidence:` fails lint. A profile you cannot audit is one you cannot correct.
2. **Beliefs only from your own writing.** Evidence must be `authored` or `hybrid` material. Research records what you *read*; a profile built from a reading list describes the reading list.
3. **Generated work is marked by the exporter**, not by the agent that wrote it — so the agent cannot leave the notice out.
4. **Nothing generated publishes without your verdict.** `approved` is a separate axis from `stable`: finished is not signed, and no flag opens that gate.
5. **A rejection is durable.** It stays on the file, so the loop cannot overrule it next iteration.
6. **Source material is published one document at a time.** `raw/` is never copied wholesale — nothing about a file says whether it is yours to publish.

[`docs/clone.md`](docs/clone.md) is the design and the reasoning.

## Install

```bash
cargo install --path .          # puts `sentinel` in ~/.cargo/bin
sentinel init ~/Documents/archive --set-default
ln -s /path/to/sentinel/skills ~/Documents/archive/.claude/skills
```

## Quick start

```bash
sentinel ingest essay.md -d philosophy -o authored -t "On Teaching"
sentinel next                   # → compile
```

`-o authored` is what marks a document as *yours* — the corpus the clone reads. Use `-o researched` for anything you did not write; it cannot be evidence for what you think.

Then, from an agent in the archive:

```
/sentinel-grow                  # work the backlog until it empties
```

`sentinel next` is the centre of the tool. It reads the whole archive and names the single most valuable thing to do, in priority order:

**fix errors → compile sources → learn your voice → write the concepts the wiki names → connect orphans → extend your thinking → revisit stalled drafts**

`learn` sits below `compile` because compiling a document *is* the close reading that makes mining it cheap; `extend` sits near the bottom because generating new work on top of a broken archive compounds whatever is wrong with it. Approval is deliberately **not** a rung — the agent cannot sign its own work, so `sentinel next` reports what is waiting on you and the loop stops rather than piling up unreviewed prose.

```console
$ sentinel next
Next: write
  2 concept(s) linked but not yet written; 'virtue' is referenced by 2 article(s)

  • virtue — referenced by 2 articles
  • ataraxia — referenced by wiki/philosophy/stoicism.md

  run: /sentinel-research virtue
```

`write` is where the wiki feeds itself: writing `[[virtue]]` when no such page exists tells the archive what it is missing, and `next` ranks those gaps by how many articles ask for each. It converges rather than running away — on a real corpus, filling five top-ranked gaps took the outstanding count from 8 to 3 and orphans to zero.

## Skills

Symlinked into the archive's `.claude/skills`, these are where the work happens. They all read the archive through `--json` and take the frontmatter contract from `sentinel schema` rather than restating it, so they keep working as the archive outgrows reading its own index.

| | |
|---|---|
| `/sentinel-grow [n]` | Runs the loop: ask `next`, do it, re-check. **Bounded** — 3 iterations by default, stopping when the backlog empties, a pass makes no progress, or something needs your judgement. Never touches `raw/`, never deletes an article, never stubs a page to silence a warning. |
| `/sentinel-clone [doc]` | Read a document the user wrote and record how they write and what they hold as cited `persona/` traits. Every claim carries `evidence:` from their own writing; nothing is marked `affirmed` except by them. |
| `/sentinel-extend [trait]` | Write a new `origin: extrapolated` article from traits the user has **affirmed** — their thinking extended, not summarised. Marked as the machine's, attributed to the traits it rests on, and unpublishable until they approve it. |
| `/sentinel-compile` | Turn raw documents into wiki articles. One source may touch a dozen pages. Preserves the author's voice for `authored` material. |
| `/sentinel-research <topic>` | Research via web search, file the trail under `raw/` as `origin: researched`, then compile it. |
| `/sentinel-ask <question>` | Answer from the wiki with citations. Files the answer back **only** when it found a connection no article records — otherwise a knowledge base fills with restatements of what it already knew. |
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

Rule 4 means sentinel works from anywhere inside the archive, the way git does. No match is an error, never a guess; `sentinel config` reports which rule applied.

```
archive/
  raw/          source documents — immutable, never modified by sentinel
  persona/      cited traits: how you write, what you hold
  wiki/         compiled and generated articles — the agent owns this layer
  index/        generated: _master, _by-domain, _recent, _orphans, _uncompiled, _dashboard
  meta/         manifest.json, link-graph.json, log.md, progress.jsonl
  templates/    article and trait templates, generated from the contracts
```

`index/_dashboard.md` is the one page to read: the recommendation, every backlog rung, health by rule, and recent activity. Regenerated by `sentinel index`; `sentinel status` says when it has fallen behind.

## Persona traits

`persona/` is what the archive holds about *you*, one claim per file. It is the whole basis of anything written in your voice, so every field on it exists to make the claim checkable.

```yaml
---
id: argues-from-cases
kind: style | pattern | principle | value | belief
claim: Builds an argument from a concrete case before generalising.
evidence:
  - raw/essays/on-teaching.md      # your own writing, never research
confidence: high | medium | low
status: proposed | affirmed | rejected
---

The passage that supports it, quoted.
```

An agent writes traits as `proposed` and can never mark one `affirmed`. That is yours:

```bash
sentinel persona                          # the whole profile, and how much of your corpus it rests on
sentinel review                           # what is waiting on you
sentinel review argues-from-cases --approve
sentinel review some-trait --reject --note "not what I think"
```

Verdicts append to the file, so the history travels with it and a rejection survives every rebuild. Only `affirmed` traits can be written from, and only `affirmed` traits are published.

## Article format

```yaml
---
title: Article Title
domain: philosophy
origin: authored | researched | hybrid | extrapolated
tags: [topic, other]
persona:
  - argues-from-cases              # required when origin is extrapolated
sources:
  - raw/philosophy/source-file.md
related:
  - "[[related-article]]"
created: 2025-01-15
updated: 2025-01-15
status: draft | review | stable
---
```

`sources:` closes the loop. A raw document counts as compiled once some article cites it, which makes `sentinel uncompiled` a work queue that empties as the wiki grows.

`origin` records whether the content is your own writing (`authored`), gathered by research (`researched`), both (`hybrid`), or written by the clone (`extrapolated`). It cannot be recovered from the file later, so `ingest -o` is worth getting right — and the first three are the only ones a *raw* document may have, because `raw/` is the provenance floor and a generated file sitting in it could later be cited as evidence for what you believe.

An `extrapolated` article must name the `persona:` traits it was written from, and they must be ones you affirmed. Writing from a rejected trait is an error.

## Publishing

```bash
sentinel export --out ./content --flat --clean --ui ./showcase
```

Two artifacts. **`--out`** is the wiki for reading: articles whose `status` qualifies (`stable` by default), links to unpublished pages rewritten as plain text so nothing dead-ends, and no HTML — feed it to [Quartz](https://quartz.jzhao.xyz) or any generator that understands wikilinks. **`--ui`** is one self-contained page showing the archive as a working system: a graph in three rings — source material at the core, the persona distilled from it in the middle, the clone's work at the rim — that you can pan, zoom, filter by topic, and read documents in. Hovering a node lights up what it is related to. No build step, no network beyond its own JSON; copy it anywhere static. (`--data <file>` emits that JSON alone if you would rather render it yourself.)

`raw/` and `meta/` are never copied. `--with-sources` publishes the source documents you marked with `sentinel sources --publish`, and only those.

Anything the clone wrote publishes **only once you have approved it**, and `export` appends an attribution notice that no flag suppresses. Finished is not the same as signed.

[`docs/publishing.md`](docs/publishing.md) has the verified Quartz setup, the showcase, and self-hosting options.

## Scripting

Every read command takes `--json`. Each payload carries `schema_version`, the `command`, and the `archive` it describes. Errors are JSON too, on stderr, so there is one thing to parse.

| Exit | Meaning |
|---|---|
| 0 | success |
| 1 | the command failed |
| 2 | it ran and found problems |

`lint` exits 2 on **errors** — anything malformed, ambiguous, or lying, including every safeguard above. Warnings alone exit 0: an archive with uncompiled sources and forward-declared `[[wikilinks]]` is healthy mid-workflow, not broken. `--strict` fails on warnings too. `sentinel schema` lists every rule.

```bash
sentinel lint --json | jq -r '.findings[] | select(.severity=="error") | "\(.rule)\t\(.path)"'
```

## Design

Three layers, following the [LLM Wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) pattern: **raw sources** you curate and sentinel never modifies; **the wiki** the agent owns entirely; and **the schema**, published by `sentinel schema` and generated from the code so it cannot drift from what the tool enforces.

[`CLAUDE.md`](CLAUDE.md) states the invariants; [`docs/design-notes.md`](docs/design-notes.md) explains what went wrong to produce each one; [`docs/architecture.md`](docs/architecture.md) maps the pieces and where they are going.

## License

MIT
