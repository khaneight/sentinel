---
name: sentinel-research
description: Research a topic and add findings to the wiki. Use when the user wants to expand the knowledge base with AI-researched content on a specific topic. Trigger on "research X", "find out about X", "add research on X".
user-invocable: true
---

# Research a Topic and Add to the Wiki

You are researching **$ARGUMENTS** and adding findings to the knowledge base. Research content is clearly distinguished from the user's authored content via the `origin: researched` field.

## Step 1: Understand existing knowledge

1. Run `sentinel search $ARGUMENTS` to find related wiki articles
2. Read `index/_master.md` to understand the current knowledge landscape
3. Read any related wiki articles to understand what's already known

## Step 2: Research

Use web search and web fetch to gather information on the topic:
- Look for authoritative sources (academic papers, established references, primary sources)
- Find multiple perspectives where relevant
- Note connections to concepts already in the wiki

## Step 3: Create raw research document

Create a research source document at `raw/{appropriate-domain}/research-{topic-slug}.md`:

```yaml
---
title: "Research: {Topic}"
domain: {domain}
origin: researched
ingested: YYYY-MM-DD
---
```

Include your research notes, sources, and key findings in this raw doc. This preserves the research trail.

Then run: `sentinel sync`

## Step 4: Compile into wiki articles

For each key concept from the research, create or update wiki articles in `wiki/{domain}/`:

- Use `origin: researched` for new articles based purely on research
- Use `origin: hybrid` if updating an existing `authored` article with research findings
- When updating authored articles, clearly add research in separate sections — don't mix it into the author's original arguments
- Always cite sources
- Link to existing wiki articles with `[[wikilinks]]`

## Step 5: Rebuild and validate

Run: `sentinel index`
Run: `sentinel lint`

## Step 6: Report

Summarize:
- What was researched
- What new articles were created
- What existing articles were enriched
- Key findings and connections discovered
- Suggestions for further research
