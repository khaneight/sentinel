use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::sync::LazyLock;

use super::paths;

static WIKILINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap());

/// Extract all [[wikilink]] targets from content.
pub fn extract_wikilinks(content: &str) -> Vec<String> {
    WIKILINK_RE
        .captures_iter(content)
        .map(|cap| cap[1].trim().to_string())
        .collect()
}

/// A concept the wiki refers to but has not written yet.
///
/// Every `[[wikilink]]` with no matching article is the archive naming a gap in
/// itself. Ranked by how many distinct articles point at it, this is a demand
/// signal: the most-referenced unwritten concept is the one the existing
/// knowledge most wants filled in.
#[derive(Debug, Clone, Serialize)]
pub struct WantedArticle {
    /// Canonical slug — the filename an article for this concept should use.
    pub slug: String,
    /// Articles that link to it, sorted. Demand is distinct articles, and
    /// spellings are folded together first, so `[[Free Will]]` in one article
    /// and `[[free-will]]` in another count as two articles wanting one thing.
    pub referrers: Vec<String>,
    /// The spellings actually used, when they differ from the canonical form.
    /// Worth surfacing: inconsistent naming across articles is itself a finding.
    pub variants: Vec<String>,
}

/// Find every wikilink target that has no article, most-wanted first.
pub fn wanted(articles: &[super::wiki::LoadedArticle]) -> Vec<WantedArticle> {
    let existing: HashSet<String> = articles.iter().map(|a| a.canonical_slug()).collect();
    let mut demand: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut spellings: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for article in articles {
        for target in extract_wikilinks(&article.content) {
            let key = super::slug::canonical(&target);
            if key.is_empty() || existing.contains(&key) {
                continue;
            }
            demand
                .entry(key.clone())
                .or_default()
                .insert(article.rel_path().to_string());
            if target != key {
                spellings.entry(key).or_default().insert(target);
            }
        }
    }

    let mut wanted: Vec<WantedArticle> = demand
        .into_iter()
        .map(|(slug, referrers)| WantedArticle {
            variants: spellings
                .get(&slug)
                .map(|v| v.iter().cloned().collect())
                .unwrap_or_default(),
            slug,
            referrers: referrers.into_iter().collect(),
        })
        .collect();

    // Most demand first; slug breaks ties so repeated runs agree.
    wanted.sort_by(|a, b| {
        b.referrers
            .len()
            .cmp(&a.referrers.len())
            .then_with(|| a.slug.cmp(&b.slug))
    });
    wanted
}

/// The link graph: maps article slug → set of linked slugs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LinkGraph {
    /// Forward links: article → articles it links to
    pub forward: HashMap<String, Vec<String>>,
    /// Backlinks: article → articles that link to it
    pub backlinks: HashMap<String, Vec<String>>,
}

/// Message for a link graph that exists but cannot be parsed.
///
/// A missing graph is legitimately empty — no `index` has run yet. A corrupt
/// one is not, and treating the two alike reports a confident zero derived from
/// an unparseable file.
pub fn corrupt_graph_note(error: &io::Error) -> String {
    format!("meta/link-graph.json could not be read ({error}); run `sentinel index` to rebuild it")
}

impl LinkGraph {
    pub fn load() -> io::Result<Self> {
        let path = paths::link_graph_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(&path)?;
        serde_json::from_str(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn save(&self) -> io::Result<()> {
        let path = paths::link_graph_path();
        let data = serde_json::to_string_pretty(self)?;
        fs::write(&path, data)
    }

    /// Rebuild the graph from a set of (slug, linked_slugs) pairs.
    pub fn rebuild(&mut self, articles: Vec<(String, Vec<String>)>) {
        self.forward.clear();
        self.backlinks.clear();

        for (slug, links) in articles {
            self.forward.insert(slug.clone(), links.clone());
            for link in links {
                self.backlinks.entry(link).or_default().push(slug.clone());
            }
        }
    }

    /// Find articles with no incoming backlinks.
    pub fn orphans(&self, all_slugs: &HashSet<String>) -> Vec<String> {
        all_slugs
            .iter()
            .filter(|slug| {
                self.backlinks
                    .get(slug.as_str())
                    .is_none_or(|v| v.is_empty())
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_wikilinks() {
        let content = "See [[stoicism]] and [[marcus-aurelius|Marcus Aurelius]] for details.";
        let links = extract_wikilinks(content);
        assert_eq!(links, vec!["stoicism", "marcus-aurelius"]);
    }

    #[test]
    fn test_no_wikilinks() {
        let links = extract_wikilinks("No links here.");
        assert!(links.is_empty());
    }

    fn loaded(rel_path: &str, body: &str) -> crate::core::wiki::LoadedArticle {
        use crate::core::frontmatter::{Frontmatter, WikiArticle};
        crate::core::wiki::LoadedArticle {
            article: WikiArticle {
                frontmatter: Frontmatter::default(),
                rel_path: rel_path.to_string(),
                frontmatter_error: None,
            },
            path: std::path::PathBuf::from(rel_path),
            content: body.to_string(),
        }
    }

    #[test]
    fn wanted_ranks_by_how_many_articles_ask_for_it() {
        let articles = vec![
            loaded("wiki/a.md", "See [[virtue]] and [[ataraxia]]."),
            loaded("wiki/b.md", "See [[virtue]]."),
            loaded("wiki/c.md", "See [[virtue]]."),
        ];
        let wanted = wanted(&articles);
        assert_eq!(wanted[0].slug, "virtue");
        assert_eq!(wanted[0].referrers.len(), 3);
        assert_eq!(wanted[1].slug, "ataraxia");
    }

    #[test]
    fn wanted_excludes_links_that_resolve() {
        let articles = vec![
            loaded("wiki/stoicism.md", "root"),
            loaded("wiki/a.md", "See [[stoicism]] and [[virtue]]."),
        ];
        let wanted = wanted(&articles);
        assert_eq!(wanted.len(), 1);
        assert_eq!(wanted[0].slug, "virtue");
    }

    #[test]
    fn one_article_linking_twice_counts_once() {
        let articles = vec![loaded("wiki/a.md", "[[virtue]] and again [[virtue]].")];
        let wanted = wanted(&articles);
        assert_eq!(
            wanted[0].referrers.len(),
            1,
            "demand is distinct articles, not raw link count"
        );
    }

    #[test]
    fn ties_break_alphabetically_so_output_is_stable() {
        let articles = vec![loaded("wiki/a.md", "[[zeta]] [[alpha]]")];
        let found = wanted(&articles);
        let slugs: Vec<&str> = found.iter().map(|w| w.slug.as_str()).collect();
        assert_eq!(slugs, ["alpha", "zeta"]);
    }
}
