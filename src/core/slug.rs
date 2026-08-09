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
/// Also folds Unicode normalisation forms: `é` written as one codepoint and as
/// `e` plus a combining acute canonicalise the same. macOS has historically
/// returned decomposed filenames while Linux preserves whatever was written,
/// and this archive's subject matter is full of Greek and accented terms.
///
/// Deliberately *not* handled: plurals and stemming. `derived-state` and
/// `derived-states` stay distinct. Merging them needs a stemmer, and a wrong
/// merge silently collapses two real concepts — worse than a missed one.
pub fn canonical(target: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    let mut out = String::with_capacity(target.len());
    let mut pending_separator = false;

    // Normalise before folding. The same accented character can be one
    // codepoint or a base plus a combining mark, and the two render
    // identically everywhere a human would look — so a link written one way
    // against a filename stored the other way reported as broken against an
    // article sitting right there, with nothing on screen to show why.
    for ch in target.trim().nfc() {
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
    fn normalisation_forms_fold_together() {
        // These two strings are visually identical and byte-different: one
        // codepoint for the accented vowel, versus a base plus combining mark.
        let precomposed = "\u{e9}tude";
        let decomposed = "e\u{301}tude";
        assert_ne!(precomposed, decomposed, "the test is meaningless otherwise");
        assert_eq!(canonical(precomposed), canonical(decomposed));
    }

    #[test]
    fn greek_normalisation_forms_fold_together() {
        // The corpus this was built against is full of these.
        let precomposed = "\u{1f28}\u{3b8}\u{3b9}\u{3ba}\u{3ac}";
        let decomposed = "\u{397}\u{313}\u{3b8}\u{3b9}\u{3ba}\u{3b1}\u{301}";
        assert_ne!(precomposed, decomposed);
        assert_eq!(canonical(precomposed), canonical(decomposed));
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
