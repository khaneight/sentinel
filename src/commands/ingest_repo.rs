use std::io;

pub fn run(path: &str, domain: &str, name: Option<&str>) -> io::Result<()> {
    let display_name = name.unwrap_or(path);
    eprintln!("sentinel ingest-repo is not yet fully implemented.");
    eprintln!("Planned: analyze codebase at '{path}' and generate structured summary in raw/{domain}/");
    eprintln!("Name: {display_name}");
    eprintln!();
    eprintln!("For now, manually create a summary document and use `sentinel ingest` instead.");
    Ok(())
}
