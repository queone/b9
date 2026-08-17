//! Private, atomic b9 user configuration.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Persisted b9 preferences; credentials never belong here.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub current_league: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub current_team_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pull_public_league_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub advisory_provider: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub advisory_model: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strategy_punts: Vec<String>,
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
        .map(|home| home.join(".config").join("b9").join("config.json"))
        .ok_or_else(|| {
            ConfigError::new(
                "resolve configuration path",
                Path::new("b9"),
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

/// Adopt legacy selections only where the b9 configuration is still empty.
pub fn adopt_legacy_at(
    target: impl AsRef<Path>,
    legacy: impl AsRef<Path>,
) -> Result<Config, ConfigError> {
    let target = target.as_ref();
    let mut current = read_at(target)?;
    let legacy_path = legacy.as_ref();
    let legacy = match read_at(legacy_path) {
        Ok(value) => value,
        Err(_error) if !legacy_path.exists() => return Ok(current),
        Err(error) => return Err(error),
    };
    let changed = (current.current_league.is_empty() && !legacy.current_league.is_empty())
        || (current.current_team_key.is_empty() && !legacy.current_team_key.is_empty());
    if current.current_league.is_empty() {
        current.current_league = legacy.current_league;
    }
    if current.current_team_key.is_empty() {
        current.current_team_key = legacy.current_team_key;
    }
    if changed {
        write_at(target, &current)?;
    }
    Ok(current)
}

/// Adopt selections from the established legacy user configuration.
pub fn adopt_legacy() -> Result<Config, ConfigError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        ConfigError::new(
            "resolve legacy configuration path",
            Path::new("skout"),
            "HOME is unavailable",
        )
    })?;
    let legacy = PathBuf::from(home)
        .join(".config")
        .join("skout")
        .join("config.json");
    adopt_legacy_at(config_path()?, legacy)
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
