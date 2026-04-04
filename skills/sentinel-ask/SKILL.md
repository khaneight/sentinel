---
name: sentinel-ask
description: Ask a question against the knowledge base. Use when the user wants to query their wiki for answers, connections, or analysis. Trigger on "ask the wiki", "what does the wiki say about", "find in my knowledge base".
user-invocable: true
---

# Answer a Question from the Knowledge Base

The user is asking: **$ARGUMENTS**

Your job is to answer this question using the knowledge base as your primary source. Cite specific articles and distinguish between authored knowledge (the user's own ideas) and researched content.

## Step 1: Find relevant articles

1. Run: `sentinel search {relevant keywords}`
2. Read `index/_master.md` to understand the full scope
3. Read `index/_by-domain.md` if the question is domain-specific

## Step 2: Read and synthesize

Read all relevant wiki articles thoroughly. Pay attention to:
- The `origin` field — distinguish the user's own thinking from researched content
- `[[wikilinks]]` — follow connection chains to find related knowledge
- `sources` — trace back to raw documents if needed for deeper context

## Step 3: Answer the question

Provide a thorough answer that:
- Draws primarily from the wiki's content
- Clearly attributes ideas: "In your article on X, you argued..." vs "The research article on Y notes..."
- Follows connection chains across articles
- Notes where the wiki has gaps relevant to the question
- Cites specific articles with `[[wikilinks]]`

## Step 4: Optionally file the answer

If the answer reveals interesting connections or synthesis worth preserving, offer to create a new wiki article capturing the insight. Use `origin: hybrid` if it combines the user's ideas with your synthesis.
