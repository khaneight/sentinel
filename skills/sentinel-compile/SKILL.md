---
name: sentinel-compile
description: Compile raw documents into wiki articles. Use when there are uncompiled raw docs that need to be distilled into structured wiki knowledge. Trigger on "compile the wiki", "process raw docs", "compile uncompiled".
user-invocable: true
allowed-tools: Bash(sentinel:*), Read, Write, Edit, Glob, Grep
---

# Compile Raw Documents into Wiki Articles

Distil raw source documents in `raw/` into structured articles in `wiki/`.

The user's `authored` writing is theirs. Your job is to distil and organize it — preserve the thesis, the arguments, and the intellectual voice. Do not editorialize, flatten, or "improve" their reasoning.

## Scope

- `$ARGUMENTS` is a raw document path → compile only that document.
- `$ARGUMENTS` is empty → run `sentinel uncompiled --json` and compile the whole queue, or the first few if it is large. Say which you chose.

## Step 1: Learn the contract

```
sentinel schema --json
```

This is authoritative: frontmatter fields, which are required, the accepted `origin` and `status` values, and the domains this archive actually has. Do not rely on what you remember the schema to be, and do not copy a frontmatter block from another skill — read it from here.

## Step 2: Find the work

```
sentinel uncompiled --json
```

Returns `{count, documents: [{raw_path, title, domain, origin, ...}]}`.

**Do not read `index/_master.md`.** It contains every article in the archive and will consume your context before you have written anything. To find out whether a concept already has an article, use `sentinel search "<concept>" --json` — it returns paths, titles, and matching lines.

## Step 3: Read each source

Read the raw document in full. Identify the concepts, arguments, and themes that deserve their own article. One dense source often becomes several articles; a thin one may become part of an existing article instead.

Before creating an article, `sentinel search` for its core concept. If a close article already exists, extend it rather than creating a near-duplicate — two articles on one idea split its backlinks and weaken both.

## Step 4: Write

One file per concept at `wiki/{domain}/{kebab-case-slug}.md`.

**Slugs must be unique across the whole wiki, not just within a domain.** A wikilink target is a bare filename stem, so `wiki/philosophy/ethics.md` and `wiki/coding/ethics.md` collide into one node in the link graph. `sentinel search` before naming a file; if the obvious slug is taken, qualify it (`stoic-ethics`, `engineering-ethics`).

Frontmatter must follow `sentinel schema`. Two fields carry weight beyond validation:

- **`sources:`** — archive-relative paths of the raw documents this came from. **This is what marks a raw document as compiled.** Omit it and the source sits in the uncompiled queue forever, no matter how well you wrote the article.
- **`related:`** — wikilinks to genuinely related articles. Do not pad this.

In the body:

- Structure with clear headings.
- Link concepts with `[[wikilinks]]` **including ones that have no article yet**. An unresolved link is not an error here — it is how the archive records what it wants next, and `sentinel next` ranks those gaps by how many articles ask for each. Forward-declaring a real concept is doing future work a favour; inventing links to concepts the source never raised is noise.
- For philosophical or academic material, preserve the thesis, the supporting arguments, and the conclusion. If the author's position is subtle or hedged, keep it subtle and hedged.

## Step 5: Cross-reference

For each new article, check whether existing articles should point at it — `sentinel search "<concept>" --json` will find them. Add the link in the body where it reads naturally and update `related:`. Only genuine connections; a forced link is worse than no link because it pollutes the graph the whole tool reasons over.

Update `updated:` on any article you modify.

## Step 6: Rebuild and validate

```
sentinel index
sentinel lint
```

`lint` exits 2 if there are **errors** and 0 if there are only warnings.

- **Fix every error.** Malformed frontmatter, invalid `origin`/`status`, duplicate slugs, a `sources:` entry pointing at nothing.
- **Do not "fix" `broken-link` warnings** by deleting the link or stubbing an empty article. They are the intended output of step 4.
- `missing-sources` on an article you just wrote is a real mistake — go back and add the citation.

## Step 7: Log

```
sentinel log compile "{N} articles from {M} raw docs: {filenames}"
```

## Step 8: Report

- Raw documents processed, articles created vs. updated.
- Key concepts identified, and connections you found between them.
- Concepts you forward-declared but did not write, and why.
- Anything in the sources you deliberately did **not** compile, and why.
