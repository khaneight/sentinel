use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
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

#[derive(Serialize)]
struct Neighbourhood {
    node: String,
    depth: usize,
    /// Every slug reachable within `depth` hops, in either direction.
    node_count: usize,
    /// Distance from `node` to each reachable slug.
    nodes: Vec<Neighbour>,
    /// Edges among those nodes only.
    forward: BTreeMap<String, Vec<String>>,
    backlinks: BTreeMap<String, Vec<String>>,
    /// True when `node` is not in the graph at all.
    unknown: bool,
}

#[derive(Serialize)]
struct Neighbour {
    slug: String,
    distance: usize,
}

pub fn run(node: Option<&str>, depth: usize) -> io::Result<()> {
    let graph = LinkGraph::load()?;

    match node {
        // Canonicalised so `--node "Compile Loop"` finds the same node the
        // wikilinks do.
        Some(node) => neighbourhood(&graph, &crate::core::slug::canonical(node), depth),
        None => whole_graph(&graph),
    }
}

/// The subgraph within `depth` hops of one article.
///
/// The full topology is the wrong shape for "what surrounds this article": on a
/// 423-article archive `graph --json` is ~65 KB, and it grows with the archive.
/// A neighbourhood answer stays the size of the answer.
fn neighbourhood(graph: &LinkGraph, node: &str, depth: usize) -> io::Result<()> {
    let known = graph.forward.contains_key(node) || graph.backlinks.contains_key(node);

    // BFS over both directions — "what surrounds this" means what it links to
    // and what links to it.
    let mut distances: BTreeMap<String, usize> = BTreeMap::new();
    distances.insert(node.to_string(), 0);
    let mut queue = VecDeque::from([(node.to_string(), 0usize)]);

    while let Some((current, d)) = queue.pop_front() {
        if d >= depth {
            continue;
        }
        let neighbours = graph
            .forward
            .get(&current)
            .into_iter()
            .flatten()
            .chain(graph.backlinks.get(&current).into_iter().flatten());
        for next in neighbours {
            if !distances.contains_key(next) {
                distances.insert(next.clone(), d + 1);
                queue.push_back((next.clone(), d + 1));
            }
        }
    }

    let included: BTreeSet<&str> = distances.keys().map(String::as_str).collect();
    let restrict =
        |map: &std::collections::HashMap<String, Vec<String>>| -> BTreeMap<String, Vec<String>> {
            map.iter()
                .filter(|(k, _)| included.contains(k.as_str()))
                .map(|(k, v)| {
                    let mut kept: Vec<String> = v
                        .iter()
                        .filter(|t| included.contains(t.as_str()))
                        .cloned()
                        .collect();
                    kept.sort();
                    (k.clone(), kept)
                })
                .filter(|(_, v)| !v.is_empty())
                .collect()
        };

    let forward = restrict(&graph.forward);
    let backlinks = restrict(&graph.backlinks);

    if output::is_json() {
        let mut nodes: Vec<Neighbour> = distances
            .iter()
            .map(|(slug, distance)| Neighbour {
                slug: slug.clone(),
                distance: *distance,
            })
            .collect();
        nodes.sort_by(|a, b| {
            a.distance
                .cmp(&b.distance)
                .then_with(|| a.slug.cmp(&b.slug))
        });

        return output::emit(
            "graph",
            Neighbourhood {
                node: node.to_string(),
                depth,
                node_count: nodes.len(),
                nodes,
                forward,
                backlinks,
                unknown: !known,
            },
        );
    }

    if !known {
        println!(
            "{} '{node}' is not in the link graph. It may be unwritten, or `sentinel index` may be stale.",
            "note:".yellow()
        );
    }

    println!(
        "{} (depth {depth}, {} node(s))\n",
        format!("Neighbourhood of {node}").bold(),
        distances.len()
    );
    let mut by_distance: BTreeMap<usize, Vec<&String>> = BTreeMap::new();
    for (slug, d) in &distances {
        by_distance.entry(*d).or_default().push(slug);
    }
    for (d, slugs) in &by_distance {
        let label = if *d == 0 { "self" } else { "hops" };
        println!(
            "  {d} {label}: {}",
            slugs
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    println!("\n{}", "Links within the neighbourhood".bold());
    for (slug, targets) in &forward {
        println!("  {} → {}", slug.cyan(), targets.join(", "));
    }

    Ok(())
}

fn whole_graph(graph: &LinkGraph) -> io::Result<()> {
    if output::is_json() {
        let forward = sorted(&graph.forward);
        let backlinks = sorted(&graph.backlinks);
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
