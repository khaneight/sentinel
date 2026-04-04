use std::path::{Path, PathBuf};

/// The root of the archive knowledge base.
pub const ARCHIVE_ROOT: &str = "/home/khaneight/Documents/archive";

pub fn archive_root() -> PathBuf {
    PathBuf::from(ARCHIVE_ROOT)
}

pub fn raw_dir() -> PathBuf {
    archive_root().join("raw")
}

pub fn wiki_dir() -> PathBuf {
    archive_root().join("wiki")
}

pub fn index_dir() -> PathBuf {
    archive_root().join("index")
}

pub fn meta_dir() -> PathBuf {
    archive_root().join("meta")
}

pub fn templates_dir() -> PathBuf {
    archive_root().join("templates")
}

pub fn manifest_path() -> PathBuf {
    meta_dir().join("manifest.json")
}

pub fn link_graph_path() -> PathBuf {
    meta_dir().join("link-graph.json")
}

pub fn log_path() -> PathBuf {
    meta_dir().join("log.md")
}

/// Default domains that get created on init.
pub const DEFAULT_DOMAINS: &[&str] = &["philosophy", "coding", "research"];

/// Given a domain, return the raw subdirectory.
pub fn raw_domain_dir(domain: &str) -> PathBuf {
    raw_dir().join(domain)
}

/// Given a domain, return the wiki subdirectory.
pub fn wiki_domain_dir(domain: &str) -> PathBuf {
    wiki_dir().join(domain)
}

/// Convert a filename to kebab-case slug.
pub fn slugify(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);

    stem.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
