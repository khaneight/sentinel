# Surfacing the commands in the UI

**Status: plan. Nothing here is built.**

The ask: add and remove documents, approve and reject, distil a document into
traits, tell the clone to write something — from the page rather than the
terminal.

## Three different asks, wearing one coat

They are not the same kind of operation, and the difference decides the
architecture.

| | example | what it needs |
|---|---|---|
| **Read** | see the queue, the graph, what is waiting | a payload |
| **Mutate** | ingest, remove, approve, reject, publish a source | a process with write access, and the lock |
| **Generate** | distil a document, write a piece | *a language model* |

The third is the one to be clear about first, because no amount of work on the
first two produces it.

## `sentinel` cannot distil or write

There is no model in the binary. `learn` and `extend` are *rungs* — the CLI
ranks what is worth doing and validates what comes back, and the actual reading
and writing is done by an agent following `skills/sentinel-clone` and
`skills/sentinel-extend`. "Distil this document" is not a command that exists;
it is a prompt to something that has a model.

So a button marked *Distil* has to do one of:

1. **Show the invocation** — `/sentinel-clone raw/philosophy/on-teaching.md` —
   and put it on the clipboard. Works today, no new machinery, human stays in
   the loop.
2. **Spawn an agent** — `claude -p "/sentinel-clone raw/…"` as a subprocess,
   streaming output back to the page.
3. **Queue a job** that an already-running agent session polls for.
4. **Call a model API from the server**, reimplementing the skills in Rust.

(4) is the wrong answer: the skills are the specification, they are already
executed against a real archive by `tests/skill_flows.rs`, and a second copy in
the server would drift the way every second copy in this project has. (1) is the
honest starting point; (2) is the eventual one.

## The thing that must not happen

`export --ui` writes a page **designed to be published**. Its whole value is
that it is dead: static, no server, copy it anywhere. A console mutates the
archive.

If those are the same artifact and the controls are live, somebody eventually
rsyncs a control panel for their archive onto a public subdomain. That failure
is silent, and it is exactly the kind this project has otherwise been careful
about.

So, whichever shape is chosen:

- controls exist only when an API answers, and the API only exists under
  `sentinel serve`;
- `serve` binds loopback and refuses anything else without an explicit flag;
- a token is required, printed once at startup;
- and a test asserts the **exported** page has no reachable control — the same
  guard style as `the_page_is_self_contained`.

## Is the current stack sufficient?

**For reads — no, and for an interesting reason.** The bundle is deliberately
the *published* subset: affirmed traits only, approved articles only, no `raw/`
paths, no lint findings, no `next` recommendation. A console needs precisely the
things that were withheld — the proposed traits waiting on a verdict, the
unapproved drafts, the uncompiled queue, the errors. It cannot reuse
`bundle.json`; it needs a fuller payload that only the local server serves. That
is a feature rather than an obstacle: the two audiences genuinely differ.

**For mutations — no, but the gap is small.** Every command already has `--json`
and a stable envelope, which is most of an API. What is missing is that `run()`
mostly *prints*: sixteen of twenty commands compute and emit in one function.
Only `next::recommend`, `status::summarize`, `dashboard::render` and
`lint::analyze` return a value a second caller can use.

Splitting compute from print is the prerequisite, and it is worth doing on its
own merits — it is the discipline those four already follow, and it is what
makes a second caller possible without a second implementation.

**For generation — no, and it is out of scope for the CLI.** See above.

## Three shapes

### A. `sentinel serve` — an HTTP server in the binary

A subcommand starting a loopback server that serves the page, a console payload,
and a small JSON API mapping onto the existing commands.

- **For:** one artifact; `cargo install` gets you everything; the API calls the
  same functions the CLI does, so there is one implementation of "approve"; and
  it can hold the archive lock across a request.
- **Against:** a new dependency, or a hand-rolled HTTP/1.1 server. Loopback,
  single client, no TLS makes that roughly two hundred lines — this project has
  hand-rolled comparable things — but header parsing is somewhere bugs live.

### B. Promote `ui/dev/serve.mjs` from dev tooling

Give the Node server routes that shell out to `sentinel … --json`.

- **For:** no Rust changes, no new Rust dependency, and the API is literally the
  CLI, so it cannot drift from it.
- **Against:** the product now needs Node. `cargo install sentinel` would give
  you a tool whose console lives somewhere else, which contradicts the
  self-contained line held everywhere else. And a shell-out cannot coordinate
  with the archive lock.

### C. No server — the browser writes files directly

File System Access API.

- **Against:** cannot take the lock, cannot run an agent, cannot run the
  validation that makes a mutation safe. It would reimplement the archive's
  rules in JavaScript, which is failure mode (4) in a different hat.

## Recommendation

**A**, with the payload/print split landing first as its own change.

B is tempting and would be right if this were only ever a personal tool, but
"the console needs Node and the CLI does not" is the kind of seam that makes a
project feel unfinished — and giving up the lock is a real loss, not a
theoretical one.

## One design risk worth naming now

The approval gate exists because *an agent must not approve its own work*. A
console that can dispatch the agent and approve its output with two adjacent
buttons has not removed that rule, but it has made rubber-stamping the path of
least resistance.

Worth designing against rather than discovering later: an approve control inert
until the piece has actually been displayed, the change shown next to the
verdict, and a note prompted for on rejection. The rule survives in the code
either way; this is about not building an interface that quietly encourages
defeating it.

## Staging

1. **Split compute from print.** Each `run()` becomes a thin printer over a
   function returning the payload it emits. No behaviour change, and the
   existing `--json` tests are the acceptance criterion.
2. **`sentinel serve`, read-only.** Loopback, token, and a console payload
   carrying what the published bundle withholds. The page grows the panels the
   showcase does not need: the review queue, the backlog, lint findings.
3. **Mutations**, one command at a time, each taking the lock: `review` first
   because it is what the human uses most, then `sources`, then `ingest` (which
   needs an upload), then `rm`/`mv` behind confirmation.
4. **Dispatch by clipboard.** Every rung shows its skill invocation.
5. **Dispatch by subprocess**, opt-in, only if 4 proves too slow in practice.
