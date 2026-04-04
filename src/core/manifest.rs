use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;

use super::paths;

/// A single entry in the manifest tracking a raw document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Relative path from archive root to the raw file (e.g. "raw/philosophy/meditations.md")
    pub raw_path: String,
    /// Display title
    pub title: String,
    /// Domain (e.g. "philosophy", "coding")
    pub domain: String,
    /// Provenance: "authored", "researched", or "hybrid"
    pub origin: String,
    /// When the document was ingested
    pub ingested_at: String,
    /// Relative paths to wiki articles compiled from this raw doc (empty if uncompiled)
    pub wiki_articles: Vec<String>,
    /// Optional source type: "document", "codebase", "url"
    #[serde(default = "default_source_type")]
    pub source_type: String,
}

fn default_source_type() -> String {
    "document".to_string()
}

/// The full manifest: a map from raw_path to ManifestEntry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub entries: HashMap<String, ManifestEntry>,
}

impl Manifest {
    /// Load the manifest from disk, or return an empty one if it doesn't exist.
    pub fn load() -> io::Result<Self> {
        let path = paths::manifest_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(&path)?;
        serde_json::from_str(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Save the manifest to disk.
    pub fn save(&self) -> io::Result<()> {
        let path = paths::manifest_path();
        let data = serde_json::to_string_pretty(self)?;
        fs::write(&path, data)
    }

    /// Add or update an entry.
    pub fn upsert(&mut self, entry: ManifestEntry) {
        self.entries.insert(entry.raw_path.clone(), entry);
    }

    /// Get entries that have no wiki articles mapped.
    pub fn uncompiled(&self) -> Vec<&ManifestEntry> {
        self.entries
            .values()
            .filter(|e| e.wiki_articles.is_empty())
            .collect()
    }

    /// Total count of entries.
    pub fn count(&self) -> usize {
        self.entries.len()
    }
}
