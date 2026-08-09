---
name: sentinel-research
description: Research a topic and add findings to the wiki. Use when the user wants to expand the knowledge base with AI-researched content on a specific topic. Trigger on "research X", "find out about X", "add research on X".
user-invocable: true
allowed-tools: Bash(sentinel:*), Read, Write, Edit, Glob, Grep, WebSearch, WebFetch
---

# Research a Topic and Add It to the Wiki

Research **$ARGUMENTS** and file the findings, clearly marked as researched rather than authored.

## Scope

- `$ARGUMENTS` names a topic → research that.
- `$ARGUMENTS` is empty → run `sentinel next --action write --json`. Its top target is the concept the wiki most wants filled in; research that and say so. Read the articles in that target's `refs` first — they tell you what the concept means here. If nothing is wanted, report that and ask rather than picking a topic yourself.

## Step 1: Learn the contract and what is already known

```
sentinel schema --json
sentinel search "<topic>" --json
```

Read the articles the search surfaces. **Do not read `index/_master.md`** — it grows with the archive and will fill your context before you have researched anything.

Knowing what is already recorded is what keeps this from producing a parallel article restating what the user already wrote. If the topic is well covered, the right output may be enriching one existing article rather than creating anything.

## Step 2: Research

Use web search and fetch. Prefer primary sources and established references over summaries of summaries. Where a question is genuinely contested, represent the disagreement rather than picking a side.

Track which claim came from which source as you go — you need it in step 3 and reconstructing it afterwards is unreliable.

## Step 3: File the research trail

Create `raw/{domain}/research-{topic-slug}.md` with your notes, findings, and full source list including URLs.

```yaml
---
title: "Research: {Topic}"
domain: {domain}
origin: researched
ingested: YYYY-MM-DD
---
```

This is the provenance record. Someone reading the wiki article in a year should be able to get from it back to where the claim came from.

```
sentinel sync
```

## Step 4: Compile into articles

Create or update articles in `wiki/{domain}/`, following `sentinel schema`.

- New article from research alone → `origin: researched`.
- Existing `authored` article you are enriching → change to `origin: hybrid`, and **put the research in its own clearly headed section**. Do not interleave your findings with the user's argument. Their reasoning must remain legible as theirs; a reader has to be able to tell where their thinking ends and yours begins.
- Never modify the raw document the authored article was compiled from.

Always cite: `sources:` must include the research document you just created, and any raw document you drew on. Link to existing articles with `[[wikilinks]]`, and forward-declare concepts the research raised that the wiki does not cover yet.

## Step 5: Rebuild and validate

```
sentinel index
sentinel lint
```

**If `sentinel index` refuses**, it will say some wiki file could not be read.
It is not being cautious for its own sake: rebuilding from a partial view
deletes everything the unreadable files account for. Do not retry it, and do not
work around it. Report the named files to the user — a permissions problem, a
lock, or a sync client mid-write — and stop. The same refusal applies to
`sentinel mv` and `sentinel rm`.


Fix every **error**. Leave `broken-link` warnings alone — they are the gaps your research just identified, and `sentinel next` will rank them.

## Step 6: Log

```
sentinel log research "{topic}: {N} articles created/updated: {filenames}"
```

## Step 7: Report

- What you researched and what you found.
- Articles created vs. enriched, and which were switched to `hybrid`.
- **Where the sources disagreed**, and how you represented that.
- What you could not establish, or found only weak sourcing for. Say this plainly — an unmarked gap in a knowledge base is worse than a marked one.
- Concepts the research surfaced that are now forward-declared and unwritten.
