use std::io;

use colored::Colorize;

use crate::core::links::LinkGraph;

pub fn run() -> io::Result<()> {
    let graph = LinkGraph::load()?;

    if graph.forward.is_empty() {
        println!("Link graph is empty. Run `sentinel index` first.");
        return Ok(());
    }

    println!("{}\n", "Forward Links (article → links to)".bold());
    let mut slugs: Vec<&String> = graph.forward.keys().collect();
    slugs.sort();
    for slug in &slugs {
        let links = &graph.forward[*slug];
        if links.is_empty() {
            println!("  {slug} → (none)");
        } else {
            println!("  {} → {}", slug.cyan(), links.join(", "));
        }
    }

    println!("\n{}\n", "Backlinks (article ← linked from)".bold());
    let mut back_slugs: Vec<&String> = graph.backlinks.keys().collect();
    back_slugs.sort();
    for slug in &back_slugs {
        let sources = &graph.backlinks[*slug];
        println!("  {} ← {}", slug.cyan(), sources.join(", "));
    }

    Ok(())
}
