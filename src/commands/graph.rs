use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::io;

use colored::Colorize;
use serde::Serialize;

use crate::core::links::{self, LinkGraph, Staleness};
use crate::core::output;
use crate::core::wiki;

#[derive(Serialize)]
struct GraphReport {
    node_count: usize,
    edge_count: usize,
    forward: BTreeMap<String, Vec<String>>,
    backlinks: BTreeMap<String, Vec<String>>,
    orphans: Vec<String>,
    /// Set when the graph disagrees with what is on disk. Every count above
    /// describes the archive as it was at the last `sentinel index`.
    #[serde(skip_serializing_if = "Option::is_none")]
    stale: Option<String>,
    /// Files under wiki/ that could not be read. The topology below was built
    /// without them, so an edge they would have contributed is simply absent.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unreadable: Vec<wiki::Unreadable>,
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
    /// True when no article of this name exists — not merely when the graph has
    /// not heard of it. Those were conflated, so a freshly written article
    /// reported `unknown: true` while sitting on disk with outgoing links.
    unknown: bool,
    /// Whether the saved graph knows this node. False with `unknown: false`
    /// means the article exists but postdates the last `sentinel index`.
    in_graph: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stale: Option<String>,
    /// Files under wiki/ that could not be read. The topology below was built
    /// without them, so an edge they would have contributed is simply absent.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unreadable: Vec<wiki::Unreadable>,
}

#[derive(Serialize)]
struct Neighbour {
    slug: String,
    distance: usize,
}

pub fn run(node: Option<&str>, depth: usize) -> io::Result<()> {
    // `--node ""` canonicalises to the empty slug, which matches nothing and
    // produced a one-node neighbourhood of it — a confident answer about an
    // article nobody asked for. `search` already refuses the same mistake.
    if node.is_some_and(|n| n.trim().is_empty()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "`--node` needs a slug. Omit it entirely for the whole topology, \
             or run `sentinel search <term>` to find the slug you meant.",
        ));
    }

    let graph = LinkGraph::load()?;

    // The graph is a cache that only `index` refreshes, so it can disagree with
    // disk. Reading the articles costs what `search` and `status` already pay,
    // and is what lets this command tell "no such article" apart from "written
    // since the last index" — which it used to report identically.
    let loaded = wiki::load_all()?;
    let stale = links::staleness(&graph, &loaded.articles);

    match node {
        // Canonicalised so `--node "Compile Loop"` finds the same node the
        // wikilinks do.
        Some(node) => neighbourhood(
            &graph,
            &loaded.articles,
            &crate::core::slug::canonical(node),
            depth,
            &stale,
            &loaded.unreadable,
        ),
        None => whole_graph(&graph, &stale, &loaded.unreadable),
    }
}

/// The subgraph within `depth` hops of one article.
///
/// The full topology is the wrong shape for "what surrounds this article": on a
/// 423-article archive `graph --json` is ~65 KB, and it grows with the archive.
/// A neighbourhood answer stays the size of the answer.
fn neighbourhood(
    graph: &LinkGraph,
    articles: &[wiki::LoadedArticle],
    node: &str,
    depth: usize,
    stale: &Staleness,
    unreadable: &[wiki::Unreadable],
) -> io::Result<()> {
    let known = graph.forward.contains_key(node) || graph.backlinks.contains_key(node);
    let on_disk = articles.iter().any(|a| a.canonical_slug() == node);

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
                unknown: !known && !on_disk,
                in_graph: known,
                stale: stale.note(),
                unreadable: unreadable.to_vec(),
            },
        );
    }

    wiki::warn_partial(unreadable, "the topology below was built without them");
    if !known {
        let reason = if on_disk {
            "it exists on disk but postdates the last `sentinel index`"
        } else {
            "no article of that name exists"
        };
        println!(
            "{} '{node}' is not in the link graph — {reason}.",
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

fn whole_graph(
    graph: &LinkGraph,
    stale: &Staleness,
    unreadable: &[wiki::Unreadable],
) -> io::Result<()> {
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
                stale: stale.note(),
                unreadable: unreadable.to_vec(),
            },
        );
    }

    wiki::warn_partial(unreadable, "the topology below was built without them");
    if let Some(note) = stale.note() {
        println!("{} {note}\n", "note:".yellow());
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
