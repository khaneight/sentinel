---
name: sentinel-improve
description: Review and improve wiki articles — find gaps, strengthen content, fix quality issues. Use when the user wants to enhance the knowledge base quality. Trigger on "improve the wiki", "review articles", "enhance quality", "health check".
user-invocable: true
allowed-tools: Bash(sentinel:*), Read, Write, Edit, Glob, Grep
---

# Review and Improve the Wiki

A health check and repair pass.

## Scope

- `$ARGUMENTS` names a domain or topic → focus there.
- `$ARGUMENTS` is empty → work from what `sentinel next` recommends, then widen if there is room.

## Step 1: Assess

```
sentinel next --json
sentinel status --json
sentinel lint --summary --json
sentinel schema --json
```

`next` gives the highest-value action plus a `backlog` count for every category.

**Start with `lint --summary`**, which returns counts per rule and omits the
findings themselves. One root cause usually produces many findings, so the
shape of the problem is what you want first — and on a large archive the full
list is tens of kilobytes. Then pull one rule at a time with
`sentinel lint --rule <id> --json` as you work it.

**Do not read `index/_master.md`.** Use `sentinel search --json` to reach specific articles.

## Step 2: Fix errors first

Every `severity: "error"` finding, in this order — earlier ones can mask later ones:

| Rule | Fix |
|---|---|
| `invalid-frontmatter` | Repair the block. The message names the failure — a padded or mis-dashed `---` delimiter, a fence that never closes, or a YAML parse error. **Do not add fields**; the fields are usually already there and unreadable. |
| `duplicate-slug` | Rename one file so its stem is unique wiki-wide, then update every `[[wikilink]]` that pointed at it. Search for the old slug before renaming. |
| `missing-field` | Add `title`, `domain`, or `origin`. Infer from the article's content and location; do not guess `origin` — check whether the source is the user's writing or research. |
| `invalid-origin` / `invalid-status` | Correct to a value `sentinel schema` lists. |
| `invalid-date` | Rewrite `created`/`updated` as `YYYY-MM-DD`. A date the tool cannot read, or one in the future, keeps the article out of the `review` step permanently — so this is worth fixing even though the prose is fine. Use the file's real modification date, not today's. |
| `unresolved-source` | The article cites a raw document that does not exist or is ambiguous. Find the real path with `sentinel uncompiled --json` or by looking in `raw/`. If the source was renamed or moved, use `sentinel mv` from now on — it repoints every citation in one step. If it is genuinely gone, remove the citation and say so in the report; do not invent one. |

Re-run `sentinel lint` after this pass. It should exit 0.

## Step 3: Work the warnings that represent real loss

Not all warnings should be "fixed":

- **`missing-sources`** — worth fixing. The article's raw document is stranded in the uncompiled queue. Find what it was compiled from and cite it. If it genuinely has no raw source (a pure synthesis article), leave it and note it.
- **`missing-tags`** — worth fixing, cheaply.
- **`uncompiled-source`** — do not fix here. That is `/sentinel-compile`'s job; recommend it.
- **`broken-link`** — **do not fix.** These are the wiki naming its own gaps and are the input to `sentinel next`'s `write` recommendation. Deleting them destroys the signal. The only broken links worth touching are genuine typos, where a near-identical slug already exists — check with `sentinel search` before assuming.

## Step 4: Improve what lint cannot see

- **Orphans.** `sentinel next --json` reports the count; `index/_orphans.md` lists them. An orphan is real knowledge that cannot be reached by following the graph. Find articles that should link to it and add the link where it reads naturally. Do not add a link just to clear the warning — a forced connection is worse than an orphan, because it corrupts the graph everything else reasons over.
- **Thin articles.** A stub that only restates its title is not knowledge. Either expand it from its `sources:`, or merge it into a fuller article and redirect the links.
- **Stale drafts.** `status: draft` untouched for months. Read it: if it is finished, promote to `review`. If it is abandoned, say so and ask.
- **Sources that should not be there.** If a raw document is genuinely obsolete,
  `sentinel rm` deletes it — and refuses if any article cites it, naming them,
  because that would sever the provenance trail permanently. Do not pass
  `--force` on your own judgement; deleting cited provenance is the user's call.
- **Contradictions.** Two articles asserting incompatible things is the most valuable finding in this whole skill and the only one no tooling can detect. Report it; do not silently resolve it — especially not between `authored` articles, where the disagreement may be the user's own thinking having changed and is theirs to reconcile.

## Step 5: Rebuild and verify

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


Confirm the error count reached 0 and that the warning count moved in the direction you intended. If a fix *increased* findings, say so rather than burying it.

## Step 6: Log

```
sentinel log improve "{N} errors fixed, {M} warnings resolved: {summary}"
```

## Step 7: Report

- Errors fixed, by rule.
- Warnings resolved, and **which warnings you deliberately left**, with the reason.
- Contradictions or quality problems found that need the user's judgement.
- Before/after counts from `sentinel status`.
- What you would do next, and why you stopped where you did.
