//! Isolated b9 SQLite storage ownership, schema migration, and transactions.

use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};

const SCHEMA: &str = include_str!("store/schema.sql");

mod fantasy;
mod freshness;
mod mlb;
mod odds;
mod seasons;
mod snapshots;
mod statcast;
mod sync_runs;

pub use fantasy::{
    CategoryWrite, FantasySnapshotWrite, IdentityCandidate, PositionWrite, StoredFantasyCategory,
    StoredFantasyTeam,
};
pub use freshness::{
    ItemRefreshPolicy, RowRefreshPolicy, SyncItemState, SyncRowState, SyncStateStatus,
};
pub use mlb::{RosterWrite, SeasonStatWrite, StoredRosterPlayer, WaiverCandidate};
pub use odds::{MoneylineQuote, StoredMoneyline};
pub use seasons::{SeasonState, SeasonSyncStatus};
pub use snapshots::CommandSnapshot;
pub use statcast::StatcastWrite;
pub use sync_runs::{SyncMode, SyncOrigin, SyncRun, SyncRunStatus};

/// The current schema version for b9-owned databases.
pub const CURRENT_SCHEMA_VERSION: i64 = 4;

/// Read-only production status fields used by `b9 st`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StoreStatus {
    pub latest_sync_status: Option<String>,
    pub latest_sync_at: Option<i64>,
    pub league_synced_at: Option<i64>,
    pub circuit_open: bool,
    pub provider_failure_count: i64,
    pub provider_last_error: Option<String>,
    pub provider_freshness_at: Option<i64>,
    pub last_run_at: Option<i64>,
    pub last_run_status: Option<String>,
    pub schema_version: Option<i64>,
    pub database_bytes: Option<u64>,
    pub mlb_identity_count: i64,
    pub yahoo_identity_count: i64,
    pub unmatched_player_count: i64,
}

/// Durable provider fields used by the local status dashboard.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DashboardStatus {
    pub provider_last_success_at: Option<i64>,
    pub provider_last_failure_at: Option<i64>,
    pub provider_failure_count: i64,
    pub circuit_open: bool,
    pub last_error: Option<String>,
    pub provider_freshness_at: Option<i64>,
    pub last_run_at: Option<i64>,
    pub last_run_status: Option<String>,
}

/// Inspect an existing database without creating, migrating, or changing it.
pub fn inspect_status_at(path: &Path, league_key: &str) -> Result<StoreStatus, StoreError> {
    if !path.is_file() {
        return Ok(StoreStatus::default());
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| StoreError::operation("open database status", path, error))?;
    let (latest_sync_status, latest_sync_at) = connection
        .query_row(
            "SELECT status, COALESCE(ended_at, started_at) FROM sync_runs WHERE mode='live' ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StoreError::operation("read latest sync status", path, error))?
        .map_or((None, None), |(status, time)| (Some(status), Some(time)));
    let league_synced_at = if league_key.is_empty() {
        None
    } else {
        connection
            .query_row(
                "SELECT synced_at FROM yahoo_leagues WHERE league_key=?1",
                [league_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StoreError::operation("read league freshness", path, error))?
    };
    let dashboard_table_exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='dashboard_status'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StoreError::operation("check dashboard status table", path, error))?;
    let dashboard = if dashboard_table_exists == 0 {
        // Older, not-yet-migrated databases predate the dashboard_status table.
        // `inspect_status_at` never migrates, so fall back to defaults instead
        // of failing a local read on a schema this connection won't upgrade.
        (0, false, None, None, None, None)
    } else {
        connection
            .query_row(
                "SELECT provider_failure_count, circuit_open, last_error, provider_freshness_at, last_run_at, last_run_status FROM dashboard_status WHERE id=1",
                [],
                |row| {
                    let error = row.get::<_, String>(2)?;
                    let status = row.get::<_, Option<String>>(5)?;
                    Ok((
                        row.get(0)?,
                        row.get::<_, i64>(1)? != 0,
                        (!error.is_empty()).then_some(error),
                        row.get(3)?,
                        row.get(4)?,
                        status.filter(|value| !value.is_empty()),
                    ))
                },
            )
            .optional()
            .map_err(|error| StoreError::operation("read dashboard status", path, error))?
            .unwrap_or((0, false, None, None, None, None))
    };
    let schema_version = connection
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .optional()
        .map_err(|error| StoreError::operation("read schema version", path, error))?;
    let database_bytes = fs::metadata(path).ok().map(|metadata| metadata.len());
    let mlb_identity_count = connection
        .query_row(
            "SELECT COUNT(*) FROM players WHERE mlbam_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StoreError::operation("count MLB identities", path, error))?;
    let yahoo_identity_count = connection
        .query_row(
            "SELECT COUNT(*) FROM players WHERE yahoo_player_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StoreError::operation("count Yahoo identities", path, error))?;
    let unmatched_player_count = connection
        .query_row(
            "SELECT COUNT(*) FROM players WHERE mlbam_id IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StoreError::operation("count unmatched players", path, error))?;
    Ok(StoreStatus {
        latest_sync_status,
        latest_sync_at,
        league_synced_at,
        provider_failure_count: dashboard.0,
        circuit_open: dashboard.1,
        provider_last_error: dashboard.2,
        provider_freshness_at: dashboard.3,
        last_run_at: dashboard.4,
        last_run_status: dashboard.5,
        schema_version,
        database_bytes,
        mlb_identity_count,
        yahoo_identity_count,
        unmatched_player_count,
    })
}

/// Supplies time to durable store state transitions.
pub trait Clock: Send + Sync {
    /// Return the current wall-clock time.
    fn now(&self) -> SystemTime;
}

/// Supplies host wall-clock time to production stores.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// A contextual failure at the b9 storage boundary.
#[derive(Debug)]
pub enum StoreError {
    /// The production database path cannot be resolved.
    HomeUnavailable,
    /// A caller supplied an invalid storage value.
    InvalidInput {
        operation: &'static str,
        detail: String,
    },
    /// A clock or stored timestamp cannot be represented safely.
    InvalidTime {
        operation: &'static str,
        detail: String,
    },
    /// One storage operation failed.
    Operation {
        operation: &'static str,
        path: PathBuf,
        source: Box<dyn Error + Send + Sync>,
    },
    /// A database state cannot be migrated safely.
    UnsupportedSchema { path: PathBuf, detail: String },
    /// A transaction operation and its rollback both failed.
    TransactionRollback {
        operation_error: Box<StoreError>,
        rollback_error: Box<StoreError>,
    },
}

impl StoreError {
    fn operation(
        operation: &'static str,
        path: &Path,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::Operation {
            operation,
            path: path.to_path_buf(),
            source: Box::new(source),
        }
    }

    fn unsupported(path: &Path, detail: impl Into<String>) -> Self {
        Self::UnsupportedSchema {
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }

    fn invalid(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::InvalidInput {
            operation,
            detail: detail.into(),
        }
    }

    fn invalid_time(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::InvalidTime {
            operation,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HomeUnavailable => write!(
                formatter,
                "resolve database path: HOME is unavailable; set HOME to the user home directory and retry"
            ),
            Self::InvalidInput { operation, detail } => write!(
                formatter,
                "{operation}: {detail}; correct the value and retry"
            ),
            Self::InvalidTime { operation, detail } => write!(
                formatter,
                "{operation}: {detail}; correct the clock or stored timestamp and retry"
            ),
            Self::Operation {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "{operation} {}: {source}; check the path and permissions, then retry",
                path.display()
            ),
            Self::UnsupportedSchema { path, detail } => write!(
                formatter,
                "inspect schema {}: {detail}; move the database aside or import it through a supported migration",
                path.display()
            ),
            Self::TransactionRollback {
                operation_error,
                rollback_error,
            } => write!(
                formatter,
                "transaction operation failed ({operation_error}) and rollback also failed ({rollback_error})"
            ),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation { source, .. } => Some(source.as_ref()),
            Self::TransactionRollback {
                operation_error, ..
            } => Some(operation_error.as_ref()),
            Self::HomeUnavailable
            | Self::InvalidInput { .. }
            | Self::InvalidTime { .. }
            | Self::UnsupportedSchema { .. } => None,
        }
    }
}

/// Owns one connection to an isolated b9 SQLite database.
pub struct Store {
    connection: Option<Connection>,
    path: PathBuf,
    clock: Arc<dyn Clock>,
}

impl Store {
    /// Open the production b9 database and migrate it to the current schema.
    pub fn open() -> Result<Self, StoreError> {
        let path = database_path()?;
        Self::open_at(path)
    }

    /// Open an explicit database path and migrate it to the current schema.
    pub fn open_at(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::open_at_with_clock(path, Arc::new(SystemClock))
    }

    /// Open an explicit database path with a controlled clock.
    pub fn open_at_with_clock(
        path: impl AsRef<Path>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        prepare_path(&path)?;
        let mut connection = Connection::open(&path)
            .map_err(|error| StoreError::operation("open database", &path, error))?;
        connection
            .busy_timeout(Duration::from_millis(5000))
            .map_err(|error| StoreError::operation("set busy timeout", &path, error))?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| StoreError::operation("enable WAL journal mode", &path, error))?;
        migrate(&mut connection, &path, SCHEMA)?;
        Ok(Self {
            connection: Some(connection),
            path,
            clock,
        })
    }

    /// Close the owned connection explicitly.
    pub fn close(mut self) -> Result<(), StoreError> {
        let connection = self.connection.take().expect("open store connection");
        connection
            .close()
            .map_err(|(_, error)| StoreError::operation("close database", &self.path, error))
    }

    /// Return this store's database path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the one current schema-version row.
    pub fn schema_version(&self) -> Result<i64, StoreError> {
        self.connection()
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .map_err(|error| StoreError::operation("read schema version", &self.path, error))
    }

    /// Report whether the store has no synchronized Yahoo leagues.
    pub fn is_empty(&self) -> Result<bool, StoreError> {
        self.connection()
            .query_row("SELECT COUNT(*) = 0 FROM yahoo_leagues", [], |row| {
                row.get(0)
            })
            .map_err(|error| StoreError::operation("check database emptiness", &self.path, error))
    }

    /// Read the durable provider dashboard state.
    pub fn dashboard_status(&self) -> Result<DashboardStatus, StoreError> {
        self.connection()
            .query_row(
                "SELECT provider_last_success_at, provider_last_failure_at, provider_failure_count, circuit_open, last_error, provider_freshness_at, last_run_at, last_run_status FROM dashboard_status WHERE id=1",
                [],
                |row| {
                    let last_run_status = row.get::<_, Option<String>>(7)?;
                    Ok(DashboardStatus {
                        provider_last_success_at: row.get(0)?,
                        provider_last_failure_at: row.get(1)?,
                        provider_failure_count: row.get(2)?,
                        circuit_open: row.get::<_, i64>(3)? != 0,
                        last_error: {
                            let value = row.get::<_, String>(4)?;
                            (!value.is_empty()).then_some(value)
                        },
                        provider_freshness_at: row.get(5)?,
                        last_run_at: row.get(6)?,
                        last_run_status: last_run_status.filter(|value| !value.is_empty()),
                    })
                },
            )
            .map_err(|error| StoreError::operation("read dashboard status", &self.path, error))
    }

    /// Record a successful authenticated provider cycle and close the circuit.
    pub fn record_provider_success(&mut self) -> Result<(), StoreError> {
        let (_, now) = self.captured_time("record provider success")?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction
                .execute(
                    "UPDATE dashboard_status SET provider_last_success_at=?1, provider_freshness_at=?1, provider_failure_count=0, circuit_open=0, last_error='', last_run_at=?1, last_run_status='success' WHERE id=1",
                    [now],
                )
                .map_err(|error| StoreError::operation("record provider success", &path, error))?;
            Ok(())
        })
    }

    /// Record a bounded provider failure and open the circuit at five failures.
    pub fn record_provider_failure(&mut self, error_summary: &str) -> Result<(), StoreError> {
        let (_, now) = self.captured_time("record provider failure")?;
        let bounded = error_summary.chars().take(240).collect::<String>();
        let path = self.path.clone();
        self.transaction(|transaction| {
            transaction
                .execute(
                    "UPDATE dashboard_status SET provider_last_failure_at=?1, provider_failure_count=MIN(provider_failure_count + 1, 5), circuit_open=CASE WHEN provider_failure_count + 1 >= 5 THEN 1 ELSE 0 END, last_error=?2, last_run_at=?1, last_run_status='failed' WHERE id=1",
                    rusqlite::params![now, bounded],
                )
                .map_err(|error| StoreError::operation("record provider failure", &path, error))?;
            Ok(())
        })
    }

    /// Execute one immediate transaction without exposing the owned connection.
    pub fn transaction<T, F>(&mut self, operation: F) -> Result<T, StoreError>
    where
        F: FnOnce(&Transaction<'_>) -> Result<T, StoreError>,
    {
        let path = self.path.clone();
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StoreError::operation("begin transaction", &path, error))?;
        match operation(&transaction) {
            Ok(value) => {
                transaction
                    .commit()
                    .map_err(|error| StoreError::operation("commit transaction", &path, error))?;
                Ok(value)
            }
            Err(operation_error) => match transaction.rollback() {
                Ok(()) => Err(operation_error),
                Err(error) => Err(StoreError::TransactionRollback {
                    operation_error: Box::new(operation_error),
                    rollback_error: Box::new(StoreError::operation(
                        "rollback transaction",
                        &path,
                        error,
                    )),
                }),
            },
        }
    }

    fn connection(&self) -> &Connection {
        self.connection.as_ref().expect("open store connection")
    }

    fn connection_mut(&mut self) -> &mut Connection {
        self.connection.as_mut().expect("open store connection")
    }

    fn captured_time(&self, operation: &'static str) -> Result<(SystemTime, i64), StoreError> {
        let now = self.clock.now();
        let elapsed = now
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StoreError::invalid_time(operation, "clock is before the Unix epoch"))?;
        if elapsed.is_zero() {
            return Err(StoreError::invalid_time(
                operation,
                "clock equals the Unix epoch reserved for missing timestamps",
            ));
        }
        let seconds = i64::try_from(elapsed.as_secs()).map_err(|_| {
            StoreError::invalid_time(operation, "clock exceeds the SQLite timestamp range")
        })?;
        Ok((now, seconds))
    }
}

fn validate_identity(
    operation: &'static str,
    field: &'static str,
    value: &str,
) -> Result<(), StoreError> {
    if value.trim().is_empty() {
        return Err(StoreError::invalid(
            operation,
            format!("{field} must not be empty"),
        ));
    }
    Ok(())
}

fn optional_time(
    operation: &'static str,
    field: &'static str,
    value: i64,
) -> Result<Option<SystemTime>, StoreError> {
    if value < 0 {
        return Err(StoreError::invalid_time(
            operation,
            format!("{field} must not be negative"),
        ));
    }
    if value == 0 {
        return Ok(None);
    }
    let seconds = u64::try_from(value).map_err(|_| {
        StoreError::invalid_time(operation, format!("{field} exceeds the timestamp range"))
    })?;
    UNIX_EPOCH
        .checked_add(Duration::from_secs(seconds))
        .map(Some)
        .ok_or_else(|| StoreError::invalid_time(operation, format!("{field} overflows SystemTime")))
}

fn required_time(
    operation: &'static str,
    field: &'static str,
    value: i64,
) -> Result<SystemTime, StoreError> {
    optional_time(operation, field, value)?.ok_or_else(|| {
        StoreError::invalid_time(operation, format!("{field} must be after the Unix epoch"))
    })
}

/// Resolve the production database path without opening or creating it.
pub fn database_path() -> Result<PathBuf, StoreError> {
    database_path_from_home(std::env::var_os("HOME"))
}

fn database_path_from_home(home: Option<OsString>) -> Result<PathBuf, StoreError> {
    let home = home.ok_or(StoreError::HomeUnavailable)?;
    Ok(PathBuf::from(home).join(".config").join("b9").join("b9.db"))
}

fn prepare_path(path: &Path) -> Result<(), StoreError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        create_secure_directories(parent, path)?;
    }
    create_secure_file(path)
}

#[cfg(unix)]
fn create_secure_directories(parent: &Path, database_path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder
        .create(parent)
        .map_err(|error| StoreError::operation("create database directory", database_path, error))
}

#[cfg(not(unix))]
fn create_secure_directories(parent: &Path, database_path: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(parent)
        .map_err(|error| StoreError::operation("create database directory", database_path, error))
}

#[cfg(unix)]
fn create_secure_file(path: &Path) -> Result<(), StoreError> {
    use std::io::ErrorKind;
    use std::os::unix::fs::OpenOptionsExt;

    match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(StoreError::operation("create database file", path, error)),
    }
}

#[cfg(not(unix))]
fn create_secure_file(path: &Path) -> Result<(), StoreError> {
    use std::io::ErrorKind;

    match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(StoreError::operation("create database file", path, error)),
    }
}

fn migrate(connection: &mut Connection, path: &Path, schema: &str) -> Result<(), StoreError> {
    let user_table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StoreError::operation("inspect schema tables", path, error))?;
    let version_table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StoreError::operation("inspect schema version table", path, error))?;

    if version_table_count == 0 {
        if user_table_count != 0 {
            return Err(StoreError::unsupported(
                path,
                "nonempty database has no schema_version table",
            ));
        }
        return migrate_empty(connection, path, schema);
    }

    let version_row_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
        .map_err(|error| StoreError::operation("count schema version rows", path, error))?;
    if version_row_count != 1 {
        return Err(StoreError::unsupported(
            path,
            format!("schema_version contains {version_row_count} rows; expected exactly one"),
        ));
    }
    let version: i64 = connection
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .map_err(|error| StoreError::operation("read schema version row", path, error))?;
    if version == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    if version > CURRENT_SCHEMA_VERSION {
        return Err(StoreError::unsupported(
            path,
            format!(
                "database schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            ),
        ));
    }
    if version == 1 {
        migrate_v1_to_v2(connection, path)?;
        migrate_v2_to_v3(connection, path)?;
        return migrate_v3_to_v4(connection, path);
    }
    if version == 2 {
        migrate_v2_to_v3(connection, path)?;
        return migrate_v3_to_v4(connection, path);
    }
    if version == 3 {
        return migrate_v3_to_v4(connection, path);
    }
    Err(StoreError::unsupported(
        path,
        format!("database schema version {version} is not a supported b9 migration source"),
    ))
}

fn migrate_v1_to_v2(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StoreError::operation("begin schema migration", path, error))?;
    transaction
        .execute_batch(
            "CREATE TABLE yahoo_free_agents (league_key TEXT NOT NULL, player_id INTEGER NOT NULL, synced_at INTEGER NOT NULL, PRIMARY KEY (league_key, player_id));",
        )
        .map_err(|error| StoreError::operation("apply version-two schema migration", path, error))?;
    transaction
        .execute("UPDATE schema_version SET version=?1", [2])
        .map_err(|error| StoreError::operation("write schema version", path, error))?;
    transaction
        .commit()
        .map_err(|error| StoreError::operation("commit schema migration", path, error))
}

fn migrate_v2_to_v3(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StoreError::operation("begin schema migration", path, error))?;
    transaction
        .execute_batch(
            "CREATE TABLE dashboard_status (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                daemon_started_at INTEGER,
                daemon_stopped_at INTEGER,
                last_run_at INTEGER,
                last_run_status TEXT,
                next_run_at INTEGER,
                provider_last_success_at INTEGER,
                provider_last_failure_at INTEGER,
                provider_failure_count INTEGER NOT NULL DEFAULT 0,
                circuit_open INTEGER NOT NULL DEFAULT 0,
                last_error TEXT NOT NULL DEFAULT '',
                provider_freshness_at INTEGER
            );
            INSERT INTO dashboard_status (id) VALUES (1);",
        )
        .map_err(|error| {
            StoreError::operation("apply version-three schema migration", path, error)
        })?;
    transaction
        .execute("UPDATE schema_version SET version=?1", [3])
        .map_err(|error| StoreError::operation("write schema version", path, error))?;
    transaction
        .commit()
        .map_err(|error| StoreError::operation("commit schema migration", path, error))
}

fn migrate_v3_to_v4(connection: &mut Connection, path: &Path) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StoreError::operation("begin schema migration", path, error))?;
    let has_statcast: bool = transaction
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_schema WHERE type='table' AND name='statcast_seasons'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| {
            StoreError::operation("inspect version-three Statcast schema", path, error)
        })?;
    if has_statcast {
        transaction
            .execute_batch(
                "ALTER TABLE statcast_seasons ADD COLUMN strikeout_pct REAL;
                 ALTER TABLE statcast_seasons ADD COLUMN walk_pct REAL;
                 ALTER TABLE statcast_seasons ADD COLUMN ops REAL;",
            )
            .map_err(|error| {
                StoreError::operation("apply version-four schema migration", path, error)
            })?;
    }
    transaction
        .execute("UPDATE schema_version SET version=?1", [4])
        .map_err(|error| StoreError::operation("write schema version", path, error))?;
    transaction
        .commit()
        .map_err(|error| StoreError::operation("commit schema migration", path, error))
}

fn migrate_empty(connection: &mut Connection, path: &Path, schema: &str) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StoreError::operation("begin schema migration", path, error))?;
    transaction
        .execute_batch(schema)
        .map_err(|error| StoreError::operation("apply schema migration", path, error))?;
    transaction
        .execute("INSERT OR IGNORE INTO dashboard_status (id) VALUES (1)", [])
        .map_err(|error| StoreError::operation("initialize dashboard status", path, error))?;
    transaction
        .execute("DELETE FROM schema_version", [])
        .map_err(|error| StoreError::operation("clear schema version", path, error))?;
    transaction
        .execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            params![CURRENT_SCHEMA_VERSION],
        )
        .map_err(|error| StoreError::operation("write schema version", path, error))?;
    transaction
        .commit()
        .map_err(|error| StoreError::operation("commit schema migration", path, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_error(operation: &'static str) -> StoreError {
        StoreError::operation(
            operation,
            Path::new("test.db"),
            std::io::Error::other(operation),
        )
    }

    #[test]
    fn migration_failure_rolls_back_every_schema_change() {
        let mut connection = Connection::open_in_memory().unwrap();
        let schema = format!("{SCHEMA}\nCREATE TABLE broken (");
        let error = migrate(&mut connection, Path::new("test.db"), &schema).unwrap_err();
        assert!(error.to_string().contains("apply schema migration"));
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn version_one_store_migrates_to_scoped_free_agents() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE schema_version (version INTEGER PRIMARY KEY); INSERT INTO schema_version VALUES (1);")
            .unwrap();
        migrate(&mut connection, Path::new("test.db"), SCHEMA).unwrap();
        let version: i64 = connection
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='yahoo_free_agents'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(table_count, 1);
        let dashboard_rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM dashboard_status", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(dashboard_rows, 1);
    }

    #[test]
    fn migration_v2_to_v3_rolls_back_and_preserves_version_on_conflict() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_version (version INTEGER PRIMARY KEY); INSERT INTO schema_version VALUES (2);
                 CREATE TABLE dashboard_status (id INTEGER PRIMARY KEY);
                 CREATE TABLE sentinel (value TEXT); INSERT INTO sentinel VALUES ('kept');",
            )
            .unwrap();
        let error = migrate_v2_to_v3(&mut connection, Path::new("test.db")).unwrap_err();
        assert!(error.to_string().contains("version-three schema migration"));
        let version: i64 = connection
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        let sentinel: String = connection
            .query_row("SELECT value FROM sentinel", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sentinel, "kept");
    }

    #[test]
    fn production_path_reports_missing_home_with_recovery() {
        let error = database_path_from_home(None).unwrap_err();
        assert!(error.to_string().contains("set HOME"));
    }

    #[cfg(unix)]
    #[test]
    fn production_path_preserves_non_unicode_home() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let home = OsString::from_vec(b"/tmp/b9-\xff-home".to_vec());
        let path = database_path_from_home(Some(home.clone())).unwrap();
        assert!(
            path.as_os_str()
                .as_bytes()
                .starts_with(home.as_os_str().as_bytes())
        );
        assert!(path.as_os_str().as_bytes().ends_with(b"/.config/b9/b9.db"));
    }

    #[test]
    fn transaction_retains_operation_and_real_rollback_failures() {
        let mut store = Store {
            connection: Some(Connection::open_in_memory().unwrap()),
            path: PathBuf::from("test.db"),
            clock: Arc::new(SystemClock),
        };
        let error = store
            .transaction(|transaction| {
                transaction.execute_batch("ROLLBACK").unwrap();
                Err::<(), _>(test_error("operation"))
            })
            .unwrap_err();
        let display = error.to_string();
        assert!(display.contains("operation"));
        assert!(display.contains("rollback"));
        assert!(error.source().is_some());
    }

    #[test]
    fn transaction_reports_real_commit_failure_with_context() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        let mut store = Store {
            connection: Some(connection),
            path: PathBuf::from("test.db"),
            clock: Arc::new(SystemClock),
        };
        let error = store
            .transaction(|transaction| {
                transaction
                    .execute_batch(
                        "CREATE TABLE parent (id INTEGER PRIMARY KEY);
                         CREATE TABLE child (
                             parent_id INTEGER,
                             FOREIGN KEY (parent_id) REFERENCES parent(id)
                                 DEFERRABLE INITIALLY DEFERRED
                         );
                         INSERT INTO child (parent_id) VALUES (1);",
                    )
                    .unwrap();
                Ok(())
            })
            .unwrap_err();
        assert!(error.to_string().contains("commit transaction"));
    }

    #[test]
    fn provider_dashboard_opens_at_five_failures_and_closes_on_success() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.db");
        let mut store = Store::open_at_with_clock(&path, Arc::new(SystemClock)).unwrap();
        for _ in 0..4 {
            store.record_provider_failure("HTTP 403").unwrap();
        }
        assert!(!store.dashboard_status().unwrap().circuit_open);
        store.record_provider_failure("HTTP 403").unwrap();
        let failed = store.dashboard_status().unwrap();
        assert!(failed.circuit_open);
        assert_eq!(failed.provider_failure_count, 5);
        assert_eq!(failed.last_error.as_deref(), Some("HTTP 403"));
        assert_eq!(failed.last_run_status.as_deref(), Some("failed"));
        store.record_provider_success().unwrap();
        let recovered = store.dashboard_status().unwrap();
        assert!(!recovered.circuit_open);
        assert_eq!(recovered.provider_failure_count, 0);
        assert_eq!(recovered.last_error, None);
        assert_eq!(recovered.last_run_status.as_deref(), Some("success"));
    }
}
