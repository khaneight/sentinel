use std::io;

use colored::Colorize;

use crate::core::compilation::Compilation;
use crate::core::manifest::Manifest;
use crate::core::wiki;

pub fn run() -> io::Result<()> {
    let manifest = Manifest::load()?;
    // Derived from the wiki on every call rather than read from the manifest,
    // so the answer is right whether or not `sentinel index` has been run.
    let articles = wiki::load_all().unwrap_or_default();
    let compilation = Compilation::derive(&articles, &manifest);
    let uncompiled = compilation.uncompiled(&manifest);

    if uncompiled.is_empty() {
        println!("{}", "All raw documents have been compiled.".green());
        return Ok(());
    }

    println!(
        "{} uncompiled raw document(s):\n",
        uncompiled.len().to_string().yellow()
    );

    // Group by domain
    let mut by_domain: std::collections::BTreeMap<&str, Vec<_>> = std::collections::BTreeMap::new();
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
