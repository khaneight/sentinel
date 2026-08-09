use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::log::{self, Entry};
use crate::core::output;

/// Entries shown unless `--limit` says otherwise.
pub const DEFAULT_LIMIT: usize = 20;

#[derive(Serialize)]
struct History {
    /// Entries in the log, before `limit` was applied.
    entry_count: usize,
    returned: usize,
    truncated: bool,
    entries: Vec<Entry>,
}

/// Append an entry, or show recent ones when given no operation.
pub fn run(operation: Option<&str>, detail: Option<&str>, limit: usize) -> io::Result<()> {
    let Some(operation) = operation else {
        return show(limit);
    };
    let detail = detail.unwrap_or_default();
    log::append(operation, detail)?;
    if output::is_json() {
        return output::emit(
            "log",
            History {
                entry_count: 1,
                returned: 1,
                truncated: false,
                entries: log::read()?.into_iter().take(1).collect(),
            },
        );
    }
    println!("{} [{operation}] {detail}", "Logged:".green());
    Ok(())
}

fn show(limit: usize) -> io::Result<()> {
    let all = log::read()?;
    let entry_count = all.len();
    let entries: Vec<Entry> = all.into_iter().take(limit).collect();
    let returned = entries.len();

    if output::is_json() {
        return output::emit(
            "log",
            History {
                entry_count,
                returned,
                truncated: entry_count > returned,
                entries,
            },
        );
    }

    if entries.is_empty() {
        println!("No activity recorded yet.");
        return Ok(());
    }

    println!("{} (newest first)\n", "Activity".bold());
    for e in &entries {
        println!(
            "  {} {:<10} {}",
            e.date.dimmed(),
            e.operation.cyan(),
            e.detail
        );
    }
    if entry_count > returned {
        println!(
            "\n{}",
            format!(
                "{} older entr{} not shown. Use --limit to see more.",
                entry_count - returned,
                if entry_count - returned == 1 {
                    "y"
                } else {
                    "ies"
                }
            )
            .dimmed()
        );
    }
    Ok(())
}
