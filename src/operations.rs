//! Bounded local operational commands.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const LOG_LIMIT: u64 = 5 * 1024 * 1024;
const LINE_LIMIT: usize = 64 * 1024;

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
    crate::daemon::stop_if_running()
        .map_err(|error| OperationsError::new("reset: stop daemon", error))?;
    let result = reset_at(&path, true)?;
    crate::daemon::remove_verified_runtime_artifacts()
        .map_err(|error| OperationsError::new("reset: clean runtime state", error))?;
    Ok(result)
}

/// Resolve the private daemon log path.
pub fn log_path() -> Result<PathBuf, OperationsError> {
    let config = crate::config::config_path()
        .map_err(|error| OperationsError::new("resolve daemon log", error))?;
    Ok(config.with_file_name("svc.log"))
}

/// Open the private daemon log for append and truncate it above its bound.
pub fn open_log(path: &Path) -> Result<File, OperationsError> {
    let parent = path
        .parent()
        .ok_or_else(|| OperationsError::new("open daemon log", "path has no parent"))?;
    create_private_directory(parent)?;
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() >= LOG_LIMIT) {
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|error| OperationsError::new("truncate daemon log", error))?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true).read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(path)
        .map_err(|error| OperationsError::new("open daemon log", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| OperationsError::new("protect daemon log", error))?;
    }
    Ok(file)
}

/// Read at most the bounded log tail.
pub fn tail_log(path: &Path, lines: usize) -> Result<String, OperationsError> {
    let mut file = File::open(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            OperationsError::new("daemon log", "not found; run b9 start first")
        } else {
            OperationsError::new("open daemon log", error)
        }
    })?;
    let length = file
        .metadata()
        .map_err(|error| OperationsError::new("inspect daemon log", error))?
        .len();
    file.seek(SeekFrom::Start(length.saturating_sub(LOG_LIMIT)))
        .map_err(|error| OperationsError::new("seek daemon log", error))?;
    let mut bytes = Vec::new();
    file.take(LOG_LIMIT)
        .read_to_end(&mut bytes)
        .map_err(|error| OperationsError::new("read daemon log", error))?;
    let text = String::from_utf8_lossy(&bytes);
    let rows = text.lines().rev().take(lines).collect::<Vec<_>>();
    let empty = rows.is_empty();
    Ok(rows.into_iter().rev().collect::<Vec<_>>().join("\n") + if empty { "" } else { "\n" })
}

/// Follow a log until the supplied cancellation callback returns true.
pub fn follow_log(
    path: &Path,
    output: &mut dyn Write,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<(), OperationsError> {
    let mut position = fs::metadata(path)
        .map_err(|error| OperationsError::new("inspect daemon log", error))?
        .len();
    while !cancelled() {
        let length = fs::metadata(path)
            .map_err(|error| OperationsError::new("inspect daemon log", error))?
            .len();
        if length < position {
            position = 0;
        }
        if length > position {
            let mut file =
                File::open(path).map_err(|error| OperationsError::new("open daemon log", error))?;
            file.seek(SeekFrom::Start(position))
                .map_err(|error| OperationsError::new("seek daemon log", error))?;
            let mut reader = BufReader::new(file.take(LOG_LIMIT));
            let mut line = String::new();
            while reader
                .by_ref()
                .take((LINE_LIMIT + 1) as u64)
                .read_line(&mut line)
                .map_err(|error| OperationsError::new("follow daemon log", error))?
                > 0
            {
                if line.len() > LINE_LIMIT || (line.len() == LINE_LIMIT && !line.ends_with('\n')) {
                    return Err(OperationsError::new(
                        "follow daemon log",
                        "line exceeds 65536 bytes",
                    ));
                }
                output
                    .write_all(line.as_bytes())
                    .map_err(|error| OperationsError::new("write daemon log", error))?;
                line.clear();
            }
            position = length;
        }
        thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), OperationsError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|error| OperationsError::new("create b9 runtime directory", error))
}
