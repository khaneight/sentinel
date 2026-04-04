---
name: sentinel-improve
description: Review and improve wiki articles — find gaps, strengthen content, fix quality issues. Use when the user wants to enhance the knowledge base quality. Trigger on "improve the wiki", "review articles", "enhance quality", "health check".
user-invocable: true
---

# Review and Improve the Wiki

You are performing a health check and improvement pass on the knowledge base. If $ARGUMENTS specifies a domain or topic, focus there. Otherwise, review broadly.

## Step 1: Assess current state

1. Run: `sentinel status`
2. Run: `sentinel lint`
3. Read `index/_master.md` for the full article list
4. Read `index/_orphans.md` for disconnected articles

## Step 2: Identify improvement opportunities

Scan wiki articles looking for:

**Structural issues:**
- Articles with missing or incomplete frontmatter
- Broken `[[wikilinks]]`
- Orphan pages (no incoming links)
- Missing `related` connections between obviously related concepts

**Content quality:**
- Articles that are too thin (stub-like, could be expanded)
- Concepts referenced in `[[wikilinks]]` that don't have articles yet
- Inconsistencies between articles covering related topics
- Articles stuck in `draft` status that could be promoted to `review`

**Knowledge gaps:**
- Topics implied by existing articles but not yet covered
- Cross-domain connections (e.g., philosophy concepts relevant to anthropology articles)
- Key arguments or counterarguments missing from philosophical pieces

## Step 3: Fix straightforward issues

Immediately fix:
- Missing frontmatter fields
- Obvious broken links (typos in wikilink slugs)
- Add `related` links between clearly connected articles
- Update `updated` dates on modified articles

## Step 4: Create improvement suggestions

For issues that require judgment or new content:
- List suggested new articles with brief descriptions
- Note where existing articles could be expanded
- Identify promising cross-domain connections
- Suggest research topics that would enrich the wiki

## Step 5: Rebuild

Run: `sentinel index`
Run: `sentinel lint`

## Step 6: Report

Summarize:
- Issues found and fixed
- Current wiki health metrics
- Prioritized list of suggested improvements
- Suggested new articles or research topics
