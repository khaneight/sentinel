use std::io;

use colored::Colorize;

use crate::core::manifest::Manifest;

pub fn run() -> io::Result<()> {
    let manifest = Manifest::load()?;
    let uncompiled = manifest.uncompiled();

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
