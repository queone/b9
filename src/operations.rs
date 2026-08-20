//! Bounded local operational commands.

use std::fmt;
use std::fs;
use std::io::{BufRead, Write};
use std::path::Path;

use crate::terminal::HelpColorMode;

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
    Ok("Database deleted. Run skout sync to rebuild.\n".into())
}

/// Run the production confirmed reset prompt.
pub fn reset(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<String, OperationsError> {
    let path = crate::store::database_path()
        .map_err(|error| OperationsError::new("reset: resolve database", error))?;
    reset_with(
        &path,
        input,
        output,
        crate::terminal::detected_help_color_mode(),
    )
}

/// Reset an explicit database through the production confirmation contract.
pub fn reset_with(
    path: &Path,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    mode: HelpColorMode,
) -> Result<String, OperationsError> {
    if !path.exists() {
        return Ok("No database found — nothing to reset.\n".into());
    }
    let path_label = path.display().to_string();
    writeln!(
        output,
        "This will delete {} and require a full re-sync.",
        crate::terminal::section(&path_label, mode)
    )
    .and_then(|()| write!(output, "Continue? [y/N] "))
    .and_then(|()| output.flush())
    .map_err(|error| OperationsError::new("reset: write confirmation", error))?;
    let mut answer = String::new();
    input
        .read_line(&mut answer)
        .map_err(|error| OperationsError::new("reset: read confirmation", error))?;
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        return Ok("Cancelled.\n".into());
    }
    reset_at(path, true)?;
    Ok(format!(
        "Database deleted. Run {} to rebuild.\n",
        crate::terminal::lineup_indicator("skout sync", true, false, mode)
    ))
}
