use std::io;
use std::path::Path;

use colored::Colorize;
use serde::Serialize;

use crate::core::output;
use crate::core::paths;

/// Width that keeps every label in this report on one column.
const LABEL: usize = 18;

#[derive(Serialize)]
struct ConfigReport {
    resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_via: Option<&'static str>,
    initialized: bool,
    inputs: Inputs,
    directories: Vec<Directory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct Inputs {
    archive_flag: Option<String>,
    env_archive: Option<String>,
    config_file: Option<String>,
    config_file_exists: bool,
    config_archive: Option<String>,
}

#[derive(Serialize)]
struct Directory {
    name: &'static str,
    path: String,
    exists: bool,
}

/// Report where sentinel thinks the archive is, and why.
///
/// This is the command you run when something is pointed at the wrong
/// directory, so it reports what it can even when resolution fails outright.
/// Returns the process exit code.
pub fn run(flag: Option<&str>) -> io::Result<i32> {
    let resolution = paths::resolve_from_environment(flag, false);
    let config_file = paths::config_path();
    let config_archive = paths::Config::load().ok().and_then(|c| c.archive);

    let inputs = Inputs {
        archive_flag: nonempty(flag),
        env_archive: std::env::var(paths::ENV_ARCHIVE)
            .ok()
            .and_then(|v| nonempty(Some(&v))),
        config_file: config_file.as_ref().map(|p| p.display().to_string()),
        config_file_exists: config_file.as_ref().is_some_and(|p| p.is_file()),
        config_archive,
    };

    if output::is_json() {
        return emit_json(&resolution, inputs);
    }

    println!("{}", "Sentinel Configuration".bold());
    println!("──────────────────────────────");

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
        Err(_) => field("Archive root:", "none".red().to_string()),
    }

    println!("\n{}", "Inputs".bold());
    field("--archive:", show(inputs.archive_flag.as_deref()));
    field(
        &format!("{}:", paths::ENV_ARCHIVE),
        show(inputs.env_archive.as_deref()),
    );
    field(
        "Config file:",
        match (&inputs.config_file, inputs.config_file_exists) {
            (Some(p), true) => p.clone().cyan().to_string(),
            (Some(p), false) => format!("{p} {}", "(absent)".dimmed()),
            (None, _) => "(no config directory; set SENTINEL_CONFIG)"
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

    Ok(0)
}

fn emit_json(resolution: &io::Result<paths::ResolvedRoot>, inputs: Inputs) -> io::Result<i32> {
    // Without a root there is nothing to base directory paths on, and
    // `output::emit` cannot name the archive either — so report the failure as
    // a plain JSON error rather than half a report.
    let Ok(resolved) = resolution else {
        let message = resolution
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default();
        output::emit_error(&message);
        return Ok(1);
    };

    // `config` runs before main installs the root, because it must survive
    // resolution failing. Now that it has succeeded, install it so the JSON
    // envelope can name the archive.
    paths::set_archive_root(resolved.path.clone());

    let directories = ["raw", "wiki", "index", "meta", "templates"]
        .iter()
        .map(|name| {
            let path = resolved.path.join(name);
            Directory {
                name,
                exists: path.is_dir(),
                path: path.display().to_string(),
            }
        })
        .collect();

    output::emit(
        "config",
        ConfigReport {
            resolved: true,
            resolved_via: Some(resolved.source.as_str()),
            initialized: resolved.path.join("meta/manifest.json").is_file(),
            inputs,
            directories,
            error: None,
        },
    )?;
    Ok(0)
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
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
