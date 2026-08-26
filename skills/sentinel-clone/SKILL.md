---
name: sentinel-clone
description: Read the user's own writing and record how they write and what they hold as cited persona traits. Use when the archive needs a model of its author, or when `sentinel next` recommends `learn`. Trigger on "learn my voice", "read my writing", "build the persona", "mine my corpus", "what do I sound like".
user-invocable: true
allowed-tools: Bash(sentinel:*), Read, Write, Edit, Glob, Grep
---

# Read the Author

Turn documents the user wrote into `persona/` traits: cited claims about how
they write and what they hold. This is the archive's model of its author, and
everything the clone later writes is built on it.

**You are describing a real person to a system that will speak as them.** That
is the whole reason the rules below are not negotiable.

## Choosing a document

When `$ARGUMENTS` is empty, ask the archive what to read:

```
sentinel next --action learn --json
```

`targets` are documents registered `origin: authored` that no trait cites,
oldest first. Take the first one. `target_count` is the true total; `targets`
is a sample of five.

When `$ARGUMENTS` is given, treat it as a raw path or a filename and work that
document instead.

## Before you start

```
sentinel schema --json
sentinel persona --json
```

The first is the trait contract — fields, kinds, statuses. Do not restate it
from memory. The second is what the archive already believes about its author,
and you need it for two reasons: a trait you are about to write may already
exist under another name, and the traits already there tell you what to look
for corroboration of.

If `sentinel persona` reports `unreadable`, some trait could not be opened.
Stop and report it — you would be adding to a profile you can only partly see,
and the first thing you would do wrong is duplicate something you cannot read.

## The three rules

**1. Every trait cites evidence, and the evidence is theirs.** `evidence:` lists
archive-relative paths to documents with `origin: authored` or `hybrid`. A trait
with no evidence is a `uncited-claim` error; one citing `researched` material is
`inferred-from-research`. Both are lint errors and both are refusing to let you
invent a person. If you cannot find the passage, **you do not have the trait.**

**2. Quote what you found.** The body of the trait is where you show the
passages that support the claim. Paths tell a reader where to look; quotes are
what let them check a sentence about themselves without re-reading an essay.

**3. Never write `status: affirmed`.** That field records the *user* agreeing
with a claim about themselves. Leave it `proposed`. Writing it yourself is the
clone approving its own reading of the person it is modelling.

## Reading a document

Read the whole thing before writing anything. You are looking for four
different things, and `sentinel schema` publishes what each `kind` means:

- **style** — how the prose actually reads. Sentence length, whether they hedge,
  whether they use the first person, how they open and close, what they do with
  examples. Be specific enough to be falsifiable: "writes plainly" is not a
  trait, "states the conclusion first and spends the paragraph defending it" is.
- **principle** — a rule they apply, whether or not they state it as one.
- **belief** — a position they hold about the world.
- **pattern** — a recurring move in how they think. Reaching for a historical
  parallel; testing an idea against the smallest case; naming the strongest
  objection before their own argument.

### What not to record

- **Anything the document reports rather than asserts.** Summarising an author
  they are criticising is not a belief they hold.
- **A single instance.** One metaphor is not a pattern. Prefer traits you can
  cite twice, and set `confidence: low` when you cannot.
- **Anything about the subject matter.** "Interested in Stoicism" is a topic,
  and topics belong in the wiki. A trait is about *how they think*, not what
  about.
- **Anything you would be embarrassed to show them.** You are writing a
  description of a person that they will read.

### One document, few traits

Two to four traits from a document is normal. A document that yields twelve
usually means you are recording the document's contents rather than its
author's habits.

Prefer **adding evidence to an existing trait** over writing a near-duplicate.
If `sentinel persona` already lists a claim and this document supports it too,
append the path to that trait's `evidence:` and quote the new passage in its
body. That is how confidence grows — the same claim seen in three places is
worth more than three claims seen once.

## Writing the trait

One file per trait at `persona/<id>.md`, where `<id>` matches the `id:` field.
`templates/persona-trait.md` is generated from the contract.

Set `confidence` honestly: `high` for something the document makes unmissable
and other documents corroborate, `low` for a reading you would defend but not
insist on. A profile of uniformly `high` traits is a profile nobody calibrated.

## After writing

```
sentinel index
sentinel lint --summary
```

`lint` must exit 0. If a persona rule fired, `/sentinel-improve` → *Persona
traits* has the repair — and in every case the repair is to find the missing
part or delete the trait, never to supply it.

**If `sentinel index` refuses**, some file could not be read. Do not retry it or
work around it: report the named files and stop. Rebuilding from a partial view
drops whatever those files account for.

```
sentinel log clone "read {document}: {what you recorded}"
```

## Escalate rather than decide

Stop and ask the user when:

- The document contradicts a trait already in the profile. That may be their
  thinking having changed, which is theirs to say — record neither reading over
  the other.
- What you would write is about the person rather than their work: their
  circumstances, health, relationships, or anything they wrote privately rather
  than published. Ask before it goes in.
- A document registered `origin: authored` plainly is not theirs — a paper, a
  quotation file, someone else's essay ingested without `-o researched`. Say so
  and recommend fixing the origin; do not mine it, and do not edit `raw/`.

## Report

- Which document you read, and the traits you wrote or extended.
- For each: the claim, its confidence, and **the passage you are relying on** —
  quoted, so the user can disagree with the reading rather than the conclusion.
- `progress.unmined` before and after.
- Anything you deliberately did not record, and why. A pass that reports only
  what it found, and never what it declined to claim, is not oversight.
- That every trait is `proposed` until they say otherwise, and that
  `sentinel persona --json` shows them the whole profile.
