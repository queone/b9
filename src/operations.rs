//! Bounded local operational commands.

use std::fmt;
use std::fs;
#[cfg(unix)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::io::Read;
use std::io::{BufRead, Write};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::net::UnixStream;

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

#[cfg(unix)]
#[derive(Clone, Debug)]
struct RetiredDaemonPaths {
    directory: PathBuf,
    owner: PathBuf,
    control: PathBuf,
}

#[cfg(unix)]
fn retired_daemon_paths() -> Result<RetiredDaemonPaths, OperationsError> {
    let config = crate::config::config_path()
        .map_err(|error| OperationsError::new("resolve retired daemon runtime", error))?;
    let directory = config
        .parent()
        .ok_or_else(|| {
            OperationsError::new(
                "resolve retired daemon runtime",
                "configuration has no parent",
            )
        })?
        .join("runtime");
    Ok(RetiredDaemonPaths {
        owner: directory.join("daemon.lock"),
        control: directory.join("daemon.sock"),
        directory,
    })
}

#[cfg(unix)]
fn request_retired_daemon(command: &str) -> Result<String, OperationsError> {
    let paths = retired_daemon_paths()?;
    let mut stream = UnixStream::connect(&paths.control)
        .map_err(|error| OperationsError::new("connect retired daemon control", error))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .and_then(|()| stream.set_write_timeout(Some(Duration::from_secs(3))))
        .map_err(|error| OperationsError::new("configure retired daemon control", error))?;
    stream
        .write_all(command.as_bytes())
        .map_err(|error| OperationsError::new("write retired daemon control", error))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| OperationsError::new("read retired daemon control", error))?;
    Ok(response)
}

#[cfg(unix)]
fn retired_daemon_is_running() -> bool {
    request_retired_daemon("status\n").is_ok_and(|response| response == "running\n")
}

#[cfg(unix)]
fn open_retired_owner(path: &Path) -> Result<Option<File>, OperationsError> {
    if !path.exists() {
        return Ok(None);
    }
    let owner = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| OperationsError::new("open retired daemon ownership", error))?;
    owner.try_lock().map_err(|_| {
        OperationsError::new(
            "clean retired daemon runtime",
            "the prior daemon still owns its lock",
        )
    })?;
    Ok(Some(owner))
}

#[cfg(unix)]
fn cleanup_retired_daemon_artifacts() -> Result<(), OperationsError> {
    if retired_daemon_is_running() {
        return Err(OperationsError::new(
            "clean retired daemon runtime",
            "the prior daemon is still running",
        ));
    }
    let paths = retired_daemon_paths()?;
    if !paths.directory.exists() {
        return Ok(());
    }
    let owner = open_retired_owner(&paths.owner)?;
    for path in [&paths.control, &paths.owner] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(OperationsError::new("clean retired daemon runtime", error));
            }
        }
    }
    drop(owner);
    Ok(())
}

#[cfg(unix)]
fn wait_for_retired_daemon_cleanup(message: &str) -> Result<String, OperationsError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !retired_daemon_is_running() && cleanup_retired_daemon_artifacts().is_ok() {
            return Ok(message.into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    match cleanup_retired_daemon_artifacts() {
        Ok(()) => Ok(message.into()),
        Err(_) => Err(OperationsError::new(
            "stop retired daemon",
            "shutdown timed out",
        )),
    }
}

/// Stop a daemon started by an older b9 release and remove its verified artifacts.
pub fn stop_retired_daemon() -> Result<String, OperationsError> {
    #[cfg(not(unix))]
    {
        Ok("b9 daemon is not running.\n".into())
    }
    #[cfg(unix)]
    {
        if !retired_daemon_is_running() {
            return wait_for_retired_daemon_cleanup("b9 daemon is not running.\n");
        }
        let response = request_retired_daemon("stop\n")?;
        if response != "stopping\n" {
            return Err(OperationsError::new(
                "stop retired daemon",
                "unexpected control response",
            ));
        }
        wait_for_retired_daemon_cleanup("b9 daemon stopped.\n")
    }
}

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
    stop_retired_daemon().map_err(|error| OperationsError::new("reset: stop daemon", error))?;
    reset_at(&path, true)
}
