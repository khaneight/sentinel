use std::collections::BTreeMap;
use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::lint::{self, Finding, Severity};
use crate::core::manifest::Manifest;
use crate::core::output;
use crate::core::wiki;

#[derive(Serialize)]
struct Report {
    errors: usize,
    warnings: usize,
    /// Findings grouped by rule. Always present — on a real archive this is the
    /// part worth reading first, because one root cause produces many findings.
    by_rule: BTreeMap<&'static str, RuleCount>,
    /// Omitted entirely in summary mode. At 423 articles the full list was
    /// ~50 KB of JSON, which is not a reasonable thing to hand an agent by
    /// default when it usually wants the shape of the problem, not every case.
    #[serde(skip_serializing_if = "Option::is_none")]
    findings: Option<Vec<Finding>>,
    /// Files that could not be read and were therefore not linted. A clean
    /// result over a partial view is not a clean archive.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unreadable: Vec<wiki::Unreadable>,
}

#[derive(Serialize)]
struct RuleCount {
    severity: Severity,
    count: usize,
}

/// Validate the archive. Returns the process exit code.
pub fn run(strict: bool, summary: bool, rule_filter: Option<&str>) -> io::Result<i32> {
    let loaded = wiki::load_all()?;
    let articles = &loaded.articles;
    let manifest = Manifest::load()?;
    let all = lint::analyze(articles, &manifest, &crate::core::paths::archive_root());

    // Counts always describe the whole archive; a filter narrows what is
    // listed, never what is counted, so `--rule` cannot make a broken archive
    // look healthy or change the exit code.
    let mut by_rule: BTreeMap<&'static str, RuleCount> = BTreeMap::new();
    for finding in &all {
        let entry = by_rule.entry(finding.rule).or_insert(RuleCount {
            severity: finding.severity,
            count: 0,
        });
        entry.count += 1;
    }

    // An unknown rule must not read as a clean one. `--rule brokenlink` for
    // `broken-link` returned zero findings and exit 0, which is indistinguishable
    // from the rule having nothing to report — and `/sentinel-improve` tells an
    // agent to work the rules one at a time, so a typo silently reports clean.
    // The sibling flag `next --action` already validates; this now matches.
    if let Some(rule) = rule_filter
        && !lint::RULES.iter().any(|r| r.rule == rule)
    {
        let known = lint::RULES
            .iter()
            .map(|r| r.rule)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Unknown lint rule '{rule}'. Expected one of: {known}.\n\
                 Run `sentinel schema` for what each rule checks."
            ),
        ));
    }

    let findings: Vec<Finding> = match rule_filter {
        Some(rule) => all.iter().filter(|f| f.rule == rule).cloned().collect(),
        None => all.clone(),
    };

    let errors = lint::count(&all, Severity::Error);
    let warnings = lint::count(&all, Severity::Warning);

    // `lint` deliberately does not touch the activity log. It is a query, and
    // the archive lives in git: a validation command that dirties the working
    // tree cannot be run to check whether the tree is clean. It also runs every
    // iteration of `/sentinel-grow`, so logging it buried the entries that
    // record actual changes under "0 error(s), 0 warning(s)".

    if output::is_json() {
        output::emit(
            "lint",
            Report {
                errors,
                warnings,
                by_rule,
                findings: (!summary).then_some(findings),
                unreadable: loaded.unreadable.clone(),
            },
        )?;
    } else if summary {
        report_summary(&by_rule, errors, warnings);
    } else {
        report_human(&findings, errors, warnings);
    }

    if !loaded.unreadable.is_empty() && !output::is_json() {
        println!(
            "\n{} {} wiki file(s) could not be read and were not linted:",
            "!".red(),
            loaded.unreadable.len()
        );
        for u in &loaded.unreadable {
            println!("    {} — {}", u.path, u.error);
        }
    }

    // Exit non-zero only for things that are actually wrong. An archive with
    // uncompiled sources and forward-declared wikilinks is healthy, and a lint
    // that fails on it would be one nobody could gate on.
    let failing = if strict { errors + warnings } else { errors };
    Ok(if failing > 0 {
        output::EXIT_FINDINGS
    } else {
        0
    })
}

fn report_summary(by_rule: &BTreeMap<&'static str, RuleCount>, errors: usize, warnings: usize) {
    if by_rule.is_empty() {
        println!("{}", "No issues found.".green());
        return;
    }
    println!(
        "{} error(s), {} warning(s) across {} rule(s):\n",
        errors.to_string().red(),
        warnings.to_string().yellow(),
        by_rule.len()
    );
    for (rule, info) in by_rule {
        let tag = match info.severity {
            Severity::Error => info.severity.label().red(),
            Severity::Warning => info.severity.label().yellow(),
        };
        println!("  {tag:<7} {:>5}  {}", info.count, rule.cyan());
    }
    println!(
        "\n{}",
        "Use --rule <id> to list one rule's findings.".dimmed()
    );
}

fn report_human(findings: &[Finding], errors: usize, warnings: usize) {
    if findings.is_empty() {
        println!("{}", "No issues found.".green());
        return;
    }

    println!(
        "{} error(s), {} warning(s):\n",
        errors.to_string().red(),
        warnings.to_string().yellow()
    );

    for finding in findings {
        let tag = match finding.severity {
            Severity::Error => finding.severity.label().red(),
            Severity::Warning => finding.severity.label().yellow(),
        };
        let location = finding
            .path
            .as_deref()
            .map(|p| format!("{}: ", p.cyan()))
            .unwrap_or_default();
        println!(
            "  {tag} {}{}  {}",
            location,
            finding.message,
            format!("[{}]", finding.rule).dimmed()
        );
    }
}
