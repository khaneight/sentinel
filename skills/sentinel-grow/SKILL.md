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
2. **Backlog empty** — `sentinel next` returns `action: "none"` **and
   `progress.link_graph_stale` is absent**. That field means the link graph no
   longer matches disk, so orphan and connection counts describe an older
   archive and `connect` work can be missing from the backlog entirely. Run
   `sentinel index`, re-run `sentinel next`, and only then treat an empty
   backlog as real. An empty backlog derived from a stale graph is the one way
   this loop can stop while there is still work to do.
3. **No progress** — an iteration finishes and **nothing in `progress` moved in
   the right direction**: `wiki_articles` did not increase, `uncompiled` did not
   decrease, and `errors` did not decrease. That means the iteration produced
   nothing durable. Report what you attempted and why you think it did not land.

   **Do not measure progress by the size of `backlog`.** Filling a gap that
   legitimately opens new ones grows the backlog while making the archive
   substantially richer — an article on *eudaimonia* that raises *oikeiosis*,
   *kathekon*, and *telos* is the loop working, not failing. Measured on a real
   archive, two write iterations moved the backlog 15 → 14 → 13; a single
   generative article would have pushed it back up and halted the loop right
   after its best work.
4. **Same target twice** — the top target is one you already worked on this run. You did not actually complete it; investigate rather than retry.
5. **Judgement required** — the recommended action needs a decision that is the
   user's to make (see *Escalate* below).
6. **A command refuses because something could not be read** — see *After
   acting* below. This is not a stall to work around; the archive is not
   fully legible and the loop cannot safely continue.

Record `progress` and the targets you have worked on after every iteration.
Conditions 3 and 4 depend on it. `progress` comes back on the same
`sentinel next` call you use to decide, so this costs nothing extra.

## Before the first iteration

```
sentinel schema --json
sentinel next --json
```

Read the contract once here rather than re-reading it inside each delegated
skill, and record the starting `progress` counts so the final report can state
what the run actually changed.

## One iteration

```
sentinel next --json
```

Note the `action`, `targets`, and the total across `backlog`.

### Scheduling: do not follow the recommendation blindly

`sentinel next` ranks; it does not budget. Its ladder is strict priority, so a
large ingest makes `compile` win every iteration — on an eight-source corpus it
recommends `compile` eight times running, and a three-iteration budget never
reaches `write` at all. `write` is the step the archive actually grows by, so a
loop that never reaches it is not a growth loop.

**Rule: do not run the same action more than twice in a row while another
category has outstanding work in `backlog`.** When you would break that rule,
ask for a different category explicitly:

```
sentinel next --action write --json
```

`--action` accepts `fix-errors`, `compile`, `write`, `connect`, `review`, and
returns that category's targets regardless of priority. The response is marked
`requested: true` so the log records that it was your scheduling choice rather
than sentinel's advice.

The one exception is **`fix-errors`, which is never deferred.** Errors mean the
archive is malformed, so every later judgement — including which gap is most
wanted — is made on data you cannot trust. Run it to completion first, however
many iterations that takes.

Then act:

### `fix-errors`

The archive is malformed. Follow `/sentinel-improve` step 2. Nothing else in the loop is trustworthy until the error count is 0, so this always runs to completion before the loop advances.

### `compile`

Raw documents nobody has cited. Follow `/sentinel-compile` for the named targets. Work **one source per iteration** — a source that becomes several articles changes the graph enough that the next recommendation should be recomputed from it.

### `write`

The wiki has linked a concept it has not written. This is the growth step: the archive is naming what it wants, ranked by how many articles ask for each.

Follow `/sentinel-research` on the top target's slug. **Before researching, read
the articles in that target's `refs`** — they define what the concept means *in
this archive*, which is usually narrower and more specific than the general
topic. An article on `[[virtue]]` written for a Stoicism-heavy wiki should not
be a general encyclopedia entry.

`targets` is itself a sample, capped at five: `target_count` is how many gaps
there actually are, and human output ends with "... and N more". Within a
target, `refs` is a sample and `ref_count` is the true total. If `variants` is present, the
concept has been spelled inconsistently across articles — name the new file
after the canonical `id`, and consider tidying the outliers.

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

**If `sentinel index` refuses**, it will say some wiki file could not be read.
It is not being cautious for its own sake: rebuilding from a partial view
deletes everything the unreadable files account for. Do not retry it, and do not
work around it. Report the named files to the user — a permissions problem, a
lock, or a sync client mid-write — and stop. The same refusal applies to
`sentinel mv` and `sentinel rm`.


```
sentinel log grow "iteration {n}: {action} — {what you did}"
```

Log every iteration, including ones that made no progress. The log is the only
durable record of what this loop did to the archive.

Read it back with `sentinel log --json`, which returns recent entries newest
first and bounded — useful at the start of a run to see what a previous one
already did, and in the final report to state what changed. Do not read
`meta/log.md` directly; it grows without bound.

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

Per iteration: the action, the target, whether it was recommended or requested,
what you wrote or changed, and `progress` before and after. If the backlog grew
because the work opened new questions, say so explicitly — that is a result, not
a regression.

Then overall:

- Which stop condition ended the run.
- Net change from `sentinel status` — articles, uncompiled sources, orphans.
- New concepts now forward-declared and unwritten. **This is the most useful part of the report**: it is what the archive wants next, discovered by doing the work.
- Anything escalated and still waiting on the user.
- Your honest assessment of whether the articles you added are worth keeping. A loop that reports only what it produced, and never that some of it was thin, is not useful oversight.
