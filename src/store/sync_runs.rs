//! Typed synchronization-run lifecycle state.

use std::collections::BTreeMap;
use std::time::SystemTime;

use rusqlite::OptionalExtension;

use super::{Store, StoreError, required_time, validate_identity};

type RunRow = (
    i64,
    String,
    i64,
    Option<i64>,
    String,
    Option<String>,
    String,
);

/// A synchronization run's operating mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncMode {
    Live,
    Events,
    History,
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
    fn injected_run_write_failure_preserves_running_row() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("state.db");
        let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock)).unwrap();
        let id = store
            .start_sync_run(SyncMode::Live, SyncOrigin::Manual)
            .unwrap();
        store
            .connection()
            .execute_batch(
                "CREATE TRIGGER reject_run_update BEFORE UPDATE ON sync_runs
                 BEGIN SELECT RAISE(ABORT, 'injected'); END;",
            )
            .unwrap();
        assert!(store.complete_sync_run(id, &BTreeMap::new()).is_err());
        let run = store.latest_sync_run(SyncMode::Live).unwrap().unwrap();
        assert_eq!(run.status, SyncRunStatus::Running);
        assert_eq!(run.ended_at, None);
    }
}

impl SyncMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Events => "events",
            Self::History => "history",
        }
    }

    fn parse(operation: &'static str, value: &str) -> Result<Self, StoreError> {
        match value {
            "live" => Ok(Self::Live),
            "events" => Ok(Self::Events),
            "history" => Ok(Self::History),
            _ => Err(StoreError::invalid(
                operation,
                format!("unknown sync mode {value:?}"),
            )),
        }
    }
}

/// The trigger that started a synchronization run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncOrigin {
    Manual,
    Automatic,
    Startup,
    PublicPull,
}

impl SyncOrigin {
    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Automatic => "automatic",
            Self::Startup => "startup",
            Self::PublicPull => "public_pull",
        }
    }

    fn parse(operation: &'static str, value: &str) -> Result<Self, StoreError> {
        match value {
            "manual" => Ok(Self::Manual),
            "automatic" => Ok(Self::Automatic),
            "startup" => Ok(Self::Startup),
            "public_pull" => Ok(Self::PublicPull),
            _ => Err(StoreError::invalid(
                operation,
                format!("unknown sync origin {value:?}"),
            )),
        }
    }
}

/// A synchronization run's lifecycle status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncRunStatus {
    Running,
    Complete,
    Failed,
}

impl SyncRunStatus {
    fn parse(operation: &'static str, value: &str) -> Result<Self, StoreError> {
        match value {
            "running" => Ok(Self::Running),
            "complete" => Ok(Self::Complete),
            "failed" => Ok(Self::Failed),
            _ => Err(StoreError::invalid(
                operation,
                format!("unknown sync run status {value:?}"),
            )),
        }
    }
}

/// One persisted synchronization run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncRun {
    pub id: i64,
    pub mode: SyncMode,
    pub origin: SyncOrigin,
    pub started_at: SystemTime,
    pub ended_at: Option<SystemTime>,
    pub status: SyncRunStatus,
    pub counts: Option<BTreeMap<String, i64>>,
}

impl Store {
    /// Start one synchronization run.
    pub fn start_sync_run(
        &mut self,
        mode: SyncMode,
        origin: SyncOrigin,
    ) -> Result<i64, StoreError> {
        const OPERATION: &str = "start sync run";
        let (_, now) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO sync_runs (mode, started_at, status, origin, counts)
                     VALUES (?1, ?2, 'running', ?3, NULL)",
                    (mode.as_str(), now, origin.as_str()),
                )
                .map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(transaction.last_insert_rowid())
        })
    }

    /// Complete one currently running synchronization run.
    pub fn complete_sync_run(
        &mut self,
        id: i64,
        counts: &BTreeMap<String, i64>,
    ) -> Result<bool, StoreError> {
        const OPERATION: &str = "complete sync run";
        validate_run_id(OPERATION, id)?;
        validate_counts(OPERATION, counts)?;
        let counts_json = serde_json::to_string(counts).map_err(|error| {
            StoreError::invalid(OPERATION, format!("serialize run counts: {error}"))
        })?;
        let (_, now) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE sync_runs SET ended_at = ?1, status = 'complete', counts = ?2
                     WHERE id = ?3 AND status = 'running'",
                    (now, counts_json.as_str(), id),
                )
                .map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(changed > 0)
        })
    }

    /// Fail one currently running synchronization run.
    pub fn fail_sync_run(&mut self, id: i64) -> Result<bool, StoreError> {
        const OPERATION: &str = "fail sync run";
        validate_run_id(OPERATION, id)?;
        let (_, now) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE sync_runs SET ended_at = ?1, status = 'failed', counts = NULL
                     WHERE id = ?2 AND status = 'running'",
                    (now, id),
                )
                .map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(changed > 0)
        })
    }

    /// Read the most recent synchronization run for one mode.
    pub fn latest_sync_run(&self, mode: SyncMode) -> Result<Option<SyncRun>, StoreError> {
        self.query_latest_sync_run(
            "read latest sync run",
            "SELECT id, mode, started_at, ended_at, status, counts, origin
             FROM sync_runs WHERE mode = ?1 ORDER BY id DESC LIMIT 1",
            (mode.as_str(), None),
        )
    }

    /// Read the most recent completed run for one mode and origin.
    pub fn latest_successful_sync_run(
        &self,
        mode: SyncMode,
        origin: SyncOrigin,
    ) -> Result<Option<SyncRun>, StoreError> {
        self.query_latest_sync_run(
            "read latest successful sync run",
            "SELECT id, mode, started_at, ended_at, status, counts, origin
             FROM sync_runs
             WHERE mode = ?1 AND origin = ?2 AND status = 'complete'
             ORDER BY id DESC LIMIT 1",
            (mode.as_str(), Some(origin.as_str())),
        )
    }

    /// Read the origin of the data currently reflected in the durable
    /// fantasy tables: the `origin` of the most recent **completed** run,
    /// not merely the most recent run regardless of status. A failed `sync`
    /// or `pp` attempt never changes this answer, since it never wrote
    /// anything durable.
    pub fn current_data_origin(&self, mode: SyncMode) -> Result<Option<SyncOrigin>, StoreError> {
        self.query_latest_sync_run(
            "read current data origin",
            "SELECT id, mode, started_at, ended_at, status, counts, origin
             FROM sync_runs WHERE mode = ?1 AND status = 'complete' ORDER BY id DESC LIMIT 1",
            (mode.as_str(), None),
        )
        .map(|run| run.map(|run| run.origin))
    }

    fn query_latest_sync_run(
        &self,
        operation: &'static str,
        sql: &str,
        parameters: (&str, Option<&str>),
    ) -> Result<Option<SyncRun>, StoreError> {
        let row: Option<RunRow> = match parameters.1 {
            Some(origin) => self
                .connection()
                .query_row(sql, (parameters.0, origin), read_row),
            None => self.connection().query_row(sql, [parameters.0], read_row),
        }
        .optional()
        .map_err(|error| StoreError::operation(operation, &self.path, error))?;
        row.map(|row| sync_run_from_row(operation, row)).transpose()
    }
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn validate_run_id(operation: &'static str, id: i64) -> Result<(), StoreError> {
    if id <= 0 {
        return Err(StoreError::invalid(operation, "run ID must be positive"));
    }
    Ok(())
}

fn validate_counts(
    operation: &'static str,
    counts: &BTreeMap<String, i64>,
) -> Result<(), StoreError> {
    for (key, value) in counts {
        validate_identity(operation, "count key", key)?;
        if *value < 0 {
            return Err(StoreError::invalid(
                operation,
                format!("count {key:?} must not be negative"),
            ));
        }
    }
    Ok(())
}

fn sync_run_from_row(operation: &'static str, row: RunRow) -> Result<SyncRun, StoreError> {
    validate_run_id(operation, row.0)?;
    let status = SyncRunStatus::parse(operation, &row.4)?;
    let counts = row
        .5
        .map(|json| {
            serde_json::from_str::<BTreeMap<String, i64>>(&json).map_err(|error| {
                StoreError::invalid(
                    operation,
                    format!("stored counts are invalid JSON: {error}"),
                )
            })
        })
        .transpose()?;
    if let Some(counts) = &counts {
        validate_counts(operation, counts)?;
    }
    match status {
        SyncRunStatus::Running if row.3.is_some() || counts.is_some() => {
            return Err(StoreError::invalid(
                operation,
                "running run must not have ended time or counts",
            ));
        }
        SyncRunStatus::Complete if row.3.is_none() || counts.is_none() => {
            return Err(StoreError::invalid(
                operation,
                "complete run must have ended time and counts",
            ));
        }
        SyncRunStatus::Failed if row.3.is_none() || counts.is_some() => {
            return Err(StoreError::invalid(
                operation,
                "failed run must have ended time and no counts",
            ));
        }
        _ => {}
    }
    Ok(SyncRun {
        id: row.0,
        mode: SyncMode::parse(operation, &row.1)?,
        origin: SyncOrigin::parse(operation, &row.6)?,
        started_at: required_time(operation, "started timestamp", row.2)?,
        ended_at: row
            .3
            .map(|value| required_time(operation, "ended timestamp", value))
            .transpose()?,
        status,
        counts,
    })
}
