use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::manifest::Manifest;
use super::wiki::LoadedArticle;

/// Which raw documents have been compiled into which wiki articles.
///
/// This is *derived*, not recorded. A wiki article declares what it was built
/// from in its `sources:` frontmatter; inverting that across the whole wiki
/// gives the raw → wiki mapping. Deriving it means the answer cannot go stale
/// and cannot disagree with the files on disk, which is what happened when the
/// manifest was expected to carry the mapping and nothing ever wrote it.
#[derive(Debug, Clone, Default)]
pub struct Compilation {
    /// Manifest raw_path → wiki article paths compiled from it, sorted.
    by_raw: BTreeMap<String, Vec<String>>,
    /// `(wiki article, source as written)` for citations matching no raw document.
    pub unresolved: Vec<Unresolved>,
}

impl Compilation {
    /// Invert every article's `sources:` list against the manifest.
    pub fn derive(articles: &[LoadedArticle], manifest: &Manifest) -> Self {
        let index = SourceIndex::new(manifest);
        let mut by_raw: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut unresolved = Vec::new();

        for article in articles {
            for source in &article.article.frontmatter.sources {
                match index.resolve(source) {
                    Some(raw_path) => {
                        by_raw
                            .entry(raw_path)
                            .or_default()
                            .insert(article.rel_path().to_string());
                    }
                    None => {
                        unresolved.push(Unresolved {
                            article: article.rel_path().to_string(),
                            source: source.clone(),
                            suggestion: index.suggest(source),
                        });
                    }
                }
            }
        }

        Self {
            by_raw: by_raw
                .into_iter()
                .map(|(k, v)| (k, v.into_iter().collect()))
                .collect(),
            unresolved,
        }
    }

    /// Wiki articles compiled from `raw_path`.
    pub fn articles_for(&self, raw_path: &str) -> &[String] {
        self.by_raw.get(raw_path).map_or(&[], Vec::as_slice)
    }

    /// Raw documents in the manifest that no wiki article cites as a source.
    pub fn uncompiled<'m>(
        &self,
        manifest: &'m Manifest,
    ) -> Vec<&'m super::manifest::ManifestEntry> {
        let mut entries: Vec<_> = manifest
            .entries
            .values()
            .filter(|e| self.articles_for(&e.raw_path).is_empty())
            .collect();
        entries.sort_by(|a, b| a.raw_path.cmp(&b.raw_path));
        entries
    }

    /// Copy the derived mapping into the manifest's `wiki_articles` fields.
    ///
    /// The manifest is a published artifact — Obsidian plugins and scripts read
    /// it — so the mapping is written down as well as derived. Nothing reads it
    /// back to make decisions; it is a projection, not a second source of truth.
    pub fn apply_to(&self, manifest: &mut Manifest) -> usize {
        let mut changed = 0;
        for entry in manifest.entries.values_mut() {
            let articles = self.articles_for(&entry.raw_path).to_vec();
            if entry.wiki_articles != articles {
                entry.wiki_articles = articles;
                changed += 1;
            }
        }
        changed
    }
}

/// Resolves a `sources:` citation to a manifest key.
/// A `sources:` entry that matched no raw document, and the nearest thing to it.
#[derive(Debug, Clone)]
pub struct Unresolved {
    pub article: String,
    pub source: String,
    /// The registered path this most likely meant, when there is an obvious one.
    pub suggestion: Option<String>,
}

/// Levenshtein distance, for suggesting what a mistyped citation meant.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

pub struct SourceIndex<'m> {
    by_rel_path: BTreeSet<&'m str>,
    /// Basename → manifest keys. Only unambiguous basenames are usable.
    by_basename: HashMap<&'m str, Vec<&'m str>>,
}

impl<'m> SourceIndex<'m> {
    pub fn new(manifest: &'m Manifest) -> Self {
        let mut by_rel_path = BTreeSet::new();
        let mut by_basename: HashMap<&str, Vec<&str>> = HashMap::new();

        for key in manifest.entries.keys() {
            by_rel_path.insert(key.as_str());
            if let Some(base) = basename(key) {
                by_basename.entry(base).or_default().push(key.as_str());
            }
        }

        Self {
            by_rel_path,
            by_basename,
        }
    }

    /// Match a citation to a raw document.
    ///
    /// Citations are hand-written by an agent into YAML, so they arrive in
    /// several shapes: `raw/philosophy/x.md`, `./raw/philosophy/x.md`,
    /// `/raw/philosophy/x.md`, `[[x]]`. Exact paths are matched first; a bare
    /// filename is accepted only when exactly one raw document has that name,
    /// because guessing between two is worse than reporting the ambiguity.
    /// The registered path a failed citation most likely meant.
    ///
    /// Resolution stays strict — matching `Seneca.txt` to `seneca.txt`
    /// automatically would invent an identity rule the filesystem does not
    /// share, and on a case-sensitive volume both can exist. But a citation
    /// that resolves to nothing is a dead end, and the manifest is right there.
    ///
    /// Case first, since that is the near-miss a person actually makes, then a
    /// small edit distance for typos. Nothing further: a "did you mean" that is
    /// usually wrong costs more than none at all.
    pub fn suggest(&self, source: &str) -> Option<String> {
        let cleaned = normalize(source);
        let wanted = basename(&cleaned)?.to_lowercase();

        let mut best: Option<(usize, &str)> = None;
        for (base, paths) in &self.by_basename {
            let [only] = paths.as_slice() else { continue };
            let candidate = base.to_lowercase();
            // Compare against the stem too: `Seneca` for `seneca.txt` is a
            // citation written from memory, not a typo, and it is the most
            // common way to get this wrong.
            let stem = candidate
                .rsplit_once('.')
                .map_or(candidate.as_str(), |(s, _)| s);
            let distance = if candidate == wanted || stem == wanted {
                0
            } else {
                edit_distance(&candidate, &wanted).min(edit_distance(stem, &wanted))
            };
            // Two edits on a short filename is most of it; scale with length.
            let ceiling = (wanted.len() / 4).clamp(1, 3);
            if distance <= ceiling && best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, only));
            }
        }
        best.map(|(_, path)| path.to_string())
    }

    pub fn resolve(&self, source: &str) -> Option<String> {
        let cleaned = normalize(source);
        if cleaned.is_empty() {
            return None;
        }

        if self.by_rel_path.contains(cleaned.as_str()) {
            return Some(cleaned);
        }

        // `philosophy/x.md` written without the `raw/` prefix.
        let prefixed = format!("raw/{cleaned}");
        if self.by_rel_path.contains(prefixed.as_str()) {
            return Some(prefixed);
        }

        let base = basename(&cleaned)?;
        match self.by_basename.get(base).map(Vec::as_slice) {
            Some([only]) => Some((*only).to_string()),
            _ => None,
        }
    }
}

/// Strip the decorations a citation may arrive with.
fn normalize(source: &str) -> String {
    let mut s = source.trim();
    // A source written as a wikilink: `[[raw/philosophy/x.md]]` or `[[x|Title]]`.
    if let Some(inner) = s.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
        s = inner.split('|').next().unwrap_or(inner).trim();
    }
    s.trim_start_matches("./")
        .trim_start_matches('/')
        .replace('\\', "/")
}

fn basename(path: &str) -> Option<&str> {
    path.rsplit('/').next().filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::frontmatter::{Frontmatter, WikiArticle};
    use crate::core::manifest::ManifestEntry;
    use std::path::PathBuf;

    fn manifest(raw_paths: &[&str]) -> Manifest {
        let mut m = Manifest::default();
        for raw in raw_paths {
            m.upsert(ManifestEntry {
                raw_path: (*raw).to_string(),
                title: "T".into(),
                domain: "philosophy".into(),
                origin: "authored".into(),
                ingested_at: "2026-01-01 00:00:00".into(),
                wiki_articles: vec![],
                source_type: "document".into(),
                content_hash: None,
                publish: false,
            });
        }
        m
    }

    fn article(rel_path: &str, sources: &[&str]) -> LoadedArticle {
        LoadedArticle {
            article: WikiArticle {
                frontmatter: Frontmatter {
                    sources: sources.iter().map(|s| (*s).to_string()).collect(),
                    ..Default::default()
                },
                rel_path: rel_path.to_string(),
                frontmatter_error: None,
            },
            path: PathBuf::from(rel_path),
            content: String::new(),
        }
    }

    #[test]
    fn an_exact_path_resolves() {
        let m = manifest(&["raw/philosophy/meditations.md"]);
        let c = Compilation::derive(
            &[article(
                "wiki/philosophy/stoicism.md",
                &["raw/philosophy/meditations.md"],
            )],
            &m,
        );
        assert_eq!(
            c.articles_for("raw/philosophy/meditations.md"),
            ["wiki/philosophy/stoicism.md"]
        );
        assert!(c.unresolved.is_empty());
    }

    #[test]
    fn one_raw_doc_can_feed_several_articles() {
        let m = manifest(&["raw/philosophy/meditations.md"]);
        let c = Compilation::derive(
            &[
                article("wiki/philosophy/b.md", &["raw/philosophy/meditations.md"]),
                article("wiki/philosophy/a.md", &["raw/philosophy/meditations.md"]),
            ],
            &m,
        );
        assert_eq!(
            c.articles_for("raw/philosophy/meditations.md"),
            ["wiki/philosophy/a.md", "wiki/philosophy/b.md"],
            "results must be sorted so generated output is diff-friendly"
        );
    }

    #[test]
    fn citation_spellings_are_tolerated() {
        let m = manifest(&["raw/philosophy/meditations.md"]);
        for spelling in [
            "raw/philosophy/meditations.md",
            "./raw/philosophy/meditations.md",
            "/raw/philosophy/meditations.md",
            "philosophy/meditations.md",
            "  raw/philosophy/meditations.md  ",
            "[[raw/philosophy/meditations.md]]",
            "meditations.md",
        ] {
            let c = Compilation::derive(&[article("wiki/a.md", &[spelling])], &m);
            assert!(
                !c.articles_for("raw/philosophy/meditations.md").is_empty(),
                "failed to resolve {spelling:?}"
            );
        }
    }

    #[test]
    fn an_ambiguous_basename_is_reported_rather_than_guessed() {
        let m = manifest(&["raw/philosophy/notes.md", "raw/coding/notes.md"]);
        let c = Compilation::derive(&[article("wiki/a.md", &["notes.md"])], &m);

        assert!(c.articles_for("raw/philosophy/notes.md").is_empty());
        assert!(c.articles_for("raw/coding/notes.md").is_empty());
        let seen: Vec<(String, String)> = c
            .unresolved
            .iter()
            .map(|u| (u.article.clone(), u.source.clone()))
            .collect();
        assert_eq!(seen, [("wiki/a.md".to_string(), "notes.md".to_string())]);
    }

    #[test]
    fn a_citation_with_no_raw_document_is_unresolved() {
        let m = manifest(&["raw/philosophy/meditations.md"]);
        let c = Compilation::derive(&[article("wiki/a.md", &["raw/philosophy/gone.md"])], &m);
        assert_eq!(c.unresolved.len(), 1);
    }

    #[test]
    fn uncompiled_lists_raw_docs_nothing_cites() {
        let m = manifest(&["raw/a.md", "raw/b.md"]);
        let c = Compilation::derive(&[article("wiki/x.md", &["raw/a.md"])], &m);

        let uncompiled: Vec<&str> = c
            .uncompiled(&m)
            .iter()
            .map(|e| e.raw_path.as_str())
            .collect();
        assert_eq!(uncompiled, ["raw/b.md"]);
    }

    #[test]
    fn applying_the_mapping_reports_only_real_changes() {
        let mut m = manifest(&["raw/a.md"]);
        let c = Compilation::derive(&[article("wiki/x.md", &["raw/a.md"])], &m);

        assert_eq!(c.apply_to(&mut m), 1);
        assert_eq!(m.entries["raw/a.md"].wiki_articles, ["wiki/x.md"]);
        assert_eq!(c.apply_to(&mut m), 0, "a second pass must be a no-op");
    }

    #[test]
    fn applying_clears_a_mapping_whose_article_stopped_citing_it() {
        let mut m = manifest(&["raw/a.md"]);
        m.entries.get_mut("raw/a.md").unwrap().wiki_articles = vec!["wiki/stale.md".into()];

        let c = Compilation::derive(&[], &m);
        assert_eq!(c.apply_to(&mut m), 1);
        assert!(m.entries["raw/a.md"].wiki_articles.is_empty());
    }
}
