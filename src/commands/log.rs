use std::io;

use colored::Colorize;

use crate::core::log;

pub fn run(operation: &str, detail: &str) -> io::Result<()> {
    log::append(operation, detail)?;
    println!("{} [{operation}] {detail}", "Logged:".green());
    Ok(())
}
