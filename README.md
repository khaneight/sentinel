# Sentinel

CLI tooling for building a personal knowledge base with LLMs.

Most people's experience with LLMs and documents is stateless — upload files, get answers, nothing accumulates. Sentinel takes a different approach: the LLM **incrementally builds and maintains a persistent wiki** from your raw sources. When you add a document, the LLM reads it, extracts key ideas, and integrates them into an interconnected wiki — updating entity pages, noting contradictions, strengthening the evolving synthesis. The knowledge compounds over time instead of being re-derived on every query.

You never write the wiki yourself. You curate sources, ask questions, and direct the analysis. The LLM does the summarizing, cross-referencing, filing, and bookkeeping that makes a knowledge base actually useful. Sentinel is the CLI that manages the structure underneath.

Designed for use with [Claude Code](https://docs.anthropic.com/en/docs/claude-code) and [Obsidian](https://obsidian.md), but the wiki is just markdown files in a git repo.

## Install

```bash
cargo install --path .
```

This puts `sentinel` in `~/.cargo/bin/`, which should be on your PATH.

## Where your archive lives

Your archive is an ordinary directory of markdown. Create one wherever you keep things:

```bash
sentinel init ~/Documents/archive --set-default
```

`--set-default` records the path in `~/.config/sentinel/config.toml`, so every later command finds it from anywhere. Without it, sentinel resolves the archive in this order:

| Precedence | Source |
|---|---|
| 1 | `--archive <PATH>` |
| 2 | `SENTINEL_ARCHIVE` environment variable |
| 3 | `archive = "..."` in `~/.config/sentinel/config.toml` |
| 4 | the nearest parent directory containing `meta/manifest.json` |

Rule 4 means that once you `cd` into your archive — or any subdirectory of it — sentinel just works, the way git does. If none of the rules match, sentinel tells you so and lists the fixes rather than writing files somewhere you didn't intend.

```bash
sentinel config     # which archive am I pointed at, and why?
```

Multiple archives are fine — keep them apart with `--archive`, or with a `SENTINEL_ARCHIVE` export per shell.

## Skills

Sentinel ships with agent skills — slash commands that let the LLM operate on your wiki. These are where the real work happens.

### `/sentinel-compile`

Process raw documents into wiki articles. The LLM reads each source, identifies key concepts, and creates or updates wiki pages with proper frontmatter and `[[wikilinks]]`. A single source might touch 10-15 pages. Preserves the author's voice for `authored` content — distills and organizes without editorializing.

### `/sentinel-research <topic>`

Research a topic via web search and add findings to the wiki. Creates a raw research document for provenance, then compiles into wiki articles marked `origin: researched`. When updating existing authored articles with research, uses `origin: hybrid` and keeps the author's ideas separate.

### `/sentinel-ask <question>`

Query the knowledge base. Searches for relevant pages, synthesizes an answer with citations, and distinguishes between your own ideas and researched content. Valuable answers can be filed back into the wiki as new pages — so your explorations compound instead of disappearing into chat history.

### `/sentinel-improve`

Health check and quality improvement pass. Finds broken links, missing frontmatter, orphan pages, thin articles, stale drafts, and missing cross-references. Fixes straightforward issues immediately, suggests deeper improvements for your review.

### Installing skills

Symlink the skills directory into your archive's Claude Code config:

```bash
ln -s /path/to/sentinel/skills /path/to/archive/.claude/skills
```

## Usage

```bash
# 1. Initialize the archive directory structure
sentinel init

# 2. Add your raw documents
sentinel ingest path/to/essay.md -d philosophy -o authored
sentinel ingest path/to/notes.md -d research -o authored

# 3. Or drop files directly into raw/ and register them
sentinel sync

# Now spin up your agent of choice and use the following skills

# 4. Compile raw docs into wiki articles (LLM-driven)
/sentinel-compile

# 5. Research a topic and add findings (LLM-driven)
/sentinel-research "stoic ethics"

# 6. Ask questions against your wiki (LLM-driven)
/sentinel-ask "what connections exist between stoicism and free will?"

# 7. Health check and improve (LLM-driven)
/sentinel-improve
```

### CLI commands

The skills above handle the LLM-driven work. These CLI commands handle the bookkeeping:

```bash
sentinel status          # overview of archive health
sentinel uncompiled      # list raw docs no wiki article cites yet
sentinel index           # rebuild indexes and link graph
sentinel lint            # validate frontmatter, links, structure
sentinel search "query"  # full-text search across wiki
sentinel graph           # print link graph topology
sentinel log op "detail" # append to activity log (meta/log.md)
```

## Driving it from a script or an agent

Every read command takes `--json`:

```bash
sentinel status --json
sentinel lint --json
sentinel uncompiled --json
sentinel search "stoicism" --json
sentinel graph --json
sentinel config --json
```

Each payload carries `schema_version`, the `command` that produced it, and the `archive` it describes. Errors come back as JSON too, on stderr, so there is only ever one thing to parse.

Exit codes distinguish "your archive has problems" from "sentinel broke":

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | the command failed |
| 2 | the command ran and found problems |

`sentinel lint` exits 2 when it finds **errors** — malformed frontmatter, invalid `origin`/`status`, colliding slugs, a `sources:` entry pointing at nothing. Warnings alone exit 0, because an archive with uncompiled sources and forward-declared `[[wikilinks]]` is a healthy archive mid-workflow, not a broken one. Use `--strict` to fail on warnings too.

```bash
sentinel lint --json | jq -r '.findings[] | select(.severity=="error") | "\(.rule)\t\(.path)"'
```

## Architecture

Three layers, following the [LLM Wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) pattern:

**Raw sources** — your curated collection of source documents. Articles, papers, notes, transcripts. Immutable — the LLM reads from them but never modifies them. This is your source of truth.

**The wiki** — LLM-generated markdown files. Summaries, entity pages, concept pages, comparisons, synthesis. The LLM owns this layer entirely — it creates pages, updates them when new sources arrive, maintains cross-references and wikilinks, and keeps everything consistent. You read it; the LLM writes it.

**The schema** — `CLAUDE.md` tells the LLM how the wiki is structured, what conventions to follow, and what workflows to use. You and the LLM co-evolve this over time.

```
archive/
  raw/          Source documents (immutable, never modified by sentinel)
  wiki/         Compiled wiki articles (LLM-maintained, structured with frontmatter)
  index/        Auto-generated indexes (rebuilt by sentinel index)
  meta/         Machine state: manifest.json, link-graph.json, log.md
  templates/    Article templates
```

## Wiki Article Format

Every wiki article uses YAML frontmatter:

```yaml
---
title: Article Title
domain: philosophy
origin: authored | researched | hybrid
tags: [topic1, topic2]
sources:
  - raw/philosophy/source-file.md
related:
  - "[[related-article]]"
created: 2025-01-15
updated: 2025-01-15
status: draft | review | stable
---
```

The `sources:` field is what closes the loop. A raw document counts as compiled once at least one wiki article cites it, so `sentinel uncompiled` is a real work queue that empties as the wiki grows. An article with no `sources:` leaves its raw document stranded — `sentinel lint` says so.

- **authored** -- distilled from the user's own writings
- **researched** -- gathered via AI research
- **hybrid** -- user's ideas enriched with research

## License

MIT