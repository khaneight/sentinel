//! Publishing source material — per document, never wholesale.
//!
//! `raw/` holds whatever its owner put there: material under someone else's
//! copyright, private notes, correspondence, drafts nobody was meant to see.
//! Nothing about a file tells the tool which of those it is, so the tests here
//! are mostly about what `export` *refuses* to copy.

mod common;

use common::Archive;

fn ingest(a: &Archive, name: &str, body: &str, extra: &[&str]) {
    let f = a.path(name);
    std::fs::write(&f, body).unwrap();
    let mut argv = vec![
        "ingest",
        f.to_str().unwrap(),
        "-d",
        "philosophy",
        "-o",
        "authored",
        "--as",
        name,
    ];
    argv.extend_from_slice(extra);
    assert_eq!(a.code(&argv), 0, "{}", a.run(&argv));
}

fn archive() -> Archive {
    let a = Archive::new();
    ingest(&a, "open.md", "A document I am happy to share.\n", &[]);
    ingest(&a, "private.md", "Something I am not.\n", &[]);
    a.write(
        "wiki/philosophy/essay.md",
        "---\ntitle: Essay\ndomain: philosophy\norigin: authored\ntags: [t]\n\
         status: stable\nsources:\n  - raw/philosophy/open.md\n  \
         - raw/philosophy/private.md\n---\n\nProse citing both.\n",
    );
    a.run(&["index"]);
    a
}

#[test]
fn nothing_is_publishable_until_it_is_said_to_be() {
    let a = archive();
    let v = a.json(&["sources"]);
    assert_eq!(v["count"], 2);
    assert_eq!(
        v["published"], 0,
        "opting in is a decision, not a default:\n{v:#}"
    );
}

#[test]
fn export_copies_only_what_was_opted_in() {
    let a = archive();
    assert_eq!(
        a.code(&["sources", "raw/philosophy/open.md", "--publish"]),
        0
    );

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("site");
    let v = a.json(&[
        "export",
        "--out",
        &dest.display().to_string(),
        "--flat",
        "--with-sources",
    ]);
    assert_eq!(v["sources_published"], 1, "{v:#}");
    assert_eq!(
        v["sources_withheld"], 1,
        "a withheld source must be counted, not silently skipped:\n{v:#}"
    );
    assert!(dest.join("sources/philosophy/open.md").exists());
    assert!(
        !dest.join("sources/philosophy/private.md").exists(),
        "a document nobody opted in must never reach the site"
    );
}

#[test]
fn without_the_flag_no_source_is_copied_even_when_opted_in() {
    // Opting in says "this *may* be published", not "publish it". The two
    // decisions are separate so that marking a document safe is not the same
    // act as putting it on the internet.
    let a = archive();
    a.run(&["sources", "raw/philosophy/open.md", "--publish"]);

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("site");
    let v = a.json(&["export", "--out", &dest.display().to_string(), "--flat"]);
    assert_eq!(v["sources_published"], 0, "{v:#}");
    assert!(!dest.join("sources").exists());
}

#[test]
fn a_published_article_links_only_to_sources_a_reader_can_open() {
    // The export's premise is that its output has no dead ends. A link naming
    // a withheld document is the worst kind, because it names the file and
    // then denies it.
    let a = archive();
    a.run(&["sources", "raw/philosophy/open.md", "--publish"]);

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("site");
    a.run(&[
        "export",
        "--out",
        &dest.display().to_string(),
        "--flat",
        "--with-sources",
    ]);
    let text = std::fs::read_to_string(dest.join("essay.md")).unwrap();
    assert!(text.contains("## Sources"), "{text}");
    assert!(text.contains("sources/philosophy/open.md"), "{text}");
    assert!(
        !text.contains("private"),
        "a withheld source must not be named at all:\n{text}"
    );
}

#[test]
fn withdrawing_a_source_removes_it_on_the_next_clean_export() {
    // Un-publishing has to actually reach the site. A file left behind in the
    // destination is still readable, and the likeliest reason to withdraw
    // something is that it should not be public.
    let a = archive();
    a.run(&["sources", "raw/philosophy/open.md", "--publish"]);
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("site");
    let argv = [
        "export".to_string(),
        "--out".to_string(),
        dest.display().to_string(),
        "--flat".to_string(),
        "--with-sources".to_string(),
    ];
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    a.run(&args);
    assert!(dest.join("sources/philosophy/open.md").exists());

    assert_eq!(
        a.code(&["sources", "raw/philosophy/open.md", "--private"]),
        0
    );
    let mut cleaned = args.to_vec();
    cleaned.push("--clean");
    a.run(&cleaned);
    assert!(
        !dest.join("sources/philosophy/open.md").exists(),
        "a withdrawn source is still on the site"
    );
}

#[test]
fn a_source_is_named_the_way_an_article_cites_it() {
    // Through the same matcher `sources:` uses, so a bare filename means here
    // what it means in an article.
    let a = archive();
    assert_eq!(a.code(&["sources", "open.md", "--publish"]), 0);
    let v = a.json(&["sources"]);
    let entry = v["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["raw_path"] == "raw/philosophy/open.md")
        .unwrap();
    assert_eq!(entry["publish"], true);
}

#[test]
fn a_target_with_no_decision_is_an_error() {
    let a = archive();
    let out = a.output(&["sources", "open.md"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(common::stderr(&out).contains("--publish"));
}

#[test]
fn an_unknown_target_says_where_to_look() {
    let a = archive();
    let out = a.output(&["sources", "nothing-like-this.md", "--publish"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(common::stderr(&out).contains("sentinel sources"));
}

#[test]
fn ingest_can_opt_a_document_in_as_it_arrives() {
    let a = Archive::new();
    ingest(&a, "shared.md", "Public from the start.\n", &["--publish"]);
    let v = a.json(&["sources"]);
    assert_eq!(v["published"], 1, "{v:#}");
}

#[test]
fn the_decision_survives_a_rename() {
    // `mv` carries the manifest entry across. If it did not, moving a file
    // would silently un-publish it — or worse, a later `sync` would re-register
    // it with the default and the owner would think it was still shared.
    let a = archive();
    a.run(&["sources", "raw/philosophy/open.md", "--publish"]);
    assert_eq!(
        a.code(&["mv", "raw/philosophy/open.md", "raw/philosophy/renamed.md"]),
        0
    );
    let v = a.json(&["sources"]);
    let entry = v["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["raw_path"] == "raw/philosophy/renamed.md")
        .expect("the entry moved with the file");
    assert_eq!(entry["publish"], true);
}

#[test]
fn a_manifest_written_before_the_field_existed_still_loads() {
    // Nothing rewrites old manifests wholesale, so every entry ever written has
    // to keep parsing — and must default to *not* published.
    let a = archive();
    let text = a
        .read("meta/manifest.json")
        .replace(",\"publish\":true", "");
    a.write("meta/manifest.json", &text);
    let v = a.json(&["sources"]);
    assert_eq!(v["count"], 2);
    assert_eq!(v["published"], 0);
}

#[test]
fn a_withheld_document_is_not_named_in_the_published_frontmatter() {
    // `sources:` lists paths under `raw/`, and a filename can be the private
    // part — "therapy-notes-2019.md" says plenty without being opened. The
    // published copy names only what a reader can actually reach.
    let a = archive();
    a.run(&["sources", "raw/philosophy/open.md", "--publish"]);

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("site");
    a.run(&[
        "export",
        "--out",
        &dest.display().to_string(),
        "--flat",
        "--with-sources",
    ]);
    let text = std::fs::read_to_string(dest.join("essay.md")).unwrap();
    assert!(
        !text.contains("raw/"),
        "no archive-internal path should reach the site:\n{text}"
    );
    assert!(
        !text.contains("private.md"),
        "a withheld document must not be named:\n{text}"
    );
    assert!(text.contains("sources/philosophy/open.md"), "{text}");
}

#[test]
fn an_ordinary_export_names_no_source_paths_at_all() {
    // Without `--with-sources` nothing is reachable, so the field claims
    // nothing rather than listing files a reader cannot open.
    let a = archive();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("site");
    a.run(&["export", "--out", &dest.display().to_string(), "--flat"]);
    let text = std::fs::read_to_string(dest.join("essay.md")).unwrap();
    assert!(!text.contains("raw/"), "{text}");
    assert!(!text.contains("sources:"), "{text}");
    assert!(
        text.contains("title: Essay") && text.contains("Prose citing both."),
        "the rest of the article must survive:\n{text}"
    );
}

// --- the bundle's provenance layers ----------------------------------------

#[test]
fn published_sources_are_nodes_at_the_core() {
    // The showcase draws depth as distance from the author's hand. A source
    // document is layer 0 because it is the thing they actually wrote; if it
    // were absent the picture would open with the archive's *reading* of them
    // at the centre.
    let a = archive();
    a.run(&["sources", "raw/philosophy/open.md", "--publish"]);

    let out = tempfile::tempdir().unwrap();
    let ui = out.path().join("ui");
    a.run(&[
        "export",
        "--out",
        &out.path().join("site").display().to_string(),
        "--flat",
        "--with-sources",
        "--ui",
        &ui.display().to_string(),
    ]);
    let bundle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(ui.join("bundle.json")).unwrap()).unwrap();

    let node = bundle["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == "source")
        .expect("a published source is in the graph");
    assert_eq!(node["layer"], 0);
    assert_eq!(node["slug"], "src:philosophy/open");
    assert!(
        node["body"].as_str().unwrap().contains("happy to share"),
        "the source's own text travels with it so it can be read:\n{node:#}"
    );

    // And the citing article points at it.
    let cited = bundle["edges"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["from"] == "essay" && e["to"] == "src:philosophy/open");
    assert!(
        cited,
        "the citation should be an edge:\n{:#}",
        bundle["edges"]
    );
}

#[test]
fn a_withheld_source_is_not_a_node_either() {
    // The graph is published output like everything else. A node for a document
    // nobody opted in would put its title and domain on the site.
    let a = archive();
    a.run(&["sources", "raw/philosophy/open.md", "--publish"]);
    let out = tempfile::tempdir().unwrap();
    let ui = out.path().join("ui");
    a.run(&[
        "export",
        "--out",
        &out.path().join("site").display().to_string(),
        "--flat",
        "--with-sources",
        "--ui",
        &ui.display().to_string(),
    ]);
    let text = std::fs::read_to_string(ui.join("bundle.json")).unwrap();
    assert!(
        !text.contains("private"),
        "a withheld source leaked into the bundle"
    );
}

#[test]
fn without_the_flag_the_graph_holds_only_articles() {
    let a = archive();
    a.run(&["sources", "raw/philosophy/open.md", "--publish"]);
    let out = tempfile::tempdir().unwrap();
    let ui = out.path().join("ui");
    a.run(&[
        "export",
        "--out",
        &out.path().join("site").display().to_string(),
        "--flat",
        "--ui",
        &ui.display().to_string(),
    ]);
    let bundle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(ui.join("bundle.json")).unwrap()).unwrap();
    assert!(
        bundle["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["kind"] == "article"),
        "sources appear only when they were asked for"
    );
}
