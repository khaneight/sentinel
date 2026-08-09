use std::io;
use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

use super::frontmatter::{self, WikiArticle};
use super::paths;

/// A wiki article as loaded from disk, with everything a command needs about it.
#[derive(Debug, Clone)]
pub struct LoadedArticle {
    pub article: WikiArticle,
    pub path: PathBuf,
    /// The complete file, frontmatter included. Wikilinks can appear in
    /// `related:` as well as in the body, so link extraction reads all of it.
    pub content: String,
}

impl LoadedArticle {
    /// Path relative to the archive root, e.g. `wiki/philosophy/stoicism.md`.
    pub fn rel_path(&self) -> &str {
        &self.article.rel_path
    }

    /// The canonical identity used to match wikilinks against this article.
    ///
    /// Link resolution goes through here, never through `slug()`, so a target
    /// spelled `[[Compile Loop]]` finds `compile-loop.md`.
    pub fn canonical_slug(&self) -> String {
        super::slug::canonical(&self.slug())
    }

    /// The wikilink target that names this article: the filename stem.
    pub fn slug(&self) -> String {
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string()
    }

    /// Title from frontmatter, falling back to the path so output is never blank.
    pub fn title(&self) -> &str {
        self.article
            .frontmatter
            .title
            .as_deref()
            .unwrap_or(&self.article.rel_path)
    }
}

/// A file under `wiki/` that could not be read.
///
/// Reported rather than skipped. A command that rewrites derived state must
/// refuse to run on a partial view; a command that only reads may continue, but
/// has to say the view was incomplete — "0 results" from an unreadable file is
/// as misleading as a wrong answer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Unreadable {
    pub path: String,
    pub error: String,
}

/// The outcome of scanning `wiki/`.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    pub articles: Vec<LoadedArticle>,
    pub unreadable: Vec<Unreadable>,
}

impl Loaded {
    /// The articles, refusing if any file could not be read.
    ///
    /// For callers that overwrite durable state from what they load. Rebuilding
    /// an index from a partial view silently deletes whatever the missing files
    /// accounted for.
    pub fn require_complete(self) -> io::Result<Vec<LoadedArticle>> {
        if self.unreadable.is_empty() {
            return Ok(self.articles);
        }
        let detail = self
            .unreadable
            .iter()
            .map(|u| format!("  {} — {}", u.path, u.error))
            .collect::<Vec<_>>()
            .join("\n");
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} wiki file(s) could not be read, so the archive cannot be \
                 rebuilt from a complete view:\n{detail}\n\n\
                 Rebuilding now would drop everything those files account for. \
                 Fix the reads and run again.",
                self.unreadable.len()
            ),
        ))
    }
}

/// Load every markdown article under `wiki/`.
///
/// Four commands walked `wiki/` with their own copy of this filter and their
/// own idea of what counted as an article. Sharing one loader keeps `index`,
/// `lint`, `status`, and `uncompiled` reasoning about the same set of files.
pub fn load_all() -> io::Result<Loaded> {
    let wiki_dir = paths::wiki_dir();
    if !wiki_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Wiki directory not found. Run `sentinel init` first.",
        ));
    }

    let mut loaded = Vec::new();
    let mut unreadable = Vec::new();
    for path in markdown_files(&wiki_dir) {
        let rel_path = paths::rel(&path);
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => {
                unreadable.push(Unreadable {
                    path: rel_path,
                    error: e.to_string(),
                });
                continue;
            }
        };
        let parsed = frontmatter::parse_content(&content);
        loaded.push(LoadedArticle {
            article: WikiArticle {
                frontmatter: parsed.frontmatter,
                rel_path,
                frontmatter_error: parsed.error,
            },
            path,
            content,
        });
    }

    // Stable ordering keeps generated indexes and JSON output diff-friendly.
    loaded.sort_by(|a, b| a.article.rel_path.cmp(&b.article.rel_path));
    unreadable.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Loaded {
        articles: loaded,
        unreadable,
    })
}

/// Every `.md` file under `dir`, skipping hidden files and directories.
pub fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "md"))
        .map(|e| e.into_path())
        .collect()
}

fn is_hidden(entry: &DirEntry) -> bool {
    entry.depth() > 0
        && entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'))
}
