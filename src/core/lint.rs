use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::Serialize;

use super::compilation::{Compilation, SourceIndex};
use super::links;
use super::manifest::Manifest;
use super::persona::{self, LoadedTrait};
use super::review;
use super::wiki::LoadedArticle;

/// How much a finding matters.
///
/// The distinction is not cosmetic: it decides the exit code, and therefore
/// whether CI fails or an agent stops to fix something. A broken `[[wikilink]]`
/// is a deliberate TODO in this workflow — the compile skill tells the agent to
/// link concepts before their articles exist — so it cannot be an error without
/// making every healthy archive fail its own lint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The archive is malformed: something is unparseable, ambiguous, or lying.
    Error,
    /// Work that is not finished yet. Expected in a living archive.
    Warning,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// A single lint result.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub severity: Severity,
    /// Stable kebab-case identifier, so output can be filtered or grouped by
    /// rule without matching on prose that may be reworded.
    pub rule: &'static str,
    /// Archive-relative path the finding is about, when it is about a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub message: String,
}

impl Finding {
    pub fn error(rule: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            rule,
            path: Some(path.into()),
            message: message.into(),
        }
    }

    pub fn warning(
        rule: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            rule,
            path: Some(path.into()),
            message: message.into(),
        }
    }

    /// A finding about the archive as a whole rather than one file.
    pub fn global(severity: Severity, rule: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity,
            rule,
            path: None,
            message: message.into(),
        }
    }
}

/// What a lint rule checks, for `sentinel schema`.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RuleInfo {
    pub rule: &'static str,
    pub severity: Severity,
    pub description: &'static str,
}

/// Every rule `analyze` can emit.
///
/// Published by `sentinel schema` so a skill can be written against the real
/// rule set instead of restating it in prose that drifts. Tests assert this
/// list and `analyze` agree in both directions, on rule ids and severities.
pub const RULES: &[RuleInfo] = &[
    RuleInfo {
        rule: "invalid-frontmatter",
        severity: Severity::Error,
        description: "The `---` block is present but is not valid YAML.",
    },
    RuleInfo {
        rule: "missing-field",
        severity: Severity::Error,
        description: "A required frontmatter field (title, domain, origin) is absent. These drive the generated indexes.",
    },
    RuleInfo {
        rule: "invalid-origin",
        severity: Severity::Error,
        description: "`origin` is not one of authored, researched, hybrid.",
    },
    RuleInfo {
        rule: "invalid-status",
        severity: Severity::Error,
        description: "`status` is not one of draft, review, stable.",
    },
    RuleInfo {
        rule: "invalid-date",
        severity: Severity::Error,
        description: "`created` or `updated` is not a YYYY-MM-DD date, or is dated in the future. `next` cannot rank a draft it cannot date, so the article never reaches the review step.",
    },
    RuleInfo {
        rule: "duplicate-slug",
        severity: Severity::Error,
        description: "Two articles share a filename stem, so [[wikilinks]] to it are ambiguous and the link graph merges them.",
    },
    RuleInfo {
        rule: "unresolved-source",
        severity: Severity::Error,
        description: "A `sources:` entry matches no raw document in the manifest, or matches more than one ambiguously.",
    },
    RuleInfo {
        rule: "broken-link",
        severity: Severity::Warning,
        description: "A [[wikilink]] target has no article yet. Expected: the compile workflow links concepts before writing them, and `sentinel next` ranks these as the articles most worth writing.",
    },
    RuleInfo {
        rule: "missing-tags",
        severity: Severity::Warning,
        description: "No `tags:` defined.",
    },
    RuleInfo {
        rule: "missing-sources",
        severity: Severity::Warning,
        description: "No `sources:` listed, so the raw document this came from will stay uncompiled.",
    },
    RuleInfo {
        rule: "missing-raw-document",
        severity: Severity::Error,
        description: "The manifest registers a raw document that is not on disk. Every article citing it resolves against an entry describing nothing.",
    },
    RuleInfo {
        rule: "uncompiled-source",
        severity: Severity::Warning,
        description: "A raw document that no wiki article cites in its `sources:`.",
    },
    RuleInfo {
        rule: "unattributed-extrapolation",
        severity: Severity::Error,
        description: "An `origin: extrapolated` article names no `persona:` traits. Generated prose that cannot be traced to a claim the author actually made is prose written in their voice on nobody's authority.",
    },
    RuleInfo {
        rule: "unvoiced-article",
        severity: Severity::Warning,
        description: "A wiki article names no `persona:` traits in an archive that has some. Everything under wiki/ is the clone's writing, so it should say which voice it was written through. A warning, not an error: an article written before the persona existed is unfinished rather than malformed, and the repair is to cite the traits it genuinely exhibits — never to attach plausible ones.",
    },
    RuleInfo {
        rule: "unresolved-trait",
        severity: Severity::Error,
        description: "A `persona:` entry names no trait in persona/. The attribution points at nothing, so the article cannot be checked against what it claims to have been written from.",
    },
    RuleInfo {
        rule: "wrote-from-rejected",
        severity: Severity::Error,
        description: "An article was written from a trait the author rejected. Their `no` is on the file; writing from it anyway is the one thing the verdict was for.",
    },
    RuleInfo {
        rule: "wrote-from-unconfirmed",
        severity: Severity::Warning,
        description: "An article was written from a trait the author has not affirmed. Not malformed — the reading may well be right — but nothing has confirmed it is theirs, so the work rests on the agent's own opinion of them.",
    },
    RuleInfo {
        rule: "invalid-verdict",
        severity: Severity::Error,
        description: "A `review:` entry's verdict is not one of approved, rejected, changes-requested, comment. A verdict nothing recognises decides nothing, and `export` reads this to know what may be published.",
    },
    RuleInfo {
        rule: "incomplete-verdict",
        severity: Severity::Error,
        description: "A `review:` entry is missing `by` or `at`, or `at` is not a YYYY-MM-DD date. A verdict attributed to nobody, or to no date, is the one thing the review mechanism exists to prevent.",
    },
    RuleInfo {
        rule: "verdict-disagrees-with-status",
        severity: Severity::Error,
        description: "A persona trait's `status:` contradicts its own latest verdict — someone edited one and not the other. The visible standing and the recorded history must say the same thing.",
    },
    RuleInfo {
        rule: "invalid-trait-frontmatter",
        severity: Severity::Error,
        description: "A persona trait's `---` block is present but is not valid YAML.",
    },
    RuleInfo {
        rule: "missing-trait-field",
        severity: Severity::Error,
        description: "A persona trait is missing a required field (id, kind, claim). Without them it is a claim about a person that cannot be cited, ranked, or read.",
    },
    RuleInfo {
        rule: "invalid-kind",
        severity: Severity::Error,
        description: "A persona trait's `kind` is not one of style, principle, belief, pattern.",
    },
    RuleInfo {
        rule: "invalid-confidence",
        severity: Severity::Error,
        description: "A persona trait's `confidence` is not one of high, medium, low.",
    },
    RuleInfo {
        rule: "invalid-trait-status",
        severity: Severity::Error,
        description: "A persona trait's `status` is not one of proposed, affirmed, rejected.",
    },
    RuleInfo {
        rule: "uncited-claim",
        severity: Severity::Error,
        description: "A persona trait cites no `evidence:`. An uncited claim about a person is the archive inventing them, and a profile that cannot be audited cannot be corrected.",
    },
    RuleInfo {
        rule: "unresolved-evidence",
        severity: Severity::Error,
        description: "A persona trait's `evidence:` entry matches no raw document in the manifest, so the claim behind it cannot be checked against anything.",
    },
    RuleInfo {
        rule: "inferred-from-research",
        severity: Severity::Error,
        description: "A persona trait cites a `researched` document as evidence. Research says what the author read, not what they think; a profile built from a reading list describes somebody else.",
    },
    RuleInfo {
        rule: "missing-reasoning",
        severity: Severity::Warning,
        description: "A persona trait has no body. The `evidence:` paths say where to look; the body is where the agent shows what in them supports the claim. Without it, auditing the profile means re-reading whole documents.",
    },
    RuleInfo {
        rule: "duplicate-trait-id",
        severity: Severity::Error,
        description: "Two persona traits share an id, so a citation to it is ambiguous about which claim was drawn on.",
    },
];

/// Run every check against a loaded archive.
///
/// Lives here rather than in the `lint` command because `sentinel next` needs
/// the same findings to decide what is most worth doing — two copies of these
/// rules would drift, and the one an agent acts on would be the stale one.
/// `root` is passed rather than read from `paths::archive_root()` so this stays
/// a pure function of its inputs — the unit tests below call it with no archive
/// installed, and a global read here made three of them panic.
pub fn analyze(
    articles: &[LoadedArticle],
    traits: &[LoadedTrait],
    manifest: &Manifest,
    root: &std::path::Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let all_slugs: HashSet<String> = articles.iter().map(|a| a.canonical_slug()).collect();

    // A wikilink names a slug, and a slug is just a filename stem. Two articles
    // sharing one across domains collapse into a single node in the link graph:
    // backlinks merge and one article's forward links overwrite the other's.
    // Nothing else in the pipeline notices, so it has to be caught here.
    let mut slug_owners: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for article in articles {
        // Canonical, so `Ethics.md` and `ethics.md` are caught as the collision
        // they are — a wikilink cannot distinguish them either.
        slug_owners
            .entry(article.canonical_slug())
            .or_default()
            .push(article.rel_path());
    }
    for (slug, owners) in &slug_owners {
        if owners.len() > 1 {
            findings.push(Finding::global(
                Severity::Error,
                "duplicate-slug",
                format!(
                    "duplicate slug '{slug}': {} — [[{slug}]] is ambiguous and the link graph merges them",
                    owners.join(", ")
                ),
            ));
        }
    }

    for loaded in articles {
        let path = loaded.rel_path();
        let frontmatter = &loaded.article.frontmatter;

        // Broken wikilinks are checked first because the body is readable even
        // when the frontmatter is not.
        //
        // Grouped per concept, not per occurrence: `[[Free Will]]` and
        // `[[free-will]]` in one article are one missing article, and listing
        // them separately makes an agent work the same gap twice.
        let mut missing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for link in links::extract_wikilinks(&loaded.content) {
            let canonical = super::slug::canonical(&link);
            if canonical.is_empty() || all_slugs.contains(canonical.as_str()) {
                continue;
            }
            missing.entry(canonical).or_default().insert(link);
        }
        for (canonical, spellings) in missing {
            // A warning, not an error: the compile workflow deliberately links
            // concepts before their articles exist.
            let as_written: Vec<String> = spellings.iter().map(|s| format!("[[{s}]]")).collect();
            let detail = if spellings.len() > 1 {
                format!(
                    "broken link [[{canonical}]] — no matching article found \
                     (written {} in this file)",
                    as_written.join(", ")
                )
            } else {
                format!("broken link {} — no matching article found", as_written[0])
            };
            findings.push(Finding::warning("broken-link", path, detail));
        }

        // Malformed YAML yields default frontmatter, so every field check below
        // would fire at once and point the reader at five imaginary problems
        // instead of the one real one.
        if let Some(error) = &loaded.article.frontmatter_error {
            findings.push(Finding::error(
                "invalid-frontmatter",
                path,
                format!("invalid frontmatter — {error}"),
            ));
            continue;
        }

        // title/domain/origin drive the generated indexes; without them an
        // article is effectively missing from the knowledge base.
        for (field, missing) in [
            ("title", frontmatter.title.is_none()),
            ("domain", frontmatter.domain.is_none()),
            ("origin", frontmatter.origin.is_none()),
        ] {
            if missing {
                findings.push(Finding::error(
                    "missing-field",
                    path,
                    format!("missing '{field}' in frontmatter"),
                ));
            }
        }

        if frontmatter.tags.is_empty() {
            findings.push(Finding::warning("missing-tags", path, "no tags defined"));
        }
        // Not for the clone's own work: an extrapolated article is not
        // compiled from anything, so "its raw document will stay uncompiled"
        // names a document that does not exist. Its provenance rule is
        // `unattributed-extrapolation` instead.
        if frontmatter.sources.is_empty() && !frontmatter.is_extrapolated() {
            findings.push(Finding::warning(
                "missing-sources",
                path,
                "no sources listed — its raw document will stay uncompiled",
            ));
        }

        if let Some(origin) = &frontmatter.origin
            && !super::frontmatter::ORIGINS.contains(&origin.as_str())
        {
            findings.push(Finding::error(
                "invalid-origin",
                path,
                format!("invalid origin '{origin}' (expected authored/researched/hybrid)"),
            ));
        }

        if let Some(status) = &frontmatter.status
            && !super::frontmatter::STATUSES.contains(&status.as_str())
        {
            findings.push(Finding::error(
                "invalid-status",
                path,
                format!("invalid status '{status}' (expected draft/review/stable)"),
            ));
        }
    }

    // Dates. An unparseable one is not cosmetic: `next` ranks stale drafts by
    // `updated`, and silently skipped any it could not parse — so a draft dated
    // `01/02/2024` was invisible to the review step no matter how long it sat.
    // A future date hides one the same way, by never becoming stale.
    // Walked over both document kinds from one list. Persona traits carry the
    // same two date fields, and a second copy of this loop for them would be a
    // second opinion about what a date is.
    let today = chrono::Local::now().date_naive();
    let dated: Vec<(String, super::frontmatter::Dates<'_>)> = articles
        .iter()
        .map(|a| (a.rel_path().to_string(), a.article.frontmatter.dates()))
        .chain(
            traits
                .iter()
                .map(|t| (t.rel_path.clone(), t.frontmatter.dates())),
        )
        .collect();
    for (path, dates) in dated {
        for (field, value) in dates {
            let Some(value) = value else {
                continue;
            };
            match super::frontmatter::parse_date(value) {
                Err(why) => findings.push(Finding::error(
                    "invalid-date",
                    path.clone(),
                    format!("`{field}`: {why}"),
                )),
                // A day of slack: an article written across a timezone boundary
                // is not a data error, and a rule that fires on one is noise.
                Ok(date) if (date - today).num_days() > 1 => {
                    findings.push(Finding::error(
                        "invalid-date",
                        path.clone(),
                        format!("`{field}` is dated {date}, in the future"),
                    ));
                }
                Ok(_) => {}
            }
        }
    }

    // The manifest against disk. Every other rule checks articles against the
    // manifest or against each other, so a manifest entry naming a file that
    // does not exist was consistent with everything and reported by nothing —
    // an archive could lose a raw document and lint clean.
    //
    // `try_exists`, not `exists`: a file we cannot stat is not a file we know
    // to be missing, and reporting it as malformed would send an agent to
    // delete a citation over a permissions problem.
    for rel_path in manifest.entries.keys() {
        if root.join(rel_path).try_exists().is_ok_and(|there| !there) {
            findings.push(Finding::error(
                "missing-raw-document",
                rel_path.clone(),
                "registered in the manifest but not on disk. Restore the file, \
                 or `sentinel rm` the entry if it is genuinely gone."
                    .to_string(),
            ));
        }
    }

    // Check the raw <-> wiki mapping, derived from what each article cites.
    let compilation = Compilation::derive(articles, manifest);
    for entry in &compilation.unresolved {
        // A citation that resolves to nothing is where the compile loop stalls,
        // and "matches no raw document" leaves the reader to guess whether they
        // mistyped, forgot to ingest, or used the wrong path form. The manifest
        // knows; say so.
        let hint = match &entry.suggestion {
            Some(path) => format!(". Did you mean '{path}'?"),
            None => ". `sentinel uncompiled --json` lists what is registered.".to_string(),
        };
        findings.push(Finding::error(
            "unresolved-source",
            entry.article.clone(),
            format!(
                "source '{}' matches no raw document in the manifest{hint}",
                entry.source
            ),
        ));
    }
    for entry in compilation.uncompiled(manifest) {
        findings.push(Finding::warning(
            "uncompiled-source",
            entry.raw_path.clone(),
            format!("not yet compiled into any wiki article ({})", entry.title),
        ));
    }

    // Attribution for generated work. Kept together because they are one
    // question — can a reader follow this prose back to something the author
    // actually said — asked four ways.
    let by_id: BTreeMap<String, &LoadedTrait> =
        traits.iter().map(|t| (t.canonical_id(), t)).collect();
    // Only once there is a voice to have been written through. Warning on
    // every article in an archive that has not built a persona yet would be
    // noise about work nobody could have done.
    let has_persona = traits.iter().any(persona::LoadedTrait::is_affirmed);
    for article in articles {
        let fm = &article.article.frontmatter;
        let path = article.rel_path();

        if !fm.is_extrapolated() && fm.persona.is_empty() && has_persona {
            findings.push(Finding::warning(
                "unvoiced-article",
                path,
                "names no `persona:` traits — everything under wiki/ is the \
                 clone's writing, so it should say which voice it was written \
                 through",
            ));
        }

        if fm.is_extrapolated() && fm.persona.is_empty() {
            findings.push(Finding::error(
                "unattributed-extrapolation",
                path,
                "written by the clone but names no `persona:` traits — nothing \
                 ties it to a claim the author actually made"
                    .to_string(),
            ));
        }

        for cited in &fm.persona {
            let Some(t) = by_id.get(&super::slug::canonical(cited)) else {
                findings.push(Finding::error(
                    "unresolved-trait",
                    path,
                    format!(
                        "`persona: {cited}` names no trait — `sentinel persona --json` lists them"
                    ),
                ));
                continue;
            };
            match t.status() {
                "rejected" => findings.push(Finding::error(
                    "wrote-from-rejected",
                    path,
                    format!(
                        "written from '{}', which the author rejected ({})",
                        t.id(),
                        t.rel_path
                    ),
                )),
                "affirmed" => {}
                other => findings.push(Finding::warning(
                    "wrote-from-unconfirmed",
                    path,
                    format!(
                        "written from '{}', which is `{other}` — nothing has confirmed it is theirs",
                        t.id()
                    ),
                )),
            }
        }
    }

    // Verdicts, on both document kinds. Walked from one list for the same
    // reason the date rule is: two copies would be two opinions about what a
    // recorded decision has to carry.
    let reviewed: Vec<(&str, &[review::Entry])> = articles
        .iter()
        .map(|a| (a.rel_path(), a.article.frontmatter.review.as_slice()))
        .chain(
            traits
                .iter()
                .map(|t| (t.rel_path.as_str(), t.frontmatter.review.as_slice())),
        )
        .collect();
    for (path, entries) in reviewed {
        for entry in entries {
            if !review::VERDICTS.contains(&entry.verdict.as_str()) {
                findings.push(Finding::error(
                    "invalid-verdict",
                    path,
                    format!(
                        "verdict '{}' is not one of {}",
                        entry.verdict,
                        review::VERDICTS.join(", ")
                    ),
                ));
            }
            if entry.by.trim().is_empty() {
                findings.push(Finding::error(
                    "incomplete-verdict",
                    path,
                    format!(
                        "verdict '{}' records no `by` — it is signed by nobody",
                        entry.verdict
                    ),
                ));
            }
            match super::frontmatter::parse_date(&entry.at) {
                Ok(_) => {}
                Err(why) => findings.push(Finding::error(
                    "incomplete-verdict",
                    path,
                    format!("verdict '{}' has `at`: {why}", entry.verdict),
                )),
            }
        }
    }

    findings.extend(persona_findings(traits, manifest));

    sort(&mut findings);
    findings
}

/// The `persona/` rules.
///
/// Separated because they are not house style — they are the safeguards the
/// clone design turns on, and they are worth being able to read in one place.
/// Two of them (`uncited-claim`, `inferred-from-research`) are the reason the
/// profile can be trusted at all: without them the archive can assert anything
/// about its author and cite nothing, or cite their reading list.
fn persona_findings(traits: &[LoadedTrait], manifest: &Manifest) -> Vec<Finding> {
    let mut findings = Vec::new();

    let mut owners: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for t in traits {
        owners
            .entry(t.canonical_id())
            .or_default()
            .push(t.rel_path.as_str());
    }
    for (id, paths) in &owners {
        if paths.len() > 1 {
            findings.push(Finding::global(
                Severity::Error,
                "duplicate-trait-id",
                format!(
                    "duplicate trait id '{id}': {} — a citation to it cannot say which claim it drew on",
                    paths.join(", ")
                ),
            ));
        }
    }

    // Evidence is matched the way `sources:` is, so `raw/essays/x.md` and a
    // bare `x.md` resolve identically. A second matcher here would mean a
    // citation that compiles but does not count as evidence, or the reverse.
    let index = SourceIndex::new(manifest);

    for t in traits {
        let path = t.rel_path.as_str();

        if let Some(error) = &t.frontmatter_error {
            findings.push(Finding::error(
                "invalid-trait-frontmatter",
                path,
                format!("invalid frontmatter — {error}"),
            ));
            continue;
        }

        for field in t.frontmatter.missing() {
            findings.push(Finding::error(
                "missing-trait-field",
                path,
                format!("missing '{field}' in frontmatter"),
            ));
        }

        for (rule, value, allowed) in [
            (
                "invalid-kind",
                t.frontmatter.kind.as_deref(),
                persona::KINDS,
            ),
            (
                "invalid-confidence",
                t.frontmatter.confidence.as_deref(),
                persona::CONFIDENCES,
            ),
            (
                "invalid-trait-status",
                t.frontmatter.status.as_deref(),
                persona::STATUSES,
            ),
        ] {
            if let Some(value) = value
                && !allowed.contains(&value)
            {
                findings.push(Finding::error(
                    rule,
                    path,
                    format!("invalid value '{value}' (expected {})", allowed.join("/")),
                ));
            }
        }

        // Safeguard 1: no uncited claim about a person.
        if t.frontmatter.evidence.is_empty() {
            findings.push(Finding::error(
                "uncited-claim",
                path,
                format!(
                    "no evidence cited for '{}'. Every trait names the raw \
                     documents it was read out of, or it is a claim about \
                     somebody that nobody can check.",
                    t.id()
                ),
            ));
        }

        // The visible standing against the recorded one. Traits carry both
        // because `status:` is what a person reads at the top of the file —
        // deriving it silently would mean a file that says `proposed` while
        // the archive treats it as affirmed.
        if let Some(decision) = review::standing(&t.frontmatter.review)
            && let Some(implied) = review::implied_status(&decision.verdict)
            && t.status() != implied
        {
            findings.push(Finding::error(
                "verdict-disagrees-with-status",
                path,
                format!(
                    "`status: {}` but the latest verdict is '{}' ({} on {}), which means `{implied}`",
                    t.status(),
                    decision.verdict,
                    decision.by,
                    decision.at
                ),
            ));
        }

        // Cited but unexplained. A warning, not an error: the claim is still
        // checkable, just expensively — and a half-written trait is unfinished
        // work, which is what warnings are for.
        if t.body.trim().is_empty() && !t.frontmatter.evidence.is_empty() {
            findings.push(Finding::warning(
                "missing-reasoning",
                path,
                "cites evidence but shows nothing from it — quote what supports the claim",
            ));
        }

        for cited in &t.frontmatter.evidence {
            let Some(resolved) = index.resolve(cited) else {
                let hint = match index.suggest(cited) {
                    Some(p) => format!(". Did you mean '{p}'?"),
                    None => String::new(),
                };
                findings.push(Finding::error(
                    "unresolved-evidence",
                    path,
                    format!("evidence '{cited}' matches no raw document in the manifest{hint}"),
                ));
                continue;
            };
            // Safeguard 2: beliefs come only from the author's own writing.
            let Some(entry) = manifest.entries.get(&resolved) else {
                continue;
            };
            if !persona::EVIDENCE_ORIGINS.contains(&entry.origin.as_str()) {
                findings.push(Finding::error(
                    "inferred-from-research",
                    path,
                    format!(
                        "evidence '{resolved}' has origin '{}' — it says what the author read, \
                         not what they think. Cite {} material.",
                        entry.origin,
                        persona::EVIDENCE_ORIGINS.join(" or ")
                    ),
                ));
            }
        }
    }

    findings
}

/// Ordering for display: errors first, then by rule, then by path — stable
/// across runs so a diff of two lint outputs shows only real changes.
pub fn sort(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.rule.cmp(b.rule))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.message.cmp(&b.message))
    });
}

pub fn count(findings: &[Finding], severity: Severity) -> usize {
    findings.iter().filter(|f| f.severity == severity).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::core::frontmatter::{Frontmatter, WikiArticle};
    use crate::core::manifest::ManifestEntry;
    use std::collections::BTreeSet;

    fn loaded(path: &str, fm: Frontmatter, content: &str, error: Option<&str>) -> LoadedArticle {
        LoadedArticle {
            article: WikiArticle {
                frontmatter: fm,
                rel_path: path.to_string(),
                frontmatter_error: error.map(ToString::to_string),
            },
            path: std::path::PathBuf::from(path),
            content: content.to_string(),
        }
    }

    fn complete(sources: &[&str]) -> Frontmatter {
        Frontmatter {
            title: Some("T".into()),
            domain: Some("philosophy".into()),
            origin: Some("authored".into()),
            tags: vec!["t".into()],
            sources: sources.iter().map(|s| (*s).to_string()).collect(),
            status: Some("draft".into()),
            ..Default::default()
        }
    }

    /// A persona trait built the way the loader builds one — through the real
    /// frontmatter parser, so a fixture cannot pass by constructing a struct
    /// the parser would never produce.
    fn trait_of(path: &str, content: &str) -> LoadedTrait {
        let parsed = crate::core::frontmatter::parse_as::<persona::TraitFrontmatter>(content);
        LoadedTrait {
            frontmatter: parsed.frontmatter,
            rel_path: path.to_string(),
            path: std::path::PathBuf::from(path),
            body: parsed.body,
            frontmatter_error: parsed.error,
        }
    }

    /// An archive rigged to trip every rule at once.
    fn everything_wrong() -> (Vec<LoadedArticle>, Vec<LoadedTrait>, Manifest) {
        let mut manifest = Manifest::default();
        // `missing-raw-document`: nothing in this fixture exists on disk, so
        // every entry fires it. That is the point — the rule compares the
        // manifest with the filesystem, and there is no filesystem here.
        // `gathered.md` is `researched`, which is what `inferred-from-research`
        // needs: a document that resolves cleanly and still must not count as
        // evidence for what its owner believes.
        for raw in ["raw/philosophy/cited.md", "raw/philosophy/stranded.md"] {
            manifest.upsert(ManifestEntry {
                raw_path: raw.to_string(),
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
        manifest.upsert(ManifestEntry {
            raw_path: "raw/philosophy/gathered.md".into(),
            title: "Gathered".into(),
            domain: "philosophy".into(),
            origin: "researched".into(),
            ingested_at: "2026-01-01 00:00:00".into(),
            wiki_articles: vec![],
            source_type: "document".into(),
            content_hash: None,
            publish: false,
        });

        let traits = vec![
            // invalid-trait-frontmatter
            trait_of("persona/broken.md", "---\n\tid: [oops\n---\n\nbody\n"),
            // missing-trait-field (id/kind/claim) + uncited-claim
            trait_of("persona/bare.md", "---\nconfidence: high\n---\n\nbody\n"),
            // invalid-kind + invalid-confidence + invalid-trait-status,
            // and no body at all: missing-reasoning
            trait_of(
                "persona/enums.md",
                "---\nid: enums\nkind: nonsense\nclaim: c\nconfidence: nonsense\n\
                 status: nonsense\nevidence: [raw/philosophy/cited.md]\n---\n",
            ),
            // unresolved-evidence
            trait_of(
                "persona/evidence.md",
                "---\nid: evidence\nkind: belief\nclaim: c\n\
                 evidence: [raw/philosophy/nowhere.md]\n---\n\nbody\n",
            ),
            // inferred-from-research: resolves, but to material the author read
            trait_of(
                "persona/research.md",
                "---\nid: research\nkind: belief\nclaim: c\n\
                 evidence: [raw/philosophy/gathered.md]\n---\n\nbody\n",
            ),
            // invalid-verdict + incomplete-verdict (no `by`, unparseable `at`)
            trait_of(
                "persona/verdicts.md",
                "---\nid: verdicts\nkind: style\nclaim: c\n\
                 evidence: [raw/philosophy/cited.md]\nreview:\n  \
                 - verdict: nonsense\n    by: \"\"\n    at: not-a-date\n---\n\nbody\n",
            ),
            // verdict-disagrees-with-status: the file says one thing, its own
            // history says another.
            trait_of(
                "persona/stale.md",
                "---\nid: stale\nkind: style\nclaim: c\nstatus: proposed\n\
                 evidence: [raw/philosophy/cited.md]\nreview:\n  \
                 - verdict: approved\n    by: someone\n    at: 2026-01-01\n---\n\nbody\n",
            ),
            // An affirmed trait, so the archive has a voice — which is what
            // `unvoiced-article` needs before it will say anything.
            trait_of(
                "persona/settled.md",
                "---\nid: settled\nkind: style\nclaim: c\nstatus: affirmed\n\
                 evidence: [raw/philosophy/cited.md]\n---\n\nbody\n",
            ),
            // A trait the author said no to, for `wrote-from-rejected`.
            trait_of(
                "persona/refused.md",
                "---\nid: refused\nkind: belief\nclaim: c\nstatus: rejected\n\
                 evidence: [raw/philosophy/cited.md]\n---\n\nbody\n",
            ),
            // duplicate-trait-id: two files, one id
            trait_of(
                "persona/first.md",
                "---\nid: shared\nkind: style\nclaim: c\n\
                 evidence: [raw/philosophy/cited.md]\n---\n\nbody\n",
            ),
            trait_of(
                "persona/second.md",
                "---\nid: shared\nkind: style\nclaim: c\n\
                 evidence: [raw/philosophy/cited.md]\n---\n\nbody\n",
            ),
        ];

        let articles = vec![
            // invalid-frontmatter
            loaded(
                "wiki/a/broken.md",
                Frontmatter::default(),
                "x",
                Some("bad yaml"),
            ),
            // missing-field (title/domain/origin) + missing-tags + missing-sources
            loaded("wiki/a/bare.md", Frontmatter::default(), "x", None),
            // invalid-origin + invalid-status
            loaded(
                "wiki/a/enums.md",
                Frontmatter {
                    origin: Some("nonsense".into()),
                    status: Some("nonsense".into()),
                    ..complete(&["raw/philosophy/cited.md"])
                },
                "x",
                None,
            ),
            // broken-link + unresolved-source
            loaded(
                "wiki/a/links.md",
                complete(&["raw/philosophy/nowhere.md"]),
                "See [[not-written]].",
                None,
            ),
            // invalid-date, both ways it can be wrong
            loaded(
                "wiki/a/dated.md",
                Frontmatter {
                    updated: Some("01/02/2024".into()),
                    created: Some("2999-01-01".into()),
                    ..complete(&["raw/philosophy/cited.md"])
                },
                "x",
                None,
            ),
            // unattributed-extrapolation
            loaded(
                "wiki/a/generated.md",
                Frontmatter {
                    origin: Some("extrapolated".into()),
                    ..complete(&["raw/philosophy/cited.md"])
                },
                "x",
                None,
            ),
            // unresolved-trait + wrote-from-rejected + wrote-from-unconfirmed
            loaded(
                "wiki/a/attributed.md",
                Frontmatter {
                    origin: Some("extrapolated".into()),
                    persona: vec!["nonexistent".into(), "refused".into(), "evidence".into()],
                    ..complete(&["raw/philosophy/cited.md"])
                },
                "x",
                None,
            ),
            // duplicate-slug: same stem, different domain
            loaded(
                "wiki/b/dup.md",
                complete(&["raw/philosophy/cited.md"]),
                "x",
                None,
            ),
            loaded(
                "wiki/c/dup.md",
                complete(&["raw/philosophy/cited.md"]),
                "x",
                None,
            ),
        ];
        (articles, traits, manifest)
    }

    #[test]
    fn every_documented_rule_can_actually_fire() {
        let (articles, traits, manifest) = everything_wrong();
        let emitted: BTreeSet<&str> = analyze(
            &articles,
            &traits,
            &manifest,
            std::path::Path::new("/nonexistent-archive"),
        )
        .iter()
        .map(|f| f.rule)
        .collect();
        let documented: BTreeSet<&str> = RULES.iter().map(|r| r.rule).collect();

        let never_fires: Vec<&&str> = documented.difference(&emitted).collect();
        assert!(
            never_fires.is_empty(),
            "RULES documents rules that analyze() never emits: {never_fires:?}"
        );
    }

    #[test]
    fn every_emitted_rule_is_documented() {
        let (articles, traits, manifest) = everything_wrong();
        let documented: BTreeSet<&str> = RULES.iter().map(|r| r.rule).collect();
        for finding in analyze(
            &articles,
            &traits,
            &manifest,
            std::path::Path::new("/nonexistent-archive"),
        ) {
            assert!(
                documented.contains(finding.rule),
                "rule '{}' is emitted but missing from RULES, so `sentinel schema` under-reports it",
                finding.rule
            );
        }
    }

    #[test]
    fn documented_severity_matches_emitted_severity() {
        let (articles, traits, manifest) = everything_wrong();
        for finding in analyze(
            &articles,
            &traits,
            &manifest,
            std::path::Path::new("/nonexistent-archive"),
        ) {
            let documented = RULES
                .iter()
                .find(|r| r.rule == finding.rule)
                .unwrap_or_else(|| panic!("undocumented rule {}", finding.rule));
            assert_eq!(
                documented.severity, finding.severity,
                "rule '{}' is documented as {:?} but emitted as {:?}",
                finding.rule, documented.severity, finding.severity
            );
        }
    }

    #[test]
    fn rule_ids_are_unique() {
        let unique: BTreeSet<&str> = RULES.iter().map(|r| r.rule).collect();
        assert_eq!(unique.len(), RULES.len());
    }

    #[test]
    fn errors_sort_before_warnings() {
        let mut findings = vec![
            Finding::warning("broken-link", "b.md", "w"),
            Finding::error("duplicate-slug", "a.md", "e"),
        ];
        sort(&mut findings);
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn sorting_is_total_so_output_does_not_churn() {
        let build = || {
            vec![
                Finding::warning("broken-link", "z.md", "1"),
                Finding::warning("broken-link", "a.md", "2"),
                Finding::error("invalid-origin", "m.md", "3"),
            ]
        };
        let mut a = build();
        let mut b = build();
        b.reverse();
        sort(&mut a);
        sort(&mut b);
        let paths = |f: &[Finding]| f.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&a), paths(&b));
    }

    #[test]
    fn severity_counts() {
        let findings = vec![
            Finding::error("a", "x", "1"),
            Finding::warning("b", "y", "2"),
            Finding::warning("c", "z", "3"),
        ];
        assert_eq!(count(&findings, Severity::Error), 1);
        assert_eq!(count(&findings, Severity::Warning), 2);
    }

    #[test]
    fn severity_serializes_as_a_lowercase_string() {
        let json = serde_json::to_string(&Severity::Warning).unwrap();
        assert_eq!(json, "\"warning\"");
    }
}
