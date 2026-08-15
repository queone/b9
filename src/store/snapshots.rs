//! Validated durable command-dataset snapshots.

use std::time::SystemTime;

use rusqlite::OptionalExtension;

use super::{Store, StoreError, required_time, validate_identity};

/// The latest successful payload for one command dataset identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSnapshot {
    pub dataset: String,
    pub source: String,
    pub scope: String,
    pub snapshot_version: String,
    pub payload: String,
    pub last_successful_at: SystemTime,
    pub stale: bool,
    pub error_message: String,
}

struct SnapshotWrite<'a> {
    dataset: &'a str,
    source: &'a str,
    scope: &'a str,
    snapshot_version: &'a str,
    payload: &'a str,
    now: i64,
}

impl Store {
    /// Atomically replace one complete, valid JSON snapshot.
    pub fn save_command_snapshot(
        &mut self,
        dataset: &str,
        source: &str,
        scope: &str,
        snapshot_version: &str,
        payload: &str,
    ) -> Result<(), StoreError> {
        const OPERATION: &str = "save command snapshot";
        validate_snapshot_identity(OPERATION, dataset, source)?;
        validate_identity(OPERATION, "snapshot version", snapshot_version)?;
        validate_identity(OPERATION, "payload", payload)?;
        validate_json(OPERATION, payload)?;
        let (_, now) = self.captured_time(OPERATION)?;
        self.save_command_snapshot_inner(
            SnapshotWrite {
                dataset,
                source,
                scope,
                snapshot_version,
                payload,
                now,
            },
            |_| Ok(()),
        )
    }

    /// Read the latest successful snapshot, including stale metadata.
    pub fn command_snapshot(
        &self,
        dataset: &str,
        source: &str,
        scope: &str,
    ) -> Result<Option<CommandSnapshot>, StoreError> {
        const OPERATION: &str = "read command snapshot";
        validate_snapshot_identity(OPERATION, dataset, source)?;
        type Row = (String, String, String, String, String, i64, i64, String);
        let row: Option<Row> = self
            .connection()
            .query_row(
                "SELECT dataset, source, scope, snapshot_version, payload, last_successful_at,
                 stale, error_message FROM command_snapshots
                 WHERE dataset = ?1 AND source = ?2 AND scope = ?3",
                (dataset, source, scope),
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| StoreError::operation(OPERATION, &self.path, error))?;
        row.map(|row| snapshot_from_row(OPERATION, row)).transpose()
    }

    /// Mark an existing snapshot stale without replacing its successful payload.
    pub fn mark_command_snapshot_stale(
        &mut self,
        dataset: &str,
        source: &str,
        scope: &str,
        error_message: &str,
    ) -> Result<bool, StoreError> {
        const OPERATION: &str = "mark command snapshot stale";
        validate_snapshot_identity(OPERATION, dataset, source)?;
        validate_identity(OPERATION, "error message", error_message)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE command_snapshots SET stale = 1, error_message = ?1
                     WHERE dataset = ?2 AND source = ?3 AND scope = ?4",
                    (error_message, dataset, source, scope),
                )
                .map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(changed > 0)
        })
    }

    fn save_command_snapshot_inner<F>(
        &mut self,
        write: SnapshotWrite<'_>,
        after_write: F,
    ) -> Result<(), StoreError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<(), StoreError>,
    {
        const OPERATION: &str = "save command snapshot";
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO command_snapshots
                     (dataset, source, scope, snapshot_version, payload, last_successful_at, stale, error_message)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, '')
                     ON CONFLICT(dataset, source, scope) DO UPDATE SET
                     snapshot_version = excluded.snapshot_version,
                     payload = excluded.payload,
                     last_successful_at = excluded.last_successful_at,
                     stale = excluded.stale,
                     error_message = excluded.error_message",
                    (
                        write.dataset,
                        write.source,
                        write.scope,
                        write.snapshot_version,
                        write.payload,
                        write.now,
                    ),
                )
                .map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            after_write(transaction)
        })
    }
}

fn validate_snapshot_identity(
    operation: &'static str,
    dataset: &str,
    source: &str,
) -> Result<(), StoreError> {
    validate_identity(operation, "dataset", dataset)?;
    validate_identity(operation, "source", source)
}

fn validate_json(operation: &'static str, payload: &str) -> Result<(), StoreError> {
    serde_json::from_str::<serde_json::Value>(payload)
        .map(|_| ())
        .map_err(|error| {
            StoreError::invalid(operation, format!("payload is not valid JSON: {error}"))
        })
}

fn snapshot_from_row(
    operation: &'static str,
    row: (String, String, String, String, String, i64, i64, String),
) -> Result<CommandSnapshot, StoreError> {
    validate_json(operation, &row.4)?;
    let stale = match row.6 {
        0 => false,
        1 => true,
        value => {
            return Err(StoreError::invalid(
                operation,
                format!("stored stale value must be 0 or 1, got {value}"),
            ));
        }
    };
    Ok(CommandSnapshot {
        dataset: row.0,
        source: row.1,
        scope: row.2,
        snapshot_version: row.3,
        payload: row.4,
        last_successful_at: required_time(operation, "last successful timestamp", row.5)?,
        stale,
        error_message: row.7,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    use tempfile::tempdir;

    use super::*;
    use crate::store::Clock;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(100)
        }
    }

    #[test]
    fn injected_post_write_failure_rolls_back_snapshot_replacement() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("state.db");
        let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock)).unwrap();
        store
            .save_command_snapshot("schedule", "mlb", "", "v1", "{\"old\":1}")
            .unwrap();
        let error = store.save_command_snapshot_inner(
            SnapshotWrite {
                dataset: "schedule",
                source: "mlb",
                scope: "",
                snapshot_version: "v2",
                payload: "{\"new\":2}",
                now: 101,
            },
            |_| Err(StoreError::invalid("inject snapshot failure", "injected")),
        );
        assert!(error.is_err());
        let snapshot = store
            .command_snapshot("schedule", "mlb", "")
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.snapshot_version, "v1");
        assert_eq!(snapshot.payload, "{\"old\":1}");
    }
}
