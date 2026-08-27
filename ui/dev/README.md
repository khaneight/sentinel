# ui/dev — the development harness

`ui/index.html` is one self-contained document. It has no build step, no
dependencies, and no network access beyond its own `bundle.json`, and
`tests/ui.rs` fails if an external reference ever appears in it.

That is right for the artifact and awkward for iterating on it. This directory
is the other half: a dev server that serves the real page, overlays the
[Agentation](https://agentation.com) annotation toolbar at request time, and
feeds it data from a real export.

**Nothing here ships.** The toolbar is injected into the HTTP response and
never written to the page, so the file that deploys is the file that was
reviewed.

## Use

```bash
sentinel export --out /tmp/site --flat --ui /tmp/showcase   # produce the data
cd ui/dev && npm install
node serve.mjs /tmp/showcase 8123
```

Then open http://localhost:8123. The page is re-read from `../index.html` on
every request, so editing it and reloading is the whole loop.

## Sending annotations to an agent

Click the toolbar in the bottom-right, click any element, type what should
change. Two ways to get that to a coding agent:

**Copy** — the toolbar's copy button puts structured markdown on the clipboard.
Paste it into a chat. Works with no further setup.

**Agent Sync** — the toolbar posts to an `agentation-mcp` server on port 4747.
Start one with `npx agentation-mcp server`, then:

```bash
./annotations.sh            # what is pending, readable
./annotations.sh --json     # the same, raw
./annotations.sh --clear    # resolve everything pending
```

`annotations.sh` exists because registering the MCP server with a coding agent
requires restarting it, and this loop should work in the session you are already
in. If you do want the MCP tools, `npx agentation-mcp init` registers the server
and `npx agentation-mcp doctor` checks it.

## If you would rather not have this

Delete the directory. Nothing in the crate, the tests, or the published page
refers to it.
