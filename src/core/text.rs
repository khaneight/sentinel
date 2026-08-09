/// Truncate to at most `max` characters, appending an ellipsis when cut.
///
/// Counts characters, not bytes. Byte-slicing a `&str` panics when the cut
/// lands inside a multibyte character, and the prose this archive is built for
/// is full of em dashes, curly quotes, and Greek.
pub fn truncate_chars(s: &str, max: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_strings_pass_through_unchanged() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn exact_length_is_not_marked_as_truncated() {
        assert_eq!(truncate_chars("hello", 5), "hello");
    }

    #[test]
    fn longer_strings_are_cut_and_marked() {
        assert_eq!(truncate_chars("hello world", 5), "hello...");
    }

    #[test]
    fn multibyte_boundaries_do_not_panic() {
        // Each em dash is three bytes; a byte-slice at 5 would land mid-character.
        let s = "——————————";
        assert_eq!(truncate_chars(s, 5), "—————...");
    }

    #[test]
    fn counts_characters_rather_than_bytes() {
        assert_eq!(truncate_chars("πρᾶξις", 3).chars().count(), 3 + 3);
    }

    #[test]
    fn zero_is_allowed() {
        assert_eq!(truncate_chars("anything", 0), "...");
    }
}
