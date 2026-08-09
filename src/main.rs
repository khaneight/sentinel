mod commands;
mod core;

use std::io;

use clap::{Parser, Subcommand};

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
     Run `sentinel config` to see which rule is in effect."
))]
struct Cli {
    /// Archive root to operate on (overrides SENTINEL_ARCHIVE and the config file)
    #[arg(short = 'A', long, global = true, value_name = "PATH")]
    archive: Option<String>,

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

    /// Reconcile the manifest with raw/ (register new files, drop deleted ones)
    Sync {
        /// Report what would change without writing the manifest
        #[arg(long)]
        dry_run: bool,
    },

    /// Show knowledge base status overview
    Status,

    /// List raw docs that haven't been compiled into wiki articles
    Uncompiled,

    /// Rebuild all index files and link graph
    Index,

    /// Validate links, frontmatter, and find orphans
    Lint,

    /// Full-text search across wiki articles
    Search {
        /// Search query
        query: String,
    },

    /// Print the backlink graph
    Graph,

    /// Append an entry to the activity log
    Log {
        /// Operation name (e.g. compile, research, improve, ask)
        operation: String,

        /// Description of what happened
        detail: String,
    },
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> io::Result<()> {
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

    match cli.command {
        Commands::Init { set_default, .. } => commands::init::run(&resolved, set_default),
        Commands::Config => unreachable!("handled above"),
        Commands::Ingest {
            path,
            domain,
            origin,
            title,
        } => commands::ingest::run(&path, &domain, &origin, title.as_deref()),
        Commands::IngestRepo { path, domain, name } => {
            commands::ingest_repo::run(&path, &domain, name.as_deref())
        }
        Commands::Sync { dry_run } => commands::sync::run(dry_run),
        Commands::Status => commands::status::run(),
        Commands::Uncompiled => commands::uncompiled::run(),
        Commands::Index => commands::index::run(),
        Commands::Lint => commands::lint::run(),
        Commands::Search { query } => commands::search::run(&query),
        Commands::Graph => commands::graph::run(),
        Commands::Log { operation, detail } => commands::log::run(&operation, &detail),
    }
}
