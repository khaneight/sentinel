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
- `src/core/paths.rs` — archive path constants and helpers (currently hardcoded to `/home/khaneight/Documents/archive`)
- `src/core/manifest.rs` — JSON manifest tracking raw documents and compilation status
- `src/core/frontmatter.rs` — YAML frontmatter parsing/rendering for wiki articles
- `src/core/links.rs` — wikilink extraction and link graph (forward + backlinks)
- `src/commands/` — one module per CLI subcommand: init, ingest, ingest-repo, sync, status, uncompiled, index, lint, search, graph

## Skills

Claude Code skill definitions live in `skills/{skill-name}/SKILL.md`. Each skill has YAML frontmatter with `name`, `description`, and `user-invocable: true`.

Skills are prefixed `sentinel-` to avoid namespace collisions:
- `sentinel-ask` — query the knowledge base
- `sentinel-compile` — compile raw docs into wiki articles
- `sentinel-research` — research a topic and add findings
- `sentinel-improve` — health check and quality improvement

The archive's `.claude/skills` is a symlink to this repo's `skills/` directory, so changes here are immediately available in the archive context.

## Known Limitations

- Archive path is hardcoded in `src/core/paths.rs` — must become configurable before distribution
- `ingest-repo` command is a stub (not yet implemented)
- `meta/log.md` is append-only — commands that mutate state (init, ingest, sync, index, lint) auto-append entries
- No CI/CD pipeline
