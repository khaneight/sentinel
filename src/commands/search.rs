use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::output;
use crate::core::text;
use crate::core::wiki::{self, LoadedArticle};

/// Results returned unless `--limit` says otherwise.
///
/// Unbounded was the old behaviour and it was a context bomb: on a 423-article
/// archive, one common word matched every file and `search --json` emitted
/// 467 KB — roughly 117k tokens into the context of whatever asked.
pub const DEFAULT_LIMIT: usize = 20;

/// Matching lines shown per result. The human output already capped at 3; the
/// JSON did not, which is where most of that 467 KB came from.
pub const DEFAULT_MATCHES: usize = 3;

// Relevance weights. A title match is a different kind of evidence from a body
// mention, and treating them alike is why searching "virtue" used to rank a
// note that mentions it twice above the article actually titled "Virtue".
const TITLE_HIT: u32 = 1000;
const SLUG_HIT: u32 = 500;
const TAG_HIT: u32 = 200;
const BODY_HIT: u32 = 1;

/// Characters kept per matching line.
///
/// Human output already truncated for display; JSON did not, so a single long
/// paragraph could contribute kilobytes to a result the caller only needs
/// enough of to recognise.
const MATCH_EXCERPT: usize = 200;

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
    /// Relevance score. Exposed so a caller can see why the order is what it is.
    score: u32,
    /// Total matching lines in the file, even when `matches` is truncated.
    match_count: usize,
    matches: Vec<Match>,
}

#[derive(Serialize)]
struct Results {
    query: String,
    /// Files that matched, before `limit` was applied.
    result_count: usize,
    /// Files actually included below.
    returned: usize,
    /// True when `result_count > returned`.
    truncated: bool,
    results: Vec<SearchResult>,
}

pub fn run(query: &str, limit: usize, max_matches: usize) -> io::Result<()> {
    let articles = wiki::load_all()?.articles;
    let needle = query.to_lowercase();

    let mut results: Vec<SearchResult> = articles
        .iter()
        .filter_map(|article| score(article, &needle, max_matches))
        .collect();

    // Highest score first; path breaks ties so repeated runs agree.
    results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));

    let result_count = results.len();
    results.truncate(limit);
    let returned = results.len();

    if output::is_json() {
        return output::emit(
            "search",
            Results {
                query: query.to_string(),
                result_count,
                returned,
                truncated: result_count > returned,
                results,
            },
        );
    }

    if results.is_empty() {
        println!("No results for '{query}'.");
        return Ok(());
    }

    println!(
        "Found matches in {} file(s) for '{}'{}:\n",
        result_count.to_string().green(),
        query.bold(),
        if result_count > returned {
            format!(" — showing top {returned}")
        } else {
            String::new()
        }
    );

    for result in &results {
        println!(
            "  {} — {} ({} match{})",
            result.title.bold(),
            result.path.cyan(),
            result.match_count,
            if result.match_count == 1 { "" } else { "es" }
        );
        for m in &result.matches {
            println!("    L{}: {}", m.line, text::truncate_chars(&m.text, 100));
        }
        let hidden = result.match_count.saturating_sub(result.matches.len());
        if hidden > 0 {
            println!("    ... and {hidden} more");
        }
        println!();
    }

    if result_count > returned {
        println!(
            "{}",
            format!(
                "{} more file(s) matched. Use --limit to see more.",
                result_count - returned
            )
            .dimmed()
        );
    }

    Ok(())
}

/// Score one article, or `None` if it does not match at all.
fn score(article: &LoadedArticle, needle: &str, max_matches: usize) -> Option<SearchResult> {
    let frontmatter = &article.article.frontmatter;
    let title = article.title().to_string();
    let slug = article.slug();

    let mut score = 0;
    if title.to_lowercase().contains(needle) {
        score += TITLE_HIT;
    }
    if slug.to_lowercase().contains(needle) {
        score += SLUG_HIT;
    }
    score += frontmatter
        .tags
        .iter()
        .filter(|t| t.to_lowercase().contains(needle))
        .count() as u32
        * TAG_HIT;

    let mut match_count = 0;
    let mut matches = Vec::new();
    for (i, line) in article.content.lines().enumerate() {
        if !line.to_lowercase().contains(needle) {
            continue;
        }
        match_count += 1;
        if matches.len() < max_matches {
            matches.push(Match {
                line: i + 1,
                text: text::truncate_chars(line.trim(), MATCH_EXCERPT),
            });
        }
    }
    score += (match_count as u32).saturating_mul(BODY_HIT);

    if score == 0 {
        return None;
    }

    Some(SearchResult {
        path: article.rel_path().to_string(),
        slug,
        title,
        score,
        match_count,
        matches,
    })
}
