use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::output;
use crate::core::text;
use crate::core::wiki;

/// How many matching lines are shown per file in human output.
const PREVIEW_LINES: usize = 3;

#[derive(Serialize)]
struct Match {
    line: usize,
    text: String,
}

#[derive(Serialize)]
struct SearchResult {
    path: String,
    slug: String,
    title: String,
    match_count: usize,
    matches: Vec<Match>,
}

#[derive(Serialize)]
struct Results {
    query: String,
    result_count: usize,
    results: Vec<SearchResult>,
}

pub fn run(query: &str) -> io::Result<()> {
    let articles = wiki::load_all()?;
    let query_lower = query.to_lowercase();

    let mut results: Vec<SearchResult> = articles
        .iter()
        .filter_map(|article| {
            let matches: Vec<Match> = article
                .content
                .lines()
                .enumerate()
                .filter(|(_, line)| line.to_lowercase().contains(&query_lower))
                .map(|(i, line)| Match {
                    line: i + 1,
                    text: line.trim().to_string(),
                })
                .collect();

            if matches.is_empty() {
                return None;
            }
            Some(SearchResult {
                path: article.rel_path().to_string(),
                slug: article.slug(),
                title: article.title().to_string(),
                match_count: matches.len(),
                matches,
            })
        })
        .collect();

    // Most matches first; path breaks ties so repeated runs agree.
    results.sort_by(|a, b| {
        b.match_count
            .cmp(&a.match_count)
            .then_with(|| a.path.cmp(&b.path))
    });

    if output::is_json() {
        return output::emit(
            "search",
            Results {
                query: query.to_string(),
                result_count: results.len(),
                results,
            },
        );
    }

    if results.is_empty() {
        println!("No results for '{query}'.");
        return Ok(());
    }

    println!(
        "Found matches in {} file(s) for '{}':\n",
        results.len().to_string().green(),
        query.bold()
    );

    for result in &results {
        println!(
            "  {} — {} ({} match{})",
            result.title.bold(),
            result.path.cyan(),
            result.match_count,
            if result.match_count == 1 { "" } else { "es" }
        );
        for m in result.matches.iter().take(PREVIEW_LINES) {
            println!("    L{}: {}", m.line, text::truncate_chars(&m.text, 100));
        }
        if result.match_count > PREVIEW_LINES {
            println!("    ... and {} more", result.match_count - PREVIEW_LINES);
        }
        println!();
    }

    Ok(())
}
