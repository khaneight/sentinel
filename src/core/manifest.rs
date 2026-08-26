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
    /// Whether this document may be published alongside the wiki.
    ///
    /// **Default false, and deliberately per-document.** `raw/` holds whatever
    /// its owner put there: material under someone else's copyright, private
    /// notes, correspondence, drafts they never meant anyone to see. Nothing
    /// about a file tells the tool which of those it is, so there is no flag
    /// that could safely publish the directory. Opting in is a decision made
    /// once per document, by hand, and recorded here.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub publish: bool,
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
    /// Hash of the file this was loaded from, or `None` if there was no file.
    ///
    /// Every command does load → modify → save, and nothing coordinates them.
    /// Two running at once both read the same state and the second save
    /// silently discards the first's work: ten concurrent `ingest` calls, all
    /// reporting success, left nine documents on disk with no manifest entry —
    /// and re-registering those through `sync` resets `origin`, which #16
    /// established is unrecoverable.
    ///
    /// Saving verifies the file still holds what was read, so a lost update
    /// becomes a reported conflict instead.
    #[serde(skip)]
    loaded_from: Option<String>,
}

impl Manifest {
    /// Load the manifest from disk, or return an empty one if it doesn't exist.
    pub fn load() -> io::Result<Self> {
        Self::load_from(&paths::manifest_path())
    }

    /// Save the manifest to disk, refusing if it changed since it was loaded.
    ///
    /// The check narrows the race from the whole command to the moment between
    /// verifying and renaming. It is not mutual exclusion — see the note on
    /// `loaded_from` — but it turns the realistic failure from silent data loss
    /// into an error the caller can act on.
    pub fn save(&self) -> io::Result<()> {
        self.save_to(&paths::manifest_path())
    }

    /// As `load`, against an explicit path. Keeps the conflict logic testable
    /// without the process-wide archive root.
    pub fn load_from(path: &std::path::Path) -> io::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(path)?;
        let mut manifest: Self = serde_json::from_str(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        manifest.loaded_from = Some(content_hash(data.as_bytes()));
        Ok(manifest)
    }

    /// As `save`, against an explicit path.
    pub fn save_to(&self, path: &std::path::Path) -> io::Result<()> {
        let current = fs::read(path).ok().map(|bytes| content_hash(&bytes));
        if current != self.loaded_from {
            return Err(io::Error::new(
                io::ErrorKind::ResourceBusy,
                "meta/manifest.json changed while this command was running, so \
                 saving would discard that change.\n\
                 Another sentinel process is most likely running. Nothing was \
                 written; re-run this command once it has finished.",
            ));
        }
        let data = serde_json::to_string_pretty(self)?;
        super::atomic::write_if_changed(path, data).map(|_| ())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(raw: &str) -> ManifestEntry {
        ManifestEntry {
            raw_path: raw.to_string(),
            title: "T".into(),
            domain: "philosophy".into(),
            origin: "researched".into(),
            ingested_at: "2026-01-01 00:00:00".into(),
            wiki_articles: vec![],
            source_type: "document".into(),
            content_hash: None,
            publish: false,
        }
    }

    /// A scratch manifest path. Explicit, so these tests do not depend on the
    /// process-wide archive root, which can only be set once.
    fn scratch() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        (dir, path)
    }

    #[test]
    fn a_save_over_a_changed_manifest_is_refused() {
        // The lost-update race: two commands load the same state, both modify
        // it, and the second save discards the first's work. Ten concurrent
        // `ingest` calls lost nine entries this way, every one reporting
        // success.
        let (_dir, path) = scratch();

        let mut first = Manifest::default();
        first.upsert(entry("raw/a.md"));
        first.save_to(&path).unwrap();

        // Two commands now load the same state.
        let mut mine = Manifest::load_from(&path).unwrap();
        let mut theirs = Manifest::load_from(&path).unwrap();

        theirs.upsert(entry("raw/theirs.md"));
        theirs.save_to(&path).unwrap();

        mine.upsert(entry("raw/mine.md"));
        let err = mine.save_to(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::ResourceBusy);
        assert!(err.to_string().contains("re-run"), "{err}");

        // And the other command's work survives.
        let on_disk = Manifest::load_from(&path).unwrap();
        assert!(on_disk.entries.contains_key("raw/theirs.md"));
        assert!(!on_disk.entries.contains_key("raw/mine.md"));
    }

    #[test]
    fn an_uncontended_save_succeeds() {
        let (_dir, path) = scratch();
        let mut m = Manifest::load_from(&path).unwrap();
        m.upsert(entry("raw/a.md"));
        m.save_to(&path).unwrap();

        let mut again = Manifest::load_from(&path).unwrap();
        again.upsert(entry("raw/b.md"));
        again.save_to(&path).unwrap();

        assert_eq!(Manifest::load_from(&path).unwrap().entries.len(), 2);
    }

    #[test]
    fn creating_the_first_manifest_is_refused_if_another_got_there_first() {
        let (_dir, path) = scratch();
        let mut mine = Manifest::load_from(&path).unwrap(); // no file yet

        let mut theirs = Manifest::default();
        theirs.upsert(entry("raw/theirs.md"));
        theirs.save_to(&path).unwrap();

        mine.upsert(entry("raw/mine.md"));
        assert!(
            mine.save_to(&path).is_err(),
            "a manifest appeared underneath us"
        );
    }
}
