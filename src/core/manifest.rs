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
    /// Wiki articles compiled from this raw doc, empty if uncompiled.
    ///
    /// A *projection*, not a source of truth. It is recomputed by
    /// `sentinel index` from the `sources:` frontmatter of every wiki article
    /// and published here for external readers. Nothing in sentinel reads it
    /// back to make a decision — use `core::compilation::Compilation`, which
    /// derives the mapping live and therefore cannot go stale.
    pub wiki_articles: Vec<String>,
    /// Optional source type: "document", "codebase", "url"
    #[serde(default = "default_source_type")]
    pub source_type: String,
    /// Hash of the file's bytes, used to recognise a document that has been
    /// renamed or moved by hand.
    ///
    /// Absent on entries written before this field existed; `sync` backfills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

fn default_source_type() -> String {
    "document".to_string()
}

/// Hash a raw document's bytes.
///
/// Not cryptographic, and it does not need to be: it exists only to recognise
/// the same content under a new name. A collision would carry metadata between
/// two byte-identical files, which is the same outcome either way.
pub fn content_hash(bytes: &[u8]) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Hash a file on disk, or `None` if it cannot be read.
pub fn hash_file(path: &std::path::Path) -> Option<String> {
    std::fs::read(path).ok().map(|bytes| content_hash(&bytes))
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

    /// Total count of entries.
    pub fn count(&self) -> usize {
        self.entries.len()
    }
}
