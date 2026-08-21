use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use rusqlite::Connection;
use skout::store::{CURRENT_SCHEMA_VERSION, Store, StoreError, database_path, inspect_status_at};
use tempfile::tempdir;

const TABLES: [&str; 23] = [
    "command_snapshots",
    "dashboard_status",
    "fangraphs_batted_ball",
    "mlb_game_schedule",
    "mlb_odds",
    "mlb_team_active_rosters",
    "mlbam_season_stats",
    "players",
    "player_projections",
    "schema_version",
    "season_sync_status",
    "statcast_seasons",
    "sync_item_state",
    "sync_log",
    "sync_row_state",
    "sync_runs",
    "yahoo_free_agents",
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
    let path = directory.path().join("nested/skout.db");
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
    // AC47: `fantasy_players` needed indexes on columns three tables' primary
    // keys don't lead with — the "no indexes" default no longer holds
    // unconditionally, but each addition should still be a deliberate,
    // named, single-column-purpose index, not a return to unindexed lookups
    // dressed up differently.
    for contract in [
        "CREATE INDEX IF NOT EXISTS idx_mlb_team_active_rosters_mlbam_id ON mlb_team_active_rosters(mlbam_id)",
        "CREATE INDEX IF NOT EXISTS idx_players_mlbam_id ON players(mlbam_id)",
        "CREATE INDEX IF NOT EXISTS idx_yahoo_roster_slots_player_id ON yahoo_roster_slots(player_id)",
    ] {
        assert!(
            schema.contains(contract),
            "missing index contract {contract}"
        );
    }
}

#[test]
fn busy_timeout_reopen_and_transactions_preserve_data() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("skout.db");
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
fn inspect_status_at_reads_a_pre_dashboard_status_database_without_migrating() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("legacy.db");
    let store = Store::open_at(&path).unwrap();
    store.close().unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .execute("DROP TABLE dashboard_status", [])
        .unwrap();
    connection
        .execute("UPDATE schema_version SET version = 2", [])
        .unwrap();
    drop(connection);

    let status = inspect_status_at(&path, "").unwrap();
    assert_eq!(status.provider_failure_count, 0);
    assert!(!status.circuit_open);
    assert_eq!(status.provider_last_error, None);
    assert_eq!(status.last_run_status, None);

    let connection = Connection::open(&path).unwrap();
    let version: i64 = connection
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 2, "a read-only status inspection must not migrate");
}

#[test]
fn version_three_migration_adds_computed_statcast_rates_without_losing_rows() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("version-three.db");
    Store::open_at(&path).unwrap().close().unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("UPDATE dashboard_status SET last_error='kept'", [])
        .unwrap();
    connection.execute_batch(
        "ALTER TABLE statcast_seasons RENAME TO statcast_seasons_v4;
         CREATE TABLE statcast_seasons (
           player_id INTEGER NOT NULL, season INTEGER NOT NULL, stat_group TEXT NOT NULL,
           xwoba REAL, fetched_at INTEGER, PRIMARY KEY(player_id,season,stat_group)
         );
         INSERT INTO statcast_seasons(player_id,season,stat_group,xwoba,fetched_at) VALUES(7,2026,'batting',.401,1);
         DROP TABLE statcast_seasons_v4;
         UPDATE schema_version SET version=3;",
    ).unwrap();
    drop(connection);

    let store = Store::open_at(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    store.close().unwrap();
    let connection = Connection::open(&path).unwrap();
    let dashboard_columns = connection
        .prepare("PRAGMA table_info(dashboard_status)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        !dashboard_columns
            .iter()
            .any(|name| name.starts_with("daemon_"))
    );
    assert_eq!(
        connection
            .query_row("SELECT last_error FROM dashboard_status", [], |row| row
                .get::<_, String>(
                0
            ))
            .unwrap(),
        "kept"
    );
    assert_eq!(
        connection
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE name='{}'",
                    concat!("projection_", "seasons")
                ),
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let row: (f64, Option<f64>, Option<f64>, Option<f64>) = connection
        .query_row(
            "SELECT xwoba,strikeout_pct,walk_pct,ops FROM statcast_seasons WHERE player_id=7",
            [],
            |result| {
                Ok((
                    result.get(0)?,
                    result.get(1)?,
                    result.get(2)?,
                    result.get(3)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(row, (0.401, None, None, None));
}

fn schema_has_index(connection: &Connection, name: &str) -> bool {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type='index' AND name=?1",
            [name],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
        > 0
}

#[test]
fn version_five_migration_adds_fantasy_players_indexes_without_losing_rows() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("version-five.db");
    Store::open_at(&path).unwrap().close().unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "INSERT INTO players (mlbam_id, name, synced_at) VALUES (501, 'Kept Player', 1)",
            [],
        )
        .unwrap();
    connection
        .execute("UPDATE schema_version SET version=5", [])
        .unwrap();
    drop(connection);

    let store = Store::open_at(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    store.close().unwrap();

    let connection = Connection::open(&path).unwrap();
    for index in [
        "idx_mlb_team_active_rosters_mlbam_id",
        "idx_players_mlbam_id",
        "idx_yahoo_roster_slots_player_id",
    ] {
        assert!(schema_has_index(&connection, index), "missing {index}");
    }
    assert_eq!(
        connection
            .query_row("SELECT name FROM players WHERE mlbam_id=501", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "Kept Player"
    );
}

#[test]
fn fresh_store_has_the_fantasy_players_indexes() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("fresh-indexes.db");
    Store::open_at(&path).unwrap().close().unwrap();
    let connection = Connection::open(&path).unwrap();
    for index in [
        "idx_mlb_team_active_rosters_mlbam_id",
        "idx_players_mlbam_id",
        "idx_yahoo_roster_slots_player_id",
    ] {
        assert!(schema_has_index(&connection, index), "missing {index}");
    }
}

#[test]
fn production_path_is_skout_owned() {
    let home = std::env::var_os("HOME").expect("HOME for test process");
    let path = database_path().unwrap();
    assert_eq!(path, Path::new(&home).join(".config/skout/skout.db"));
}

#[cfg(unix)]
#[test]
fn unix_creation_is_private_and_existing_modes_are_unchanged() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let new_path = directory.path().join("new/private/skout.db");
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
    let existing_path = existing_parent.join("skout.db");
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
