use std::fs;

use skout::config::{Config, read_at, write_at};
use tempfile::tempdir;

#[test]
fn private_atomic_configuration_round_trips_and_rejects_malformed_state() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nested/config.json");
    assert_eq!(read_at(&path).unwrap(), Config::default());
    let expected = Config {
        current_league: "mlb.l.1".into(),
        current_team_key: "mlb.l.1.t.1".into(),
        pull_public_league_id: "1".into(),
    };
    write_at(&path, &expected).unwrap();
    let read = read_at(&path).unwrap();
    assert_eq!(read.pull_public_league_id, "");
    assert_eq!(
        read,
        Config {
            pull_public_league_id: String::new(),
            ..expected
        }
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    fs::write(&path, b"{").unwrap();
    assert!(
        read_at(&path)
            .unwrap_err()
            .to_string()
            .contains("malformed")
    );
}

#[test]
fn legacy_public_league_id_is_read_but_not_written() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.json");
    let config = Config {
        pull_public_league_id: "170874".into(),
        ..Config::default()
    };
    write_at(&path, &config).unwrap();
    let serialized = fs::read_to_string(&path).unwrap();
    assert!(!serialized.contains("pull_public_league_id"));
    assert!(read_at(&path).unwrap().pull_public_league_id.is_empty());

    fs::write(&path, r#"{"pull_public_league_id":"170874"}"#).unwrap();
    assert_eq!(read_at(&path).unwrap().pull_public_league_id, "170874");
}
