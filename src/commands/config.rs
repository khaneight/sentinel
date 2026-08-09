use std::io;
use std::path::Path;

use colored::Colorize;

use crate::core::paths;

/// Width that keeps every label in this report on one column.
const LABEL: usize = 18;

/// Report where sentinel thinks the archive is, and why.
///
/// This is the command you run when something is pointed at the wrong
/// directory, so it reports what it can even when resolution fails outright.
pub fn run(flag: Option<&str>) -> io::Result<()> {
    println!("{}", "Sentinel Configuration".bold());
    println!("──────────────────────────────");

    let resolution = paths::resolve_from_environment(flag, false);

    match &resolution {
        Ok(resolved) => {
            field(
                "Archive root:",
                resolved.path.display().to_string().green().to_string(),
            );
            field("Resolved via:", resolved.source.describe().to_string());
            field(
                "Initialized:",
                yes_no(resolved.path.join("meta/manifest.json").is_file()),
            );
        }
        Err(_) => {
            field("Archive root:", "none".red().to_string());
        }
    }

    println!("\n{}", "Inputs".bold());
    field("--archive:", show(flag));
    field(
        &format!("{}:", paths::ENV_ARCHIVE),
        show(std::env::var(paths::ENV_ARCHIVE).ok().as_deref()),
    );
    field(
        "Config file:",
        match paths::config_path() {
            Some(p) if p.is_file() => p.display().to_string().cyan().to_string(),
            Some(p) => format!("{} {}", p.display(), "(absent)".dimmed()),
            None => "(no config directory; set SENTINEL_CONFIG)"
                .dimmed()
                .to_string(),
        },
    );
    // A malformed config file is worth naming explicitly rather than letting it
    // surface as a confusing "no archive found".
    field(
        "Config archive:",
        match paths::Config::load() {
            Ok(config) => show(config.archive.as_deref()),
            Err(e) => format!("{} {e}", "unreadable:".red()),
        },
    );

    let resolved = match resolution {
        Ok(resolved) => resolved,
        Err(e) => {
            println!("\n{}", "No archive resolved.".red().bold());
            return Err(e);
        }
    };

    println!("\n{}", "Directories".bold());
    for name in ["raw", "wiki", "index", "meta", "templates"] {
        let path = resolved.path.join(name);
        println!("  {:<11} {}", format!("{name}/"), marker(&path));
    }

    Ok(())
}

fn field(label: &str, value: String) {
    println!("  {label:<width$} {value}", width = LABEL);
}

fn show(value: Option<&str>) -> String {
    match value {
        Some(v) if !v.trim().is_empty() => v.cyan().to_string(),
        _ => "(unset)".dimmed().to_string(),
    }
}

fn yes_no(value: bool) -> String {
    if value {
        "yes".green().to_string()
    } else {
        format!("{} — run `sentinel init`", "no".yellow())
    }
}

fn marker(path: &Path) -> String {
    if path.is_dir() {
        format!("{} {}", "✓".green(), path.display())
    } else {
        format!("{} {}", "✗".yellow(), path.display().to_string().dimmed())
    }
}
