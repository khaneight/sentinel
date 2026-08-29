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

Then point a generator at `./publish`. Two that consume markdown with wikilinks:

- **[Quartz](https://quartz.jzhao.xyz)** — built for Obsidian vaults, renders a
  graph, search and backlinks. Free, static output, hosts anywhere. Setup below.
- **Obsidian Publish** — no build step, paid, and it publishes from the vault
  rather than from an export, so you would be choosing pages by hand instead.

## Quartz, end to end

Verified against the 26-article corpus on Quartz **v5.0.0**. It needs Node ≥22
and npm ≥10.9.2.

```
git clone --depth 1 https://github.com/jackyzha0/quartz.git
cd quartz && npm i
npx quartz plugin install --from-config

sentinel export --out ./content --flat --clean
npx quartz build --serve          # http://localhost:8080
```

`--flat` writes every article at the top level, which is what Quartz's `content/`
expects; without it you get `/<domain>/<slug>`, which is the better shape once
the archive covers more than one subject. `--clean` removes articles you have
since unpublished — without it they stay on the site.

What that produced here:

```
Parsed 26 Markdown files in 129ms · Emitted 130 files in 3s
39 HTML pages · 0 unresolved internal links
graph, search and backlinks all present
1.8 MB of static output
```

`node_modules` is ~240 MB but is build-time only; nothing in it ships.

## Self-hosting the built site

`quartz build` writes plain static files to `public/`. Anything that serves a
directory will do:

| | |
|---|---|
| **Caddy** | `caddy file-server --root public --domain wiki.example.com` — automatic HTTPS, single binary |
| **nginx / Apache** | point `root` at `public/` |
| **Docker** | `nginx:alpine` plus `COPY public /usr/share/nginx/html`, ~25 MB image |
| **Tailscale** | bind any of the above to the tailnet for a private wiki |
| **Pages hosts** | GitHub, Cloudflare, Netlify, Vercel — not self-hosting, but `npx quartz sync` pushes |

The Tailscale route is worth considering while the `raw/` licensing question is
open: nothing is publicly reachable, so the question does not arise yet.

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

It is not incremental by default. Each run writes the current publishable set
and **reports** anything already in the destination it did not write — an
article unpublished since the last run is still sitting there, still readable.
`--clean` removes those; nothing is ever deleted without asking, and `--dry-run`
does not delete even with `--clean`.

`export` is deliberately not reachable from any skill. An agent that can publish
can publish a draft, and unlike everything else in this archive that is not
recoverable by running the command again.

## The showcase page

The markdown export is for *reading* — a static site generator turns it into a
wiki. The showcase is a different artifact: one page that shows the archive as a
working system, which is what a generator cannot do from markdown alone.

```bash
sentinel export --out ./content --flat --ui ./showcase
```

That writes `showcase/index.html` and `showcase/bundle.json`. Copy both to any
static host — a subdomain, an S3 bucket, a `docs/` folder on GitHub Pages. There
is no build step and no dependency: the page is a single self-contained
document that reads its own JSON and nothing else, so it works offline, behind a
strict content policy, and in five years.

It shows:

- **the graph**, in three rings: the source material at the core, everything
  distilled from it in the middle — traits describing the author *and* articles
  setting down what their sources say — and at the rim only the work that says
  something no source contains. The split is derived versus invented, not who
  typed it; every article is model prose, and compiling one is the same act as
  reading a corpus into a trait. The middle ring is the point — an
  affirmed trait connects down to every document it was read out of and up to
  every article written from it, so a claim in a generated essay can be
  followed back to the sentence in the corpus that supports it. Rings are drawn
  in perspective, so panning gives real parallax rather than a flat slide.
- **hovering** follows the arrows, transitively. On a source document you see
  what came of it — the traits read out of it, and what was written from those.
  On a piece of the clone's work you see where it came from, back through the
  persona to the corpus. Colour is the layer and nothing else — gold for source
  material, blue for the persona, purple for the clone's work — and a
  connection takes the colour of the layer it leaves, so a hue means the same
  thing on a dot and on a line.
- **topic filters** built from the archive's own vocabulary — the domains
  things are filed under and the tags on them. Traits have no topic of their
  own, so they stay while anything they touch does.
- **the documents themselves.** Click any node to read it: the bundle carries
  the published prose, so reading costs no extra request. Cross-references and
  source citations inside the text are live — following one opens that document
  and moves the graph to it. There is a fullscreen view for reading properly.
- **what the clone writes from** — the persona traits the author has affirmed,
  with how much evidence stands behind each and what has been written from it
- **what is in flight** — unpublished drafts, work awaiting the owner's
  approval, unconfirmed traits, and concepts the wiki has named but not written
- **growth** — articles and links over time, from `meta/progress.jsonl`

Drag to pan, scroll to zoom toward the cursor, arrow keys to move, `0` to fit.
Source documents appear only when you exported with `--with-sources`; without
it the graph is articles alone and the core is simply the author's own writing.

The outer ring is exactly the machine-original work, so its colour is already
the mark and nothing else is needed to say it. Those articles also open with the
attribution notice — the exporter's own wording, lifted
from the foot of the document to the top, because on screen the foot is after
you have read the thing believing it might be theirs. That is not decoration: the bundle carries
`extrapolated` on every node precisely so a front end cannot render machine
prose as though its author wrote it.

### Keeping the two in step

`--ui` writes its own `bundle.json` beside the page, so the two cannot get out
of step with each other. Re-run the same command after any `sentinel index` and
both are current.

The page itself lives at `ui/index.html` in this repository and is compiled into
the binary, so a released `sentinel` always writes a page that matches the
bundle it produces. Editing `ui/index.html` requires a rebuild to take effect.

### Using both together

They serve different visitors and can live at different addresses:

| | |
|---|---|
| `wiki.example.com` | the Quartz site, for reading the articles |
| `wiki.example.com/showcase/` | the page above, for seeing how it was built |

Nothing links them automatically — add a link to your Quartz landing page if you
want one.
