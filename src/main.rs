mod commands;
mod core;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sentinel")]
#[command(about = "CLI tooling for the /archive knowledge base")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the archive directory structure
    Init,

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

    /// Sync raw/ directory with manifest (register untracked files)
    Sync,

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

}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init => commands::init::run(),
        Commands::Ingest { path, domain, origin, title } => {
            commands::ingest::run(&path, &domain, &origin, title.as_deref())
        }
        Commands::IngestRepo { path, domain, name } => {
            commands::ingest_repo::run(&path, &domain, name.as_deref())
        }
        Commands::Sync => commands::sync::run(),
        Commands::Status => commands::status::run(),
        Commands::Uncompiled => commands::uncompiled::run(),
        Commands::Index => commands::index::run(),
        Commands::Lint => commands::lint::run(),
        Commands::Search { query } => commands::search::run(&query),
        Commands::Graph => commands::graph::run(),

    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
