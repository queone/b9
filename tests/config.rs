use std::fs;

use b9::config::{Config, adopt_legacy_at, read_at, write_at};
use tempfile::tempdir;

#[test]
fn private_atomic_configuration_round_trips_and_rejects_malformed_state() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nested/config.json");
    assert_eq!(read_at(&path).unwrap(), Config::default());
    let expected = Config {
        current_league: "mlb.l.1".into(),
        current_team_key: "mlb.l.1.t.1".into(),
        advisory_provider: "openai".into(),
        advisory_model: "gpt-4.1-mini".into(),
        strategy_punts: vec!["ERA".into()],
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

#[test]
fn legacy_selections_fill_only_empty_b9_fields() {
    let directory = tempdir().unwrap();
    let target = directory.path().join("b9.json");
    let legacy = directory.path().join("skout.json");
    write_at(
        &legacy,
        &Config {
            current_league: "legacy.l.1".into(),
            current_team_key: "legacy.l.1.t.2".into(),
            ..Config::default()
        },
    )
    .unwrap();
    write_at(
        &target,
        &Config {
            current_league: "b9.l.1".into(),
            current_team_key: String::new(),
            ..Config::default()
        },
    )
    .unwrap();
    let adopted = adopt_legacy_at(&target, &legacy).unwrap();
    assert_eq!(adopted.current_league, "b9.l.1");
    assert_eq!(adopted.current_team_key, "legacy.l.1.t.2");
    assert_eq!(read_at(&target).unwrap(), adopted);
}

#[test]
fn advisory_configuration_serializes_no_credential_material() {
    let config = Config {
        advisory_provider: "claude".into(),
        advisory_model: "model".into(),
        ..Config::default()
    };
    let serialized = serde_json::to_string(&config).unwrap();
    assert!(serialized.contains("claude"));
    assert!(!serialized.contains("credential"));
    assert!(!serialized.contains("api_key"));
}
