//! Verdicts: what the archive's owner said about a claim or a piece of work.
//!
//! The clone writes things about a person and, eventually, as them. Nothing it
//! produces is publishable on the strength of the tool being satisfied with it,
//! so there has to be somewhere the person's answer lives — and it has to be
//! somewhere that survives every rebuild, every re-index, and every git clone.
//!
//! That place is the document's own frontmatter. A verdict recorded in `meta/`
//! is a verdict that comes apart from the thing it was about the first time a
//! file is renamed by hand.
//!
//! Entries append. The history is the point: `changes-requested`, then a note,
//! then `approved` three weeks later is the useful record, and a single
//! mutable field would have thrown away everything except the last word.

use std::io;

use serde::{Deserialize, Serialize};

use super::frontmatter;

/// Everything that can be recorded about a piece of work.
pub const VERDICTS: &[&str] = &["approved", "rejected", "changes-requested", "comment"];

/// Verdicts that decide standing.
///
/// `comment` is deliberately not one. A remark left on an approved article
/// should not un-approve it, and a reviewer who wants to say something without
/// changing anything needs a way to.
pub const DECISIONS: &[&str] = &["approved", "rejected", "changes-requested"];

/// One recorded verdict.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub verdict: String,
    /// Who said so. Never defaulted to a placeholder — a verdict attributed to
    /// nobody is the one thing this whole mechanism exists to prevent.
    pub by: String,
    /// YYYY-MM-DD, like every other date in the archive.
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The operative verdict: the most recent entry that decided anything.
pub fn standing(entries: &[Entry]) -> Option<&Entry> {
    entries
        .iter()
        .rev()
        .find(|e| DECISIONS.contains(&e.verdict.as_str()))
}

/// Whether the archive's owner has signed this off.
///
/// The gate `export` reads. Absence of a verdict is *not* approval: work
/// nobody has looked at and work somebody refused both come back false.
pub fn is_approved(entries: &[Entry]) -> bool {
    standing(entries).is_some_and(|e| e.verdict == "approved")
}

/// The trait `status` a decision implies.
///
/// Persona traits carry both: `status:` is what a person reads at the top of
/// the file, and the review list is the history behind it. They are written
/// together and `lint` asserts they agree, rather than one being silently
/// derived from the other at read time — a file whose visible `status` says
/// `affirmed` while its history says otherwise is worth reporting, not
/// papering over.
pub fn implied_status(verdict: &str) -> Option<&'static str> {
    match verdict {
        "approved" => Some("affirmed"),
        "rejected" => Some("rejected"),
        "changes-requested" => Some("proposed"),
        _ => None,
    }
}

/// Append a verdict to a document's frontmatter, returning the new content.
///
/// Textual, like `sentinel mv`'s citation rewriting and for the same reasons:
/// the block is a file the user also opens by hand, and round-tripping it
/// through serde would reorder its keys and strip its comments to add four
/// lines. The entry itself *is* serialised, so quoting and escaping in a note
/// are YAML's problem rather than this function's.
pub fn append(content: &str, entry: &Entry) -> io::Result<String> {
    let Some((start, end)) = frontmatter::block_span(content) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no frontmatter block to record a verdict in. A document with no \
             frontmatter cannot carry one; add the block first.",
        ));
    };

    let yaml = &content[start..end];
    let item = as_list_item(entry)?;

    let updated = match find_review_key(yaml) {
        Some(Key::Empty(line_start, line_end)) => {
            // `review: []` or `review:` with nothing under it. Replace the
            // whole line so an inline empty list becomes a block one.
            let mut out = String::with_capacity(yaml.len() + item.len() + 8);
            out.push_str(&yaml[..line_start]);
            out.push_str("review:\n");
            out.push_str(&item);
            out.push_str(&yaml[line_end..]);
            out
        }
        Some(Key::Block(insert_at)) => {
            let mut out = String::with_capacity(yaml.len() + item.len());
            out.push_str(&yaml[..insert_at]);
            out.push_str(&item);
            out.push_str(&yaml[insert_at..]);
            out
        }
        Some(Key::Inline(line)) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "`review:` is written as an inline list ({line}), which this \
                     cannot append to without rewriting the line and losing \
                     whatever else is on it. Reformat it as a block list — one \
                     `- verdict: ...` per line — and run this again."
                ),
            ));
        }
        None => {
            let mut out = String::with_capacity(yaml.len() + item.len() + 8);
            out.push_str(yaml);
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("review:\n");
            out.push_str(&item);
            out
        }
    };

    Ok(format!("{}{updated}{}", &content[..start], &content[end..]))
}

/// One YAML list item, indented, from a serialised entry.
fn as_list_item(entry: &Entry) -> io::Result<String> {
    let yaml = serde_yaml::to_string(entry)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let mut out = String::new();
    for (i, line) in yaml.lines().enumerate() {
        if i == 0 {
            out.push_str("  - ");
        } else {
            out.push_str("    ");
        }
        out.push_str(line);
        out.push('\n');
    }
    Ok(out)
}

enum Key {
    /// `review:` with no items under it, as a byte range covering its line.
    Empty(usize, usize),
    /// A block list; the offset to insert a new item at.
    Block(usize),
    /// `review: [something]`, which we will not rewrite.
    Inline(String),
}

/// Locate the top-level `review:` key and decide how to add to it.
fn find_review_key(yaml: &str) -> Option<Key> {
    let mut offset = 0usize;
    for line in yaml.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        // Top level only: an indented `review:` belongs to something else.
        if let Some(rest) = trimmed.strip_prefix("review:")
            && !trimmed.starts_with(char::is_whitespace)
        {
            let rest = rest.trim();
            if !rest.is_empty() && rest != "[]" {
                return Some(Key::Inline(trimmed.to_string()));
            }
            let line_end = offset + line.len();
            // Walk the items beneath it. A blank line inside a block list is
            // legal YAML, so it does not end the list — only a line that
            // starts a new top-level key does.
            let mut cursor = line_end;
            let mut last_item_end = None;
            for next in yaml[line_end..].split_inclusive('\n') {
                let t = next.trim_end_matches(['\n', '\r']);
                if t.trim().is_empty() {
                    cursor += next.len();
                    continue;
                }
                if !t.starts_with(char::is_whitespace) {
                    break;
                }
                cursor += next.len();
                last_item_end = Some(cursor);
            }
            return Some(match last_item_end {
                Some(end) => Key::Block(end),
                None => Key::Empty(offset, line_end),
            });
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(verdict: &str) -> Entry {
        Entry {
            verdict: verdict.to_string(),
            by: "khaneight".into(),
            at: "2026-08-25".into(),
            note: None,
        }
    }

    fn parsed(content: &str) -> Vec<Entry> {
        #[derive(Deserialize, Default)]
        struct Doc {
            #[serde(default)]
            review: Vec<Entry>,
        }
        frontmatter::parse_as::<Doc>(content).frontmatter.review
    }

    #[test]
    fn a_first_verdict_creates_the_field() {
        let out = append("---\ntitle: T\n---\n\nBody.\n", &entry("approved")).unwrap();
        assert_eq!(parsed(&out), vec![entry("approved")]);
        assert!(out.ends_with("\nBody.\n"), "the body must survive:\n{out}");
    }

    #[test]
    fn a_second_verdict_appends_rather_than_replacing() {
        // The history is the point. A mutable field would keep only the last
        // word, and "changes requested, then approved" is the useful record.
        let first = append("---\ntitle: T\n---\n\nBody.\n", &entry("changes-requested")).unwrap();
        let second = append(&first, &entry("approved")).unwrap();
        let entries = parsed(&second);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].verdict, "changes-requested");
        assert_eq!(standing(&entries).unwrap().verdict, "approved");
    }

    #[test]
    fn an_empty_inline_list_becomes_a_block_one() {
        let out = append("---\ntitle: T\nreview: []\n---\n\nB.\n", &entry("approved")).unwrap();
        assert_eq!(parsed(&out), vec![entry("approved")]);
    }

    #[test]
    fn a_key_that_follows_the_list_is_not_swallowed() {
        // The insertion point is the end of the list, not the end of the file.
        let start = "---\ntitle: T\nreview:\n  - verdict: comment\n    by: a\n    at: 2026-01-01\nstatus: draft\n---\n\nB.\n";
        let out = append(start, &entry("approved")).unwrap();
        assert!(
            out.contains("status: draft"),
            "a later key was lost:\n{out}"
        );
        assert_eq!(parsed(&out).len(), 2);
    }

    #[test]
    fn other_fields_and_their_comments_survive() {
        // Why this is textual rather than a serde round trip.
        let start = "---\n# who wrote it\ntitle: T\ntags: [a, b]\n---\n\nB.\n";
        let out = append(start, &entry("approved")).unwrap();
        assert!(out.contains("# who wrote it"), "{out}");
        assert!(out.contains("tags: [a, b]"), "{out}");
    }

    #[test]
    fn an_indented_review_key_is_not_mistaken_for_the_top_level_one() {
        let start = "---\ntitle: T\nnested:\n  review: something\n---\n\nB.\n";
        let out = append(start, &entry("approved")).unwrap();
        assert!(out.contains("  review: something"), "{out}");
        assert_eq!(parsed(&out), vec![entry("approved")]);
    }

    #[test]
    fn a_populated_inline_list_is_refused_rather_than_mangled() {
        let start =
            "---\ntitle: T\nreview: [{verdict: comment, by: a, at: 2026-01-01}]\n---\n\nB.\n";
        let err = append(start, &entry("approved")).unwrap_err();
        assert!(
            err.to_string().contains("block list"),
            "the refusal should say what to do: {err}"
        );
    }

    #[test]
    fn a_document_with_no_frontmatter_is_refused() {
        let err = append("Just prose.\n", &entry("approved")).unwrap_err();
        assert!(err.to_string().contains("no frontmatter"), "{err}");
    }

    #[test]
    fn a_comment_does_not_change_where_a_document_stands() {
        let entries = vec![entry("approved"), entry("comment")];
        assert_eq!(
            standing(&entries).unwrap().verdict,
            "approved",
            "a remark left on approved work must not un-approve it"
        );
        assert!(is_approved(&entries));
        assert!(
            !is_approved(&[]),
            "work nobody has looked at is not approved work"
        );
        assert!(!is_approved(&[entry("rejected")]));
    }

    #[test]
    fn every_verdict_that_decides_something_implies_a_trait_status() {
        // Derived from the two lists rather than checked case by case: a
        // verdict added to `DECISIONS` without a mapping would leave `lint`
        // unable to say what `status:` should agree with.
        for verdict in DECISIONS {
            assert!(
                implied_status(verdict).is_some(),
                "`{verdict}` decides standing but implies no trait status"
            );
            assert!(VERDICTS.contains(verdict), "`{verdict}` is not a verdict");
        }
        assert_eq!(implied_status("comment"), None);
        assert!(
            DECISIONS.len() < VERDICTS.len(),
            "if every verdict decides, `comment` does nothing"
        );
    }

    #[test]
    fn a_note_with_awkward_characters_survives_the_round_trip() {
        // Serialised rather than formatted by hand, so quoting is YAML's job.
        let e = Entry {
            note: Some("not what I think: \"at all\"\nand a second line".into()),
            ..entry("rejected")
        };
        let out = append("---\ntitle: T\n---\n\nB.\n", &e).unwrap();
        assert_eq!(parsed(&out), vec![e]);
    }
}
