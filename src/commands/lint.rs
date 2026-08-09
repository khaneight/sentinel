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
    findings: Vec<Finding>,
}

/// Validate the archive. Returns the process exit code.
pub fn run(strict: bool) -> io::Result<i32> {
    let articles = wiki::load_all()?;
    let manifest = Manifest::load()?;
    let findings = lint::analyze(&articles, &manifest);

    let errors = lint::count(&findings, Severity::Error);
    let warnings = lint::count(&findings, Severity::Warning);

    crate::core::log::append("lint", &format!("{errors} error(s), {warnings} warning(s)"))?;

    if output::is_json() {
        output::emit(
            "lint",
            Report {
                errors,
                warnings,
                findings,
            },
        )?;
    } else {
        report_human(&findings, errors, warnings);
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
