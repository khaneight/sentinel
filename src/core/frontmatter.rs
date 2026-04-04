use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

/// Parsed YAML frontmatter from a wiki article.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub domain: Option<String>,
    pub origin: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub related: Vec<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub status: Option<String>,
}

/// A wiki article with parsed frontmatter and body content.
#[derive(Debug, Clone)]
pub struct WikiArticle {
    pub frontmatter: Frontmatter,
    pub body: String,
    /// Relative path from archive root
    pub rel_path: String,
}

/// Parse a markdown file's YAML frontmatter and body.
pub fn parse_file(path: &Path, rel_path: &str) -> io::Result<WikiArticle> {
    let content = fs::read_to_string(path)?;
    let (frontmatter, body) = parse_content(&content);
    Ok(WikiArticle {
        frontmatter,
        body,
        rel_path: rel_path.to_string(),
    })
}

/// Parse frontmatter from markdown content.
pub fn parse_content(content: &str) -> (Frontmatter, String) {
    if !content.starts_with("---") {
        return (Frontmatter::default(), content.to_string());
    }

    // Find the closing ---
    if let Some(end) = content[3..].find("\n---") {
        let yaml_str = &content[3..end + 3].trim();
        let body = &content[end + 3 + 4..]; // skip past the closing ---\n
        let frontmatter: Frontmatter = serde_yaml::from_str(yaml_str).unwrap_or_default();
        (frontmatter, body.trim().to_string())
    } else {
        (Frontmatter::default(), content.to_string())
    }
}

/// Generate frontmatter YAML string.
pub fn render_frontmatter(fm: &Frontmatter) -> String {
    let yaml = serde_yaml::to_string(fm).unwrap_or_default();
    format!("---\n{}---\n", yaml)
}
