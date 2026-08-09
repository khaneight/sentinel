use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

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
    pub body: String,
    /// Relative path from archive root
    pub rel_path: String,
    /// Why the frontmatter block failed to parse, if it did.
    ///
    /// A malformed block yields default frontmatter, which is indistinguishable
    /// from an article that has none. Carrying the reason lets `lint` say
    /// "invalid YAML" instead of inventing five missing-field errors.
    pub frontmatter_error: Option<String>,
}

/// Parse a markdown file's YAML frontmatter and body.
pub fn parse_file(path: &Path, rel_path: &str) -> io::Result<WikiArticle> {
    let content = fs::read_to_string(path)?;
    let parsed = parse_content(&content);
    Ok(WikiArticle {
        frontmatter: parsed.frontmatter,
        body: parsed.body,
        rel_path: rel_path.to_string(),
        frontmatter_error: parsed.error,
    })
}

/// The result of splitting a markdown document into frontmatter and body.
#[derive(Debug, Clone, Default)]
pub struct ParsedMarkdown {
    pub frontmatter: Frontmatter,
    pub body: String,
    /// Set when a delimited block was present but did not parse.
    pub error: Option<String>,
}

/// Parse frontmatter from markdown content.
///
/// A frontmatter block is a `---` line at the very start of the file, a YAML
/// document, and a closing `---` on a line of its own. Anything else is treated
/// as a document with no frontmatter — including a `---` used as a horizontal
/// rule partway down the page.
pub fn parse_content(content: &str) -> ParsedMarkdown {
    let Some(rest) = strip_opening_fence(content) else {
        return ParsedMarkdown {
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
                return ParsedMarkdown {
                    body: body.trim().to_string(),
                    ..Default::default()
                };
            }
            return match serde_yaml::from_str::<Frontmatter>(yaml) {
                Ok(frontmatter) => ParsedMarkdown {
                    frontmatter,
                    body: body.trim().to_string(),
                    error: None,
                },
                Err(e) => ParsedMarkdown {
                    frontmatter: Frontmatter::default(),
                    body: body.trim().to_string(),
                    error: Some(e.to_string()),
                },
            };
        }
        offset += line.len();
    }

    // Opened but never closed: the whole file is body, and that is worth saying.
    ParsedMarkdown {
        body: content.to_string(),
        error: Some("frontmatter block opened with `---` but never closed".to_string()),
        ..Default::default()
    }
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

/// Generate frontmatter YAML string.
pub fn render_frontmatter(fm: &Frontmatter) -> String {
    let yaml = serde_yaml::to_string(fm).unwrap_or_default();
    format!("---\n{}---\n", yaml)
}

#[cfg(test)]
mod tests {
    use super::*;

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
