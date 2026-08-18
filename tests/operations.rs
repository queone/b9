use std::fs;

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
