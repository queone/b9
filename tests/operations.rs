use std::fs;
use std::io::{Read, Write};
use std::process::{Command, Stdio};

use b9::operations::reset_at;
use tempfile::tempdir;

#[test]
fn reset_is_idempotent_and_requires_confirmation() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("b9.db");
    assert!(reset_at(&database, true).unwrap().contains("nothing"));
    fs::write(&database, b"database").unwrap();
    assert!(reset_at(&database, false).unwrap().contains("cancelled"));
    assert_eq!(fs::read(&database).unwrap(), b"database");
}

#[cfg(unix)]
fn retired_daemon_fixture(home: &std::path::Path) -> std::thread::JoinHandle<()> {
    use std::os::unix::net::UnixListener;

    let runtime = home.join(".config/b9/runtime");
    fs::create_dir_all(&runtime).unwrap();
    let owner = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(runtime.join("daemon.lock"))
        .unwrap();
    owner.try_lock().unwrap();
    fs::write(runtime.join("sync.lock"), b"").unwrap();
    let listener = UnixListener::bind(runtime.join("daemon.sock")).unwrap();
    std::thread::spawn(move || {
        for expected in ["status\n", "stop\n"] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 32];
            let count = stream.read(&mut bytes).unwrap();
            assert_eq!(std::str::from_utf8(&bytes[..count]).unwrap(), expected);
            stream
                .write_all(if expected == "status\n" {
                    b"running\n"
                } else {
                    b"stopping\n"
                })
                .unwrap();
        }
        drop(listener);
        std::thread::sleep(std::time::Duration::from_millis(150));
        drop(owner);
    })
}

#[cfg(unix)]
fn short_home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("b9")
        .tempdir_in("/tmp")
        .unwrap()
}

#[cfg(unix)]
#[test]
fn transitional_stop_shuts_down_prior_protocol_and_preserves_foreground_state() {
    let home = short_home();
    let fixture = retired_daemon_fixture(home.path());
    let output = Command::new(env!("CARGO_BIN_EXE_b9"))
        .env("HOME", home.path())
        .arg("stop")
        .output()
        .unwrap();
    fixture.join().unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "b9 daemon stopped.\n"
    );
    let runtime = home.path().join(".config/b9/runtime");
    assert!(!runtime.join("daemon.sock").exists());
    assert!(!runtime.join("daemon.lock").exists());
    assert!(runtime.join("sync.lock").exists());
    let absent = Command::new(env!("CARGO_BIN_EXE_b9"))
        .env("HOME", home.path())
        .arg("stop")
        .output()
        .unwrap();
    assert!(absent.status.success(), "{:?}", absent.stderr);
    assert_eq!(
        String::from_utf8(absent.stdout).unwrap(),
        "b9 daemon is not running.\n"
    );
}

#[cfg(unix)]
#[test]
fn confirmed_cli_reset_preserves_every_unrelated_local_file() {
    let home = short_home();
    let b9 = home.path().join(".config/b9");
    let skout = home.path().join(".config/skout");
    fs::create_dir_all(b9.join("cache")).unwrap();
    fs::create_dir_all(&skout).unwrap();
    fs::write(b9.join("b9.db"), b"database").unwrap();
    fs::write(b9.join("config.json"), b"{}\n").unwrap();
    fs::write(b9.join("cache/entry"), b"cache").unwrap();
    fs::write(b9.join("svc.log"), b"log\n").unwrap();
    fs::create_dir_all(b9.join("runtime")).unwrap();
    fs::write(b9.join("runtime/sync.lock"), b"").unwrap();
    fs::write(skout.join("kept"), b"legacy").unwrap();
    let fixture = retired_daemon_fixture(home.path());
    let mut child = Command::new(env!("CARGO_BIN_EXE_b9"))
        .env("HOME", home.path())
        .arg("reset")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    let output = child.wait_with_output().unwrap();
    fixture.join().unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(!b9.join("b9.db").exists());
    assert!(!b9.join("runtime/daemon.sock").exists());
    assert!(!b9.join("runtime/daemon.lock").exists());
    for path in [
        b9.join("config.json"),
        b9.join("cache/entry"),
        b9.join("svc.log"),
        b9.join("runtime/sync.lock"),
        skout.join("kept"),
    ] {
        assert!(path.exists(), "{}", path.display());
    }
}
