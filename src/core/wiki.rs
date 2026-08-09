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

/// Load every markdown article under `wiki/`.
///
/// Four commands walked `wiki/` with their own copy of this filter and their
/// own idea of what counted as an article. Sharing one loader keeps `index`,
/// `lint`, `status`, and `uncompiled` reasoning about the same set of files.
pub fn load_all() -> io::Result<Vec<LoadedArticle>> {
    let wiki_dir = paths::wiki_dir();
    if !wiki_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Wiki directory not found. Run `sentinel init` first.",
        ));
    }

    let mut loaded = Vec::new();
    for path in markdown_files(&wiki_dir) {
        let rel_path = paths::rel(&path);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
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
    Ok(loaded)
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
