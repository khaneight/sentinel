mod commands;
mod core;

use std::io;

use clap::{Parser, Subcommand};

use crate::core::output::{self, Format};
use crate::core::paths;

#[derive(Parser)]
#[command(name = "sentinel")]
#[command(about = "CLI tooling for a markdown knowledge base")]
#[command(version)]
#[command(after_help = concat!(
    "The archive root is resolved in this order:\n  \
     1. --archive <PATH>\n  \
     2. SENTINEL_ARCHIVE environment variable\n  \
     3. archive = \"...\" in ~/.config/sentinel/config.toml\n  \
     4. the nearest parent directory containing meta/manifest.json\n\n\
     Run `sentinel config` to see which rule is in effect.\n\n\
     Exit codes: 0 success, 1 error, 2 the command ran and found problems."
))]
struct Cli {
    /// Archive root to operate on (overrides SENTINEL_ARCHIVE and the config file)
    #[arg(short = 'A', long, global = true, value_name = "PATH")]
    archive: Option<String>,

    /// Emit machine-readable JSON instead of formatted text
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the archive directory structure
    Init {
        /// Where to create the archive (defaults to the resolved archive, else the current directory)
        path: Option<String>,

        /// Record this archive as the default in ~/.config/sentinel/config.toml
        #[arg(long)]
        set_default: bool,
    },

    /// Show the resolved archive root and how it was resolved
    Config,

    /// Ingest a document into raw/
    Ingest {
        /// Path to the file to ingest
        path: String,

        /// Domain to file under (e.g. philosophy, coding, research)
        #[arg(short, long)]
        domain: String,

        /// Provenance: authored (your writing) or researched (AI-gathered)
        #[arg(short, long, default_value = "authored")]
        origin: String,

        /// Optional title override (defaults to filename)
        #[arg(short, long)]
        title: Option<String>,

        /// Filename to store it under in raw/{domain}/ (defaults to the title's
        /// slug, else the source filename)
        #[arg(long = "as", value_name = "FILENAME")]
        filename: Option<String>,
    },

    /// Ingest and analyze a codebase
    IngestRepo {
        /// Path or URL to the repository
        path: String,

        /// Domain to file under
        #[arg(short, long, default_value = "coding")]
        domain: String,

        /// Name for the codebase entry
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Move or rename a raw document, repointing every citation to it
    Mv {
        /// Existing raw document: archive-relative path, or a unique filename
        from: String,

        /// New location: archive-relative path under raw/, or a bare filename
        /// to rename it within its current domain
        to: String,

        /// Report what would change without writing anything
        #[arg(long)]
        dry_run: bool,
    },

    /// Delete a raw document, refusing if wiki articles cite it
    Rm {
        /// Raw document: archive-relative path, or a unique filename
        target: String,

        /// Delete even though articles cite it, breaking their provenance
        #[arg(long)]
        force: bool,

        /// Report what would happen without deleting anything
        #[arg(long)]
        dry_run: bool,
    },

    /// Reconcile the manifest with raw/ (register new files, drop deleted ones)
    Sync {
        /// Report what would change without writing the manifest
        #[arg(long)]
        dry_run: bool,
    },

    /// Show knowledge base status overview
    Status,

    /// Recommend the single most valuable next action
    Next {
        /// Report targets for this action instead of the recommendation.
        /// Lets a caller schedule across categories rather than always
        /// following strict priority.
        #[arg(long, value_name = "ACTION")]
        action: Option<String>,
    },

    /// Print the wiki article contract: frontmatter fields, domains, lint rules
    Schema,

    /// List raw docs that haven't been compiled into wiki articles
    Uncompiled,

    /// Rebuild all index files and link graph
    Index,

    /// Validate frontmatter, links, and the raw/wiki mapping
    ///
    /// Exits 2 when errors are found; warnings alone exit 0 unless --strict.
    Lint {
        /// Treat warnings as failures too
        #[arg(long)]
        strict: bool,

        /// Report counts per rule instead of listing every finding
        #[arg(long)]
        summary: bool,

        /// List only findings for one rule id (counts still cover everything)
        #[arg(long, value_name = "ID")]
        rule: Option<String>,
    },

    /// Full-text search across wiki articles, ranked by relevance
    Search {
        /// Search query
        query: String,

        /// Maximum files to return
        #[arg(long, default_value_t = commands::search::DEFAULT_LIMIT)]
        limit: usize,

        /// Maximum matching lines to show per file
        #[arg(long, default_value_t = commands::search::DEFAULT_MATCHES)]
        matches: usize,
    },

    /// Print the link graph, or one article's neighbourhood
    Graph {
        /// Show only what surrounds this slug, instead of the whole topology
        #[arg(long, value_name = "SLUG")]
        node: Option<String>,

        /// How many hops from --node to include
        #[arg(long, default_value_t = 1)]
        depth: usize,
    },

    /// Append an entry to the activity log, or show recent entries
    Log {
        /// Operation name (e.g. compile, research, improve, ask).
        /// Omit both arguments to show recent activity instead.
        operation: Option<String>,

        /// Description of what happened
        detail: Option<String>,

        /// Entries to show when reading
        #[arg(long, default_value_t = commands::log::DEFAULT_LIMIT)]
        limit: usize,
    },
}

fn main() {
    let cli = Cli::parse();
    // Installed before anything can fail, so even resolution errors honour it.
    output::set_format(if cli.json {
        Format::Json
    } else {
        Format::Human
    });

    match run(cli) {
        Ok(0) => {}
        Ok(code) => std::process::exit(code),
        Err(e) => {
            if output::is_json() {
                output::emit_error(&e.to_string());
            } else {
                eprintln!("Error: {e}");
            }
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) -> io::Result<i32> {
    // `sentinel config` exists to diagnose resolution problems, so it must not
    // die on the very failure the user is trying to inspect.
    if matches!(cli.command, Commands::Config) {
        return commands::config::run(cli.archive.as_deref());
    }

    // `init [PATH]` is a second spelling of `--archive PATH`, and wins over it:
    // the positional argument is the more specific, more local statement of intent.
    let init_path = match &cli.command {
        Commands::Init { path, .. } => path.clone(),
        _ => None,
    };
    let requested = init_path.as_deref().or(cli.archive.as_deref());

    // Only `init` may fall back to the working directory — every other command
    // operating on a directory that merely *looks* like an archive would be a
    // surprising way to scatter files around the filesystem.
    let creating = matches!(cli.command, Commands::Init { .. });
    let resolved = paths::resolve_from_environment(requested, creating)?;
    paths::set_archive_root(resolved.path.clone());

    // Commands that read the manifest, change it, and write it back must not
    // interleave: two doing so both read the same state and the second write
    // discards the first's. Measured, that lost one entry per pair of
    // concurrent `ingest` calls, every one reporting success.
    let _lock = match &cli.command {
        Commands::Ingest { .. }
        | Commands::IngestRepo { .. }
        | Commands::Sync { .. }
        | Commands::Index
        | Commands::Mv { .. }
        | Commands::Rm { .. } => Some(core::lock::ArchiveLock::acquire(&paths::meta_dir())?),
        // Queries and commands that touch no shared state run unserialised.
        _ => None,
    };

    match cli.command {
        Commands::Init { set_default, .. } => {
            commands::init::run(&resolved, set_default).map(|()| 0)
        }
        Commands::Config => unreachable!("handled above"),
        Commands::Ingest {
            path,
            domain,
            origin,
            title,
            filename,
        } => commands::ingest::run(
            &path,
            &domain,
            &origin,
            title.as_deref(),
            filename.as_deref(),
        )
        .map(|()| 0),
        Commands::IngestRepo { path, domain, name } => {
            commands::ingest_repo::run(&path, &domain, name.as_deref()).map(|()| 0)
        }
        Commands::Mv { from, to, dry_run } => commands::mv::run(&from, &to, dry_run).map(|()| 0),
        Commands::Rm {
            target,
            force,
            dry_run,
        } => commands::rm::run(&target, force, dry_run).map(|()| 0),
        Commands::Sync { dry_run } => commands::sync::run(dry_run).map(|()| 0),
        Commands::Status => commands::status::run().map(|()| 0),
        Commands::Next { action } => {
            let action = action
                .map(|a| a.parse())
                .transpose()
                .map_err(|e: String| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            commands::next::run(action).map(|()| 0)
        }
        Commands::Schema => commands::schema::run().map(|()| 0),
        Commands::Uncompiled => commands::uncompiled::run().map(|()| 0),
        Commands::Index => commands::index::run().map(|()| 0),
        Commands::Lint {
            strict,
            summary,
            rule,
        } => commands::lint::run(strict, summary, rule.as_deref()),
        Commands::Search {
            query,
            limit,
            matches,
        } => commands::search::run(&query, limit, matches).map(|()| 0),
        Commands::Graph { node, depth } => commands::graph::run(node.as_deref(), depth).map(|()| 0),
        Commands::Log {
            operation,
            detail,
            limit,
        } => commands::log::run(operation.as_deref(), detail.as_deref(), limit).map(|()| 0),
    }
}
