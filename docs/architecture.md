# Architecture: what exists, and what the goal needs

The goal, stated plainly: a place to **write or upload documents**, where an AI
**distils a persona from them over time**, where you can **instruct it to write
or research** — by directive or freeform — and where you can **share the graph
or a single piece by generating a link**.

This document maps what is built against that, and says where the two diverge.

## What exists now

```
                     ┌──────────────────────────────────────┐
                     │  the archive — files, in git         │
                     │                                      │
                     │  raw/        what you wrote          │
                     │  persona/    traits, each cited      │
                     │  wiki/       compiled + written      │
                     │  meta/       manifest, graph, log,   │
                     │              progress.jsonl          │
                     └──────────────────────────────────────┘
                            ▲                    │
                            │ reads / writes     │ reads
                            │                    ▼
              ┌─────────────────────┐   ┌────────────────────┐
              │  sentinel  (Rust)   │   │  an agent, running │
              │  ~12,800 lines      │   │  skills/*.md       │
              │                     │   │                    │
              │  20 subcommands     │   │  clone   compile   │
              │  --json on all      │   │  extend  research  │
              │  no model in it     │   │  ask     improve   │
              └─────────────────────┘   │  grow              │
                            │           └────────────────────┘
                            │ export --ui
                            ▼
              ┌──────────────────────────────┐
              │  index.html + bundle.json    │
              │  static, read-only, dead      │
              └──────────────────────────────┘
```

**`sentinel` knows the rules and has no model.** It resolves the archive,
validates frontmatter, derives the raw→wiki mapping and the persona's coverage,
ranks what is most worth doing next, records verdicts, and publishes. It cannot
write a sentence.

**The skills are the other half.** Distilling a document into traits, compiling
a source, writing something new — all of that is an agent reading
`skills/*.md` and doing the work, with the CLI ranking it beforehand and
validating it afterwards. `tests/skill_flows.rs` executes every command those
skills publish, so they stay real.

**The published page is deliberately inert.** One HTML file, one JSON file, no
server. Its value is that you can copy it anywhere and nothing can happen to it.

### Five properties worth keeping

1. **Files are the source of truth.** The archive is markdown in git. Every
   safeguard — citations, verdicts, approvals — lives in frontmatter, so it
   survives export, clone, and this tool being abandoned.
2. **Derived, not recorded.** Compilation mapping, persona coverage, layers,
   the backlog: all computed live. A stored copy is right until someone edits a
   file by hand.
3. **The model is outside the boundary.** The rules are testable Rust; the
   judgement is a prompt. Neither pretends to be the other.
4. **One implementation per rule.** `sentinel review` is the only writer of
   `review:`; `SourceIndex` is the only matcher of a citation.
5. **Nothing publishes itself.** Approval is a separate axis from maturity, and
   the exporter — not the agent — writes the attribution notice.

## Where the goal diverges

| the goal wants | today | gap |
|---|---|---|
| write or upload documents in a UI | `sentinel ingest <file>` | no process, no upload, no editor |
| AI distils **over time** | `learn` rung, run by hand | no jobs, no schedule, no consolidation |
| instruct with directives or freeform | skills take `$ARGUMENTS` | nothing dispatches them |
| share the graph or one piece by link | `export` dumps the whole subset | no per-item grant, no revocation |

Four gaps. Three are missing machinery. The one that is a genuine *design* gap
is the second, and it is worth stating on its own.

### "Over time" is the hard word

A persona that only accumulates gets worse. Distil forty documents and you have
forty traits, many of them near-duplicates, some contradicted by things you
wrote later, and a review queue nobody will ever finish. There is no rung for
that today: `learn` only adds.

What "over time" actually needs:

- **consolidation** — merging traits that say the same thing, and raising
  confidence when a second document corroborates one rather than writing a
  third near-copy;
- **contradiction** — two affirmed traits that cannot both be true is a fact
  worth surfacing, and reconciling them is the author's call, not the archive's;
- **staleness** — a trait whose evidence is five years old, next to newer
  writing that goes the other way. Not automatically wrong. Worth asking about.

And a throttle. **The review queue is the scarce resource**, not compute: if
background work outruns your attention, the tool has manufactured a backlog of
decisions only you can make. Background generation should stop while more than a
handful of things await a verdict — the same shape as `/sentinel-grow`'s
existing stop conditions, applied to the scheduler.

## The target

```
        ┌──────────────────────────────────────────────┐
        │  the archive — unchanged, still files, git    │
        └──────────────────────────────────────────────┘
              ▲                ▲                 ▲
              │                │                 │
     ┌────────┴──────┐  ┌──────┴────────────────┴───────┐
     │ sentinel CLI  │  │   sentinel serve  (loopback)  │
     │ unchanged     │  │   token-gated, holds the lock │
     └───────────────┘  └───┬──────────┬────────────┬───┘
                            │          │            │
                    ┌───────▼──┐ ┌─────▼──────┐ ┌───▼────────┐
                    │ console  │ │ job runner │ │ publisher  │
                    │ web UI   │ │            │ │            │
                    │          │ │ spawns an  │ │ per-share  │
                    │ write ·  │ │ agent with │ │ artifacts, │
                    │ review · │ │ a directive│ │ revocable  │
                    │ dispatch │ │            │ │            │
                    └──────────┘ └────────────┘ └────────────┘
```

Everything new sits *beside* the CLI, not inside it. The archive does not
change. The rules do not move.

### `sentinel serve`

A loopback HTTP server, token-gated, refusing non-local binds without an
explicit flag. Its API is the commands — after each `run()` is split into a
function that computes a payload and a thin printer over it, so there is never a
second implementation of "approve".

It serves a **console payload**, which is not `bundle.json`. The published
bundle is deliberately the *published subset*: affirmed traits only, approved
articles only, no `raw/` paths, no findings. A console needs exactly what was
withheld. Two audiences, two payloads — and that separation is what keeps the
published artifact safe.

### The job runner

The subsystem that does not exist at all today, and the one that makes "over
time" and "instruct it" possible.

```
meta/jobs/<id>.json
  { id, kind, directive, status, started, finished, produced[], log, error? }
```

`kind` is a rung — `distil`, `compile`, `write`, `research` — and `directive` is
your freeform instruction, appended to the skill invocation the same way
`$ARGUMENTS` already works. The runner spawns an agent headlessly in the archive
directory, streams its output to a log the console tails, and on completion runs
`index` and `lint` and records what changed.

Three rules it has to obey:

- **one at a time, holding the lock** — two agents writing the archive at once
  is the failure `meta/.lock` exists for;
- **it produces, it never approves** — a job ends at *awaiting your verdict*,
  and the existing gate is what makes that meaningful;
- **it stops when the queue is deep** — see the throttle above.

### Sharing

A share is an explicit, revocable grant on one thing:

```
meta/shares.jsonl
  { token, kind: graph | article | trait, target, created, revoked_at? }
```

`sentinel share <target>` mints an unguessable token and prints a URL;
`--revoke` retires it. `export` then writes one self-contained artifact per
active share, containing **only** what that share grants — the same
published-subset discipline, applied per link rather than per archive.

This keeps sharing local-first: it is still static files on a host you choose,
so no service holds your archive. Two honest limits. Revocation removes the
artifact on the next `export --clean`, but anyone who already fetched it still
has it — an unguessable URL is a capability, not an access check. And a shared
`extrapolated` piece still carries its attribution notice and still needs
approval first: sharing must never become a way around the gate.

### Writing in the console

Two paths into `raw/`, both ending in `ingest`: upload a file, or write one in
the editor. The `origin` choice is the consequential one and should be a
deliberate act rather than a defaulted dropdown — `authored` is what feeds the
persona, and a research paper filed as your own writing will be distilled into
claims about you.

This closes a loop worth naming: **the console is where you write, and what you
write is what it learns from.**

## The three decisions that matter

1. **Local-first, or hosted?** Local. The file-based archive is the reason every
   safeguard survives this tool. A hosted version would be a different product
   and would give that up. Sharing can be static artifacts; nothing here needs a
   database.
2. **Who runs the model?** A subprocess the runner spawns, following the same
   skills a human invokes. Not a reimplementation of the skills inside the
   server — those are the specification, they are already executed against a
   real archive by the test suite, and a second copy would drift like every
   second copy in this project has.
3. **What throttles it?** Your attention. Not tokens, not a schedule.

## Order

1. Split compute from print. No behaviour change; the existing `--json` tests
   are the acceptance criterion.
2. `sentinel serve`, read-only, with the console payload.
3. Mutations, one at a time under the lock: `review`, `sources`, `ingest`,
   then `rm`/`mv` behind confirmation.
4. Writing and upload in the console.
5. The job runner: dispatch by clipboard first, then subprocess.
6. Consolidation and contradiction rungs — the part that makes "over time" mean
   something.
7. Shares.

Steps 1–4 make the tool usable without a terminal. Step 5 is where it starts
working on its own. Step 6 is what stops it becoming noise.
