use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::frontmatter;
use crate::core::lint::{self, RuleInfo};
use crate::core::output;
use crate::core::paths;
use crate::core::persona;
use crate::core::review;

/// The archive's contract, in one call.
///
/// An agent writing a wiki article needs to know the frontmatter fields, which
/// are required, what values the enums accept, which domains exist, and what
/// will be flagged. Every skill previously restated all of that in prose, and
/// the prose drifted — `/sentinel-compile` documented five domains where the
/// code had three. Publishing the contract from the code makes that class of
/// mismatch impossible.
#[derive(Serialize)]
struct Schema {
    frontmatter: &'static [Field],
    /// The `persona/` contract — the second document schema in the archive.
    persona: &'static [Field],
    domains: Domains,
    layout: &'static [Directory],
    lint_rules: &'static [RuleInfo],
    next_actions: Vec<NextAction>,
}

#[derive(Serialize)]
pub struct Field {
    pub name: &'static str,
    pub required: bool,
    #[serde(rename = "type")]
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<&'static [&'static str]>,
    pub description: &'static str,
}

#[derive(Serialize)]
struct Domains {
    /// Created by `sentinel init`.
    default: &'static [&'static str],
    /// Actually present in this archive, from disk.
    present: Vec<String>,
}

#[derive(Serialize)]
struct Directory {
    pub name: &'static str,
    purpose: &'static str,
}

#[derive(Serialize)]
struct NextAction {
    /// Position in `next::Action::LADDER`, 1-based. Computed rather than
    /// written down: two copies of an ordering is two orderings, and this one
    /// is the archive's editorial judgement about what matters most.
    priority: usize,
    action: &'static str,
    pub description: &'static str,
}

/// What each rung of the ladder is for.
///
/// Descriptions only. The order and the numbering come from
/// `next::Action::LADDER`, which is the ladder — a hand-numbered second copy
/// drifted from it the moment a rung was inserted anywhere but the end.
fn next_actions() -> Vec<NextAction> {
    crate::commands::next::Action::LADDER
        .iter()
        .enumerate()
        .map(|(i, action)| NextAction {
            priority: i + 1,
            action: action.as_str(),
            description: describe(*action),
        })
        .collect()
}

fn describe(action: crate::commands::next::Action) -> &'static str {
    use crate::commands::next::Action;
    match action {
        Action::FixErrors => {
            "Lint errors exist. Every later judgement would be made on data they call into question."
        }
        Action::Learn => {
            "Documents the author wrote that no persona trait has been read from. Above `compile` deliberately: a corpus read after the wiki is built shaped nothing, and the clone cannot write in a voice it has not read."
        }
        Action::Compile => "Raw documents no wiki article cites. Knowledge already in hand.",
        Action::Write => {
            "Wikilinks with no article behind them, ranked by how many distinct articles ask for each. The wiki naming its own gaps."
        }
        Action::Connect => "Articles nothing links to.",
        Action::Review => "Drafts untouched for over 30 days.",
        // Not a rung; `LADDER` never yields it, and the match is exhaustive so
        // that a new variant fails to compile rather than shipping undescribed.
        Action::None => "Nothing outstanding.",
    }
}

pub const FIELDS: &[Field] = &[
    Field {
        name: "title",
        required: true,
        kind: "string",
        values: None,
        description: "Human-readable title. Used as the display name in every generated index.",
    },
    Field {
        name: "domain",
        required: true,
        kind: "string",
        values: None,
        description: "Subject area. Should match the wiki/ subdirectory the article lives in.",
    },
    Field {
        name: "origin",
        required: true,
        kind: "enum",
        values: Some(frontmatter::ORIGINS),
        description: "Provenance. authored = distilled from the user's own writing; researched = gathered by an agent; hybrid = the user's ideas enriched with research.",
    },
    Field {
        name: "tags",
        required: false,
        kind: "string[]",
        values: None,
        description: "Topic tags.",
    },
    Field {
        name: "sources",
        required: false,
        kind: "string[]",
        values: None,
        description: "Archive-relative paths to the raw documents this was compiled from, e.g. raw/philosophy/meditations.md. This is what marks a raw document as compiled — an article with no sources leaves its source in the uncompiled queue forever.",
    },
    Field {
        name: "related",
        required: false,
        kind: "string[]",
        values: None,
        description: "Wikilinks to related articles, e.g. \"[[stoicism]]\".",
    },
    Field {
        name: "created",
        required: false,
        kind: "date",
        values: None,
        description: "YYYY-MM-DD.",
    },
    Field {
        name: "updated",
        required: false,
        kind: "date",
        values: None,
        description: "YYYY-MM-DD. Drives index/_recent.md and the stale-draft check in `sentinel next`.",
    },
    Field {
        name: "status",
        required: false,
        kind: "enum",
        values: Some(frontmatter::STATUSES),
        description: "Maturity of the article. Separate from approval: `stable` means finished, not signed off.",
    },
    Field {
        name: "review",
        required: false,
        kind: "entry[]",
        values: Some(review::VERDICTS),
        description: "Verdicts the archive's owner recorded, oldest first — {verdict, by, at, note}. Written by `sentinel review`, never by an agent. The operative one is the latest that decided something; a `comment` leaves standing unchanged.",
    },
];

/// The `persona/` trait contract.
///
/// Separate from `FIELDS` because a trait is a different document: it makes a
/// claim about a person rather than about a subject, and the fields that keep
/// that honest — `evidence`, `confidence`, `status` — have no article
/// equivalent. Published for the same reason `FIELDS` is: a skill that has to
/// restate the contract in prose is a skill whose prose will drift from it.
pub const PERSONA_FIELDS: &[Field] = &[
    Field {
        name: "id",
        required: true,
        kind: "string",
        values: None,
        description: "Stable identifier, matching the filename stem. What an article cites when it says which traits it wrote from.",
    },
    Field {
        name: "kind",
        required: true,
        kind: "enum",
        values: Some(persona::KINDS),
        description: "style = how the prose reads; principle = a rule the author applies; belief = a position they hold; pattern = a recurring move in how they think.",
    },
    Field {
        name: "claim",
        required: true,
        kind: "string",
        values: None,
        description: "The claim itself, in one sentence.",
    },
    Field {
        name: "evidence",
        required: false,
        kind: "string[]",
        values: None,
        description: "Archive-relative paths to the raw documents this was read out of. Required in practice: a trait with none is a `uncited-claim` error, because a claim about a person that cites nothing is the archive inventing them. Only `authored` or `hybrid` documents count — research says what the author read, not what they think.",
    },
    Field {
        name: "confidence",
        required: false,
        kind: "enum",
        values: Some(persona::CONFIDENCES),
        description: "How well the evidence supports the claim.",
    },
    Field {
        name: "status",
        required: false,
        kind: "enum",
        values: Some(persona::STATUSES),
        description: "proposed = the agent's reading, unconfirmed (the default when absent); affirmed = the author confirmed it; rejected = they did not. A rejected trait stays on disk carrying the rejection, so the next iteration cannot re-propose it.",
    },
    Field {
        name: "created",
        required: false,
        kind: "date",
        values: None,
        description: "YYYY-MM-DD.",
    },
    Field {
        name: "updated",
        required: false,
        kind: "date",
        values: None,
        description: "YYYY-MM-DD.",
    },
    Field {
        name: "review",
        required: false,
        kind: "entry[]",
        values: Some(review::VERDICTS),
        description: "Verdicts the author recorded about this claim, oldest first. `status:` is what a reader sees; this is the history behind it. Written by `sentinel review`, never by an agent.",
    },
];

const LAYOUT: &[Directory] = &[
    Directory {
        name: "raw/",
        purpose: "Source documents. Immutable — never edited by sentinel or by an agent.",
    },
    Directory {
        name: "persona/",
        purpose: "Cited traits describing how the author writes and what they hold. Agent-owned; every claim carries `evidence:` pointing at their own raw documents.",
    },
    Directory {
        name: "wiki/",
        purpose: "Compiled articles with YAML frontmatter. Agent-owned.",
    },
    Directory {
        name: "index/",
        purpose: "Generated by `sentinel index`. Never edit by hand; edits are overwritten.",
    },
    Directory {
        name: "meta/",
        purpose: "Machine state: manifest.json, link-graph.json, log.md.",
    },
    Directory {
        name: "templates/",
        purpose: "Article templates.",
    },
];

/// A blank frontmatter block built from the published field list.
///
/// The article template used to be a hand-written fourth copy of the contract,
/// alongside the lint rule, the schema output, and `ingest`'s validation. Three
/// of those already drifted apart in this codebase (#6, #31); generating the
/// fourth removes the possibility.
pub fn blank_frontmatter() -> String {
    let mut out = String::from("---\n");
    for field in FIELDS {
        let default = match (field.kind, field.name) {
            (_, "origin") => " authored",
            (_, "status") => " draft",
            ("string[]", _) => " []",
            _ => "",
        };
        out.push_str(&format!("{}:{default}\n", field.name));
    }
    out.push_str("---\n\n");
    out
}

/// A blank `persona/` trait, built from the published field list.
pub fn blank_trait() -> String {
    let mut out = String::from("---\n");
    for field in PERSONA_FIELDS {
        let default = match (field.kind, field.name) {
            (_, "status") => " proposed",
            ("string[]", _) => " []",
            _ => "",
        };
        out.push_str(&format!("{}:{default}\n", field.name));
    }
    out.push_str(
        "---\n\n\
         What in the evidence supports the claim. Quote it — the paths above say \n\
         where to look, and this is what saves a reader from re-reading whole \n\
         documents to check a sentence about themselves.\n",
    );
    out
}

pub fn run() -> io::Result<()> {
    let schema = Schema {
        frontmatter: FIELDS,
        persona: PERSONA_FIELDS,
        domains: Domains {
            default: paths::DEFAULT_DOMAINS,
            present: paths::present_domains(),
        },
        layout: LAYOUT,
        lint_rules: lint::RULES,
        next_actions: next_actions(),
    };

    if output::is_json() {
        return output::emit("schema", schema);
    }

    println!("{}", "Wiki Article Frontmatter".bold());
    for field in schema.frontmatter {
        let required = if field.required {
            "required".red().to_string()
        } else {
            "optional".dimmed().to_string()
        };
        let kind = match field.values {
            Some(values) => values.join(" | "),
            None => field.kind.to_string(),
        };
        println!("  {:<10} {required}  {}", field.name.cyan(), kind.dimmed());
        println!("    {}", field.description);
    }

    println!("\n{}", "Persona Trait Frontmatter".bold());
    println!(
        "  {}",
        "persona/*.md — what the archive holds about its author".dimmed()
    );
    for field in schema.persona {
        let required = if field.required {
            "required".red().to_string()
        } else {
            "optional".dimmed().to_string()
        };
        let kind = match field.values {
            Some(values) => values.join(" | "),
            None => field.kind.to_string(),
        };
        println!("  {:<12} {required}  {}", field.name.cyan(), kind.dimmed());
        println!("    {}", field.description);
    }

    println!("\n{}", "Domains".bold());
    println!("  created by init:  {}", schema.domains.default.join(", "));
    println!(
        "  present here:     {}",
        if schema.domains.present.is_empty() {
            "(none)".dimmed().to_string()
        } else {
            schema.domains.present.join(", ")
        }
    );

    println!("\n{}", "Layout".bold());
    for dir in schema.layout {
        println!("  {:<12} {}", dir.name.cyan(), dir.purpose);
    }

    println!("\n{}", "Lint Rules".bold());
    for rule in schema.lint_rules {
        let tag = match rule.severity {
            lint::Severity::Error => rule.severity.label().red(),
            lint::Severity::Warning => rule.severity.label().yellow(),
        };
        println!("  {tag:<7} {}", rule.rule.cyan());
        println!("    {}", rule.description);
    }

    println!("\n{}", "`sentinel next` Priority".bold());
    for action in &schema.next_actions {
        println!("  {}. {}", action.priority, action.action.cyan());
        println!("     {}", action.description);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn the_published_fields_are_exactly_the_frontmatter_struct() {
        // `FIELDS` is hand-written and `Frontmatter` is the thing agents
        // actually write. Nothing tied them together: a field added to the
        // struct would be silently absent from `sentinel schema` and from the
        // generated template, so an agent following the published contract
        // would not know it exists — and a field removed from the struct would
        // leave the contract advertising something the parser ignores.
        //
        // Derived from the struct by serialising it, rather than from a second
        // hand-written list.
        let value = serde_json::to_value(crate::core::frontmatter::Frontmatter::default())
            .expect("Frontmatter must serialise");
        let actual: BTreeSet<&str> = value
            .as_object()
            .expect("a struct serialises to an object")
            .keys()
            .map(String::as_str)
            .collect();
        let published: BTreeSet<&str> = FIELDS.iter().map(|f| f.name).collect();

        let unpublished: Vec<_> = actual.difference(&published).collect();
        assert!(
            unpublished.is_empty(),
            "Frontmatter has field(s) {unpublished:?} that `sentinel schema` does \
             not publish — agents cannot know they exist"
        );
        let phantom: Vec<_> = published.difference(&actual).collect();
        assert!(
            phantom.is_empty(),
            "`sentinel schema` publishes field(s) {phantom:?} that Frontmatter \
             does not have — the parser would ignore them"
        );
    }

    #[test]
    fn the_published_persona_contract_matches_the_struct_that_parses_it() {
        // The same round trip `FIELDS` gets, for the same reason: a field on
        // `TraitFrontmatter` that `schema` does not publish is a field no agent
        // knows to write, and a published field the struct lacks is one the
        // parser silently drops.
        let value = serde_json::to_value(persona::TraitFrontmatter::default())
            .expect("TraitFrontmatter must serialise");
        let actual: BTreeSet<&str> = value
            .as_object()
            .expect("a struct serialises to an object")
            .keys()
            .map(String::as_str)
            .collect();
        let published: BTreeSet<&str> = PERSONA_FIELDS.iter().map(|f| f.name).collect();
        assert_eq!(
            actual, published,
            "the persona contract and the struct that parses it disagree"
        );
    }

    #[test]
    fn every_required_persona_field_is_one_the_checker_requires() {
        // `REQUIRED` drives the lint rule; this list drives what an agent is
        // told. Published-as-optional but checked-as-required is a field an
        // agent omits and is then told off for omitting.
        let published: BTreeSet<&str> = PERSONA_FIELDS
            .iter()
            .filter(|f| f.required)
            .map(|f| f.name)
            .collect();
        let checked: BTreeSet<&str> = persona::REQUIRED.iter().copied().collect();
        assert_eq!(published, checked);
    }

    #[test]
    fn the_blank_trait_template_covers_every_published_field() {
        let template = blank_trait();
        for field in PERSONA_FIELDS {
            assert!(
                template.contains(&format!("{}:", field.name)),
                "trait template omits `{}`:\n{template}",
                field.name
            );
        }
    }

    #[test]
    fn persona_enum_fields_publish_the_shared_constants() {
        let by_name = |n: &str| PERSONA_FIELDS.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by_name("kind").values, Some(persona::KINDS));
        assert_eq!(by_name("confidence").values, Some(persona::CONFIDENCES));
        assert_eq!(by_name("status").values, Some(persona::STATUSES));
    }

    #[test]
    fn enum_fields_publish_the_shared_constants() {
        // Not a second copy of the values: the same constants the lint rule and
        // `ingest` validate against.
        let by_name = |n: &str| FIELDS.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by_name("origin").values, Some(frontmatter::ORIGINS));
        assert_eq!(by_name("status").values, Some(frontmatter::STATUSES));
    }

    #[test]
    fn the_blank_template_covers_every_published_field() {
        let template = blank_frontmatter();
        for field in FIELDS {
            assert!(
                template.contains(&format!("{}:", field.name)),
                "template omits `{}`:\n{template}",
                field.name
            );
        }
    }
}
