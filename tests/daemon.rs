use std::process::Command;

use tempfile::tempdir;

#[test]
fn hidden_daemon_entry_is_not_in_public_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_b9"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        !String::from_utf8(output.stdout)
            .unwrap()
            .contains("_daemon")
    );
}

#[test]
fn unrelated_command_does_not_create_daemon_runtime() {
    let home = tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_b9"))
        .env("HOME", home.path())
        .args(["m", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!home.path().join(".config/b9/runtime").exists());
}

#[test]
fn explicit_daemon_lifecycle_is_exclusive_private_and_clean() {
    let home = tempdir().unwrap();
    let run = |command: &str| {
        Command::new(env!("CARGO_BIN_EXE_b9"))
            .env("HOME", home.path())
            .arg(command)
            .output()
            .unwrap()
    };
    let started = run("start");
    assert!(started.status.success(), "{:?}", started.stderr);
    let duplicate = run("start");
    assert!(duplicate.status.success(), "{:?}", duplicate.stderr);
    assert!(
        String::from_utf8(duplicate.stdout)
            .unwrap()
            .contains("already running")
    );
    let socket = home.path().join(".config/b9/runtime/daemon.sock");
    assert!(socket.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let stopped = run("stop");
    assert!(stopped.status.success(), "{:?}", stopped.stderr);
    assert!(!socket.exists());
    let stopped_again = run("stop");
    assert!(stopped_again.status.success(), "{:?}", stopped_again.stderr);
}

#[test]
fn status_reports_read_only_uptime_while_the_daemon_runs_and_reverts_after_stop() {
    let home = tempdir().unwrap();
    let run = |command: &str| {
        Command::new(env!("CARGO_BIN_EXE_b9"))
            .env("HOME", home.path())
            .arg(command)
            .output()
            .unwrap()
    };
    let started = run("start");
    assert!(started.status.success(), "{:?}", started.stderr);

    let status = run("st");
    assert!(status.status.success(), "{:?}", status.stderr);
    let stdout = String::from_utf8(status.stdout).unwrap();
    assert!(stdout.contains("Service: running (uptime "));

    let stopped = run("stop");
    assert!(stopped.status.success(), "{:?}", stopped.stderr);

    let status_after_stop = run("st");
    assert!(
        status_after_stop.status.success(),
        "{:?}",
        status_after_stop.stderr
    );
    let stdout_after_stop = String::from_utf8(status_after_stop.stdout).unwrap();
    assert!(stdout_after_stop.contains("Service: stopped"));
}
