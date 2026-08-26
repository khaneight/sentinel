use serde::{Deserialize, Serialize};

/// Accepted values for `origin`.
///
/// Shared by the lint rule and by `sentinel schema` so the checker and the
/// published contract cannot disagree about what is legal.
pub const ORIGINS: &[&str] = &["authored", "researched", "hybrid"];

/// Accepted values for `status`.
pub const STATUSES: &[&str] = &["draft", "review", "stable"];

/// Parsed YAML frontmatter from a wiki article.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub domain: Option<String>,
    pub origin: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub related: Vec<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub status: Option<String>,
}

/// A wiki article with parsed frontmatter and body content.
#[derive(Debug, Clone)]
pub struct WikiArticle {
    pub frontmatter: Frontmatter,
    /// Relative path from archive root
    pub rel_path: String,
    /// Why the frontmatter block failed to parse, if it did.
    ///
    /// A malformed block yields default frontmatter, which is indistinguishable
    /// from an article that has none. Carrying the reason lets `lint` say
    /// "invalid YAML" instead of inventing five missing-field errors.
    pub frontmatter_error: Option<String>,
}

/// A document's date fields, paired with their contract names.
///
/// Named because both document kinds expose it and `lint` walks them from one
/// list — a shape two structs must agree on is worth having a name.
pub type Dates<'a> = [(&'static str, Option<&'a str>); 2];

impl Frontmatter {
    /// Every date field, paired with its name.
    ///
    /// An accessor rather than a list of names, so a rule iterating it cannot
    /// check one field and miss the other — `updated` was the only one anything
    /// looked at, and `created` went unvalidated beside it.
    pub fn dates(&self) -> Dates<'_> {
        [
            ("created", self.created.as_deref()),
            ("updated", self.updated.as_deref()),
        ]
    }
}

/// The result of splitting a markdown document into frontmatter and body.
///
/// Generic over the frontmatter type because the archive now holds two
/// document schemas — wiki articles and `persona/` traits — and the fence
/// splitting below is the subtle part. A second copy of it for traits would be
/// a second opinion on whether a `---` partway down the page opens a block.
#[derive(Debug, Clone, Default)]
pub struct Parsed<T> {
    pub frontmatter: T,
    /// The document with its frontmatter block removed.
    ///
    /// Part of the parser's contract and asserted on by its tests — they are
    /// what prove the block ends where it should — but no command consumes it
    /// directly. Link extraction wants the whole file, since wikilinks appear
    /// in `related:` as well as the body. `search` wants prose only, but has to
    /// report file line numbers with its excerpts, so it goes through
    /// `LoadedArticle::body_with_offset` and the `block_end` boundary instead.
    #[allow(dead_code)]
    pub body: String,
    /// Set when a delimited block was present but did not parse.
    pub error: Option<String>,
}

/// The wiki-article case, which is what almost every caller wants.
pub type ParsedMarkdown = Parsed<Frontmatter>;

/// Parse frontmatter from markdown content.
///
/// A frontmatter block is a `---` line at the very start of the file, a YAML
/// document, and a closing `---` on a line of its own. Anything else is treated
/// as a document with no frontmatter — including a `---` used as a horizontal
/// rule partway down the page.
/// The one date format the archive uses. ISO 8601, so lexical order is
/// chronological order and `_recent.md` can sort without parsing.
pub const DATE_FORMAT: &str = "%Y-%m-%d";

/// Parse a frontmatter date, or say why it is not one.
///
/// Shared so `lint` and `next` cannot disagree about what a date is — they did:
/// `next` dropped anything unparseable with `.ok()?` and `lint` never looked.
pub fn parse_date(value: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(value.trim(), DATE_FORMAT)
        .map_err(|_| format!("expected YYYY-MM-DD, got '{value}'"))
}

pub fn parse_content(content: &str) -> ParsedMarkdown {
    parse_as(content)
}

/// Parse a markdown document whose frontmatter deserialises into `T`.
///
/// Absent frontmatter yields `T::default()` — the same contract wiki articles
/// have always had, where a missing block is reported as missing fields rather
/// than as a parse failure.
pub fn parse_as<T>(content: &str) -> Parsed<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    let Some(rest) = strip_opening_fence(content) else {
        return Parsed {
            error: opening_fence_problem(content),
            body: content.to_string(),
            ..Default::default()
        };
    };

    // Byte offsets from `lines()` are recovered by accumulating lengths, which
    // keeps every index on a character boundary regardless of the content.
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        if is_fence(line) {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            // An empty block is legal — a template stub, not a parse failure.
            if yaml.trim().is_empty() {
                return Parsed {
                    body: body.trim().to_string(),
                    ..Default::default()
                };
            }
            return match serde_yaml::from_str::<T>(yaml) {
                Ok(frontmatter) => Parsed {
                    frontmatter,
                    body: body.trim().to_string(),
                    error: None,
                },
                Err(e) => Parsed {
                    frontmatter: T::default(),
                    body: body.trim().to_string(),
                    error: Some(e.to_string()),
                },
            };
        }
        offset += line.len();
    }

    // Opened but never closed: the whole file is body, and that is worth saying.
    // If a line was clearly *meant* as the closing fence, name it — "never
    // closed" sends someone looking for a missing line rather than at the one
    // that is already there.
    let near = rest
        .lines()
        .enumerate()
        .find_map(|(i, l)| near_fence(l).map(|why| (i + 2, why)));
    let message = match near {
        Some((line, why)) => format!(
            "frontmatter block opened with `---` but never closed; line {line} \
             looks like the intended delimiter: {why}"
        ),
        None => "frontmatter block opened with `---` but never closed".to_string(),
    };
    Parsed {
        body: content.to_string(),
        error: Some(message),
        ..Default::default()
    }
}

/// Byte offset just past the closing `---` line, if there is a frontmatter block.
///
/// Lets a caller edit inside the block without touching the body — and without
/// round-tripping through serde, which would reorder keys and strip comments
/// from a file the user may also edit by hand.
pub fn block_end(content: &str) -> Option<usize> {
    let rest = strip_opening_fence(content)?;
    let prefix = content.len() - rest.len();
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        if is_fence(line) {
            return Some(prefix + offset + line.len());
        }
        offset += line.len();
    }
    None
}

/// Why a file with no frontmatter block looks like it was meant to have one.
///
/// The strictness above is deliberate — a `---` used as a horizontal rule is not
/// frontmatter — but it produced a diagnosis that named the wrong problem. A
/// file opening `--- ` with a trailing space, or with a blank line above the
/// fence, parsed as "no frontmatter", and `lint` then reported `missing 'title'`
/// three times over for a file whose second line reads `title: …`. An agent
/// told a title is missing adds one; the file is then still broken and now has
/// two.
///
/// Returns `None` for a document that genuinely has no frontmatter, so the
/// ordinary missing-field diagnosis still applies to those.
fn opening_fence_problem(content: &str) -> Option<String> {
    let mut blanks = 0usize;
    let mut first: Option<(usize, &str)> = None;
    for (i, line) in content.lines().enumerate() {
        // A byte-order mark is invisible in every editor that writes one, so
        // "`---` must be the first line" reads as already true.
        let line = if i == 0 {
            line.trim_start_matches('\u{feff}')
        } else {
            line
        };
        if line.trim().is_empty() {
            blanks += 1;
            continue;
        }
        first = Some((i, line));
        break;
    }
    let (index, first) = first?;
    let has_bom = content.starts_with('\u{feff}');

    let problem = if first == "---" {
        // A well-formed fence that `strip_opening_fence` still rejected, so
        // something precedes it.
        if has_bom {
            "the file begins with a UTF-8 byte-order mark, so `---` is not the \
             first thing in it"
                .to_string()
        } else if blanks > 0 {
            format!(
                "{blanks} blank line(s) precede the opening `---`, which must be \
                 the very first line"
            )
        } else {
            return None;
        }
    } else {
        let mut detail = near_fence(first)?;
        if has_bom {
            detail.push_str(" (and the file begins with a byte-order mark)");
        }
        detail
    };

    // Only speak up when this really was an attempt at frontmatter. A page that
    // opens with a horizontal rule is not a broken article, and a wrong
    // diagnosis is what this function exists to stop producing.
    if !looks_like_frontmatter(content, index) {
        return None;
    }
    Some(problem)
}

/// A line probably meant as a `---` delimiter that is not one.
fn near_fence(line: &str) -> Option<String> {
    let line = line.trim_end_matches(['\n', '\r']);
    if line == "---" {
        return None;
    }
    let squeezed = line.trim();
    if squeezed.len() < 3 || !squeezed.chars().all(|c| c == '-') {
        return None;
    }
    Some(if squeezed.len() == 3 {
        format!("`{line}` is padded with whitespace; the delimiter must be `---` alone")
    } else {
        format!(
            "`{squeezed}` has {} dashes; the delimiter is exactly three",
            squeezed.len()
        )
    })
}

/// Whether the lines after `index` read as a YAML mapping rather than prose.
fn looks_like_frontmatter(content: &str, index: usize) -> bool {
    content
        .lines()
        .skip(index + 1)
        // Long enough for a real block, short enough that a `key: value` line
        // deep in an essay does not retroactively make the page look broken.
        .take(40)
        .take_while(|l| near_fence(l).is_none() && l.trim_end() != "---")
        .any(|l| {
            let Some((key, _)) = l.split_once(':') else {
                return false;
            };
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

/// Consume a leading `---` line, returning everything after it.
fn strip_opening_fence(content: &str) -> Option<&str> {
    let rest = content.strip_prefix("---")?;
    // `---foo` on the first line is not a fence.
    rest.strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
}

/// True for a line consisting only of the `---` delimiter.
fn is_fence(line: &str) -> bool {
    line.trim_end_matches(['\n', '\r']) == "---"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frontmatter every case below is trying to express.
    const FIELDS: &str = "title: T\ndomain: philosophy\norigin: authored\n";

    /// Ways of getting the opening delimiter wrong, and what each must say.
    ///
    /// Enumerated together because they share one cause and produced one
    /// identical, wrong diagnosis: `missing 'title'`, three times over, for a
    /// file whose second line is `title: T`. Fixing the case in front of me
    /// would have left the other three saying it.
    const MALFORMED_OPENINGS: &[(&str, &str, &str)] = &[
        ("trailing space", "--- \n", "padded with whitespace"),
        ("leading tab", "\t---\n", "padded with whitespace"),
        ("four dashes", "----\n", "4 dashes"),
        ("blank line first", "\n---\n", "blank line(s) precede"),
        ("byte-order mark", "\u{feff}---\n", "byte-order mark"),
    ];

    #[test]
    fn a_malformed_opening_delimiter_says_what_is_wrong_with_it() {
        for (name, opening, expected) in MALFORMED_OPENINGS {
            let parsed = parse_content(&format!("{opening}{FIELDS}---\nBody\n"));
            let error = parsed.error.unwrap_or_else(|| {
                panic!(
                    "{name}: parsed as a plain document, so `lint` will report the title as missing"
                )
            });
            assert!(
                error.contains(expected),
                "{name}: message does not name the problem\n  got: {error}\n  want substring: {expected}"
            );
            assert!(
                parsed.frontmatter.title.is_none(),
                "{name}: the block is still not parsed — only the diagnosis changes"
            );
        }
    }

    #[test]
    fn a_document_with_no_frontmatter_is_not_accused_of_having_broken_some() {
        // The whole point of the diagnosis is that it is accurate. A page that
        // opens with prose, or with a horizontal rule, has not failed at
        // anything and must fall through to the ordinary missing-field path.
        for (name, text) in [
            ("plain prose", "Just a note.\n"),
            ("rule after prose", "Intro.\n\n---\n\nNote: a colon.\n"),
            ("empty file", ""),
            ("heading first", "# Title\n\nSome text.\n"),
            // Not included: a file opening with exactly `---` and never
            // closing. That is an unclosed block by long-standing contract,
            // asserted by `an_unclosed_block_is_reported`, and unrelated to
            // the opening-delimiter diagnosis added here.
        ] {
            let parsed = parse_content(text);
            assert!(
                parsed.error.is_none(),
                "{name}: reported a frontmatter problem in a file that has none: {:?}",
                parsed.error
            );
        }
    }

    #[test]
    fn a_valid_block_is_untouched_by_any_of_this() {
        let parsed = parse_content(&format!("---\n{FIELDS}---\nBody\n"));
        assert!(parsed.error.is_none(), "{:?}", parsed.error);
        assert_eq!(parsed.frontmatter.title.as_deref(), Some("T"));
        assert_eq!(parsed.body, "Body");
    }

    #[test]
    fn crlf_line_endings_are_still_a_valid_block() {
        // Windows editors write these and they are not an error.
        let text = format!("---\n{FIELDS}---\nBody\n").replace('\n', "\r\n");
        let parsed = parse_content(&text);
        assert!(parsed.error.is_none(), "{:?}", parsed.error);
        assert_eq!(parsed.frontmatter.title.as_deref(), Some("T"));
    }

    #[test]
    fn an_unclosed_block_names_the_line_that_was_meant_to_close_it() {
        // "never closed" sends a reader looking for a line to add, when the
        // line is already there and merely padded.
        let parsed = parse_content(&format!("---\n{FIELDS}--- \nBody\n"));
        let error = parsed.error.expect("still an error");
        assert!(error.contains("line 5"), "must name the line: {error}");
        assert!(error.contains("padded with whitespace"), "{error}");
    }

    #[test]
    fn a_genuinely_unclosed_block_still_says_so_plainly() {
        let parsed = parse_content(&format!("---\n{FIELDS}Body with no delimiter\n"));
        let error = parsed.error.expect("still an error");
        assert!(error.contains("never closed"), "{error}");
        assert!(
            !error.contains("looks like"),
            "nothing to point at: {error}"
        );
    }

    #[test]
    fn parses_a_well_formed_block() {
        let parsed = parse_content(
            "---\ntitle: Stoicism\ndomain: philosophy\ntags: [ethics]\n---\n\n# Body\n",
        );
        assert_eq!(parsed.frontmatter.title.as_deref(), Some("Stoicism"));
        assert_eq!(parsed.frontmatter.domain.as_deref(), Some("philosophy"));
        assert_eq!(parsed.frontmatter.tags, vec!["ethics"]);
        assert_eq!(parsed.body, "# Body");
        assert!(parsed.error.is_none());
    }

    #[test]
    fn a_document_without_frontmatter_is_all_body() {
        let parsed = parse_content("# Just a heading\n");
        assert!(parsed.error.is_none());
        assert_eq!(parsed.body, "# Just a heading\n");
    }

    #[test]
    fn malformed_yaml_is_reported_not_swallowed() {
        let parsed = parse_content("---\ntitle: [unclosed\n---\n\nbody\n");
        assert!(
            parsed.error.is_some(),
            "invalid YAML must surface, not masquerade as absent frontmatter"
        );
        assert_eq!(parsed.body, "body");
    }

    #[test]
    fn an_unclosed_block_is_reported() {
        let parsed = parse_content("---\ntitle: Stoicism\n\n# Body without a closing fence\n");
        assert!(parsed.error.is_some());
    }

    #[test]
    fn a_horizontal_rule_in_the_body_does_not_end_the_block() {
        let parsed = parse_content("---\ntitle: Stoicism\n---\n\nIntro\n\n---\n\nOutro\n");
        assert_eq!(parsed.frontmatter.title.as_deref(), Some("Stoicism"));
        assert!(parsed.body.contains("Outro"));
    }

    #[test]
    fn a_dashed_line_inside_the_body_is_not_mistaken_for_the_fence() {
        // The old parser searched for the substring "\n---" and would have cut
        // the document here, silently truncating the frontmatter.
        let parsed = parse_content("---\ntitle: Em — dash\n---\ncontent ---not a fence\n");
        assert_eq!(parsed.frontmatter.title.as_deref(), Some("Em — dash"));
        assert_eq!(parsed.body, "content ---not a fence");
    }

    #[test]
    fn multibyte_content_does_not_panic() {
        let parsed = parse_content("---\ntitle: Ἠθικά — “virtue”\n---\n\nπρᾶξις\n");
        assert_eq!(
            parsed.frontmatter.title.as_deref(),
            Some("Ἠθικά — “virtue”")
        );
        assert_eq!(parsed.body, "πρᾶξις");
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let parsed = parse_content("---\r\ntitle: Stoicism\r\n---\r\n\r\nBody\r\n");
        assert_eq!(parsed.frontmatter.title.as_deref(), Some("Stoicism"));
        assert!(parsed.error.is_none());
    }
}
