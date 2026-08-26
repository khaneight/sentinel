//! `persona/` — a cited model of the archive's author.
//!
//! The wiki records what the author knows. This records *who they are*: how
//! they argue, what they hold, the moves they make. It is what lets the
//! archive produce new work in their voice rather than a neutral encyclopedia
//! entry.
//!
//! One file per trait, because a trait is a claim about a person and a claim
//! about a person needs a citation, a confidence, and somewhere to record that
//! they disagreed with it. A single profile document has nowhere to put any of
//! that.
//!
//! Two rules here are load-bearing, and both are enforced as lint *errors*
//! rather than left to the agent's discretion — see [`docs/clone.md`]:
//!
//! 1. **Every trait cites evidence.** A claim about someone with nothing behind
//!    it is the tool inventing them, and a profile that cannot be audited
//!    cannot be corrected.
//! 2. **Evidence is the author's own writing.** Only `authored` and `hybrid`
//!    raw documents count. Deriving someone's principles from material an agent
//!    researched *for* them builds a person out of their reading list.

use std::collections::BTreeSet;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::compilation::SourceIndex;
use super::frontmatter;
use super::manifest::Manifest;
use super::paths;
use super::wiki::{self, Unreadable};

/// What a trait is about.
///
/// `style` is how the prose reads; `principle` is a rule the author applies;
/// `belief` is a position they hold; `pattern` is a recurring move in how they
/// think — reaching for a historical parallel, testing an idea against a
/// concrete case. Separated because generation uses them differently: style
/// shapes every sentence, a belief may not be relevant at all.
pub const KINDS: &[&str] = &["style", "principle", "belief", "pattern"];

/// How well the evidence supports the claim.
pub const CONFIDENCES: &[&str] = &["high", "medium", "low"];

/// Where the trait stands with the author.
///
/// `proposed` is the agent's reading of the corpus and nothing more. `affirmed`
/// means the author confirmed it. `rejected` means they did not, and the file
/// stays on disk carrying that — a rejection the tool forgets is a rejection
/// the next iteration overrules.
pub const STATUSES: &[&str] = &["proposed", "affirmed", "rejected"];

/// Raw-document origins that can serve as evidence for a trait.
///
/// A subset of `frontmatter::INGESTABLE_ORIGINS`, asserted as one below.
/// Material an
/// agent researched says what the author *read*, not what they think, and a
/// profile built from a reading list is a profile of somebody else.
pub const EVIDENCE_ORIGINS: &[&str] = &["authored", "hybrid"];

/// Fields required on every trait.
///
/// Derived from here by both the lint rule and `sentinel schema`, so the
/// checker and the published contract cannot disagree about what is mandatory.
pub const REQUIRED: &[&str] = &["id", "kind", "claim"];

/// Frontmatter of a `persona/` trait.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraitFrontmatter {
    /// Stable identifier, matching the filename stem. What an extrapolated
    /// article cites when it says which traits it wrote from.
    pub id: Option<String>,
    pub kind: Option<String>,
    /// The claim itself, in one sentence.
    pub claim: Option<String>,
    /// Archive-relative paths to the raw documents this was read out of.
    #[serde(default)]
    pub evidence: Vec<String>,
    pub confidence: Option<String>,
    pub status: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    /// Verdicts the author recorded about this claim, oldest first. `status:`
    /// above is what a reader sees; this is the history behind it, including
    /// the note explaining a rejection.
    #[serde(default)]
    pub review: Vec<super::review::Entry>,
}

impl TraitFrontmatter {
    /// Required fields that are absent or blank, by name.
    ///
    /// Iterates `REQUIRED` rather than checking three fields inline, so a field
    /// added to the contract cannot be added without a check.
    pub fn missing(&self) -> Vec<&'static str> {
        REQUIRED
            .iter()
            .copied()
            .filter(|name| self.field(name).is_none_or(str::is_empty))
            .collect()
    }

    /// A required field's value, by contract name.
    fn field(&self, name: &str) -> Option<&str> {
        let value = match name {
            "id" => self.id.as_deref(),
            "kind" => self.kind.as_deref(),
            "claim" => self.claim.as_deref(),
            // Unreachable while `REQUIRED` and this match agree; the test below
            // is what keeps them agreeing.
            _ => None,
        };
        value.map(str::trim)
    }

    /// Every date field, paired with its name — the same shape wiki articles
    /// expose, so `lint`'s date rule can walk both without knowing which it has.
    pub fn dates(&self) -> frontmatter::Dates<'_> {
        [
            ("created", self.created.as_deref()),
            ("updated", self.updated.as_deref()),
        ]
    }
}

/// A trait as loaded from disk.
#[derive(Debug, Clone)]
pub struct LoadedTrait {
    pub frontmatter: TraitFrontmatter,
    /// Path relative to the archive root, e.g. `persona/argues-from-cases.md`.
    pub rel_path: String,
    pub path: PathBuf,
    /// The prose behind the claim, quoting the evidence.
    pub body: String,
    /// Why the frontmatter block failed to parse, if it did. Carried for the
    /// same reason wiki articles carry it: a malformed block deserialises to
    /// defaults, which is indistinguishable from a file that has no block, and
    /// reporting three missing fields for one broken indent sends a reader to
    /// the wrong place.
    pub frontmatter_error: Option<String>,
}

impl LoadedTrait {
    /// The identifier other documents cite.
    ///
    /// `id:` when set, else the filename stem — so a trait with a missing `id`
    /// still appears in output under a usable name instead of a blank, while
    /// `lint` reports the omission.
    pub fn id(&self) -> String {
        match self.frontmatter.id.as_deref().map(str::trim) {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => self.stem(),
        }
    }

    pub fn stem(&self) -> String {
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string()
    }

    /// The identity every comparison against a trait goes through.
    ///
    /// The same canonicalisation wikilinks use. An article citing
    /// `persona: [Argues From Cases]` must find `argues-from-cases.md`, for the
    /// same reason `[[Compile Loop]]` finds `compile-loop.md`.
    pub fn canonical_id(&self) -> String {
        super::slug::canonical(&self.id())
    }
}

/// Accessors for what the clone does with a trait, rather than what `lint`
/// checks about it.
///
/// Nothing reads these yet: `sentinel persona` and the `learn` rung land in the
/// next two PRs. They are defined here because they carry the layer's
/// semantics — above all that an unset `status` means `proposed` — and those
/// belong beside the constants they interpret, with the tests that pin them.
#[allow(dead_code)]
impl LoadedTrait {
    pub fn kind(&self) -> &str {
        self.frontmatter.kind.as_deref().unwrap_or("")
    }

    /// The trait's standing, defaulting to `proposed`.
    ///
    /// An absent `status` is the agent having written a trait and not yet asked
    /// about it, which is exactly what `proposed` means. Defaulting to
    /// `affirmed` would let the clone write from an unreviewed reading of the
    /// corpus.
    pub fn status(&self) -> &str {
        match self.frontmatter.status.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => s,
            _ => "proposed",
        }
    }

    /// Whether the clone may write from this trait.
    pub fn is_affirmed(&self) -> bool {
        self.status() == "affirmed"
    }
}

/// The outcome of scanning `persona/`.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    pub traits: Vec<LoadedTrait>,
    pub unreadable: Vec<Unreadable>,
}

impl Loaded {
    /// The traits, refusing if any file could not be read.
    ///
    /// For callers that rewrite durable state. The stakes are higher here than
    /// for the wiki: a trait that could not be read looks exactly like a trait
    /// that was never written, so a rebuild on a partial view can conclude the
    /// author holds nothing.
    pub fn require_complete(self) -> io::Result<Vec<LoadedTrait>> {
        if self.unreadable.is_empty() {
            return Ok(self.traits);
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
                "{} persona trait(s) could not be read, so the author's profile \
                 cannot be read from a complete view:\n{detail}\n\n\
                 Acting on a partial profile means writing as somebody the \
                 archive has only half of. Fix the reads and run again.",
                self.unreadable.len()
            ),
        ))
    }
}

/// How much of the author's own writing the profile has actually been read out
/// of.
///
/// Derived live from the traits and the manifest, never recorded — the same
/// rule `Compilation` follows, and for the same reason: a stored copy is right
/// until somebody edits a file, and this one decides whether the clone has read
/// enough of a person to write as them.
#[derive(Debug, Clone, Default)]
pub struct Coverage {
    /// Raw documents that could serve as evidence, in manifest order.
    pub eligible: Vec<String>,
    /// Those some trait cites.
    pub mined: BTreeSet<String>,
}

impl Coverage {
    pub fn derive(traits: &[LoadedTrait], manifest: &Manifest) -> Self {
        let mut eligible: Vec<String> = manifest
            .entries
            .iter()
            .filter(|(_, e)| EVIDENCE_ORIGINS.contains(&e.origin.as_str()))
            .map(|(path, _)| path.clone())
            .collect();
        eligible.sort();

        // Through the same matcher `sources:` uses, so `mine.md` and
        // `raw/philosophy/mine.md` count the same document once. Counting
        // spellings instead of documents would report a corpus as mined
        // because one file was cited two ways.
        let index = SourceIndex::new(manifest);
        let eligible_set: BTreeSet<&str> = eligible.iter().map(String::as_str).collect();
        let mut mined = BTreeSet::new();
        for t in traits {
            // A rejected trait is one the author disagreed with. Whatever it
            // was read out of has not been read *correctly*, so it stays in the
            // queue rather than counting as covered.
            if t.status() == "rejected" {
                continue;
            }
            for cited in &t.frontmatter.evidence {
                if let Some(resolved) = index.resolve(cited)
                    && eligible_set.contains(resolved.as_str())
                {
                    mined.insert(resolved);
                }
            }
        }

        Self { eligible, mined }
    }

    /// Eligible documents no trait has been read out of — the `learn` queue.
    pub fn unmined(&self) -> Vec<&str> {
        self.eligible
            .iter()
            .map(String::as_str)
            .filter(|p| !self.mined.contains(*p))
            .collect()
    }
}

/// Load every trait under `persona/`.
///
/// A missing directory is an empty profile, not an error: archives created
/// before the persona layer existed have no `persona/`, and every read command
/// has to keep working on them. An unreadable one is a different thing
/// entirely and is reported through `unreadable`.
pub fn load_all() -> io::Result<Loaded> {
    let dir = paths::persona_dir();
    if !dir.exists() {
        return Ok(Loaded::default());
    }

    let (files, mut unreadable) = wiki::markdown_files(&dir);
    let mut traits = Vec::new();
    for path in files {
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
        let parsed = frontmatter::parse_as::<TraitFrontmatter>(&content);
        traits.push(LoadedTrait {
            frontmatter: parsed.frontmatter,
            rel_path,
            path,
            body: parsed.body,
            frontmatter_error: parsed.error,
        });
    }

    traits.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    unreadable.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Loaded { traits, unreadable })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> TraitFrontmatter {
        frontmatter::parse_as::<TraitFrontmatter>(&format!("---\n{yaml}\n---\n\nbody\n"))
            .frontmatter
    }

    #[test]
    fn every_required_field_is_reachable_by_name() {
        // `missing()` walks `REQUIRED` and looks each name up in `field()`. A
        // field added to one and not the other would silently never be
        // checked — the failure would be a rule that passes everything.
        let empty = TraitFrontmatter::default();
        let missing = empty.missing();
        assert_eq!(
            missing.len(),
            REQUIRED.len(),
            "a required field is not reachable through `field()`: {missing:?}"
        );
    }

    #[test]
    fn evidence_origins_are_a_subset_of_the_archives_origins() {
        // Spelled here rather than in `frontmatter`, so a renamed origin would
        // leave this list naming a value the manifest can never hold — and the
        // safeguard would pass every trait by accident.
        for origin in EVIDENCE_ORIGINS {
            assert!(
                frontmatter::INGESTABLE_ORIGINS.contains(origin),
                "'{origin}' is not an origin any raw document can have"
            );
        }
        assert!(
            EVIDENCE_ORIGINS.len() < frontmatter::INGESTABLE_ORIGINS.len(),
            "if every origin counts as evidence, the rule checks nothing"
        );
    }

    #[test]
    fn a_blank_required_field_counts_as_missing() {
        let fm = parse("id: \"  \"\nkind: style\nclaim: c");
        assert_eq!(fm.missing(), vec!["id"]);
    }

    #[test]
    fn evidence_defaults_to_empty_rather_than_failing_to_parse() {
        // An uncited trait must reach the lint rule as a trait with no
        // evidence. If it failed to deserialise it would be reported as invalid
        // YAML instead, and `uncited-claim` — the safeguard — would never fire.
        let fm = parse("id: x\nkind: belief\nclaim: c");
        assert!(fm.evidence.is_empty());
        assert!(fm.missing().is_empty());
    }

    #[test]
    fn an_unset_status_is_proposed_not_affirmed() {
        // Defaulting the other way would let the clone write from a reading of
        // the corpus nobody has confirmed.
        let t = LoadedTrait {
            frontmatter: parse("id: x\nkind: belief\nclaim: c"),
            rel_path: "persona/x.md".into(),
            path: PathBuf::from("persona/x.md"),
            body: String::new(),
            frontmatter_error: None,
        };
        assert_eq!(t.status(), "proposed");
        assert!(!t.is_affirmed());
    }

    #[test]
    fn a_trait_missing_its_id_still_has_a_usable_name() {
        let t = LoadedTrait {
            frontmatter: parse("kind: belief\nclaim: c"),
            rel_path: "persona/argues-from-cases.md".into(),
            path: PathBuf::from("/a/persona/argues-from-cases.md"),
            body: String::new(),
            frontmatter_error: None,
        };
        assert_eq!(t.id(), "argues-from-cases");
        assert_eq!(t.canonical_id(), "argues-from-cases");
    }

    #[test]
    fn an_id_is_matched_the_way_a_wikilink_is() {
        let t = LoadedTrait {
            frontmatter: parse("id: Argues From Cases\nkind: style\nclaim: c"),
            rel_path: "persona/argues-from-cases.md".into(),
            path: PathBuf::from("/a/persona/argues-from-cases.md"),
            body: String::new(),
            frontmatter_error: None,
        };
        assert_eq!(t.canonical_id(), "argues-from-cases");
    }
}
