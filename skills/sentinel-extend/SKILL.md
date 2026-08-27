---
name: sentinel-extend
description: Write a new article from the author's affirmed persona traits — their thinking extended, not summarised — with research, and hold it for their approval. Use when `sentinel next` recommends `extend`, or when the user asks the clone to write something new. Trigger on "write something new", "extend my thinking", "what would I say about", "have the clone write".
user-invocable: true
allowed-tools: Bash(sentinel:*), Read, Write, Edit, Glob, Grep, WebSearch, WebFetch
---

# Write as the Clone

Produce an `origin: extrapolated` article: new work that extends the author's
thinking, written from traits they have affirmed, researched where it needs
research, and **marked as the machine's** from the moment it exists.

This is the one thing in the archive that puts words in a real person's mouth.
Everything below is about making that traceable and reversible.

## Choosing what to write

When `$ARGUMENTS` is empty:

```
sentinel next --action extend --json
```

`targets` are traits the author has affirmed that nothing has been written
from — views they hold the archive has never expressed — ranked by how much
evidence stands behind each. Take the first.

When `$ARGUMENTS` is given, treat it as a trait id or a subject. If it names a
subject rather than a trait, find which affirmed traits bear on it with
`sentinel persona --json` and say which you chose.

## Before you write

```
sentinel schema --json
sentinel persona --affirmed --json
```

**`--affirmed` is the whole set you may write from.** A `proposed` trait is the
agent's own reading of the author, unconfirmed; writing from it would let the
clone bootstrap a voice out of its own guesses. A `rejected` one is a person
saying *that is not me* — `wrote-from-rejected` is a lint error, and there is no
argument that gets round it.

Then read the archive, so the new article extends the wiki rather than
restating it:

```
sentinel search "<topic>"
sentinel graph --node <topic>
```

If an article already covers this, **say so and stop.** A knowledge base that
fills with the clone's restatements of what it already knew is worse than one
that stays small.

## Writing it

The article's job is to say something the archive does not already say, that
follows from what the author holds. Not a summary of their traits — nobody
wants to read a description of themselves — but the *move* those traits make
when applied to something new.

- **Ground it in the evidence, not the paraphrase.** Read the raw documents
  behind the traits you are using. The trait is a one-line summary; the writing
  is in the source.
- **Research what you do not know.** File the trail under `raw/` with
  `sentinel ingest -o researched` and cite it in `sources:`, exactly as
  `/sentinel-research` does. An extrapolated article may have sources; what it
  may not have is an empty `persona:`.
- **Link out.** `[[wikilinks]]`, including to things not written yet — the same
  fuel every other article provides.
- **Stay inside what they have said.** If the argument needs a premise no trait
  supports, that is where the article stops. Name the open question in the prose
  rather than answering it for them.

### Frontmatter

`origin: extrapolated`, and `persona:` listing the trait ids you wrote from —
`unattributed-extrapolation` is an error precisely so that generated prose can
always be traced back to a claim the author actually made. `status: draft`.

**Never write a `review:` entry.** That is the author's, and `sentinel review`
is the only thing that writes it.

## After writing

```
sentinel index
sentinel lint --summary
```

`lint` must exit 0. A `wrote-from-unconfirmed` warning means you cited a trait
they have not affirmed — replace it or drop it, do not leave it.

**If `sentinel index` refuses**, some file could not be read. Report the named
files and stop; do not retry or work around it.

```
sentinel log extend "{what you wrote, and from which traits}"
```

## It is not published, and you cannot publish it

`sentinel export` refuses to publish an extrapolated article until its latest
verdict is `approved`, and only the author can record one. Do not run `export`,
do not suggest running it, and do not describe the article as published.

End by telling them how to answer:

```
sentinel review <slug> --approve
sentinel review <slug> --reject --note "why"
sentinel review <slug> --request-changes --note "what to change"
```

## Escalate rather than decide

- The traits you would need contradict each other. Reconciling them is theirs.
- The subject is one where being wrong in their name causes real harm — their
  professional judgement, a public position, anything about a named person.
  Ask before writing, not after.
- The strongest version of the argument requires a view they have never
  expressed. Say what the missing premise is and let them supply it.

## Report

- What you wrote, and the traits it rests on — by id, with each claim quoted.
- **Where you extended rather than restated**: the step in the argument that is
  new. If you cannot point to one, the article is a summary and should not be
  kept.
- What you researched and filed under `raw/`.
- Any premise you needed and did not have.
- That it is `draft`, unapproved, and unpublishable until they say otherwise —
  and that `sentinel review` is how they say it.
