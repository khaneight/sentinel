---
name: sentinel-ask
description: Ask a question against the knowledge base. Use when the user wants to query their wiki for answers, connections, or analysis. Trigger on "ask the wiki", "what does the wiki say about", "find in my knowledge base".
user-invocable: true
allowed-tools: Bash(sentinel:*), Read, Write, Edit, Glob, Grep
---

# Answer a Question from the Knowledge Base

Answer **$ARGUMENTS** from the archive.

## Scope

- `$ARGUMENTS` is a question → answer it.
- `$ARGUMENTS` is empty → ask the user what they want to know. Do not guess.

## Step 1: Find the relevant articles

```
sentinel search "<key terms>" --json
```

Returns paths, titles, slugs, and matching lines ranked by match count. Search a few different phrasings — the wiki may name a concept differently than the question does.

**Do not read `index/_master.md`.** It is the whole archive; reading it to answer one question wastes the context you need for the articles that actually matter.

Once you have candidate articles, follow their connections with
`sentinel graph --node <slug> --depth 2 --json`. Use `--node`, not the bare
form — the full graph is the entire topology and grows with the archive, which
is the same context problem as reading the master index.

## Step 2: Read and synthesize

Read the relevant articles in full. As you do:

- **Track `origin` on every article you use.** `authored` is the user's own thinking. `researched` is AI-gathered. `hybrid` is both, in separate sections. This distinction is the entire point of the field and it must survive into your answer.
- Follow `[[wikilinks]]` where they lead somewhere relevant. Note the ones that lead nowhere — an unwritten concept sitting in the middle of the answer is a finding.
- Check `sources:` and read the raw document when the article's summary is not enough.

## Step 3: Answer

- Draw from the wiki first. If you need knowledge the archive does not contain, say so explicitly and mark that part as outside the wiki — do not blend it in silently.
- **Attribute precisely.** "In your article on X you argued…" versus "The researched article on Y notes…". Never present the user's own idea back to them as if it were a finding.
- Cite articles as `[[wikilinks]]`.
- Name the gaps. If the question touches something the wiki has forward-declared but not written, say which concept and that it is unwritten.
- If two articles disagree, say so rather than silently picking one. A contradiction between articles is worth more to the user than a smooth answer.

## Step 4: File only a genuine connection

Do **not** file the answer as an article by default. A knowledge base that accumulates answered questions fills up with restatements of what it already contained.

File a new article only when the work established a **connection or synthesis that no existing article records** — for example, that two ideas in different domains are the same argument, or that one article's conclusion undercuts another's premise.

When you do:

- The article is about **the connection**, not about the question. Title it after the idea, not after what was asked.
- `origin: hybrid` if it joins the user's ideas to your synthesis; `origin: researched` if it is entirely yours.
- `sources:` must cite the raw documents behind the articles you drew on, so the new article is reachable from the provenance trail.
- Add `[[wikilinks]]` from the articles it connects, so it is not born an orphan.
- Follow `sentinel schema`, then run `sentinel index` and `sentinel lint`. If
  `sentinel index` refuses because a wiki file could not be read, do not retry
  or work around it — report the named files and stop; rebuilding from a partial
  view would delete whatever they account for.

Offer this to the user rather than doing it unprompted, and say what the connection is so they can judge whether it is real.

## Step 5: Log

```
sentinel log ask "{question} — answered from {N} articles{, filed as {slug}.md}"
```
