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
/// Folding is by **NFKC**, which handles three further ways the same word
/// arrives looking different:
///
/// - decomposed vs precomposed accents (`é` as one codepoint or two) — macOS
///   has historically returned decomposed filenames, Linux preserves what was
///   written, and this archive's subject matter is full of Greek and accents
/// - ligatures (`ﬁle` vs `file`) — pervasive in text extracted from PDFs,
///   which this tool ingests
/// - full-width Latin (`ｆｉｌｅ`) — produced by CJK input methods
///
/// Format characters are **dropped rather than treated as separators**. A
/// zero-width space or a soft hyphen inside a word is invisible, and turning
/// it into a `-` produced `fi-le` from something that looked exactly like
/// `file`. Soft hyphens in particular survive copy-paste out of PDFs in bulk.
///
/// Deliberately *not* handled: plurals and stemming, and the Turkish dotted
/// capital `İ`, whose lowercase is `i` plus a combining dot above. Folding that
/// correctly is locale-dependent, and guessing wrong merges two real words. `derived-state` and
/// `derived-states` stay distinct. Merging them needs a stemmer, and a wrong
/// merge silently collapses two real concepts — worse than a missed one.
pub fn canonical(target: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    let mut out = String::with_capacity(target.len());
    let mut pending_separator = false;

    // Normalise before folding, so that text which renders identically
    // canonicalises identically regardless of how it was encoded.
    for ch in target.trim().nfkc() {
        if is_format_character(ch) {
            // Invisible: contributes nothing to identity, and must not become
            // a separator.
            continue;
        }
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

/// Characters that carry no identity: invisible formatting that survives
/// copy-paste and renders as nothing.
///
/// Rust's standard library does not expose `Default_Ignorable_Code_Point`, so
/// this is the practical subset — the ones that actually turn up in pasted
/// prose and in text extracted from PDFs.
fn is_format_character(ch: char) -> bool {
    matches!(
        ch,
        '\u{00ad}'            // soft hyphen
        | '\u{200b}'..='\u{200f}' // zero-width space/joiner/non-joiner, LRM, RLM
        | '\u{2028}'..='\u{202e}' // line/paragraph separator, bidi embedding
        | '\u{2060}'..='\u{2064}' // word joiner, invisible operators
        | '\u{feff}'            // byte-order mark
    )
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
    fn invisible_characters_do_not_split_a_word() {
        // These render as nothing, so `fi<ZWSP>le` looks exactly like `file`.
        // Treating them as separators produced `fi-le` and a broken link
        // against an article whose name looked identical on screen.
        for invisible in ['\u{200b}', '\u{200c}', '\u{200d}', '\u{00ad}', '\u{feff}'] {
            let spelled = format!("fi{invisible}le");
            assert_ne!(spelled, "file", "the test is meaningless otherwise");
            assert_eq!(canonical(&spelled), "file", "U+{:04X}", invisible as u32);
        }
    }

    #[test]
    fn ligatures_fold_to_their_letters() {
        // Text extracted from a PDF is full of these.
        assert_eq!(canonical("\u{fb01}le"), "file");
        assert_eq!(canonical("\u{fb02}ow"), "flow");
    }

    #[test]
    fn full_width_latin_folds_to_ascii() {
        assert_eq!(canonical("\u{ff26}\u{ff49}\u{ff4c}\u{ff45}"), "file");
    }

    #[test]
    fn a_bidi_mark_does_not_change_identity() {
        assert_eq!(canonical("\u{200e}free will"), canonical("free will"));
    }

    #[test]
    fn visible_punctuation_is_still_a_separator() {
        // Dropping format characters must not turn into dropping punctuation.
        assert_eq!(canonical("free will"), "free-will");
        assert_eq!(canonical("free/will"), "free-will");
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
