# The clone

Sentinel's tagline is *clone yourself from your corpus of work*: a system that
learns how you write, what you argue from, and what you believe — and then
produces new work that extends your thinking, with research, in your voice.

This document is the design. It is deliberately written before the code,
because the feature's failure modes are not bugs. A tool that writes in a real
person's voice, asserts their beliefs, and publishes it under their name is an
impersonation engine if it is careless. Most of what follows is structure
whose purpose is to make careless impossible.

## What exists today

Three layers: `raw/` documents you curate, `wiki/` articles an agent compiles
from them, and generated indexes. The wiki is *what you know*. `sentinel next`
ranks the gaps and an agent fills them.

That is a knowledge base. It has no model of the person who owns it, and
nothing it produces is meant to be theirs.

## What the clone adds

A fourth layer, `persona/`, and a second kind of output.

```
raw/       source documents — immutable
persona/   traits: how you write, what you hold — each one cited to raw/
wiki/      articles compiled from sources (what you know)
           + articles extrapolated by the clone (what you might say)
index/     generated
meta/      machine state
```

**`persona/` is a model of the author, derived from their own writing.** One
file per trait:

```yaml
---
id: argues-from-cases
kind: style | principle | belief | pattern
claim: Builds an argument from a concrete case before generalising.
evidence:
  - raw/essays/on-teaching.md
  - raw/notes/2019-reading.md
confidence: high | medium | low
status: proposed | affirmed | rejected
created: 2026-08-25
updated: 2026-08-25
---

The reasoning, quoting the evidence.
```

**Extrapolated articles are the clone's own work.** A new `origin` value,
`extrapolated`, for an article that is not distilled from any source but
written *from* the persona — carrying `persona:` citations to the traits it
drew on, and a recorded human verdict before it can be published.

## The six safeguards

Each of these is structural — a lint error, a refusal, or a gate in `export` —
not advice in a skill that an agent can talk itself out of.

**1. No uncited claim about a person.** A trait with an empty `evidence:` list
is a lint *error*. The clone does not get to assert what you believe on a
hunch, and a profile you cannot audit is a profile you cannot correct.

**2. Beliefs come only from your own writing.** Every `evidence:` entry must
resolve to a manifest document whose `origin` is `authored` or `hybrid`.
Inferring someone's principles from an article an agent researched *for* them
is inventing a person out of their reading list. Lint error.

**3. Generated work is marked, everywhere.** `origin: extrapolated` in the
archive, and an attribution line that `export` writes into the published file
unconditionally. The agent cannot suppress it, because the agent does not write
it — the exporter does.

**4. Nothing generated publishes without a recorded human verdict.** `export`
refuses to publish an extrapolated article whose latest `review:` entry is not
`approved`, regardless of its `status`. Approval is a separate axis from
maturity: `stable` means finished, `approved` means *you* signed it.

**5. A rejection is durable, and the loop obeys it.** A rejected article stays
on disk carrying its rejection. That is what stops the loop regenerating it
next iteration — a "no" that evaporates is not a permission system.

**6. Source material is never published by default.** `export` copies no
`raw/` today and will not start. Publishing a source is per-document opt-in,
recorded in the manifest, because the licensing and privacy of what is in
`raw/` is the owner's call and cannot be inferred by a flag.

Safeguard 1 and 2 apply to the persona; 3, 4 and 5 to its output; 6 to the
corpus. The gap they leave is deliberate and worth stating: a derived persona
is *a model's summary of a corpus*, not ground truth about a person. Citation
and confidence keep it honest; they do not make it right. Which is why the
verdict system covers traits as well as articles — the worst failure here is
not a weak essay, it is the archive asserting a belief you do not hold.

## The ladder

`sentinel next` gains two rungs and renames one:

| | action | means | counter |
|---|---|---|---|
| 1 | `fix-errors` | the archive is malformed | `errors` |
| 2 | `compile` | raw documents no article cites | `uncompiled` |
| 3 | `learn` | **new** — `authored` sources no trait cites | `unmined` |
| 4 | `write` | concepts linked but unwritten | `wiki_articles` |
| 5 | `connect` | orphaned articles | `orphans` |
| 6 | `extend` | **new** — affirmed traits nothing has written from | `unexpressed` |
| 7 | `revise` | **renamed** from `review` — stalled drafts | `drafts` |

Every rung moves a counter, as the existing invariant requires; a rung that
does not is a loop that halts on its own correct work.

`review` is renamed to `revise` because the word is needed for the human gate,
and `revise` is what that rung always actually did. This is a breaking change
to the `--action` vocabulary and to the JSON payload, so `SCHEMA_VERSION` goes
to 2.

**`learn` sits below `compile` and above `write`.** This document first argued
for the top of the ladder — "a corpus read after the fact shaped nothing" — and
building it showed that was overstated, in two ways. Compiling a document *is*
the close reading that makes mining it cheap, so `compile` first is the cheaper
order. And what a thin profile actually degrades is *generated* work, which is
`extend`, four rungs down. What `learn` does earn is a place above `write`: the
profile shapes how the next article is written.

The rung fires only on documents registered `origin: authored` or `hybrid`, so
an archive that holds only research never sees it. That matters more than it
sounds: a backlog category the people it applies to cannot satisfy would mean
`sentinel next` never says "nothing outstanding" again.

**`extend` is placed below `connect`**: it is the payoff, not the maintenance,
and generating new work on top of a malformed or disconnected archive compounds
whatever is wrong with it.

**Awaiting approval is not a rung.** The agent cannot approve its own work.
It surfaces as `progress.awaiting_approval` and as a stop condition in
`/sentinel-grow`: a loop that keeps generating while a queue of unreviewed work
piles up is a loop producing volume, not value.

## The permission system

```
sentinel review                       # what is waiting on you
sentinel review <target> --approve
sentinel review <target> --reject  --note "not what I think"
sentinel review <target> --comment "the third section is the weak one"
```

Verdicts append to a `review:` list in the target's frontmatter — articles and
traits alike — so the history travels with the file and survives every rebuild.
The latest entry is the operative one. `changes-requested` is a verdict too:
it neither publishes nor closes, and it is what puts an article back in front
of the agent with your note attached.

## Publishing

`export` already writes the publishable subset and rewrites links to
unpublished pages as plain text. It gains:

- the approval gate (safeguard 4) and the attribution line (safeguard 3)
- `--with-sources`, writing *only* opted-in raw documents, so a reader can
  follow a claim to the material it came from
- persona, review queue, and in-progress work in the `--data` bundle

The last of those is what a front end needs to show the clone as a working
system rather than a finished site: what it has published, what it is drafting,
what it is waiting on you for.

## Implementation order

Each is a PR, stacked, each independently reviewable.

1. **this document**
2. `persona/` — the layer, its loader, its lint rules, `init`, `schema`
3. `sentinel persona` — read the profile; coverage against the corpus
4. the `learn` rung + `/sentinel-clone`, the skill that works it
5. `sentinel review` — verdicts on traits and articles
6. `origin: extrapolated` + the `extend` rung + the ladder rename
7. `export` — approval gate, attribution, opted-in sources, richer bundle
8. the front end
9. skills — `sentinel-clone`, `sentinel-extend`, and `sentinel-grow` rewritten
   around the new ladder

## What this does not do

- It does not fine-tune anything. The "clone" is a cited profile plus a prompt
  contract, and it is legible and correctable for exactly that reason.
- It does not detect that a trait has gone stale because you changed your mind.
  Beliefs are dated and cited; noticing that you now argue the opposite is
  `revise` work and, for now, yours.
- It does not resolve two traits that contradict each other. It reports the
  pair. Reconciling them is the author's, as reconciling contradictory
  `authored` articles already is.
