//! Private, atomic skout user configuration.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Persisted skout preferences.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub current_league: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub current_team_key: String,
    #[serde(default, skip_serializing)]
    pub pull_public_league_id: String,
}

/// One contextual configuration failure.
#[derive(Debug)]
pub struct ConfigError {
    operation: &'static str,
    path: PathBuf,
    detail: String,
}

impl ConfigError {
    fn new(operation: &'static str, path: &Path, detail: impl Into<String>) -> Self {
        Self {
            operation,
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {}: {}; check the path and permissions, then retry",
            self.operation,
            self.path.display(),
            self.detail
        )
    }
}

impl std::error::Error for ConfigError {}

/// Resolve the production configuration path without creating it.
pub fn config_path() -> Result<PathBuf, ConfigError> {
    config_path_from_home(std::env::var_os("HOME"))
}

fn config_path_from_home(home: Option<OsString>) -> Result<PathBuf, ConfigError> {
    let path = home
        .map(PathBuf::from)
        .map(|home| home.join(".config").join("skout").join("config.json"))
        .ok_or_else(|| {
            ConfigError::new(
                "resolve configuration path",
                Path::new("skout"),
                "HOME is unavailable",
            )
        })?;
    Ok(path)
}

/// Read an explicit configuration path; absence yields an empty configuration.
pub fn read_at(path: impl AsRef<Path>) -> Result<Config, ConfigError> {
    let path = path.as_ref();
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => {
            return Err(ConfigError::new(
                "read configuration",
                path,
                error.to_string(),
            ));
        }
    };
    serde_json::from_slice(&bytes).map_err(|_| {
        ConfigError::new(
            "parse configuration",
            path,
            "configuration JSON is malformed",
        )
    })
}

/// Read the production configuration.
pub fn read() -> Result<Config, ConfigError> {
    read_at(config_path()?)
}

/// Atomically write an explicit private configuration path.
pub fn write_at(path: impl AsRef<Path>, config: &Config) -> Result<(), ConfigError> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .ok_or_else(|| ConfigError::new("write configuration", path, "path has no parent"))?;
    create_private_directory(parent, path)?;
    let bytes = serde_json::to_vec_pretty(config).map_err(|_| {
        ConfigError::new("serialize configuration", path, "configuration is invalid")
    })?;
    let temporary = parent.join(format!(".config-{}.tmp", std::process::id()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|error| {
            ConfigError::new("create temporary configuration", path, error.to_string())
        })?;
        file.write_all(&bytes)
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|error| {
                ConfigError::new("write temporary configuration", path, error.to_string())
            })?;
        fs::rename(&temporary, path)
            .map_err(|error| ConfigError::new("replace configuration", path, error.to_string()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Write the production configuration.
pub fn write(config: &Config) -> Result<(), ConfigError> {
    write_at(config_path()?, config)
}

#[cfg(unix)]
fn create_private_directory(parent: &Path, path: &Path) -> Result<(), ConfigError> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|error| {
            ConfigError::new("create configuration directory", path, error.to_string())
        })
}

#[cfg(not(unix))]
fn create_private_directory(parent: &Path, path: &Path) -> Result<(), ConfigError> {
    fs::create_dir_all(parent).map_err(|error| {
        ConfigError::new("create configuration directory", path, error.to_string())
    })
}
