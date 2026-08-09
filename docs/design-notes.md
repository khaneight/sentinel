# Design notes

Why the invariants in `CLAUDE.md` are what they are. Every one of them was a
bug first. This file is not loaded into an agent's context — read it before
changing an invariant, or when a rule looks arbitrary.

## Identity: what makes two wikilinks the same

`slug::canonical` is thirty lines and the whole archive depends on it. Anything
that reads a wikilink, ranks a gap, detects a duplicate, or builds the link
graph routes through it. **It has been wrong four times, each less visible than
the last:**

| route | what broke | visible? |
|---|---|---|
| capitalisation | `[[Compile Loop]]` did not resolve to `compile-loop.md` | if you look carefully |
| case-insensitive filesystem | `mv notes.md Notes.md` refused, naming a file no listing shows | no |
| NFC vs NFD | `étude` and `étude` are different bytes | no — pixel-identical |
| invisible characters, ligatures, full-width | `fi<ZWSP>le` canonicalised to `fi-le` | no — zero pixels |

The consequences compound. Beyond false `broken-link` warnings, demand
fragments: one concept spelled two ways ranks as two half-wanted gaps, so a
popular gap sorts below a rare one — and `sentinel next` can recommend writing
an article that already exists, which `/sentinel-grow` would act on by writing a
duplicate.

Soft hyphens are the practical case: they survive copy-paste out of PDFs in
bulk, because that is how PDFs record line-break hyphenation, and this tool
ingests PDFs.

Plurals and the Turkish dotted `İ` are left alone deliberately. Folding them
needs a stemmer or a locale, and a wrong merge silently collapses two real
concepts — worse than a missed one.

## Derivation: the compile loop

Nothing ever wrote `ManifestEntry.wiki_articles`. Not `ingest`, not `sync`, not
`index` — it was written exactly twice, both times as `vec![]`. Since
`uncompiled()` filtered on that field being empty, `sentinel uncompiled` listed
every raw document forever and the archive had no notion of progress. The state
machine had no transition out of its initial state.

Deriving the mapping from `sources:` instead means the answer cannot go stale,
and the loop closes the moment an article is written rather than when someone
remembers to run `index`.

Citation matching is lenient — `raw/d/x.md`, `./raw/d/x.md`, `d/x.md`,
`[[raw/d/x.md]]`, bare `x.md` — because an agent hand-writes these into YAML. A
bare filename matching two documents is reported, not guessed: attributing
provenance to the wrong source is worse than leaving both in the queue.

## Completeness: partial views

`wiki::load_all` used to skip unreadable files silently. `index` then rebuilt
from what remained:

```
$ sentinel index          # one article locked by an editor
Index rebuilt.
  Articles indexed: 0
$ echo $?
0
```

It reported success, wiped the manifest's compilation mapping, and blanked every
generated index. On an archive kept in git — which the README recommends —
committing after that discards the index entirely.

The fix covers directories too: a directory that cannot be traversed hides every
article inside it just as effectively, and `filter_map(Result::ok)` on the walk
dropped that error. That gap survived one PR *past* the fix that was meant to
close it.

## Queries

`sentinel lint` appended to `meta/log.md` on every run, so validating an archive
dirtied its working tree — `sentinel lint && git diff --exit-code` could never
pass. `/sentinel-grow` runs lint every iteration, so the log filled with
`0 error(s), 0 warning(s)` and buried the entries recording real changes.

`sync --dry-run` had asserted "writes nothing" since the second PR. Nobody asked
whether any *other* command had the property, because query commands are the
ones that feel safe.

## Durability and exclusivity

`fs::write` truncates before writing. An interruption between the two leaves the
file truncated, and for `meta/manifest.json` that is unrecoverable twice over:
it holds `origin` and `ingested_at`, which cannot be derived from disk, and a
torn manifest fails to parse so every command stops. `sync` cannot rebuild it,
because `sync` has to read it first.

Concurrency needed more than atomic writes. Ten concurrent `ingest` calls left
ten files on disk and **one** manifest entry — nine lost, every command
reporting success. Recovering them through `sync` re-registers them as
`origin: authored`, so the archive silently converts researched documents into
the user's own writing via a path that looks like repair.

**Compare-on-save was not enough**, and this is the part worth remembering: both
processes read, both compare successfully, and both write, because nothing
orders the compare against the write. Measured, it caught 4 of 12 and left 7
silently lost; at concurrency 2 it caught nothing. A partial fix would have been
worse than none, because the conflict errors imply the rest are safe. The lock
took it to 0 lost at concurrency 2, 4, and 12.

## What `sync` may throw away

Pruning was made the default with the justification that "everything in an entry
is derivable from disk, so nothing unrecoverable is lost." That was false.
`origin` and `ingested_at` are not derivable, and a hand-renamed file looks
exactly like a deletion plus an addition — so a `researched` document became
`authored` with no trace, both states being internally consistent.

`content_hash` recognises the move. Genuine deletions still prune and now print
what they discard.

**"Nothing unrecoverable is lost" is a claim requiring proof, not a
justification for a destructive default.**

## Scheduling: why `next` ranks but does not budget

Strict priority is right for "what is most valuable now" and wrong as a
schedule. On a real ingest of eight sources it recommends `compile` eight times
running, and `/sentinel-grow`'s default budget is three — so the `write` step,
the self-generating behaviour the whole design exists for, was never reached in
the ordinary case. Every synthetic test had one or two sources, so the ladder
always fell through within an iteration.

The progress counters then had the same shape of bug one level down. The
original three — `wiki_articles`, `uncompiled`, `errors` — covered exactly the
three actions in front of me when I wrote them. `connect` adds a link to an
existing article and `review` promotes a draft; neither changes an article
count, an uncompiled count, or an error count, so **two of the five actions the
ladder can recommend registered as no progress**, halting the loop immediately
after a correct iteration. Every action now maps to a counter, and a test
derives that mapping from the published ladder so a new action cannot be added
without one.

Progress is measured by `progress`, not by backlog size, for a related reason:
an article that fills one gap and legitimately opens three grows the backlog
while making the archive richer. A loop halting on "backlog did not shrink"
stops immediately after its most generative work.

## What the loop actually does, measured

Driven on a real corpus — *Meditations* and the *Enchiridion* from Project
Gutenberg, eight sources, articles written from the source text:

| wiki articles | uncompiled | wanted | orphans |
|---|---|---|---|
| 8 | 6 | 8 | 1 |
| 13 | 5 | 8 | 1 |
| 18 | 5 | 3 | 0 |
| 26 | 0 | 0 | 0 |

**It converges.** Early articles name concepts that do not exist yet; later ones
increasingly link to pages already written, so filling a gap stops creating new
ones. A loop that diverged is the more intuitive design and is not what happens.

**Growth is bounded by the sources** — the wiki completes what the raw documents
imply and stops. That is intended: an archive that kept generating would be
inventing territory with no provenance.

At the terminal state the graph is **one connected component**: 26 nodes, 116
edges, no article without an outgoing link, and the most linked-to concepts are
`prohairesis` (19 inbound), `dichotomy-of-control` (12), and `akrasia` (12) —
the actual centre of Stoicism. The graph's centre matched the material's without
anyone arranging it.

**Demand ranking was correct at every size measured.** `prohairesis` at 8
articles, `assent-to-impressions` at 13 — each the central concept for what the
surrounding articles were about.

The caveat: one corpus, one domain, articles written by one agent. This says the
mechanism behaves as designed, not that the resulting wiki is good.

It also showed what the tool does not track. All 26 articles were still
`status: draft` while `next` reported "nothing outstanding" — complete by every
measure the tool takes, and read by nobody. Hence the maturity line in `status`.

## Bounded output

Measured on a generated archive of 423 articles:

| command | before | after |
|---|---|---|
| `search <common> --json` | 467 KB (~117k tokens) | 11 KB |
| `graph --json` | 65 KB | 1.8 KB via `--node` |
| `lint --json` | 50 KB | 315 B via `--summary` |

Ranking was independently broken: scoring by raw substring count meant an
article mentioning a term twenty times in passing outranked the article titled
with it.

The skills had just been rewritten to stop reading `index/_master.md` — and were
pointed at three commands with the same defect. `CLAUDE.md` itself later grew to
31 KB, ~7,700 tokens loaded every session, for the same reason: nobody was
watching the size of the thing they were appending to.

## Method

Findings came from, roughly in order of how much they produced:

1. **Running it on a real corpus.** Priority starvation, unactionable
   recommendations, undeclared truncation, the maturity gap.
2. **Auditing mutating commands** with "what could this silently corrupt?" —
   three silent-corruption bugs, all more severe than anything usage found.
3. **Grepping for the failure shape** rather than the site — `unwrap_or_default`,
   `.ok()`, `filter_map(Result::ok)`. Found two more in one pass, including one
   in a fix shipped an iteration earlier.
4. **Asking artifact questions**: for each file the tool writes, who reads it
   and by what means? Who maintains it, and does it claim anything they will
   not?
5. **Platform differences** — case sensitivity, Unicode normalisation. A
   cross-platform CI matrix proves the code *runs* everywhere, not that it does
   the right thing where platforms differ.

The recurring shape across nearly all of them: **an error path that produces a
plausible-looking result instead of an error.** Not crashes, not wrong
algorithms — places where a failure was handled by continuing with less
information and not saying so. Every one had a passing test above it exercising
the feature working.
