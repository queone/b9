//! Isolated b9 SQLite storage ownership, schema migration, and transactions.

use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, Transaction, TransactionBehavior, params};

const SCHEMA: &str = include_str!("store/schema.sql");

/// The current schema version for b9-owned databases.
pub const CURRENT_SCHEMA_VERSION: i64 = 1;

/// A contextual failure at the b9 storage boundary.
#[derive(Debug)]
pub enum StoreError {
    /// The production database path cannot be resolved.
    HomeUnavailable,
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
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HomeUnavailable => write!(
                formatter,
                "resolve database path: HOME is unavailable; set HOME to the user home directory and retry"
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
            Self::HomeUnavailable | Self::UnsupportedSchema { .. } => None,
        }
    }
}

/// Owns one connection to an isolated b9 SQLite database.
pub struct Store {
    connection: Option<Connection>,
    path: PathBuf,
}

impl Store {
    /// Open the production b9 database and migrate it to the current schema.
    pub fn open() -> Result<Self, StoreError> {
        let path = database_path()?;
        Self::open_at(path)
    }

    /// Open an explicit database path and migrate it to the current schema.
    pub fn open_at(path: impl AsRef<Path>) -> Result<Self, StoreError> {
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
    Err(StoreError::unsupported(
        path,
        format!("database schema version {version} is not a supported b9 migration source"),
    ))
}

fn migrate_empty(connection: &mut Connection, path: &Path, schema: &str) -> Result<(), StoreError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StoreError::operation("begin schema migration", path, error))?;
    transaction
        .execute_batch(schema)
        .map_err(|error| StoreError::operation("apply schema migration", path, error))?;
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
}
