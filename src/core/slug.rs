/// Canonical form of a wikilink target or article slug.
///
/// Wikilinks are written by an LLM in running prose, so the same concept
/// arrives spelled several ways: `[[compile-loop]]` when it is being careful,
/// `[[Compile Loop]]` mid-sentence because that reads naturally, `[[Compile-Loop]]`
/// at the start of one. Matching them byte-for-byte had two consequences, and
/// the second is the serious one:
///
/// 1. Links to articles that exist were reported as broken.
/// 2. Demand for an unwritten concept fragmented across spellings, so
///    `sentinel next` ranked three one-referrer gaps instead of one
///    three-referrer gap — and could recommend writing an article that
///    already existed under a different capitalisation.
///
/// Obsidian resolves links case-insensitively, so this also matches what the
/// user's editor already does.
///
/// Deliberately *not* handled: plurals and stemming. `derived-state` and
/// `derived-states` stay distinct. Merging them needs a stemmer, and a wrong
/// merge silently collapses two real concepts — worse than a missed one.
pub fn canonical(target: &str) -> String {
    let mut out = String::with_capacity(target.len());
    let mut pending_separator = false;

    for ch in target.trim().chars() {
        if ch.is_alphanumeric() {
            if pending_separator && !out.is_empty() {
                out.push('-');
            }
            pending_separator = false;
            out.extend(ch.to_lowercase());
        } else {
            // Any run of non-alphanumerics — spaces, hyphens, underscores,
            // punctuation — collapses to a single separator.
            pending_separator = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_is_ignored() {
        assert_eq!(canonical("Compile-Loop"), "compile-loop");
        assert_eq!(canonical("COMPILE-LOOP"), "compile-loop");
    }

    #[test]
    fn separators_are_equivalent() {
        for spelling in [
            "compile loop",
            "compile-loop",
            "compile_loop",
            "compile  loop",
            "compile--loop",
        ] {
            assert_eq!(canonical(spelling), "compile-loop", "{spelling}");
        }
    }

    #[test]
    fn surrounding_whitespace_and_punctuation_are_dropped() {
        assert_eq!(canonical("  compile-loop.  "), "compile-loop");
        assert_eq!(canonical("-compile-loop-"), "compile-loop");
    }

    #[test]
    fn plurals_stay_distinct() {
        // Merging these would need a stemmer, and a wrong merge silently
        // collapses two real concepts.
        assert_ne!(canonical("derived-state"), canonical("derived-states"));
    }

    #[test]
    fn non_ascii_is_preserved_and_lowercased() {
        assert_eq!(canonical("Ἠθικά"), "ἠθικά");
        assert_eq!(canonical("Être et Néant"), "être-et-néant");
    }

    #[test]
    fn an_empty_or_punctuation_only_target_canonicalises_to_nothing() {
        assert_eq!(canonical(""), "");
        assert_eq!(canonical("   "), "");
        assert_eq!(canonical("---"), "");
    }

    #[test]
    fn already_canonical_input_is_unchanged() {
        assert_eq!(canonical("compile-loop"), "compile-loop");
    }
}
