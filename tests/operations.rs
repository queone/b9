use std::fs;
use std::io::Cursor;

use skout::operations::{reset_at, reset_with};
use skout::terminal::HelpColorMode;
use tempfile::tempdir;

#[test]
fn reset_is_idempotent_and_requires_confirmation() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("skout.db");
    assert!(reset_at(&database, true).unwrap().contains("nothing"));
    fs::write(&database, b"database").unwrap();
    assert!(reset_at(&database, false).unwrap().contains("cancelled"));
    assert_eq!(fs::read(&database).unwrap(), b"database");
    assert_eq!(
        reset_at(&database, true).unwrap(),
        "Database deleted. Run skout sync to rebuild.\n"
    );
    assert!(!database.exists());
}

#[test]
fn reset_prompt_matches_confirmation_and_recovery_text() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("skout.db");
    fs::write(&database, b"database").unwrap();
    let mut output = Vec::new();
    let cancelled = reset_with(
        &database,
        &mut Cursor::new("no\n"),
        &mut output,
        HelpColorMode::Plain,
    )
    .unwrap();
    assert_eq!(cancelled, "Cancelled.\n");
    assert!(database.exists());

    output.clear();
    let message = reset_with(
        &database,
        &mut Cursor::new("yes\n"),
        &mut output,
        HelpColorMode::Plain,
    )
    .unwrap();

    let prompt = String::from_utf8(output).unwrap();
    assert!(prompt.contains(&format!("This will delete {}", database.display())));
    assert!(prompt.contains(" and require a full re-sync.\nContinue? [y/N] "));
    assert_eq!(message, "Database deleted. Run skout sync to rebuild.\n");
    assert!(!database.exists());
}
