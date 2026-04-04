---
name: sentinel-compile
description: Compile raw documents into wiki articles. Use when there are uncompiled raw docs that need to be distilled into structured wiki knowledge. Trigger on "compile the wiki", "process raw docs", "compile uncompiled".
user-invocable: true
---

# Compile Raw Documents into Wiki Articles

You are compiling raw source documents from `raw/` into structured wiki articles in `wiki/`. The user's raw writings are **authored** content — preserve their voice, ideas, and arguments faithfully. Your job is to distill and organize, not to editorialize.

## Step 1: Identify what needs compiling

Run: `sentinel uncompiled`

If a specific file was provided as $ARGUMENTS, focus on that file only. Otherwise, process all uncompiled documents.

## Step 2: Read each raw document

For each uncompiled raw doc:
1. Read the full document carefully
2. Identify the key concepts, arguments, and themes
3. Note which existing wiki articles (if any) are related — check `index/_master.md`

## Step 3: Write wiki articles

For each raw document, create one or more wiki articles in `wiki/{domain}/`:

**Filename**: `kebab-case.md` using the core concept (e.g., `semi-compatibilism.md`, `problem-of-other-minds.md`)

**Required frontmatter**:
```yaml
---
title: Human-Readable Title
domain: philosophy | anthropology | religion | coding | research
origin: authored
tags: [relevant, topic, tags]
sources:
  - raw/domain/source-filename.md
related:
  - "[[other-article]]"
created: YYYY-MM-DD
updated: YYYY-MM-DD
status: draft
---
```

**Body guidelines**:
- Faithfully represent the author's arguments and ideas
- Structure with clear headings
- Use `[[wikilinks]]` to link related concepts — even if the target article doesn't exist yet (it creates a natural TODO for future compilation)
- For philosophical/academic writings: preserve the thesis, key arguments, and conclusions
- Keep the author's intellectual voice — don't flatten or oversimplify

## Step 4: Cross-reference

After writing articles:
- Check if any existing wiki articles should link to the new ones
- Update their `related` frontmatter and add `[[wikilinks]]` in the body where natural
- Don't force connections — only link genuinely related concepts

## Step 5: Rebuild indexes

Run: `sentinel index`

## Step 6: Validate

Run: `sentinel lint`

Report any issues found and fix them if straightforward (missing frontmatter fields, etc.). Don't fix broken links to articles that simply haven't been written yet — those are natural TODOs.

## Step 7: Log

Run: `sentinel log compile "{N} articles created/updated from {M} raw docs: {list of filenames}"`

## Step 8: Summary

Report what was compiled:
- How many raw docs processed
- How many wiki articles created/updated
- Key concepts identified
- Notable connections between articles
