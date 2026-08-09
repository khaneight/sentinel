# Sentinel

Rust CLI for managing a personal knowledge base. Operates on an archive directory containing raw source documents, compiled wiki articles with YAML frontmatter, Obsidian-style `[[wikilinks]]`, and auto-generated indexes.

## Build

```
cargo build
cargo test
cargo clippy
cargo fmt --check
```

## Architecture

- `src/main.rs` — clap-derive CLI entry point, defines all subcommands
- `src/core/paths.rs` — archive root resolution, derived path helpers, user config file
- `src/core/manifest.rs` — JSON manifest tracking raw documents and compilation status
- `src/core/frontmatter.rs` — YAML frontmatter parsing/rendering for wiki articles
- `src/core/links.rs` — wikilink extraction and link graph (forward + backlinks)
- `src/commands/` — one module per CLI subcommand: init, config, ingest, ingest-repo, sync, status, uncompiled, index, lint, search, graph
- `tests/` — integration tests that drive the compiled binary against temporary archives

## Skills

Claude Code skill definitions live in `skills/{skill-name}/SKILL.md`. Each skill has YAML frontmatter with `name`, `description`, and `user-invocable: true`.

Skills are prefixed `sentinel-` to avoid namespace collisions:
- `sentinel-ask` — query the knowledge base
- `sentinel-compile` — compile raw docs into wiki articles
- `sentinel-research` — research a topic and add findings
- `sentinel-improve` — health check and quality improvement

The archive's `.claude/skills` is a symlink to this repo's `skills/` directory, so changes here are immediately available in the archive context.

## Archive resolution

Sentinel never assumes where the archive lives. `src/core/paths.rs::resolve` picks a root in this order:

1. `--archive <PATH>` (global flag; `sentinel init <PATH>` is an equivalent spelling)
2. `SENTINEL_ARCHIVE`
3. `archive = "..."` in `~/.config/sentinel/config.toml` (override the file with `SENTINEL_CONFIG`)
4. the nearest ancestor of the working directory containing `meta/manifest.json`

Only `init` may fall back to the working directory, and only when that directory is empty. Everything else errors with the four options above rather than guessing. `sentinel config` prints the resolved root and which rule produced it.

Commands reach the root through `paths::archive_root()`, which reads a `OnceLock` installed by `main` before dispatch. Path resolution logic lives in the pure `paths::resolve` function so it can be unit tested without touching the process environment.

## Known Limitations

- `ingest-repo` command is a stub (not yet implemented)
- `meta/log.md` is append-only — commands that mutate state (init, ingest, sync, index, lint) auto-append entries
- No CI/CD pipeline
