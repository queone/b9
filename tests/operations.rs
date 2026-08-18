use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use b9::operations::{follow_log, open_log, reset_at, tail_log};
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

#[test]
fn log_tail_is_line_bounded() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("svc.log");
    fs::write(&path, b"one\ntwo\nthree\n").unwrap();
    assert_eq!(tail_log(&path, 2).unwrap(), "two\nthree\n");
    assert_eq!(tail_log(&path, 0).unwrap(), "");
}

#[test]
fn opening_an_oversized_log_truncates_it_and_keeps_it_private() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("svc.log");
    fs::write(&path, vec![b'x'; 5 * 1024 * 1024]).unwrap();
    drop(open_log(&path).unwrap());
    assert_eq!(fs::metadata(&path).unwrap().len(), 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn following_detects_truncation_without_replaying_the_old_file() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("svc.log");
    fs::write(&path, b"old line that is longer\n").unwrap();
    let mut calls = 0;
    let mut cancelled = || {
        calls += 1;
        if calls == 1 {
            fs::write(&path, b"new\n").unwrap();
            false
        } else {
            true
        }
    };
    let mut output = Vec::new();
    follow_log(&path, &mut output, &mut cancelled).unwrap();
    assert_eq!(output, b"new\n");
}

#[test]
fn confirmed_cli_reset_preserves_every_unrelated_local_file() {
    let home = tempdir().unwrap();
    let b9 = home.path().join(".config/b9");
    let skout = home.path().join(".config/skout");
    fs::create_dir_all(b9.join("cache")).unwrap();
    fs::create_dir_all(&skout).unwrap();
    fs::write(b9.join("b9.db"), b"database").unwrap();
    fs::write(b9.join("config.json"), b"{}\n").unwrap();
    fs::write(b9.join("cache/entry"), b"cache").unwrap();
    fs::write(b9.join("svc.log"), b"log\n").unwrap();
    fs::write(skout.join("kept"), b"legacy").unwrap();
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
    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(!b9.join("b9.db").exists());
    for path in [
        b9.join("config.json"),
        b9.join("cache/entry"),
        b9.join("svc.log"),
        skout.join("kept"),
    ] {
        assert!(path.exists(), "{}", path.display());
    }
}
