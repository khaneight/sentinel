---
name: sentinel-grow
description: Run the archive's self-maintenance loop — repeatedly ask sentinel what to do next, do it, and re-check, until the backlog empties or the budget runs out. Use when the user wants the wiki to advance on its own. Trigger on "grow the wiki", "work the backlog", "keep improving the archive", "run the loop".
user-invocable: true
allowed-tools: Bash(sentinel:*), Read, Write, Edit, Glob, Grep, WebSearch, WebFetch
---

# Grow the Archive

Run the maintenance loop: ask what is most worth doing, do it, re-check, repeat.

Every individual step here is something the other skills already do. What this adds is the loop — and a loop that writes to the user's knowledge base needs a budget, a stop condition, and a record of what it did. All three are non-negotiable.

## Budget

Parse `$ARGUMENTS` for an iteration count (e.g. `5`, `10 iterations`).

**Default: 3 iterations.** Deliberately small. This loop writes articles into the user's archive; the cost of running too few is that they ask again, and the cost of running too many is churn they have to review and undo. Prefer too few.

Announce the budget before starting.

## Stop conditions

Stop and report when **any** of these holds. Do not push past one.

1. **Budget exhausted** — the iteration count is used up.
2. **Backlog empty** — `sentinel next` returns `action: "none"`.
3. **No progress** — an iteration finishes and the total `backlog` count has not decreased. This means the loop is not converging, and continuing will churn rather than build. Report what you attempted and why you think it did not land.
4. **Same target twice** — the top target is one you already worked on this run. You did not actually complete it; investigate rather than retry.
5. **Judgement required** — the recommended action needs a decision that is the user's to make (see *Escalate* below).

Track the `backlog` totals and the targets you have worked on across iterations. Conditions 3 and 4 depend on it.

## Before the first iteration

```
sentinel schema --json
sentinel status --json
```

Read the contract once here rather than re-reading it inside each delegated
skill, and record the starting counts so the final report can state what the
run actually changed.

## One iteration

```
sentinel next --json
```

Note the `action`, `targets`, and the total across `backlog`. Then act:

### `fix-errors`

The archive is malformed. Follow `/sentinel-improve` step 2. Nothing else in the loop is trustworthy until the error count is 0, so this always runs to completion before the loop advances.

### `compile`

Raw documents nobody has cited. Follow `/sentinel-compile` for the named targets. Work **one source per iteration** — a source that becomes several articles changes the graph enough that the next recommendation should be recomputed from it.

### `write`

The wiki has linked a concept it has not written. This is the growth step: the archive is naming what it wants, ranked by how many articles ask for each.

Follow `/sentinel-research` on the top target's slug. Before researching, read the articles listed in that target's referrers — they define what the concept means *in this archive*, which is usually narrower and more specific than the general topic. An article on `[[virtue]]` written for a Stoicism-heavy wiki should not be a general encyclopedia entry.

Write exactly one article, then re-run `next`. Writing it will usually create new forward-declared links, and those change what is most wanted.

### `connect`

Orphans. Follow `/sentinel-improve` step 4. Only add links that are genuinely warranted — a fabricated connection corrupts the graph this whole loop steers by, and it will keep steering by it after you are gone.

### `review`

Stalled drafts. Read each and either finish it or promote its `status`. If a draft is stalled because it needs the user's input, that is an escalation, not a task.

### After acting

```
sentinel index
sentinel lint --summary
```

`lint` must exit 0. If your work introduced an **error**, fix it before the next iteration — never carry one forward.

```
sentinel log grow "iteration {n}: {action} — {what you did}"
```

Log every iteration, including ones that made no progress. The log is the only durable record of what this loop did to the archive.

## Escalate rather than decide

Stop and ask the user when:

- Two articles contradict each other. Reconciling `authored` content is the user's call — the disagreement may be their own thinking having changed.
- The recommended action would require deleting an article or removing a source citation.
- Research on a `write` target finds the concept is genuinely contested, and choosing a framing would put words in the user's mouth.
- A `compile` target is ambiguous enough that you would be inventing its meaning rather than distilling it.

## Never

- Modify anything in `raw/`. It is immutable; it is the provenance floor the whole archive rests on.
- Delete a wiki article.
- Delete `[[wikilinks]]` to unwritten concepts. They are the loop's fuel, not defects.
- Create a stub article to clear a `broken-link` warning. An empty page satisfies the linter, destroys the demand signal, and adds nothing.
- Continue past a stop condition because the next step looks easy.

## Report

Per iteration: the action, the target, what you wrote or changed, and the backlog count before and after.

Then overall:

- Which stop condition ended the run.
- Net change from `sentinel status` — articles, uncompiled sources, orphans.
- New concepts now forward-declared and unwritten. **This is the most useful part of the report**: it is what the archive wants next, discovered by doing the work.
- Anything escalated and still waiting on the user.
- Your honest assessment of whether the articles you added are worth keeping. A loop that reports only what it produced, and never that some of it was thin, is not useful oversight.
