//! Bounded local operational commands.

use std::fmt;
use std::fs;
use std::io::{BufRead, Write};
use std::path::Path;

/// One contextual operational failure.
#[derive(Debug)]
pub struct OperationsError(String);

impl OperationsError {
    fn new(operation: &str, detail: impl fmt::Display) -> Self {
        Self(format!(
            "{operation}: {detail}; correct the condition and retry"
        ))
    }
}

impl fmt::Display for OperationsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for OperationsError {}

/// Reset an explicit database after confirmation while preserving unrelated files.
pub fn reset_at(path: &Path, confirmed: bool) -> Result<String, OperationsError> {
    if !path.exists() {
        return Ok("No database found — nothing to reset.\n".into());
    }
    if !confirmed {
        return Ok("Reset cancelled.\n".into());
    }
    fs::remove_file(path).map_err(|error| OperationsError::new("reset: delete database", error))?;
    Ok("Local b9 database reset.\n".into())
}

/// Run the production confirmed reset prompt.
pub fn reset(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<String, OperationsError> {
    let path = crate::store::database_path()
        .map_err(|error| OperationsError::new("reset: resolve database", error))?;
    if !path.exists() {
        return Ok("No database found — nothing to reset.\n".into());
    }
    write!(output, "Delete the local b9 database? [y/N] ")
        .and_then(|()| output.flush())
        .map_err(|error| OperationsError::new("reset: write confirmation", error))?;
    let mut answer = String::new();
    input
        .read_line(&mut answer)
        .map_err(|error| OperationsError::new("reset: read confirmation", error))?;
    if !answer.trim().eq_ignore_ascii_case("y") {
        return Ok("Reset cancelled.\n".into());
    }
    reset_at(&path, true)
}
