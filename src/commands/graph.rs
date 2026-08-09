use std::collections::{BTreeMap, HashSet};
use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::links::LinkGraph;
use crate::core::output;

#[derive(Serialize)]
struct GraphReport {
    node_count: usize,
    edge_count: usize,
    forward: BTreeMap<String, Vec<String>>,
    backlinks: BTreeMap<String, Vec<String>>,
    orphans: Vec<String>,
}

pub fn run() -> io::Result<()> {
    let graph = LinkGraph::load()?;

    if output::is_json() {
        let forward: BTreeMap<String, Vec<String>> = sorted(&graph.forward);
        let backlinks: BTreeMap<String, Vec<String>> = sorted(&graph.backlinks);
        let all_slugs: HashSet<String> = forward.keys().cloned().collect();
        let mut orphans = graph.orphans(&all_slugs);
        orphans.sort();

        return output::emit(
            "graph",
            GraphReport {
                node_count: forward.len(),
                edge_count: forward.values().map(Vec::len).sum(),
                forward,
                backlinks,
                orphans,
            },
        );
    }

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

/// Sorted keys and sorted values, so JSON output is byte-stable between runs.
fn sorted(map: &std::collections::HashMap<String, Vec<String>>) -> BTreeMap<String, Vec<String>> {
    map.iter()
        .map(|(k, v)| {
            let mut v = v.clone();
            v.sort();
            (k.clone(), v)
        })
        .collect()
}
