use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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

/// The link graph: maps article slug → set of linked slugs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LinkGraph {
    /// Forward links: article → articles it links to
    pub forward: HashMap<String, Vec<String>>,
    /// Backlinks: article → articles that link to it
    pub backlinks: HashMap<String, Vec<String>>,
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
                    .map_or(true, |v| v.is_empty())
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
}
