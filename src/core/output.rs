use std::io;
use std::sync::OnceLock;

use serde::Serialize;

/// Version of the JSON envelope. Bump on any breaking change to a payload
/// shape so consumers can detect drift instead of silently misreading fields.
pub const SCHEMA_VERSION: u32 = 1;

/// Exit code for "the command ran and found problems", as distinct from exit 1
/// which means "the command failed". Agents and CI need to tell those apart.
pub const EXIT_FINDINGS: i32 = 2;

/// How a command should render its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Human,
    Json,
}

impl Format {
    pub fn is_json(self) -> bool {
        self == Format::Json
    }
}

static FORMAT: OnceLock<Format> = OnceLock::new();

/// Install the output format. Called once from `main` before dispatch, for the
/// same reason the archive root is: threading it through every command
/// signature would add a parameter that only two lines in each function read.
pub fn set_format(format: Format) {
    let _ = FORMAT.set(format);
}

pub fn format() -> Format {
    FORMAT.get().copied().unwrap_or_default()
}

pub fn is_json() -> bool {
    format().is_json()
}

/// Every JSON payload carries these, so a consumer can identify what it is
/// holding without tracking which command produced it.
#[derive(Serialize)]
struct Envelope<'a, T: Serialize> {
    schema_version: u32,
    command: &'a str,
    /// Absent only when no archive could be resolved, which is a state exactly
    /// one command can report on rather than die of. See `emit_unrooted`.
    #[serde(skip_serializing_if = "Option::is_none")]
    archive: Option<String>,
    #[serde(flatten)]
    data: T,
}

/// Print `data` as a JSON object on stdout.
pub fn emit<T: Serialize>(command: &str, data: T) -> io::Result<()> {
    emit_envelope(
        command,
        Some(super::paths::archive_root().display().to_string()),
        data,
    )
}

/// Print a payload for a command running without a resolved archive root.
///
/// Only `sentinel config` needs this, and only because its job is to explain
/// why resolution failed — the one situation where the envelope's `archive`
/// field is the thing being asked about. It previously emitted a bare error
/// string instead, so the command an agent runs when nothing works was the
/// command that told it least.
pub fn emit_unrooted<T: Serialize>(command: &str, data: T) -> io::Result<()> {
    emit_envelope(command, None, data)
}

fn emit_envelope<T: Serialize>(command: &str, archive: Option<String>, data: T) -> io::Result<()> {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        command,
        archive,
        data,
    };
    let text = serde_json::to_string_pretty(&envelope)?;
    println!("{text}");
    Ok(())
}

/// Print an error as JSON on stderr.
///
/// A caller that asked for `--json` gets JSON for failures too; otherwise every
/// consumer needs a second, prose-shaped parser for the unhappy path.
pub fn emit_error(message: &str) {
    #[derive(Serialize)]
    struct ErrorPayload<'a> {
        schema_version: u32,
        error: Message<'a>,
    }
    #[derive(Serialize)]
    struct Message<'a> {
        message: &'a str,
    }

    let payload = ErrorPayload {
        schema_version: SCHEMA_VERSION,
        error: Message { message },
    };
    match serde_json::to_string_pretty(&payload) {
        Ok(text) => eprintln!("{text}"),
        // Serialising a struct of two strings cannot realistically fail, but
        // swallowing the original error to report a serialisation error would
        // be the worst possible trade.
        Err(_) => eprintln!("Error: {message}"),
    }
}
