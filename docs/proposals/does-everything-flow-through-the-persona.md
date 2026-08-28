# Should the clone's work always emerge from the persona?

**Status: open question. Nothing here is built.**

## A correction first

An earlier draft of this document argued against routing everything through the
persona, and its main evidence was that on the demo archive *five of eight
entries into the outer ring skip the persona*.

That evidence was worthless. The demo was built by compiling sources first and
mining the persona second, because that is the order the tool currently
recommends. Counting the result and calling it evidence about the right design
is circular — it measures the current ladder, not the world.

Worse, it is not even a real corpus. It is four documents I wrote to demonstrate
the feature. Reasoning from it is reasoning from my own output.

So: from the flow instead.

## The flow

Someone has a body of writing. They want a system that becomes a version of them
that keeps working. What has to happen, in order:

1. **They hand over the corpus.** Essays, notes, whatever they have written.
2. **It is distilled.** How they argue, what they hold, the moves they make.
   Cited, and confirmed by them.
3. **The clone works.** From that model, in that voice.

There is no step in which the corpus is summarised into neutral articles before
anyone has read it for voice. That step exists in sentinel because sentinel was
a knowledge-base tool before it was a clone, and `compile` is the load-bearing
rung of the older product. It was never re-derived for this one.

## What that means for the ladder

`learn` currently sits **below** `compile`. [#70](https://github.com/khaneight/sentinel/pull/70)
moved it there, arguing that compiling a document *is* the close reading that
makes mining it cheap.

That argument is about cost, and it is true. But it answers the wrong question.
The order should follow what the product is for, and reading someone's corpus
for their voice is not an optimisation of compiling it — it is the point. The
original design put `learn` first and I talked myself out of it.

**`learn` moves back above `compile`.**

The objection I raised then was that this deadlocks a new archive: compiling
would wait on affirmed traits, traits wait on documents being read, affirming
waits on the user. But that is not a deadlock, it is the intended sequence.
Hand over the corpus, see what the archive made of you, say which parts are
right, then let it work. A first run that says *learn* and then *these eight
traits are waiting on you* is a better introduction to a clone than one that
says *compile*.

## What that means for the shape

If every article is written by the clone through the persona, there is only one
way into the outer ring, and the picture is finally what it claims.

But an article still cites `sources:`, and that is a real relation. The mistake
in the current model is treating it as *authorship* — a `compiles` edge running
source → article, as though the document wrote the article. It did not. The
clone did, through the persona. The source is what the article is **grounded
in**.

Two different relations, both true:

| edge | means | drawn |
|---|---|---|
| `distils` | corpus → persona | the primary chain |
| `writes` | persona → the clone's work | the primary chain |
| `links` | one piece of work referring to another | within the ring |
| `grounds` | corpus → work: what a piece is evidenced by | secondary — dashed, dimmer |

`grounds` is not a shortcut through the middle layer, because it is not
answering the question the middle layer answers. Authorship radiates. Grounding
is a citation, and citations legitimately reach back past whatever produced the
text.

Three rings, one path out, and nothing lied about.

## The part I am least sure of

Requiring `persona:` on every article means **a compiled article is written in
the author's voice too**. For an article distilling their own essays, obviously
right. For one summarising somebody else's paper, less so — a summary should be
accurate before it is characteristic, and a clone that editorialises source
material is worse than one that writes flatly.

I think the resolution is that `persona:` records *who wrote this and in what
voice*, not *whose opinions these are*. The rule against editorialising a source
belongs in `/sentinel-compile`, where it can be stated precisely, rather than in
the graph shape, where it can only be approximated.

But this is the point where the design could go wrong quietly, so it is worth
disagreeing with if it reads wrong.

## And one question only you can answer

This makes the persona **mandatory**. An archive that just wants a research wiki
— no clone, no voice — would have to build one anyway, or live with a permanent
warning.

That is a narrowing of what the tool is for. It is probably the right narrowing,
since the tagline is *clone yourself* and a tool that is two products is usually
worse at both. But it is a product decision rather than a design one.

## What it would take

1. **Ladder**: `learn` above `compile`. One line, plus the tests that enumerate
   the ladder and the counters, plus `/sentinel-grow`.
2. **Contract**: `persona:` becomes required on every wiki article. New lint —
   **warning**, not error, so existing archives are not broken overnight, with
   a rung or an `/sentinel-improve` pass to attribute what can honestly be
   attributed and report what cannot.
3. **Edges**: `compiles` becomes `grounds`, and the UI draws it dashed and
   dimmer so the primary chain reads first. `writes` is emitted for every
   article rather than only extrapolated ones.
4. **Onboarding**: `next` on a corpus with no affirmed traits recommends
   `learn`, then stops on the review queue rather than proceeding to `compile`.
   `tests/onboarding.rs` is where that gets asserted, since it is a change to
   what a user sees *in sequence*.
5. **Docs**: `docs/clone.md`'s ladder table and its "where it differs" section,
   which will need a second correction — this one.

Nothing here touches the layout or the hover walk. Both are already driven by
`layer` and the published edge kinds.
