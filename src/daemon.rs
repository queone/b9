//! Explicit, private background synchronization lifecycle.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

const SCHEDULE_INTERVAL: Duration = Duration::from_secs(8 * 60 * 60);

/// One secret-free daemon lifecycle failure.
#[derive(Debug)]
pub struct DaemonError(String);

impl DaemonError {
    fn new(operation: &str, detail: impl fmt::Display) -> Self {
        Self(format!("{operation}: {detail}; inspect b9 log and retry"))
    }
}

impl fmt::Display for DaemonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DaemonError {}

#[derive(Clone, Debug)]
struct RuntimePaths {
    directory: PathBuf,
    owner: PathBuf,
    control: PathBuf,
    sync: PathBuf,
}

fn paths() -> Result<RuntimePaths, DaemonError> {
    let config = crate::config::config_path()
        .map_err(|error| DaemonError::new("resolve daemon runtime", error))?;
    let directory = config
        .parent()
        .ok_or_else(|| DaemonError::new("resolve daemon runtime", "configuration has no parent"))?
        .join("runtime");
    Ok(RuntimePaths {
        owner: directory.join("daemon.lock"),
        control: directory.join("daemon.sock"),
        sync: directory.join("sync.lock"),
        directory,
    })
}

fn private_directory(path: &Path) -> Result<(), DaemonError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|error| DaemonError::new("create daemon runtime", error))
}

fn claim(path: &Path) -> Result<File, DaemonError> {
    let mut options = OpenOptions::new();
    options.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| DaemonError::new("claim daemon ownership", error))?;
    #[cfg(unix)]
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(DaemonError::new(
            "claim daemon ownership",
            "another b9 process owns the lock",
        ));
    }
    file.set_len(0)
        .map_err(|error| DaemonError::new("clear daemon ownership", error))?;
    writeln!(file, "{}", std::process::id())
        .and_then(|()| file.sync_all())
        .map_err(|error| DaemonError::new("publish daemon ownership", error))?;
    Ok(file)
}

/// A process-local guard that rejects overlapping foreground and daemon synchronization.
pub struct SyncGuard {
    path: PathBuf,
    _claim: File,
}

impl SyncGuard {
    /// Claim the b9 synchronization execution boundary.
    pub fn acquire() -> Result<Self, DaemonError> {
        let paths = paths()?;
        private_directory(&paths.directory)?;
        let claim = claim(&paths.sync).map_err(|_| {
            DaemonError::new(
                "start synchronization",
                "another synchronization is running",
            )
        })?;
        Ok(Self {
            path: paths.sync,
            _claim: claim,
        })
    }
}

impl Drop for SyncGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
fn request(command: &str) -> Result<String, DaemonError> {
    let paths = paths()?;
    let mut stream = UnixStream::connect(&paths.control)
        .map_err(|error| DaemonError::new("connect daemon control", error))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| DaemonError::new("configure daemon control", error))?;
    stream
        .write_all(command.as_bytes())
        .map_err(|error| DaemonError::new("write daemon control", error))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| DaemonError::new("read daemon control", error))?;
    Ok(response)
}

#[cfg(not(unix))]
fn request(_command: &str) -> Result<String, DaemonError> {
    Err(DaemonError::new(
        "connect daemon control",
        "supported host requires Unix sockets",
    ))
}

/// Return whether the private control endpoint identifies a live b9 daemon.
#[must_use]
pub fn is_running() -> bool {
    request("status\n").is_ok_and(|response| response == "running\n")
}

/// Start the explicit daemon process without changing unrelated command behavior.
pub fn start() -> Result<String, DaemonError> {
    if is_running() {
        return Ok("b9 daemon already running.\n".into());
    }
    let executable = std::env::current_exe()
        .map_err(|error| DaemonError::new("resolve b9 executable", error))?;
    let log_path = crate::operations::log_path()
        .map_err(|error| DaemonError::new("resolve daemon log", error))?;
    let log = crate::operations::open_log(&log_path)
        .map_err(|error| DaemonError::new("open daemon log", error))?;
    let stderr = log
        .try_clone()
        .map_err(|error| DaemonError::new("clone daemon log", error))?;
    let mut command = Command::new(executable);
    command
        .arg("_daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid has no Rust aliasing requirements and runs before exec.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    command
        .spawn()
        .map_err(|error| DaemonError::new("start daemon", error))?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if is_running() {
            return Ok(format!(
                "b9 daemon started; logging to {}\n",
                log_path.display()
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(DaemonError::new(
        "start daemon",
        "control endpoint was not published",
    ))
}

/// Stop a verified daemon through its private control endpoint.
pub fn stop() -> Result<String, DaemonError> {
    if !is_running() {
        remove_verified_runtime_artifacts()?;
        return Ok("b9 daemon is not running.\n".into());
    }
    let response = request("stop\n")?;
    if response != "stopping\n" {
        return Err(DaemonError::new("stop daemon", "unexpected response"));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !is_running() {
            return Ok("b9 daemon stopped.\n".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(DaemonError::new("stop daemon", "shutdown timed out"))
}

/// Stop the daemon only when its private control endpoint verifies ownership.
pub fn stop_if_running() -> Result<(), DaemonError> {
    if is_running() {
        stop().map(|_| ())
    } else {
        Ok(())
    }
}

/// Restart the explicit daemon process.
pub fn restart() -> Result<String, DaemonError> {
    stop_if_running()?;
    start()
}

/// Remove runtime artifacts only after confirming no daemon owns the control endpoint.
pub fn remove_verified_runtime_artifacts() -> Result<(), DaemonError> {
    if is_running() {
        return Err(DaemonError::new(
            "clean daemon runtime",
            "verified daemon is still running",
        ));
    }
    let paths = paths()?;
    private_directory(&paths.directory)?;
    let _owner = claim(&paths.owner)?;
    match fs::remove_file(&paths.control) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(DaemonError::new("clean daemon runtime", error)),
    }
    Ok(())
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_secs())
}

fn log(message: &str) {
    if let Ok(path) = crate::operations::log_path()
        && let Ok(mut file) = crate::operations::open_log(&path)
    {
        let _ = writeln!(file, "{} {}", timestamp(), message);
    }
}

#[cfg(unix)]
fn run_sync(origin: crate::store::SyncOrigin) -> String {
    match crate::sync::synchronize_for_origin(None, origin) {
        Ok(summary) => summary.trim_end().to_owned(),
        Err(error) => format!("synchronization failed: {error}"),
    }
}

#[cfg(unix)]
fn start_sync_worker(
    origin: crate::store::SyncOrigin,
    label: &'static str,
    running: &Arc<AtomicBool>,
    synchronize: &Arc<dyn Fn(crate::store::SyncOrigin) -> String + Send + Sync>,
) -> Option<thread::JoinHandle<()>> {
    if running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        log(&format!(
            "{label} synchronization skipped: another synchronization is running"
        ));
        return None;
    }
    let running = Arc::clone(running);
    let synchronize = Arc::clone(synchronize);
    Some(thread::spawn(move || {
        log(&format!("{label} synchronization started"));
        log(&synchronize(origin));
        running.store(false, Ordering::Release);
    }))
}

#[cfg(unix)]
fn run_loop_with(
    listener: UnixListener,
    synchronize: Arc<dyn Fn(crate::store::SyncOrigin) -> String + Send + Sync>,
    on_schedule: Arc<dyn Fn(u64) + Send + Sync>,
) -> Result<(), DaemonError> {
    listener
        .set_nonblocking(true)
        .map_err(|error| DaemonError::new("configure daemon control", error))?;
    let synchronization_running = Arc::new(AtomicBool::new(false));
    let mut worker = start_sync_worker(
        crate::store::SyncOrigin::Startup,
        "startup",
        &synchronization_running,
        &synchronize,
    );
    let mut next_sync = Instant::now() + SCHEDULE_INTERVAL;
    on_schedule(timestamp() + SCHEDULE_INTERVAL.as_secs());
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut bytes = [0_u8; 32];
                let count = stream
                    .read(&mut bytes)
                    .map_err(|error| DaemonError::new("read daemon control", error))?;
                match std::str::from_utf8(&bytes[..count]).unwrap_or("") {
                    "status\n" => {
                        let _ = stream.write_all(b"running\n");
                    }
                    "stop\n" => {
                        let _ = stream.write_all(b"stopping\n");
                        drop(stream);
                        drop(listener);
                        if let Some(worker) = worker.take() {
                            let _ = worker.join();
                        }
                        return Ok(());
                    }
                    _ => {
                        let _ = stream.write_all(b"invalid\n");
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(DaemonError::new("accept daemon control", error)),
        }
        if worker.as_ref().is_some_and(|worker| worker.is_finished())
            && let Some(finished) = worker.take()
        {
            let _ = finished.join();
        }
        if Instant::now() >= next_sync {
            worker = start_sync_worker(
                crate::store::SyncOrigin::Automatic,
                "scheduled",
                &synchronization_running,
                &synchronize,
            );
            next_sync = Instant::now() + SCHEDULE_INTERVAL;
            on_schedule(timestamp() + SCHEDULE_INTERVAL.as_secs());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(unix)]
fn run_loop(listener: UnixListener) -> Result<(), DaemonError> {
    run_loop_with(
        listener,
        Arc::new(run_sync),
        Arc::new(|next_run_at| {
            if let Ok(mut store) = crate::store::Store::open() {
                let _ = store.record_next_run_at(next_run_at as i64);
            }
        }),
    )
}

/// Run the hidden foreground daemon entry point.
pub fn run() -> Result<String, DaemonError> {
    #[cfg(not(unix))]
    {
        return Err(DaemonError::new(
            "run daemon",
            "supported host requires Unix sockets",
        ));
    }
    #[cfg(unix)]
    {
        let paths = paths()?;
        private_directory(&paths.directory)?;
        let owner = claim(&paths.owner)?;
        let _ = fs::remove_file(&paths.control);
        let listener = UnixListener::bind(&paths.control)
            .map_err(|error| DaemonError::new("publish daemon control", error))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&paths.control, fs::Permissions::from_mode(0o600))
                .map_err(|error| DaemonError::new("protect daemon control", error))?;
        }
        if let Ok(mut store) = crate::store::Store::open() {
            let _ = store.record_daemon_started();
        }
        log("b9 daemon started");
        let result = run_loop(listener);
        drop(owner);
        let _ = fs::remove_file(&paths.control);
        if let Ok(mut store) = crate::store::Store::open() {
            let _ = store.record_daemon_stopped();
        }
        log("b9 daemon stopped");
        result.map(|()| String::new())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};

    use super::{claim, run_loop_with, timestamp};

    #[test]
    fn held_lock_rejects_overlap_and_stale_file_recovers_after_release() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("held.lock");
        let owner = claim(&path).unwrap();
        assert!(claim(&path).is_err());
        drop(owner);
        assert!(claim(&path).is_ok());
    }

    #[test]
    fn control_remains_responsive_while_startup_synchronization_runs() {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let started = Arc::new(Barrier::new(2));
        let worker_started = Arc::clone(&started);
        let completed = Arc::new(AtomicBool::new(false));
        let worker_completed = Arc::clone(&completed);
        let loop_thread = std::thread::spawn(move || {
            run_loop_with(
                listener,
                Arc::new(move |_| {
                    worker_started.wait();
                    std::thread::sleep(Duration::from_millis(500));
                    worker_completed.store(true, Ordering::Release);
                    "finished".into()
                }),
                Arc::new(|_| {}),
            )
            .unwrap();
        });
        started.wait();
        let began = Instant::now();
        let mut status = UnixStream::connect(&socket).unwrap();
        status.write_all(b"status\n").unwrap();
        let mut response = String::new();
        status.read_to_string(&mut response).unwrap();
        assert_eq!(response, "running\n");
        assert!(began.elapsed() < Duration::from_millis(300));
        let mut stop = UnixStream::connect(&socket).unwrap();
        stop.write_all(b"stop\n").unwrap();
        let mut response = String::new();
        stop.read_to_string(&mut response).unwrap();
        assert_eq!(response, "stopping\n");
        loop_thread.join().unwrap();
        assert!(completed.load(Ordering::Acquire));
    }

    #[test]
    fn startup_publishes_the_next_scheduled_run_without_touching_the_store() {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let scheduled = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = Arc::clone(&scheduled);
        let before = timestamp();
        let loop_thread = std::thread::spawn(move || {
            run_loop_with(
                listener,
                Arc::new(|_| "finished".into()),
                Arc::new(move |next_run_at| recorded.lock().unwrap().push(next_run_at)),
            )
            .unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while scheduled.lock().unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut stop = UnixStream::connect(&socket).unwrap();
        stop.write_all(b"stop\n").unwrap();
        let mut response = String::new();
        stop.read_to_string(&mut response).unwrap();
        loop_thread.join().unwrap();
        let recorded = scheduled.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert!(recorded[0] >= before + super::SCHEDULE_INTERVAL.as_secs());
    }
}
