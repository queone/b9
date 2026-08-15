use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use b9::store::{CURRENT_SCHEMA_VERSION, Store, StoreError, database_path};
use rusqlite::Connection;
use tempfile::tempdir;

const TABLES: [&str; 20] = [
    "command_snapshots",
    "mlb_game_schedule",
    "mlb_odds",
    "mlb_team_active_rosters",
    "mlbam_season_stats",
    "players",
    "projection_seasons",
    "schema_version",
    "season_sync_status",
    "statcast_seasons",
    "sync_item_state",
    "sync_log",
    "sync_row_state",
    "sync_runs",
    "yahoo_leagues",
    "yahoo_roster_positions",
    "yahoo_roster_slots",
    "yahoo_stat_categories",
    "yahoo_teams",
    "yahoo_transactions",
];

fn user_tables(connection: &Connection) -> BTreeSet<String> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn create_fixture(path: &Path, statements: &str) {
    let connection = Connection::open(path).unwrap();
    connection.execute_batch(statements).unwrap();
}

#[test]
fn fresh_store_has_the_exact_schema_and_connection_policy() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nested/b9.db");
    let store = Store::open_at(&path).unwrap();
    assert_eq!(store.path(), path);
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert!(store.is_empty().unwrap());
    store.close().unwrap();

    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        user_tables(&connection),
        TABLES.into_iter().map(str::to_owned).collect()
    );
    let version_rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version_rows, 1);
    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode.to_lowercase(), "wal");

    let schema = include_str!("../src/store/schema.sql");
    for contract in [
        "version INTEGER PRIMARY KEY",
        "yahoo_player_id     INTEGER UNIQUE",
        "id         INTEGER PRIMARY KEY AUTOINCREMENT",
        "origin     TEXT    NOT NULL DEFAULT 'automatic'",
        "CHECK (primary_type IN ('H','P'))",
        "CHECK (market IN ('moneyline','total','pitcher_strikeouts'))",
        "CHECK (side IN ('home','away','over','under'))",
        "player_mlbam_id INTEGER NOT NULL DEFAULT 0",
        "PRIMARY KEY (player_id, season, stat_group)",
        "PRIMARY KEY (game_date, team_abbr)",
        "PRIMARY KEY (game_pk, market, side, player_mlbam_id, sportsbook)",
    ] {
        assert!(
            schema.contains(contract),
            "missing schema contract {contract}"
        );
    }
    assert!(!schema.to_uppercase().contains("FOREIGN KEY"));
    assert!(!schema.to_uppercase().contains("CREATE INDEX"));
}

#[test]
fn busy_timeout_reopen_and_transactions_preserve_data() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("b9.db");
    let mut store = Store::open_at(&path).unwrap();
    let timeout: i64 = store
        .transaction(|transaction| {
            Ok(transaction
                .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
                .unwrap())
        })
        .unwrap();
    assert_eq!(timeout, 5000);

    store
        .transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO yahoo_leagues (league_key, name, season, num_teams, scoring_type, synced_at) VALUES ('league', 'League', 2026, 12, 'head', 1)",
                    [],
                )
                .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(!store.is_empty().unwrap());
    let rollback = store.transaction(|transaction| {
        transaction
            .execute("DELETE FROM yahoo_leagues", [])
            .unwrap();
        Err::<(), _>(StoreError::UnsupportedSchema {
            path: path.clone(),
            detail: "injected operation failure".into(),
        })
    });
    assert!(rollback.unwrap_err().to_string().contains("injected"));
    assert!(!store.is_empty().unwrap());
    store.close().unwrap();

    let reopened = Store::open_at(&path).unwrap();
    assert_eq!(reopened.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert!(!reopened.is_empty().unwrap());
}

#[test]
fn unsupported_schema_states_fail_closed() {
    let cases = [
        (
            "versionless",
            "CREATE TABLE sentinel (value TEXT); INSERT INTO sentinel VALUES ('kept');",
        ),
        (
            "zero_rows",
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY); CREATE TABLE sentinel (value TEXT); INSERT INTO sentinel VALUES ('kept');",
        ),
        (
            "duplicate_rows",
            "CREATE TABLE schema_version (version INTEGER); INSERT INTO schema_version VALUES (1), (1); CREATE TABLE sentinel (value TEXT); INSERT INTO sentinel VALUES ('kept');",
        ),
        (
            "text_row",
            "CREATE TABLE schema_version (version TEXT PRIMARY KEY); INSERT INTO schema_version VALUES ('bad'); CREATE TABLE sentinel (value TEXT); INSERT INTO sentinel VALUES ('kept');",
        ),
        (
            "version_zero",
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY); INSERT INTO schema_version VALUES (0); CREATE TABLE sentinel (value TEXT); INSERT INTO sentinel VALUES ('kept');",
        ),
        (
            "future",
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY); INSERT INTO schema_version VALUES (99); CREATE TABLE sentinel (value TEXT); INSERT INTO sentinel VALUES ('kept');",
        ),
    ];
    for (name, statements) in cases {
        let directory = tempdir().unwrap();
        let path = directory.path().join(format!("{name}.db"));
        create_fixture(&path, statements);
        let error = Store::open_at(&path).err().expect("reject fixture");
        assert!(error.to_string().contains("schema"), "{name}: {error}");
        let connection = Connection::open(&path).unwrap();
        let sentinel: String = connection
            .query_row("SELECT value FROM sentinel", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sentinel, "kept", "{name}");
    }
}

#[test]
fn production_path_is_b9_owned() {
    let home = std::env::var_os("HOME").expect("HOME for test process");
    let path = database_path().unwrap();
    assert_eq!(path, Path::new(&home).join(".config/b9/b9.db"));
    assert!(
        !path
            .to_string_lossy()
            .contains(&["/.config/", "sk", "out/"].concat())
    );
}

#[cfg(unix)]
#[test]
fn unix_creation_is_private_and_existing_modes_are_unchanged() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let new_path = directory.path().join("new/private/b9.db");
    Store::open_at(&new_path).unwrap().close().unwrap();
    assert_eq!(
        fs::metadata(directory.path().join("new"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(new_path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&new_path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let existing_parent = directory.path().join("existing");
    fs::create_dir(&existing_parent).unwrap();
    fs::set_permissions(&existing_parent, fs::Permissions::from_mode(0o750)).unwrap();
    let existing_path = existing_parent.join("b9.db");
    fs::write(&existing_path, []).unwrap();
    fs::set_permissions(&existing_path, fs::Permissions::from_mode(0o640)).unwrap();
    Store::open_at(&existing_path).unwrap().close().unwrap();
    assert_eq!(
        fs::metadata(&existing_parent).unwrap().permissions().mode() & 0o777,
        0o750
    );
    assert_eq!(
        fs::metadata(&existing_path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}
