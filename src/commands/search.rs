use std::io;

use colored::Colorize;
use walkdir::WalkDir;

use crate::core::paths;
use crate::core::text;

struct SearchResult {
    rel_path: String,
    matches: Vec<(usize, String)>, // (line_number, line_content)
}

pub fn run(query: &str) -> io::Result<()> {
    let wiki_dir = paths::wiki_dir();
    if !wiki_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Wiki directory not found. Run `sentinel init` first.",
        ));
    }

    let query_lower = query.to_lowercase();
    let mut results: Vec<SearchResult> = Vec::new();

    for entry in WalkDir::new(&wiki_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let path = entry.path();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let matches: Vec<(usize, String)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| line.to_lowercase().contains(&query_lower))
            .map(|(i, line)| (i + 1, line.to_string()))
            .collect();

        if !matches.is_empty() {
            let rel_path = paths::rel(path);

            results.push(SearchResult { rel_path, matches });
        }
    }

    // Sort by number of matches (most relevant first)
    results.sort_by(|a, b| b.matches.len().cmp(&a.matches.len()));

    if results.is_empty() {
        println!("No results for '{query}'.");
    } else {
        println!(
            "Found matches in {} file(s) for '{}':\n",
            results.len().to_string().green(),
            query.bold()
        );

        for result in &results {
            println!(
                "  {} ({} match{})",
                result.rel_path.cyan(),
                result.matches.len(),
                if result.matches.len() == 1 { "" } else { "es" }
            );
            for (line_num, line) in result.matches.iter().take(3) {
                let display = text::truncate_chars(line.trim(), 100);
                println!("    L{line_num}: {display}");
            }
            if result.matches.len() > 3 {
                println!("    ... and {} more", result.matches.len() - 3);
            }
            println!();
        }
    }

    Ok(())
}
