use std::fs;

use b9::config::{Config, read_at, write_at};
use tempfile::tempdir;

#[test]
fn private_atomic_configuration_round_trips_and_rejects_malformed_state() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nested/config.json");
    assert_eq!(read_at(&path).unwrap(), Config::default());
    let expected = Config {
        current_league: "mlb.l.1".into(),
        current_team_key: "mlb.l.1.t.1".into(),
    };
    write_at(&path, &expected).unwrap();
    assert_eq!(read_at(&path).unwrap(), expected);
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
