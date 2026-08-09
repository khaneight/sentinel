use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

/// Environment variable that points at the archive root.
pub const ENV_ARCHIVE: &str = "SENTINEL_ARCHIVE";

/// Environment variable that overrides the config file location.
pub const ENV_CONFIG: &str = "SENTINEL_CONFIG";

/// Relative path that marks a directory as an initialized archive.
const MARKER: &str = "meta/manifest.json";

/// How far up the tree `discover` will walk before giving up.
const MAX_DISCOVERY_DEPTH: usize = 64;

/// Where a resolved archive root came from.
///
/// Surfaced by `sentinel config` so a human — or an agent — can tell *why*
/// sentinel is pointed at a particular directory instead of guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSource {
    /// `--archive <PATH>`
    Flag,
    /// `SENTINEL_ARCHIVE`
    Env,
    /// `archive = "..."` in the config file
    Config,
    /// Found by walking up from the working directory
    Discovered,
    /// The working directory itself (`init` only)
    Cwd,
}

impl RootSource {
    /// Stable machine-readable token, for `--json` consumers that branch on it.
    pub fn as_str(self) -> &'static str {
        match self {
            RootSource::Flag => "flag",
            RootSource::Env => "env",
            RootSource::Config => "config",
            RootSource::Discovered => "discovered",
            RootSource::Cwd => "cwd",
        }
    }

    /// Human-readable explanation of the precedence rule that won.
    pub fn describe(self) -> &'static str {
        match self {
            RootSource::Flag => "--archive flag",
            RootSource::Env => "SENTINEL_ARCHIVE environment variable",
            RootSource::Config => "archive key in config file",
            RootSource::Discovered => "discovered by walking up from the working directory",
            RootSource::Cwd => "working directory",
        }
    }
}

/// An archive root plus the rule that selected it.
#[derive(Debug, Clone)]
pub struct ResolvedRoot {
    pub path: PathBuf,
    pub source: RootSource,
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/// Resolve the archive root from explicit inputs.
///
/// Precedence: flag → env → config → upward discovery → (init only) cwd.
///
/// Kept free of ambient state so the precedence rules can be unit tested
/// without mutating the process environment.
pub fn resolve(
    flag: Option<&str>,
    env_value: Option<&str>,
    config_value: Option<&str>,
    home: Option<&Path>,
    cwd: &Path,
    allow_cwd_fallback: bool,
) -> io::Result<ResolvedRoot> {
    let explicit = [
        (flag, RootSource::Flag),
        (env_value, RootSource::Env),
        (config_value, RootSource::Config),
    ];

    for (value, source) in explicit {
        let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
            continue;
        };
        return Ok(ResolvedRoot {
            path: absolutize(&expand_tilde(value, home), cwd),
            source,
        });
    }

    if let Some(found) = discover(cwd) {
        return Ok(ResolvedRoot {
            path: found,
            source: RootSource::Discovered,
        });
    }

    if allow_cwd_fallback {
        return Ok(ResolvedRoot {
            path: cwd.to_path_buf(),
            source: RootSource::Cwd,
        });
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "No archive found.\n\
             Sentinel looked for `{MARKER}` in {} and every parent directory.\n\n\
             Point it at one of these ways (highest precedence first):\n  \
             sentinel --archive <PATH> <COMMAND>\n  \
             {ENV_ARCHIVE}=<PATH> sentinel <COMMAND>\n  \
             archive = \"<PATH>\"   in {}\n\n\
             Or create a new archive with `sentinel init <PATH>`.",
            cwd.display(),
            config_path_display()
        ),
    ))
}

/// Resolve using the real process environment and config file.
pub fn resolve_from_environment(
    flag: Option<&str>,
    allow_cwd_fallback: bool,
) -> io::Result<ResolvedRoot> {
    let env_value = std::env::var(ENV_ARCHIVE).ok();
    let config = Config::load()?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let cwd = std::env::current_dir()?;

    resolve(
        flag,
        env_value.as_deref(),
        config.archive.as_deref(),
        home.as_deref(),
        &cwd,
        allow_cwd_fallback,
    )
}

/// Walk up from `start` looking for a directory that contains the archive marker.
fn discover(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .take(MAX_DISCOVERY_DEPTH)
        .find(|dir| dir.join(MARKER).is_file())
        .map(Path::to_path_buf)
}

/// Expand a leading `~` against `home`. Left untouched when home is unknown.
fn expand_tilde(value: &str, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return PathBuf::from(value);
    };
    match value.strip_prefix('~') {
        Some("") => home.to_path_buf(),
        Some(rest) => match rest.strip_prefix('/') {
            Some(rest) => home.join(rest),
            // `~user/...` is not ours to interpret.
            None => PathBuf::from(value),
        },
        None => PathBuf::from(value),
    }
}

/// Make `path` absolute relative to `base`, then collapse `.` and `..`
/// lexically so error messages and the manifest stay readable.
fn absolutize(path: &Path, base: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };

    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop real names — never climb past the root prefix.
                if out.components().next_back().is_some_and(is_poppable) {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
                // At the filesystem root, `..` is a no-op — mirror that.
            }
            other => out.push(other),
        }
    }
    out
}

fn is_poppable(component: Component<'_>) -> bool {
    matches!(component, Component::Normal(_))
}

// ---------------------------------------------------------------------------
// Config file
// ---------------------------------------------------------------------------

/// User-level defaults, read from `~/.config/sentinel/config.toml`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// Default archive root. May start with `~`.
    pub archive: Option<String>,
}

impl Config {
    /// Load the config file, or return defaults when there is none.
    pub fn load() -> io::Result<Self> {
        let Some(path) = config_path() else {
            return Ok(Self::default());
        };
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {e}", path.display()),
            )
        })
    }

    /// Write the config file, creating parent directories as needed.
    pub fn save(&self) -> io::Result<PathBuf> {
        let path = config_path().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Cannot locate a config directory. Set {ENV_CONFIG} to a file path."),
            )
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, text)?;
        Ok(path)
    }
}

/// Path to the config file: `$SENTINEL_CONFIG`, else
/// `$XDG_CONFIG_HOME/sentinel/config.toml`, else `$HOME/.config/sentinel/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os(ENV_CONFIG) {
        return Some(PathBuf::from(explicit));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("sentinel").join("config.toml"))
}

fn config_path_display() -> String {
    config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.config/sentinel/config.toml".to_string())
}

// ---------------------------------------------------------------------------
// Process-wide archive root
// ---------------------------------------------------------------------------

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Install the resolved archive root. Called once from `main` before dispatch.
pub fn set_archive_root(path: PathBuf) {
    let _ = ROOT.set(path);
}

/// The archive root for this process.
///
/// # Panics
/// If called before `set_archive_root`. `main` always installs it before
/// dispatching a command, so reaching this is a bug in sentinel itself.
pub fn archive_root() -> PathBuf {
    ROOT.get()
        .cloned()
        .expect("archive root was not initialized before command dispatch")
}

pub fn raw_dir() -> PathBuf {
    archive_root().join("raw")
}

pub fn wiki_dir() -> PathBuf {
    archive_root().join("wiki")
}

pub fn index_dir() -> PathBuf {
    archive_root().join("index")
}

pub fn meta_dir() -> PathBuf {
    archive_root().join("meta")
}

pub fn templates_dir() -> PathBuf {
    archive_root().join("templates")
}

pub fn manifest_path() -> PathBuf {
    meta_dir().join("manifest.json")
}

pub fn link_graph_path() -> PathBuf {
    meta_dir().join("link-graph.json")
}

pub fn log_path() -> PathBuf {
    meta_dir().join("log.md")
}

/// Default domains that get created on init.
pub const DEFAULT_DOMAINS: &[&str] = &["philosophy", "coding", "research"];

/// Given a domain, return the raw subdirectory.
pub fn raw_domain_dir(domain: &str) -> PathBuf {
    raw_dir().join(domain)
}

/// Given a domain, return the wiki subdirectory.
pub fn wiki_domain_dir(domain: &str) -> PathBuf {
    wiki_dir().join(domain)
}

/// Render `path` relative to the archive root, using forward slashes.
///
/// Every path sentinel records or prints goes through here so the manifest,
/// link graph, and lint output stay stable regardless of where the archive
/// lives on disk.
pub fn rel(path: &Path) -> String {
    let relative = path.strip_prefix(archive_root()).unwrap_or(path);
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Convert a filename to kebab-case slug.
pub fn slugify(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);

    stem.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd() -> PathBuf {
        PathBuf::from("/work/here")
    }

    fn home() -> PathBuf {
        PathBuf::from("/home/kn")
    }

    #[test]
    fn flag_beats_env_and_config() {
        let r = resolve(
            Some("/from/flag"),
            Some("/from/env"),
            Some("/from/config"),
            Some(&home()),
            &cwd(),
            false,
        )
        .unwrap();
        assert_eq!(r.path, PathBuf::from("/from/flag"));
        assert_eq!(r.source, RootSource::Flag);
    }

    #[test]
    fn env_beats_config() {
        let r = resolve(
            None,
            Some("/from/env"),
            Some("/from/config"),
            Some(&home()),
            &cwd(),
            false,
        )
        .unwrap();
        assert_eq!(r.path, PathBuf::from("/from/env"));
        assert_eq!(r.source, RootSource::Env);
    }

    #[test]
    fn config_is_used_when_nothing_else_is_set() {
        let r = resolve(
            None,
            None,
            Some("/from/config"),
            Some(&home()),
            &cwd(),
            false,
        )
        .unwrap();
        assert_eq!(r.path, PathBuf::from("/from/config"));
        assert_eq!(r.source, RootSource::Config);
    }

    #[test]
    fn blank_values_are_ignored() {
        let r = resolve(
            Some("   "),
            Some(""),
            Some("/from/config"),
            Some(&home()),
            &cwd(),
            false,
        )
        .unwrap();
        assert_eq!(r.source, RootSource::Config);
    }

    #[test]
    fn relative_values_resolve_against_the_working_directory() {
        let r = resolve(Some("../archive"), None, None, Some(&home()), &cwd(), false).unwrap();
        assert_eq!(r.path, PathBuf::from("/work/archive"));
    }

    #[test]
    fn tilde_expands_against_home() {
        let r = resolve(
            Some("~/Documents/archive"),
            None,
            None,
            Some(&home()),
            &cwd(),
            false,
        )
        .unwrap();
        assert_eq!(r.path, PathBuf::from("/home/kn/Documents/archive"));
    }

    #[test]
    fn bare_tilde_is_home() {
        let r = resolve(Some("~"), None, None, Some(&home()), &cwd(), false).unwrap();
        assert_eq!(r.path, home());
    }

    #[test]
    fn other_users_tilde_is_left_alone() {
        let r = resolve(Some("~alice/kb"), None, None, Some(&home()), &cwd(), false).unwrap();
        assert_eq!(r.path, PathBuf::from("/work/here/~alice/kb"));
    }

    #[test]
    fn missing_archive_is_an_error_not_a_guess() {
        let err = resolve(None, None, None, Some(&home()), &cwd(), false).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        // The message has to tell the caller how to fix it.
        let msg = err.to_string();
        assert!(msg.contains("--archive"), "{msg}");
        assert!(msg.contains(ENV_ARCHIVE), "{msg}");
        assert!(msg.contains("sentinel init"), "{msg}");
    }

    #[test]
    fn init_may_fall_back_to_the_working_directory() {
        let r = resolve(None, None, None, Some(&home()), &cwd(), true).unwrap();
        assert_eq!(r.path, cwd());
        assert_eq!(r.source, RootSource::Cwd);
    }

    #[test]
    fn absolutize_collapses_dot_segments() {
        assert_eq!(
            absolutize(Path::new("a/./b/../c"), Path::new("/base")),
            PathBuf::from("/base/a/c")
        );
    }

    #[test]
    fn absolutize_never_climbs_above_the_root() {
        assert_eq!(
            absolutize(Path::new("/../.."), Path::new("/base")),
            PathBuf::from("/")
        );
    }

    #[test]
    fn discovery_finds_the_archive_from_a_nested_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("archive");
        std::fs::create_dir_all(root.join("meta")).unwrap();
        std::fs::write(root.join(MARKER), "{}").unwrap();
        let nested = root.join("wiki/philosophy");
        std::fs::create_dir_all(&nested).unwrap();

        let r = resolve(None, None, None, None, &nested, false).unwrap();
        assert_eq!(r.path, root);
        assert_eq!(r.source, RootSource::Discovered);
    }

    #[test]
    fn discovery_ignores_a_directory_without_the_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("not/an/archive");
        std::fs::create_dir_all(&nested).unwrap();

        assert!(resolve(None, None, None, None, &nested, false).is_err());
    }

    #[test]
    fn explicit_configuration_beats_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("archive");
        std::fs::create_dir_all(root.join("meta")).unwrap();
        std::fs::write(root.join(MARKER), "{}").unwrap();

        let r = resolve(None, Some("/elsewhere"), None, None, &root, false).unwrap();
        assert_eq!(r.path, PathBuf::from("/elsewhere"));
        assert_eq!(r.source, RootSource::Env);
    }

    #[test]
    fn slugify_normalizes_separators_and_case() {
        assert_eq!(
            slugify("The Problem of Other Minds.md"),
            "the-problem-of-other-minds"
        );
        assert_eq!(slugify("already-kebab"), "already-kebab");
        assert_eq!(slugify("Trailing   spaces  .txt"), "trailing-spaces");
    }
}
