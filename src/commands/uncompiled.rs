use std::collections::BTreeMap;
use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::compilation::Compilation;
use crate::core::manifest::Manifest;
use crate::core::output;
use crate::core::wiki;

#[derive(Serialize)]
struct Document {
    raw_path: String,
    title: String,
    domain: String,
    origin: String,
    source_type: String,
    ingested_at: String,
}

#[derive(Serialize)]
struct Queue {
    count: usize,
    documents: Vec<Document>,
}

pub fn run() -> io::Result<()> {
    let manifest = Manifest::load()?;
    // Derived from the wiki on every call rather than read from the manifest,
    // so the answer is right whether or not `sentinel index` has been run.
    let articles = wiki::load_all().unwrap_or_default().articles;
    let compilation = Compilation::derive(&articles, &manifest);
    let uncompiled = compilation.uncompiled(&manifest);

    if output::is_json() {
        let documents = uncompiled
            .iter()
            .map(|e| Document {
                raw_path: e.raw_path.clone(),
                title: e.title.clone(),
                domain: e.domain.clone(),
                origin: e.origin.clone(),
                source_type: e.source_type.clone(),
                ingested_at: e.ingested_at.clone(),
            })
            .collect::<Vec<_>>();
        return output::emit(
            "uncompiled",
            Queue {
                count: documents.len(),
                documents,
            },
        );
    }

    if uncompiled.is_empty() {
        println!("{}", "All raw documents have been compiled.".green());
        return Ok(());
    }

    println!(
        "{} uncompiled raw document(s):\n",
        uncompiled.len().to_string().yellow()
    );

    let mut by_domain: BTreeMap<&str, Vec<_>> = BTreeMap::new();
    for entry in &uncompiled {
        by_domain.entry(&entry.domain).or_default().push(entry);
    }

    for (domain, entries) in &by_domain {
        println!("  {}:", domain.bold());
        for entry in entries {
            let origin_tag = match entry.origin.as_str() {
                "authored" => "[authored]".cyan(),
                "researched" => "[researched]".magenta(),
                _ => format!("[{}]", entry.origin).normal(),
            };
            println!("    {} {} — {}", origin_tag, entry.raw_path, entry.title);
        }
        println!();
    }

    Ok(())
}
