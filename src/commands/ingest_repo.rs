use std::io;

/// Not implemented.
///
/// This used to print an apology and return `Ok(())`, so the process exited 0
/// and a caller had no way to tell that nothing had happened. A command that
/// silently succeeds at doing nothing is worse than one that is absent: it is
/// listed in `--help`, so an agent choosing from the available commands will
/// pick it, believe it worked, and carry on.
///
/// Returning the guidance as the error lets `main` render it through the same
/// path as every other failure, so `--json` callers get it as JSON.
pub fn run(path: &str, domain: &str, name: Option<&str>) -> io::Result<()> {
    let display_name = name.unwrap_or(path);
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!(
            "`sentinel ingest-repo` is not implemented.\n\
             Planned: analyze the codebase at '{path}' and write a structured summary \
             to raw/{domain}/ as '{display_name}'.\n\
             For now: write the summary yourself and register it with `sentinel ingest`."
        ),
    ))
}
