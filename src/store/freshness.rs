//! Item- and row-level persisted freshness state.

use std::time::{Duration, SystemTime};

use rusqlite::OptionalExtension;

use super::{Store, StoreError, optional_time, validate_identity};

type ItemRow = (String, String, String, i64, i64, String, String, String);
type FreshnessRow = (
    String,
    String,
    String,
    String,
    String,
    Option<i64>,
    i64,
    i64,
    String,
    String,
    String,
);

/// A persisted item or row refresh status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncStateStatus {
    Running,
    Complete,
    Failed,
}

impl SyncStateStatus {
    fn parse(operation: &'static str, value: &str) -> Result<Self, StoreError> {
        match value {
            "running" => Ok(Self::Running),
            "complete" => Ok(Self::Complete),
            "failed" => Ok(Self::Failed),
            _ => Err(StoreError::invalid(
                operation,
                format!("unknown sync status {value:?}"),
            )),
        }
    }
}

/// Persisted freshness for one logical source item and scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncItemState {
    pub source: String,
    pub item: String,
    pub scope: String,
    pub last_attempted_at: Option<SystemTime>,
    pub last_successful_at: Option<SystemTime>,
    pub status: SyncStateStatus,
    pub error_message: String,
    pub pipeline_version: String,
}

/// Decides whether one logical item must refresh.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemRefreshPolicy {
    pub ttl: Duration,
    pub force: bool,
    pub pipeline_version: String,
}

/// Persisted freshness for one normalized source entity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncRowState {
    pub source: String,
    pub item: String,
    pub scope: String,
    pub entity_kind: String,
    pub entity_key: String,
    pub local_id: Option<i64>,
    pub last_attempted_at: Option<SystemTime>,
    pub last_successful_at: Option<SystemTime>,
    pub status: SyncStateStatus,
    pub error_message: String,
    pub pipeline_version: String,
}

/// Decides whether one normalized source entity must refresh.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RowRefreshPolicy {
    pub ttl: Duration,
    pub force: bool,
    pub pipeline_version: String,
}

impl Store {
    /// Read one item freshness row.
    pub fn sync_item_state(
        &self,
        source: &str,
        item: &str,
        scope: &str,
    ) -> Result<Option<SyncItemState>, StoreError> {
        const OPERATION: &str = "read item freshness";
        validate_item_identity(OPERATION, source, item)?;
        let row: Option<ItemRow> = self
            .connection()
            .query_row(
                "SELECT source, item, scope, last_attempted_at, last_successful_at, status, error_message, pipeline_version
                 FROM sync_item_state WHERE source = ?1 AND item = ?2 AND scope = ?3",
                (source, item, scope),
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
        row.map(|row| item_state_from_row(OPERATION, row))
            .transpose()
    }

    /// Report whether one item must refresh under the supplied policy.
    pub fn needs_sync_item(
        &self,
        source: &str,
        item: &str,
        scope: &str,
        policy: &ItemRefreshPolicy,
    ) -> Result<bool, StoreError> {
        const OPERATION: &str = "evaluate item freshness";
        validate_item_identity(OPERATION, source, item)?;
        validate_identity(OPERATION, "pipeline version", &policy.pipeline_version)?;
        if policy.force {
            return Ok(true);
        }
        let Some(state) = self.sync_item_state(source, item, scope)? else {
            return Ok(true);
        };
        if state.status != SyncStateStatus::Complete
            || state.pipeline_version != policy.pipeline_version
        {
            return Ok(true);
        }
        let Some(successful_at) = state.last_successful_at else {
            return Ok(true);
        };
        let (now, _) = self.captured_time(OPERATION)?;
        let age = now.duration_since(successful_at).unwrap_or(Duration::ZERO);
        Ok(age > policy.ttl)
    }

    /// Record an item refresh attempt without advancing prior success.
    pub fn mark_sync_item_attempt(
        &mut self,
        source: &str,
        item: &str,
        scope: &str,
        pipeline_version: &str,
    ) -> Result<(), StoreError> {
        const OPERATION: &str = "record item refresh attempt";
        validate_item_write(OPERATION, source, item, pipeline_version)?;
        let (_, now) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO sync_item_state
                     (source, item, scope, last_attempted_at, status, pipeline_version)
                     VALUES (?1, ?2, ?3, ?4, 'running', ?5)
                     ON CONFLICT(source, item, scope) DO UPDATE SET
                     last_attempted_at = excluded.last_attempted_at,
                     status = excluded.status,
                     pipeline_version = excluded.pipeline_version",
                    (source, item, scope, now, pipeline_version),
                )
                .map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(())
        })
    }

    /// Record a successful item refresh.
    pub fn mark_sync_item_success(
        &mut self,
        source: &str,
        item: &str,
        scope: &str,
        pipeline_version: &str,
    ) -> Result<(), StoreError> {
        const OPERATION: &str = "record item refresh success";
        validate_item_write(OPERATION, source, item, pipeline_version)?;
        let (_, now) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO sync_item_state
                     (source, item, scope, last_attempted_at, last_successful_at, status, error_message, pipeline_version)
                     VALUES (?1, ?2, ?3, ?4, ?4, 'complete', '', ?5)
                     ON CONFLICT(source, item, scope) DO UPDATE SET
                     last_attempted_at = excluded.last_attempted_at,
                     last_successful_at = excluded.last_successful_at,
                     status = excluded.status,
                     error_message = excluded.error_message,
                     pipeline_version = excluded.pipeline_version",
                    (source, item, scope, now, pipeline_version),
                )
                .map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(())
        })
    }

    /// Record a complete but degraded item refresh with bounded issue detail.
    pub fn mark_sync_item_degraded(
        &mut self,
        source: &str,
        item: &str,
        scope: &str,
        pipeline_version: &str,
        detail: &str,
    ) -> Result<(), StoreError> {
        const OPERATION: &str = "record degraded item refresh";
        validate_item_write(OPERATION, source, item, pipeline_version)?;
        validate_identity(OPERATION, "issue detail", detail)?;
        let (_, now) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction.execute(
                "INSERT INTO sync_item_state
                 (source, item, scope, last_attempted_at, last_successful_at, status, error_message, pipeline_version)
                 VALUES (?1, ?2, ?3, ?4, ?4, 'complete', ?5, ?6)
                 ON CONFLICT(source, item, scope) DO UPDATE SET
                 last_attempted_at = excluded.last_attempted_at,
                 last_successful_at = excluded.last_successful_at,
                 status = excluded.status,
                 error_message = excluded.error_message,
                 pipeline_version = excluded.pipeline_version",
                (source, item, scope, now, detail, pipeline_version),
            ).map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(())
        })
    }

    /// Record a failed item refresh while retaining prior success.
    pub fn mark_sync_item_failure(
        &mut self,
        source: &str,
        item: &str,
        scope: &str,
        pipeline_version: &str,
        error_message: &str,
    ) -> Result<(), StoreError> {
        const OPERATION: &str = "record item refresh failure";
        validate_item_write(OPERATION, source, item, pipeline_version)?;
        validate_identity(OPERATION, "error message", error_message)?;
        let (_, now) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO sync_item_state
                     (source, item, scope, last_attempted_at, status, error_message, pipeline_version)
                     VALUES (?1, ?2, ?3, ?4, 'failed', ?5, ?6)
                     ON CONFLICT(source, item, scope) DO UPDATE SET
                     last_attempted_at = excluded.last_attempted_at,
                     status = excluded.status,
                     error_message = excluded.error_message,
                     pipeline_version = excluded.pipeline_version",
                    (source, item, scope, now, error_message, pipeline_version),
                )
                .map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(())
        })
    }

    /// Read one row freshness record.
    pub fn sync_row_state(
        &self,
        source: &str,
        item: &str,
        scope: &str,
        entity_kind: &str,
        entity_key: &str,
    ) -> Result<Option<SyncRowState>, StoreError> {
        const OPERATION: &str = "read row freshness";
        validate_row_identity(OPERATION, source, item, entity_kind, entity_key)?;
        let row: Option<FreshnessRow> = self
            .connection()
            .query_row(
                "SELECT source, item, scope, entity_kind, entity_key, local_id,
                 last_attempted_at, last_successful_at, status, error_message, pipeline_version
                 FROM sync_row_state
                 WHERE source = ?1 AND item = ?2 AND scope = ?3 AND entity_kind = ?4 AND entity_key = ?5",
                (source, item, scope, entity_kind, entity_key),
                |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                        row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?))
                },
            )
            .optional()
            .map_err(|error| StoreError::operation(OPERATION, &self.path, error))?;
        row.map(|row| row_state_from_row(OPERATION, row))
            .transpose()
    }

    /// Report whether one row must refresh under the supplied policy.
    pub fn needs_sync_row(
        &self,
        source: &str,
        item: &str,
        scope: &str,
        entity_kind: &str,
        entity_key: &str,
        policy: &RowRefreshPolicy,
    ) -> Result<bool, StoreError> {
        const OPERATION: &str = "evaluate row freshness";
        validate_row_identity(OPERATION, source, item, entity_kind, entity_key)?;
        validate_identity(OPERATION, "pipeline version", &policy.pipeline_version)?;
        if policy.force {
            return Ok(true);
        }
        let Some(state) = self.sync_row_state(source, item, scope, entity_kind, entity_key)? else {
            return Ok(true);
        };
        if state.status != SyncStateStatus::Complete
            || state.pipeline_version != policy.pipeline_version
        {
            return Ok(true);
        }
        let Some(successful_at) = state.last_successful_at else {
            return Ok(true);
        };
        let (now, _) = self.captured_time(OPERATION)?;
        let age = now.duration_since(successful_at).unwrap_or(Duration::ZERO);
        Ok(age > policy.ttl)
    }

    /// Record successful freshness for one source row.
    #[allow(clippy::too_many_arguments)]
    pub fn mark_sync_row_success(
        &mut self,
        source: &str,
        item: &str,
        scope: &str,
        entity_kind: &str,
        entity_key: &str,
        local_id: Option<i64>,
        pipeline_version: &str,
    ) -> Result<(), StoreError> {
        const OPERATION: &str = "record row refresh success";
        validate_row_write(
            OPERATION,
            source,
            item,
            entity_kind,
            entity_key,
            local_id,
            pipeline_version,
        )?;
        let (_, now) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction.execute(
                "INSERT INTO sync_row_state
                 (source, item, scope, entity_kind, entity_key, local_id, last_attempted_at,
                  last_successful_at, status, error_message, pipeline_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 'complete', '', ?8)
                 ON CONFLICT(source, item, scope, entity_kind, entity_key) DO UPDATE SET
                 local_id = excluded.local_id, last_attempted_at = excluded.last_attempted_at,
                 last_successful_at = excluded.last_successful_at, status = excluded.status,
                 error_message = excluded.error_message, pipeline_version = excluded.pipeline_version",
                (source, item, scope, entity_kind, entity_key, local_id, now, pipeline_version),
            ).map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(())
        })
    }

    /// Record failed freshness for one source row while retaining prior success.
    #[allow(clippy::too_many_arguments)]
    pub fn mark_sync_row_failure(
        &mut self,
        source: &str,
        item: &str,
        scope: &str,
        entity_kind: &str,
        entity_key: &str,
        local_id: Option<i64>,
        pipeline_version: &str,
        error_message: &str,
    ) -> Result<(), StoreError> {
        const OPERATION: &str = "record row refresh failure";
        validate_row_write(
            OPERATION,
            source,
            item,
            entity_kind,
            entity_key,
            local_id,
            pipeline_version,
        )?;
        validate_identity(OPERATION, "error message", error_message)?;
        let (_, now) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO sync_row_state
                 (source, item, scope, entity_kind, entity_key, local_id, last_attempted_at,
                  status, error_message, pipeline_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'failed', ?8, ?9)
                 ON CONFLICT(source, item, scope, entity_kind, entity_key) DO UPDATE SET
                 local_id = excluded.local_id, last_attempted_at = excluded.last_attempted_at,
                 status = excluded.status, error_message = excluded.error_message,
                 pipeline_version = excluded.pipeline_version",
                    (
                        source,
                        item,
                        scope,
                        entity_kind,
                        entity_key,
                        local_id,
                        now,
                        error_message,
                        pipeline_version,
                    ),
                )
                .map_err(|error| StoreError::operation(OPERATION, &path, error))?;
            Ok(())
        })
    }
}

fn validate_item_identity(
    operation: &'static str,
    source: &str,
    item: &str,
) -> Result<(), StoreError> {
    validate_identity(operation, "source", source)?;
    validate_identity(operation, "item", item)
}

fn validate_item_write(
    operation: &'static str,
    source: &str,
    item: &str,
    pipeline: &str,
) -> Result<(), StoreError> {
    validate_item_identity(operation, source, item)?;
    validate_identity(operation, "pipeline version", pipeline)
}

fn validate_row_identity(
    operation: &'static str,
    source: &str,
    item: &str,
    kind: &str,
    key: &str,
) -> Result<(), StoreError> {
    validate_item_identity(operation, source, item)?;
    validate_identity(operation, "entity kind", kind)?;
    validate_identity(operation, "entity key", key)
}

fn validate_row_write(
    operation: &'static str,
    source: &str,
    item: &str,
    kind: &str,
    key: &str,
    local_id: Option<i64>,
    pipeline: &str,
) -> Result<(), StoreError> {
    validate_row_identity(operation, source, item, kind, key)?;
    if local_id.is_some_and(|id| id <= 0) {
        return Err(StoreError::invalid(
            operation,
            "local ID must be positive when present",
        ));
    }
    validate_identity(operation, "pipeline version", pipeline)
}

fn item_state_from_row(operation: &'static str, row: ItemRow) -> Result<SyncItemState, StoreError> {
    Ok(SyncItemState {
        source: row.0,
        item: row.1,
        scope: row.2,
        last_attempted_at: optional_time(operation, "last attempted timestamp", row.3)?,
        last_successful_at: optional_time(operation, "last successful timestamp", row.4)?,
        status: SyncStateStatus::parse(operation, &row.5)?,
        error_message: row.6,
        pipeline_version: row.7,
    })
}

fn row_state_from_row(
    operation: &'static str,
    row: FreshnessRow,
) -> Result<SyncRowState, StoreError> {
    if row.5.is_some_and(|id| id <= 0) {
        return Err(StoreError::invalid(
            operation,
            "stored local ID must be positive",
        ));
    }
    Ok(SyncRowState {
        source: row.0,
        item: row.1,
        scope: row.2,
        entity_kind: row.3,
        entity_key: row.4,
        local_id: row.5,
        last_attempted_at: optional_time(operation, "last attempted timestamp", row.6)?,
        last_successful_at: optional_time(operation, "last successful timestamp", row.7)?,
        status: SyncStateStatus::parse(operation, &row.8)?,
        error_message: row.9,
        pipeline_version: row.10,
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
    fn injected_item_write_failure_preserves_prior_row() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("state.db");
        let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock)).unwrap();
        store
            .mark_sync_item_success("mlb", "hitting", "", "v1")
            .unwrap();
        store
            .connection()
            .execute_batch(
                "CREATE TRIGGER reject_item_update BEFORE UPDATE ON sync_item_state
                 BEGIN SELECT RAISE(ABORT, 'injected'); END;",
            )
            .unwrap();
        assert!(
            store
                .mark_sync_item_failure("mlb", "hitting", "", "v2", "offline")
                .is_err()
        );
        let state = store
            .sync_item_state("mlb", "hitting", "")
            .unwrap()
            .unwrap();
        assert_eq!(state.status, SyncStateStatus::Complete);
        assert_eq!(state.pipeline_version, "v1");
    }
}
