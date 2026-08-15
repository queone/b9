use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use b9::store::{
    Clock, ItemRefreshPolicy, RowRefreshPolicy, SeasonSyncStatus, Store, SyncMode, SyncOrigin,
    SyncRunStatus, SyncStateStatus,
};
use rusqlite::Connection;
use tempfile::tempdir;

#[derive(Clone)]
struct AdjustableClock {
    now: Arc<Mutex<SystemTime>>,
}

impl AdjustableClock {
    fn at(seconds: u64) -> Self {
        Self {
            now: Arc::new(Mutex::new(UNIX_EPOCH + Duration::from_secs(seconds))),
        }
    }

    fn set(&self, seconds: u64) {
        *self.now.lock().unwrap() = UNIX_EPOCH + Duration::from_secs(seconds);
    }
}

impl Clock for AdjustableClock {
    fn now(&self) -> SystemTime {
        *self.now.lock().unwrap()
    }
}

struct CountingClock {
    calls: AtomicUsize,
}

impl CountingClock {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}

impl Clock for CountingClock {
    fn now(&self) -> SystemTime {
        self.calls.fetch_add(1, Ordering::SeqCst);
        UNIX_EPOCH + Duration::from_secs(100)
    }
}

fn item_policy(seconds: u64, force: bool, version: &str) -> ItemRefreshPolicy {
    ItemRefreshPolicy {
        ttl: Duration::from_secs(seconds),
        force,
        pipeline_version: version.into(),
    }
}

fn row_policy(seconds: u64, force: bool, version: &str) -> RowRefreshPolicy {
    RowRefreshPolicy {
        ttl: Duration::from_secs(seconds),
        force,
        pipeline_version: version.into(),
    }
}

#[test]
fn item_freshness_uses_status_version_scope_and_exact_time() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.db");
    let clock = AdjustableClock::at(100);
    let mut store = Store::open_at_with_clock(&path, Arc::new(clock.clone())).unwrap();

    assert!(
        store
            .needs_sync_item("mlb", "hitting", "2026", &item_policy(10, false, "v1"))
            .unwrap()
    );
    store
        .mark_sync_item_attempt("mlb", "hitting", "2026", "v1")
        .unwrap();
    let attempt = store
        .sync_item_state("mlb", "hitting", "2026")
        .unwrap()
        .unwrap();
    assert_eq!(attempt.status, SyncStateStatus::Running);
    assert_eq!(
        attempt.last_attempted_at,
        Some(UNIX_EPOCH + Duration::from_secs(100))
    );
    assert_eq!(attempt.last_successful_at, None);

    store
        .mark_sync_item_success("mlb", "hitting", "2026", "v1")
        .unwrap();
    clock.set(110);
    assert!(
        !store
            .needs_sync_item("mlb", "hitting", "2026", &item_policy(10, false, "v1"))
            .unwrap()
    );
    assert!(
        store
            .needs_sync_item("mlb", "hitting", "2026", &item_policy(10, false, "v2"))
            .unwrap()
    );
    assert!(
        store
            .needs_sync_item("mlb", "hitting", "2026", &item_policy(10, true, "v1"))
            .unwrap()
    );
    assert!(
        store
            .needs_sync_item("mlb", "hitting", "2025", &item_policy(10, false, "v1"))
            .unwrap()
    );
    clock.set(111);
    assert!(
        store
            .needs_sync_item("mlb", "hitting", "2026", &item_policy(10, false, "v1"))
            .unwrap()
    );
    clock.set(99);
    assert!(
        !store
            .needs_sync_item("mlb", "hitting", "2026", &item_policy(10, false, "v1"))
            .unwrap()
    );

    clock.set(120);
    store
        .mark_sync_item_failure("mlb", "hitting", "2026", "v1", "offline")
        .unwrap();
    let failed = store
        .sync_item_state("mlb", "hitting", "2026")
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, SyncStateStatus::Failed);
    assert_eq!(
        failed.last_successful_at,
        Some(UNIX_EPOCH + Duration::from_secs(100))
    );
    assert_eq!(failed.error_message, "offline");
    assert!(
        store
            .needs_sync_item("mlb", "hitting", "2026", &item_policy(1000, false, "v1"))
            .unwrap()
    );
}

#[test]
fn row_freshness_preserves_success_and_uses_optional_local_id() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.db");
    let clock = AdjustableClock::at(200);
    let mut store = Store::open_at_with_clock(&path, Arc::new(clock.clone())).unwrap();

    store
        .mark_sync_row_success("mlb", "people", "", "player", "42", None, "v1")
        .unwrap();
    let state = store
        .sync_row_state("mlb", "people", "", "player", "42")
        .unwrap()
        .unwrap();
    assert_eq!(state.local_id, None);
    assert_eq!(state.status, SyncStateStatus::Complete);
    assert!(
        !store
            .needs_sync_row(
                "mlb",
                "people",
                "",
                "player",
                "42",
                &row_policy(60, false, "v1")
            )
            .unwrap()
    );
    assert!(
        store
            .needs_sync_row(
                "mlb",
                "people",
                "",
                "player",
                "42",
                &row_policy(60, false, "v2")
            )
            .unwrap()
    );
    assert!(
        store
            .needs_sync_row(
                "mlb",
                "people",
                "",
                "player",
                "missing",
                &row_policy(60, false, "v1")
            )
            .unwrap()
    );
    assert!(
        store
            .needs_sync_row(
                "mlb",
                "people",
                "",
                "player",
                "42",
                &row_policy(60, true, "v1")
            )
            .unwrap()
    );
    clock.set(260);
    assert!(
        !store
            .needs_sync_row(
                "mlb",
                "people",
                "",
                "player",
                "42",
                &row_policy(60, false, "v1")
            )
            .unwrap()
    );
    clock.set(261);
    assert!(
        store
            .needs_sync_row(
                "mlb",
                "people",
                "",
                "player",
                "42",
                &row_policy(60, false, "v1")
            )
            .unwrap()
    );
    clock.set(199);
    assert!(
        !store
            .needs_sync_row(
                "mlb",
                "people",
                "",
                "player",
                "42",
                &row_policy(60, false, "v1")
            )
            .unwrap()
    );
    assert!(
        store
            .mark_sync_row_success("mlb", "people", "", "player", "43", Some(0), "v1")
            .is_err()
    );

    clock.set(210);
    store
        .mark_sync_row_failure(
            "mlb",
            "people",
            "",
            "player",
            "42",
            Some(7),
            "v1",
            "offline",
        )
        .unwrap();
    let failed = store
        .sync_row_state("mlb", "people", "", "player", "42")
        .unwrap()
        .unwrap();
    assert_eq!(failed.local_id, Some(7));
    assert_eq!(
        failed.last_successful_at,
        Some(UNIX_EPOCH + Duration::from_secs(200))
    );
    assert!(
        store
            .needs_sync_row(
                "mlb",
                "people",
                "",
                "player",
                "42",
                &row_policy(1000, false, "v1")
            )
            .unwrap()
    );
}

#[test]
fn snapshots_validate_replace_and_preserve_stale_payloads() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.db");
    let clock = AdjustableClock::at(300);
    let mut store = Store::open_at_with_clock(&path, Arc::new(clock.clone())).unwrap();
    assert!(
        store
            .command_snapshot("schedule", "mlb", "")
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .save_command_snapshot("schedule", "mlb", "", "v1", "not-json")
            .is_err()
    );
    let exact = "{ \"games\" : [1] }";
    store
        .save_command_snapshot("schedule", "mlb", "", "v1", exact)
        .unwrap();
    assert!(
        store
            .save_command_snapshot("schedule", "mlb", "", "v2", "not-json")
            .is_err()
    );
    let saved = store
        .command_snapshot("schedule", "mlb", "")
        .unwrap()
        .unwrap();
    assert_eq!(saved.payload, exact);
    assert!(!saved.stale);
    assert!(
        store
            .mark_command_snapshot_stale("schedule", "mlb", "missing", "offline")
            .is_ok_and(|changed| !changed)
    );
    assert!(
        store
            .mark_command_snapshot_stale("schedule", "mlb", "", "offline")
            .unwrap()
    );
    let stale = store
        .command_snapshot("schedule", "mlb", "")
        .unwrap()
        .unwrap();
    assert_eq!(stale.payload, exact);
    assert_eq!(stale.snapshot_version, "v1");
    assert_eq!(stale.last_successful_at, saved.last_successful_at);
    assert!(stale.stale);

    clock.set(301);
    store
        .save_command_snapshot("schedule", "mlb", "", "v2", "{\"games\":[]}")
        .unwrap();
    let replaced = store
        .command_snapshot("schedule", "mlb", "")
        .unwrap()
        .unwrap();
    assert_eq!(replaced.snapshot_version, "v2");
    assert!(!replaced.stale);
    assert!(replaced.error_message.is_empty());
}

#[test]
fn seasons_are_typed_versioned_and_clocked() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.db");
    let clock = AdjustableClock::at(400);
    let mut store = Store::open_at_with_clock(&path, Arc::new(clock.clone())).unwrap();
    assert!(store.season_state("mlb", 2026).unwrap().is_none());
    assert!(!store.is_season_complete("mlb", 2026, 1).unwrap());
    store.mark_season_partial("mlb", 2026, 10, 1).unwrap();
    assert_eq!(
        store.season_state("mlb", 2026).unwrap().unwrap().status,
        SeasonSyncStatus::Partial
    );
    clock.set(401);
    store.mark_season_complete("mlb", 2026, 20, 2).unwrap();
    let complete = store.season_state("mlb", 2026).unwrap().unwrap();
    assert_eq!(complete.fetched_at, UNIX_EPOCH + Duration::from_secs(401));
    assert!(store.is_season_complete("mlb", 2026, 1).unwrap());
    assert!(store.is_season_complete("mlb", 2026, 2).unwrap());
    assert!(!store.is_season_complete("mlb", 2026, 3).unwrap());
    store.mark_season_failed("mlb", 2026, 5, 3).unwrap();
    assert!(!store.is_season_complete("mlb", 2026, 1).unwrap());
    assert!(store.mark_season_complete("mlb", 2025, -1, 1).is_err());
}

#[test]
fn sync_runs_enforce_terminal_transitions_and_deterministic_counts() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.db");
    let clock = AdjustableClock::at(500);
    let mut store = Store::open_at_with_clock(&path, Arc::new(clock.clone())).unwrap();
    assert!(store.latest_sync_run(SyncMode::Live).unwrap().is_none());
    let first = store
        .start_sync_run(SyncMode::Live, SyncOrigin::Manual)
        .unwrap();
    let running = store.latest_sync_run(SyncMode::Live).unwrap().unwrap();
    assert_eq!(running.status, SyncRunStatus::Running);
    assert_eq!(running.counts, None);
    let counts = BTreeMap::from([("players".to_owned(), 2), ("teams".to_owned(), 1)]);
    clock.set(501);
    assert!(store.complete_sync_run(first, &counts).unwrap());
    assert!(!store.complete_sync_run(first, &counts).unwrap());
    let complete = store
        .latest_successful_sync_run(SyncMode::Live, SyncOrigin::Manual)
        .unwrap()
        .unwrap();
    assert_eq!(complete.counts, Some(counts));
    let connection = Connection::open(&path).unwrap();
    let stored_counts: String = connection
        .query_row(
            "SELECT counts FROM sync_runs WHERE id = ?1",
            [first],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_counts, "{\"players\":2,\"teams\":1}");
    drop(connection);

    clock.set(502);
    let second = store
        .start_sync_run(SyncMode::Live, SyncOrigin::Automatic)
        .unwrap();
    clock.set(503);
    assert!(store.fail_sync_run(second).unwrap());
    assert!(!store.fail_sync_run(second).unwrap());
    let failed = store.latest_sync_run(SyncMode::Live).unwrap().unwrap();
    assert_eq!(failed.status, SyncRunStatus::Failed);
    assert_eq!(failed.counts, None);

    let third = store
        .start_sync_run(SyncMode::Events, SyncOrigin::Startup)
        .unwrap();
    assert!(store.complete_sync_run(third, &BTreeMap::new()).unwrap());
    assert_eq!(
        store
            .latest_sync_run(SyncMode::Events)
            .unwrap()
            .unwrap()
            .counts,
        Some(BTreeMap::new())
    );
    assert!(!store.complete_sync_run(9999, &BTreeMap::new()).unwrap());
    assert!(
        store
            .complete_sync_run(first, &BTreeMap::from([("bad".into(), -1)]))
            .is_err()
    );
}

#[test]
fn invalid_inputs_fail_before_clock_capture() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.db");
    let clock = Arc::new(CountingClock::new());
    let mut store = Store::open_at_with_clock(&path, clock.clone()).unwrap();
    assert!(store.mark_sync_item_success("", "item", "", "v1").is_err());
    assert!(
        store
            .mark_sync_item_failure("mlb", "item", "", "v1", " ")
            .is_err()
    );
    assert!(store.mark_season_complete("", 2026, 1, 1).is_err());
    assert!(
        store
            .save_command_snapshot("", "mlb", "", "v1", "{}")
            .is_err()
    );
    assert_eq!(clock.calls.load(Ordering::SeqCst), 0);
    store
        .mark_sync_item_success("mlb", "item", "", "v1")
        .unwrap();
    assert_eq!(clock.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn corrupt_stored_values_surface_contextual_errors() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.db");
    let clock = AdjustableClock::at(600);
    let mut store = Store::open_at_with_clock(&path, Arc::new(clock.clone())).unwrap();
    store
        .save_command_snapshot("schedule", "mlb", "", "v1", "{}")
        .unwrap();
    store.mark_season_complete("mlb", 2026, 1, 1).unwrap();
    store
        .mark_sync_item_success("mlb", "hitting", "", "v1")
        .unwrap();
    let run = store
        .start_sync_run(SyncMode::History, SyncOrigin::Manual)
        .unwrap();
    store.close().unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .execute("UPDATE command_snapshots SET stale = 2", [])
        .unwrap();
    connection
        .execute("UPDATE season_sync_status SET status = 'unknown'", [])
        .unwrap();
    connection
        .execute("UPDATE sync_item_state SET last_successful_at = -1", [])
        .unwrap();
    connection
        .execute(
            "UPDATE sync_runs SET status = 'complete', ended_at = 601, counts = 'bad' WHERE id = ?1",
            [run],
        )
        .unwrap();
    drop(connection);

    let store = Store::open_at_with_clock(&path, Arc::new(clock)).unwrap();
    assert!(store.command_snapshot("schedule", "mlb", "").is_err());
    assert!(store.season_state("mlb", 2026).is_err());
    assert!(store.sync_item_state("mlb", "hitting", "").is_err());
    assert!(store.latest_sync_run(SyncMode::History).is_err());
}

#[test]
fn epoch_or_earlier_clock_is_rejected() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.db");
    let epoch = AdjustableClock::at(0);
    let mut store = Store::open_at_with_clock(&path, Arc::new(epoch)).unwrap();
    let error = store
        .mark_sync_item_success("mlb", "hitting", "", "v1")
        .unwrap_err();
    assert!(error.to_string().contains("Unix epoch"));
}

#[test]
fn corrupt_enum_json_timestamp_and_sqlite_reads_are_errors() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.db");
    let clock = AdjustableClock::at(700);
    let mut store = Store::open_at_with_clock(&path, Arc::new(clock.clone())).unwrap();
    store
        .mark_sync_item_success("mlb", "unknown-status", "", "v1")
        .unwrap();
    store
        .mark_sync_row_success("mlb", "people", "", "player", "negative", None, "v1")
        .unwrap();
    store
        .mark_sync_row_success("mlb", "people", "", "player", "unknown", None, "v1")
        .unwrap();
    store
        .save_command_snapshot("bad-json", "mlb", "", "v1", "{}")
        .unwrap();
    let run = store
        .start_sync_run(SyncMode::History, SyncOrigin::Manual)
        .unwrap();
    store.close().unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE sync_item_state SET status = 'unknown' WHERE item = 'unknown-status'",
            [],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE sync_row_state SET last_successful_at = -1 WHERE entity_key = 'negative'",
            [],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE sync_row_state SET status = 'unknown' WHERE entity_key = 'unknown'",
            [],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE command_snapshots SET payload = 'bad' WHERE dataset = 'bad-json'",
            [],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE sync_runs SET origin = 'unknown' WHERE id = ?1",
            [run],
        )
        .unwrap();
    drop(connection);

    let store = Store::open_at_with_clock(&path, Arc::new(clock)).unwrap();
    assert!(store.sync_item_state("mlb", "unknown-status", "").is_err());
    assert!(
        store
            .sync_row_state("mlb", "people", "", "player", "negative")
            .is_err()
    );
    assert!(
        store
            .sync_row_state("mlb", "people", "", "player", "unknown")
            .is_err()
    );
    assert!(store.command_snapshot("bad-json", "mlb", "").is_err());
    assert!(store.latest_sync_run(SyncMode::History).is_err());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("DROP TABLE sync_item_state", [])
        .unwrap();
    drop(connection);
    assert!(
        store
            .needs_sync_item("mlb", "hitting", "", &item_policy(1, false, "v1"))
            .is_err()
    );
}
