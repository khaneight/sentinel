use std::fs::OpenOptions;
use std::io::{self, Write};

use crate::core::paths;

/// Append an entry to the activity log.
///
/// Format: `## [YYYY-MM-DD] {operation} | {detail}`
pub fn append(operation: &str, detail: &str) -> io::Result<()> {
    let path = paths::log_path();
    let date = chrono::Local::now().format("%Y-%m-%d");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    writeln!(file, "## [{date}] {operation} | {detail}\n")
}
