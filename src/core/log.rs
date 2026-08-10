use std::fs::OpenOptions;
use std::io::{self, Write};

use crate::core::paths;

/// One recorded operation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Entry {
    pub date: String,
    pub operation: String,
    pub detail: String,
}

/// Append an entry to the activity log.
///
/// Format: `## [YYYY-MM-DD] {operation} | {detail}`, one entry per line.
///
/// The detail is collapsed to a single line. The file documents itself as
/// parseable with `grep "^## \["`, and a detail containing a newline produced
/// continuation lines that grep silently drops — so the recorded text and the
/// text any documented reader sees would differ.
pub fn append(operation: &str, detail: &str) -> io::Result<()> {
    // The log is append-only and never pruned, so a blank entry is permanent
    // litter — and it reaches further than the file: `sentinel log --json`
    // returns it as a real entry and the dashboard prints it as a bullet with
    // nothing in it. An operation nobody named is not an operation.
    let operation = operation.trim();
    if operation.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "An operation name is required: `sentinel log <operation> [detail]`. \
             The log is append-only, so a blank entry cannot be removed later.",
        ));
    }

    let path = paths::log_path();
    let date = chrono::Local::now().format("%Y-%m-%d");
    let detail = detail.split_whitespace().collect::<Vec<_>>().join(" ");

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    writeln!(file, "## [{date}] {operation} | {detail}\n")
}

/// Read recorded entries, newest first.
///
/// The log grows without bound and is the only durable record of what changed
/// the archive — `/sentinel-grow` says so. Without a bounded reader the only
/// way to consult it was to read the whole file, which is the context problem
/// this tool spent PRs removing from `search`, `graph` and `lint`.
pub fn read() -> io::Result<Vec<Entry>> {
    let path = paths::log_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)?;
    let mut entries: Vec<Entry> = text.lines().filter_map(parse_line).collect();
    entries.reverse();
    Ok(entries)
}

/// Parse one `## [date] operation | detail` line.
fn parse_line(line: &str) -> Option<Entry> {
    let rest = line.strip_prefix("## [")?;
    let (date, rest) = rest.split_once("] ")?;
    let (operation, detail) = match rest.split_once(" | ") {
        Some((op, detail)) => (op, detail),
        // An entry written before details were required, or by hand.
        None => (rest, ""),
    };
    Some(Entry {
        date: date.to_string(),
        operation: operation.to_string(),
        detail: detail.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_line_parses() {
        let e = parse_line("## [2026-08-09] compile | 3 articles").unwrap();
        assert_eq!(e.date, "2026-08-09");
        assert_eq!(e.operation, "compile");
        assert_eq!(e.detail, "3 articles");
    }

    #[test]
    fn a_detail_containing_the_separator_keeps_all_of_it() {
        let e = parse_line("## [2026-08-09] mv | a.md | b.md -> c.md").unwrap();
        assert_eq!(e.operation, "mv");
        assert_eq!(e.detail, "a.md | b.md -> c.md");
    }

    #[test]
    fn prose_and_headings_are_not_entries() {
        assert!(parse_line("# Activity Log").is_none());
        assert!(parse_line("*Append-only record.*").is_none());
        assert!(parse_line("").is_none());
    }

    #[test]
    fn an_entry_without_a_detail_still_parses() {
        let e = parse_line("## [2026-08-09] init").unwrap();
        assert_eq!(e.operation, "init");
        assert_eq!(e.detail, "");
    }
}
