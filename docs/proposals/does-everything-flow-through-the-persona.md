# Should the clone's work always emerge from the persona?

**Status: open question. Nothing here is built.**

The showcase draws three layers — source material, persona, the clone's work —
and tells the reader that is the shape of the system. It is not, quite. There
are two ways into the outer ring:

| edge | meaning |
|---|---|
| `writes` | persona → article. An `extrapolated` article, written from affirmed traits. |
| `compiles` | source → article. An article compiled straight from a raw document, no persona involved. |

On the demo archive, **five of eight entries into the outer ring skip the middle
layer**. The picture promises a chain the data mostly does not walk.

That is the problem. What follows is three ways to fix it, and the reason I do
not think the obvious one is right.

## Option A — force every article through the persona

Require `persona:` on every wiki article, not just extrapolated ones. Compiling
a source would mean distilling it *and* recording which traits shaped how it was
written.

**Why it is tempting.** It makes the picture true, it makes every page
attributable, and it matches the tagline: if this is a clone, its wiki should be
in its voice rather than a neutral encyclopedia.

**Why I do not think it works.**

1. **It deadlocks a new archive.** Compiling would require affirmed traits;
   traits are read out of documents; and affirming needs the *user*. So nothing
   can be compiled until somebody sits down and approves a persona. Today's
   first run is `ingest` → `next` → `compile`, and this would make it
   `ingest` → read everything → wait for the human → compile. `learn` sits below
   `compile` in the ladder for exactly this reason: compiling a document *is*
   the close reading that makes mining it cheap.
2. **It would require lying about existing work.** Every already-compiled
   article would need a `persona:` naming traits it was not written from.
   Stamping attributions on prose that did not come from them is the same class
   of dishonesty the rest of this design is arranged against.
3. **Not all knowledge should be voiced.** An article recording what a source
   actually says should be accurate, not stylish. Requiring persona attribution
   on it invites the clone to editorialise source material, which is a worse
   failure than a flat summary.

## Option B — a fourth layer

Split the outer ring in two, because it is holding two different things:

```
0  source material     what the author wrote
1  persona             the model of them, distilled from it
2  knowledge           articles compiled from sources — accurate, unvoiced
3  the clone's work    articles written from the persona
```

`compiles` becomes source → knowledge, `writes` becomes persona → work, and a
new edge lets the clone's work draw on compiled knowledge as well as on traits —
which is what actually happens when it writes something researched.

**Cost:** four rings instead of the three that were asked for, and one more
thing for a reader to hold.

**What it buys:** every edge tells the truth with no new rules, no ordering
problem, and nothing to retrofit. The shortcut stops being a shortcut and
becomes an honest edge between two layers that genuinely differ.

## Option C — reclassify, keeping three layers

Notice that a compiled article is not the clone speaking. It is a distillation
of a source — closer to corpus than to output. So:

```
0  the corpus     raw documents *and* the articles compiled from them
1  persona        distilled from the corpus
2  the clone      articles written from the persona
```

`compiles` becomes an edge *within* layer 0, and the only way into the outer
ring is through the persona. The picture becomes exactly the promise.

**Cost:** the wiki — the bulk of the archive, and the thing most people would
call the product — sits in the same ring as `raw/`. That may read as a demotion.
It also means the outer ring is small and often empty, which is honest but
undersells a mature archive.

## Recommendation

**Option B**, and I would take the fourth ring over the tidier story.

The reason is the one that keeps coming up in this project: the diagram is a
claim, and a claim that is convenient is still a claim. Three layers with a
shortcut is a picture that is *nearly* true and reads as fully true. Four layers
with no shortcut is true. Option C also gets there, but by asserting that a
compiled article is corpus rather than output, which I think is defensible but
not obviously right — and it hides a real distinction (`compiles` vs `links`)
inside a single ring.

If the three-ring shape matters more than the extra distinction, **C beats A**.
A is the one to avoid: it is the only option that would require attributing prose
to traits it did not come from.

## What B would take

- `LAYERS` gains an entry; `layer_of` maps `extrapolated` → 3 and the rest → 2.
- `EDGE_KINDS` gains `informs` (knowledge → clone's work), derived from an
  extrapolated article's `sources:`, which today produces a `compiles` edge and
  is arguably already mislabelled.
- The ring layout and the hover walk need no changes — both are already driven
  by `layer` and the published edge kinds.
- The tests that enumerate layers and edge kinds fail until updated, which is
  the intended behaviour.
- `docs/publishing.md`, the layer key in the page, and CLAUDE.md.

No workflow, lint rule, or ladder change in any of it. The reason the fix is
cheap is that the layers were already derived rather than hard-coded — which is
also why getting the *number* wrong now is not expensive to correct later.
