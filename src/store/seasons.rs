//! Typed season-completeness manifests.

use std::time::SystemTime;

use rusqlite::OptionalExtension;

use super::{Store, StoreError, required_time, validate_identity};

/// The completeness state for one source season.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeasonSyncStatus {
    Complete,
    Partial,
    Failed,
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
    fn injected_season_write_failure_preserves_prior_row() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("state.db");
        let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock)).unwrap();
        store.mark_season_complete("mlb", 2026, 20, 1).unwrap();
        store
            .connection()
            .execute_batch(
                "CREATE TRIGGER reject_season_update BEFORE UPDATE ON season_sync_status
                 BEGIN SELECT RAISE(ABORT, 'injected'); END;",
            )
            .unwrap();
        assert!(store.mark_season_failed("mlb", 2026, 5, 2).is_err());
        let state = store.season_state("mlb", 2026).unwrap().unwrap();
        assert_eq!(state.status, SeasonSyncStatus::Complete);
        assert_eq!(state.pipeline_version, 1);
    }
}

impl SeasonSyncStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Failed => "failed",
        }
    }

    fn parse(operation: &'static str, value: &str) -> Result<Self, StoreError> {
        match value {
            "complete" => Ok(Self::Complete),
            "partial" => Ok(Self::Partial),
            "failed" => Ok(Self::Failed),
            _ => Err(StoreError::invalid(
                operation,
                format!("unknown season status {value:?}"),
            )),
        }
    }
}

/// Persisted completeness for one source and season.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeasonState {
    pub source: String,
    pub season: i64,
    pub status: SeasonSyncStatus,
    pub fetched_at: SystemTime,
    pub record_count: i64,
    pub pipeline_version: i64,
}

impl Store {
    /// Read one source-season completeness row.
    pub fn season_state(
        &self,
        source: &str,
        season: i64,
    ) -> Result<Option<SeasonState>, StoreError> {
        const OPERATION: &str = "read season state";
        validate_identity(OPERATION, "source", source)?;
        let row: Option<(String, i64, String, i64, i64, i64)> = self
            .connection()
            .query_row(
                "SELECT source, season, status, fetched_at, record_count, pipeline_version
                 FROM season_sync_status WHERE source = ?1 AND season = ?2",
                (source, season),
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| StoreError::operation(OPERATION, &self.path, error))?;
        row.map(|row| season_from_row(OPERATION, row)).transpose()
    }

    /// Report whether one source season is complete at the requested pipeline version.
    pub fn is_season_complete(
        &self,
        source: &str,
        season: i64,
        pipeline_version: i64,
    ) -> Result<bool, StoreError> {
        const OPERATION: &str = "evaluate season completeness";
        validate_identity(OPERATION, "source", source)?;
        validate_nonnegative(OPERATION, "pipeline version", pipeline_version)?;
        let Some(state) = self.season_state(source, season)? else {
            return Ok(false);
        };
        Ok(
            state.status == SeasonSyncStatus::Complete
                && state.pipeline_version >= pipeline_version,
        )
    }

    /// Mark one source season complete.
    pub fn mark_season_complete(
        &mut self,
        source: &str,
        season: i64,
        record_count: i64,
        pipeline_version: i64,
    ) -> Result<(), StoreError> {
        self.write_season_state(
            source,
            season,
            SeasonSyncStatus::Complete,
            record_count,
            pipeline_version,
        )
    }

    /// Mark one source season partial.
    pub fn mark_season_partial(
        &mut self,
        source: &str,
        season: i64,
        record_count: i64,
        pipeline_version: i64,
    ) -> Result<(), StoreError> {
        self.write_season_state(
            source,
            season,
            SeasonSyncStatus::Partial,
            record_count,
            pipeline_version,
        )
    }

    /// Mark one source season failed.
    pub fn mark_season_failed(
        &mut self,
        source: &str,
        season: i64,
        record_count: i64,
        pipeline_version: i64,
    ) -> Result<(), StoreError> {
        self.write_season_state(
            source,
            season,
            SeasonSyncStatus::Failed,
            record_count,
            pipeline_version,
        )
    }

    fn write_season_state(
        &mut self,
        source: &str,
        season: i64,
        status: SeasonSyncStatus,
        record_count: i64,
        pipeline_version: i64,
    ) -> Result<(), StoreError> {
        const OPERATION: &str = "write season state";
        validate_identity(OPERATION, "source", source)?;
        validate_nonnegative(OPERATION, "record count", record_count)?;
        validate_nonnegative(OPERATION, "pipeline version", pipeline_version)?;
        let (_, now) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO season_sync_status
                     (source, season, status, fetched_at, record_count, pipeline_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(source, season) DO UPDATE SET
                     status = excluded.status, fetched_at = excluded.fetched_at,
                     record_count = excluded.record_count,
                     pipeline_version = excluded.pipeline_version",
                    (
                        source,
                        season,
                        status.as_str(),
                        now,
                        record_count,
                        pipeline_version,
                    ),
                )
                .map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(())
        })
    }
}

fn validate_nonnegative(
    operation: &'static str,
    field: &'static str,
    value: i64,
) -> Result<(), StoreError> {
    if value < 0 {
        return Err(StoreError::invalid(
            operation,
            format!("{field} must not be negative"),
        ));
    }
    Ok(())
}

fn season_from_row(
    operation: &'static str,
    row: (String, i64, String, i64, i64, i64),
) -> Result<SeasonState, StoreError> {
    validate_nonnegative(operation, "stored record count", row.4)?;
    validate_nonnegative(operation, "stored pipeline version", row.5)?;
    Ok(SeasonState {
        source: row.0,
        season: row.1,
        status: SeasonSyncStatus::parse(operation, &row.2)?,
        fetched_at: required_time(operation, "fetched timestamp", row.3)?,
        record_count: row.4,
        pipeline_version: row.5,
    })
}
