# Publishing the wiki

`sentinel export` decides *what is publishable* and writes only that. It renders
no HTML: a static site generator that already understands `[[wikilinks]]` takes
the output from there.

## Why an export step exists at all

Three things are true of this archive internally and wrong in public.

**Drafts.** `status:` exists to mark what is unfinished. The Stoicism corpus this
was built against is 27 articles and every one is a draft — copying `wiki/`
would publish all of them.

**Forward-declared links.** `broken-link` is a *warning*, not an error, because
the compile loop names concepts before writing them; that is the signal the
`write` rung reads. Internally it is the archive saying what it wants. To a
reader it is a dead end.

**Provenance.** `raw/` holds the source documents. Their licence is yours to
know — the Stoicism corpus includes a published edition of the *Enchiridion*
whose text is public domain and whose edition may not be. `meta/` is a working
record. Neither belongs on a website, and `export` never writes them.

## The workflow

```
sentinel lint                    # errors first; publish nothing malformed
sentinel index                   # graph and dashboard current
sentinel export --dry-run        # what would go, and what is held back
sentinel export --out ./publish
```

Then point a generator at `./publish`:

- **[Quartz](https://quartz.jzhao.xyz)** — built for Obsidian vaults, understands
  wikilinks and renders a graph. Free, static output, hosts anywhere.
- **Obsidian Publish** — no build step, paid, and it publishes from the vault
  rather than from an export, so you would be choosing pages by hand instead.

Both consume markdown with wikilinks, which is what `export` produces.

## What it guarantees

- Only articles whose `status` qualifies. `stable` by default; `--status
  draft,review` or `--include-drafts` to widen it, always explicitly.
- **No dead links.** A `[[target]]` no published article provides is rewritten
  to readable text — the words stay, the link goes. An unpublished article is
  named by its title, not its filename; `[[slug|Label]]` keeps the label; a
  concept that was never written keeps the target as written.
- Nothing under `wiki/`, `raw/`, or `index/`, which `sentinel index` walks; an
  export there would be indexed as archive content on the next rebuild.
- It refuses on a partial view. A reader cannot tell an article that failed to
  export from one that was never written.

## What it does not do

It does not decide whether you may publish `raw/`. It never copies it, and that
is the only safe default a tool can pick — the licence question is yours.

It is not incremental. Each run writes the current publishable set; it does not
remove files an earlier run left behind. Export into a fresh directory, or clear
it first, if articles have since been unpublished.

`export` is deliberately not reachable from any skill. An agent that can publish
can publish a draft, and unlike everything else in this archive that is not
recoverable by running the command again.
