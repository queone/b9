//! Bounded, atomic disk caching for provider payload bytes.

use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::store::{Clock, SystemClock};

const MAGIC: &[u8] = b"b9-cache-v1\n";
const MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = MAX_PAYLOAD_BYTES as u64 + 128;
const PRUNE_AGE: Duration = Duration::from_secs(24 * 60 * 60);
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// One complete cached payload and its capture time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheEntry {
    pub fetched_at: SystemTime,
    pub payload: Vec<u8>,
}

/// The typed disposition of a cache lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheLookup {
    Hit(CacheEntry),
    Missing,
    Expired(CacheEntry),
    Corrupt { path: PathBuf, reason: String },
}

/// One file-level pruning problem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PruneIssue {
    pub path: PathBuf,
    pub detail: String,
}

/// Deterministic results from one namespace pruning pass.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PruneReport {
    pub scanned: usize,
    pub removed: usize,
    pub malformed: usize,
    pub unrelated: usize,
    pub failed: usize,
    pub issues: Vec<PruneIssue>,
}

/// A contextual cache operation failure.
#[derive(Debug)]
pub enum CacheError {
    Invalid {
        operation: &'static str,
        detail: String,
    },
    FileSystem {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    DurabilityUncertain {
        path: PathBuf,
        source: io::Error,
    },
}

impl CacheError {
    fn invalid(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::Invalid {
            operation,
            detail: detail.into(),
        }
    }

    fn filesystem(operation: &'static str, path: &Path, source: io::Error) -> Self {
        Self::FileSystem {
            operation,
            path: path.to_path_buf(),
            source,
        }
    }
}

impl fmt::Display for CacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { operation, detail } => {
                write!(
                    formatter,
                    "{operation}: {detail}; correct the value and retry"
                )
            }
            Self::FileSystem {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "{operation} {}: {source}; check the cache path and permissions, then retry",
                path.display()
            ),
            Self::DurabilityUncertain { path, source } => write!(
                formatter,
                "sync cache directory after replacing {}: {source}; the new entry is visible but durability is uncertain",
                path.display()
            ),
        }
    }
}

impl Error for CacheError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FileSystem { source, .. } | Self::DurabilityUncertain { source, .. } => {
                Some(source)
            }
            Self::Invalid { .. } => None,
        }
    }
}

/// Owns one cache root and its clock without creating filesystem state eagerly.
pub struct DiskCache {
    root: PathBuf,
    clock: Arc<dyn Clock>,
}

impl DiskCache {
    /// Construct a cache at the production b9 cache root.
    pub fn production() -> Result<Self, CacheError> {
        Ok(Self::at(production_cache_root()?))
    }

    /// Construct a cache at an explicit root with the host clock.
    pub fn at(root: impl AsRef<Path>) -> Self {
        Self::at_with_clock(root, Arc::new(SystemClock))
    }

    /// Construct a cache at an explicit root with a controlled clock.
    pub fn at_with_clock(root: impl AsRef<Path>, clock: Arc<dyn Clock>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            clock,
        }
    }

    /// Return the selected root without creating it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Return the deterministic path for a validated logical entry.
    pub fn entry_path(&self, namespace: &str, key: &str) -> Result<PathBuf, CacheError> {
        validate_component("derive cache path", "namespace", namespace, 64)?;
        validate_component("derive cache path", "key", key, 256)?;
        Ok(self.root.join(namespace).join(filename(namespace, key)))
    }

    /// Read one cache entry under a positive TTL.
    pub fn get(
        &self,
        namespace: &str,
        key: &str,
        ttl: Duration,
    ) -> Result<CacheLookup, CacheError> {
        const OPERATION: &str = "read cache entry";
        validate_component(OPERATION, "namespace", namespace, 64)?;
        validate_component(OPERATION, "key", key, 256)?;
        if ttl.is_zero() {
            return Err(CacheError::invalid(OPERATION, "TTL must be positive"));
        }
        let now = captured_time(self.clock.as_ref(), OPERATION)?.0;
        if !inspect_namespace_for_read(&self.root, namespace)? {
            return Ok(CacheLookup::Missing);
        }
        let path = self.root.join(namespace).join(filename(namespace, key));
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(CacheLookup::Missing);
            }
            Err(error) => return Err(CacheError::filesystem(OPERATION, &path, error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(CacheError::invalid(
                OPERATION,
                "cache path is not a regular non-symlink file",
            ));
        }
        if metadata.len() > MAX_ENTRY_BYTES {
            return Ok(CacheLookup::Corrupt {
                path,
                reason: "cache entry exceeds the 32 MiB payload bound".into(),
            });
        }
        let data =
            fs::read(&path).map_err(|error| CacheError::filesystem(OPERATION, &path, error))?;
        let entry = match decode_entry(&data) {
            Ok(entry) => entry,
            Err(reason) => return Ok(CacheLookup::Corrupt { path, reason }),
        };
        let age = now
            .duration_since(entry.fetched_at)
            .unwrap_or(Duration::ZERO);
        if age >= ttl {
            Ok(CacheLookup::Expired(entry))
        } else {
            Ok(CacheLookup::Hit(entry))
        }
    }

    /// Atomically store one bounded payload.
    pub fn put(&self, namespace: &str, key: &str, payload: &[u8]) -> Result<(), CacheError> {
        self.put_inner(namespace, key, payload, |_| Ok(()))
    }

    /// Prune old owned entries from one namespace.
    pub fn prune(&self, namespace: &str) -> Result<PruneReport, CacheError> {
        const OPERATION: &str = "prune cache namespace";
        validate_component(OPERATION, "namespace", namespace, 64)?;
        let now = captured_time(self.clock.as_ref(), OPERATION)?.0;
        if !inspect_namespace_for_read(&self.root, namespace)? {
            return Ok(PruneReport::default());
        }
        let directory = self.root.join(namespace);
        let metadata = match fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(PruneReport::default());
            }
            Err(error) => return Err(CacheError::filesystem(OPERATION, &directory, error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(CacheError::invalid(
                OPERATION,
                "namespace path is not a non-symlink directory",
            ));
        }
        let mut entries: Vec<(OsString, PathBuf)> = fs::read_dir(&directory)
            .map_err(|error| CacheError::filesystem(OPERATION, &directory, error))?
            .map(|entry| entry.map(|entry| (entry.file_name(), entry.path())))
            .collect::<Result<_, _>>()
            .map_err(|error| CacheError::filesystem(OPERATION, &directory, error))?;
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        let mut report = PruneReport::default();
        for (name, path) in entries {
            report.scanned += 1;
            let Some(name) = name.to_str() else {
                report.unrelated += 1;
                continue;
            };
            if !is_owned_filename(name) {
                report.unrelated += 1;
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(value) => value,
                Err(error) => {
                    record_prune_failure(&mut report, path, error);
                    continue;
                }
            };
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                report.unrelated += 1;
                continue;
            }
            if metadata.len() > MAX_ENTRY_BYTES {
                report.malformed += 1;
                continue;
            }
            let data = match fs::read(&path) {
                Ok(value) => value,
                Err(error) => {
                    record_prune_failure(&mut report, path, error);
                    continue;
                }
            };
            let entry = match decode_entry(&data) {
                Ok(value) => value,
                Err(_) => {
                    report.malformed += 1;
                    continue;
                }
            };
            let age = now
                .duration_since(entry.fetched_at)
                .unwrap_or(Duration::ZERO);
            if age >= PRUNE_AGE {
                match fs::remove_file(&path) {
                    Ok(()) => report.removed += 1,
                    Err(error) => record_prune_failure(&mut report, path, error),
                }
            }
        }
        Ok(report)
    }

    fn put_inner<F>(
        &self,
        namespace: &str,
        key: &str,
        payload: &[u8],
        mut stage: F,
    ) -> Result<(), CacheError>
    where
        F: FnMut(WriteStage) -> Result<(), CacheError>,
    {
        const OPERATION: &str = "write cache entry";
        validate_component(OPERATION, "namespace", namespace, 64)?;
        validate_component(OPERATION, "key", key, 256)?;
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(CacheError::invalid(OPERATION, "payload exceeds 32 MiB"));
        }
        let (_, timestamp) = captured_time(self.clock.as_ref(), OPERATION)?;
        let directory = self.root.join(namespace);
        prepare_directory(&self.root)?;
        prepare_directory(&directory)?;
        let target = directory.join(filename(namespace, key));
        reject_symlink_target(&target)?;
        let data = encode_entry(timestamp, payload);
        let temporary = directory.join(format!(
            ".b9-cache-{}-{}.tmp",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        stage(WriteStage::BeforeCreate)?;
        let result = write_and_replace(&temporary, &target, &directory, &data, &mut stage);
        if result.is_err() && temporary.exists() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[derive(Clone, Copy)]
enum WriteStage {
    BeforeCreate,
    AfterCreate,
    AfterWrite,
    AfterFileSync,
    AfterRename,
}

fn write_and_replace<F>(
    temporary: &Path,
    target: &Path,
    directory: &Path,
    data: &[u8],
    stage: &mut F,
) -> Result<(), CacheError>
where
    F: FnMut(WriteStage) -> Result<(), CacheError>,
{
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(temporary)
        .map_err(|error| CacheError::filesystem("create cache temporary file", temporary, error))?;
    stage(WriteStage::AfterCreate)?;
    file.write_all(data)
        .map_err(|error| CacheError::filesystem("write cache temporary file", temporary, error))?;
    stage(WriteStage::AfterWrite)?;
    file.sync_all()
        .map_err(|error| CacheError::filesystem("sync cache temporary file", temporary, error))?;
    stage(WriteStage::AfterFileSync)?;
    fs::rename(temporary, target)
        .map_err(|error| CacheError::filesystem("replace cache entry", target, error))?;
    stage(WriteStage::AfterRename).map_err(|error| match error {
        CacheError::FileSystem { source, .. } => CacheError::DurabilityUncertain {
            path: target.to_path_buf(),
            source,
        },
        other => CacheError::DurabilityUncertain {
            path: target.to_path_buf(),
            source: io::Error::other(other.to_string()),
        },
    })?;
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|source| CacheError::DurabilityUncertain {
            path: target.to_path_buf(),
            source,
        })
}

fn prepare_directory(path: &Path) -> Result<(), CacheError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(CacheError::invalid(
                "prepare cache directory",
                format!("{} is not a non-symlink directory", path.display()),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir_all(path)
            .map_err(|error| CacheError::filesystem("create cache directory", path, error))?,
        Err(error) => {
            return Err(CacheError::filesystem(
                "inspect cache directory",
                path,
                error,
            ));
        }
    }
    set_file_mode(path, 0o700)
}

fn reject_symlink_target(path: &Path) -> Result<(), CacheError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(CacheError::invalid(
                "inspect cache target",
                "target is not a regular non-symlink file",
            ))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CacheError::filesystem("inspect cache target", path, error)),
    }
}

fn inspect_namespace_for_read(root: &Path, namespace: &str) -> Result<bool, CacheError> {
    const OPERATION: &str = "inspect cache namespace";
    let root_metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(CacheError::filesystem(OPERATION, root, error)),
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(CacheError::invalid(
            OPERATION,
            "cache root is not a non-symlink directory",
        ));
    }
    let directory = root.join(namespace);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(CacheError::filesystem(OPERATION, &directory, error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CacheError::invalid(
            OPERATION,
            "namespace path is not a non-symlink directory",
        ));
    }
    Ok(true)
}

#[cfg(unix)]
fn set_file_mode(path: &Path, mode: u32) -> Result<(), CacheError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| CacheError::filesystem("set cache permissions", path, error))
}

#[cfg(not(unix))]
fn set_file_mode(_path: &Path, _mode: u32) -> Result<(), CacheError> {
    Ok(())
}

fn validate_component(
    operation: &'static str,
    field: &str,
    value: &str,
    maximum: usize,
) -> Result<(), CacheError> {
    if value.is_empty()
        || value.len() > maximum
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(CacheError::invalid(
            operation,
            format!("{field} must contain 1 through {maximum} portable ASCII characters"),
        ));
    }
    Ok(())
}

fn filename(namespace: &str, key: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(namespace.as_bytes());
    hash.update([0]);
    hash.update(key.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in hash.finalize() {
        use fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write to string");
    }
    format!("b9c-{hex}.cache")
}

fn encode_entry(timestamp: i64, payload: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(MAGIC.len() + 48 + payload.len());
    data.extend_from_slice(MAGIC);
    data.extend_from_slice(timestamp.to_string().as_bytes());
    data.push(b'\n');
    data.extend_from_slice(payload.len().to_string().as_bytes());
    data.push(b'\n');
    data.extend_from_slice(payload);
    data
}

fn decode_entry(data: &[u8]) -> Result<CacheEntry, String> {
    let rest = data.strip_prefix(MAGIC).ok_or("unknown cache format")?;
    let (timestamp, rest) = split_line(rest).ok_or("missing cache timestamp")?;
    let (length, payload) = split_line(rest).ok_or("missing cache payload length")?;
    let timestamp = parse_canonical_positive(timestamp, "timestamp")?;
    let length = parse_canonical_usize(length, "payload length")?;
    if length > MAX_PAYLOAD_BYTES || payload.len() != length {
        return Err("cache payload length is invalid".into());
    }
    let fetched_at = UNIX_EPOCH
        .checked_add(Duration::from_secs(timestamp as u64))
        .ok_or("cache timestamp is out of range")?;
    Ok(CacheEntry {
        fetched_at,
        payload: payload.to_vec(),
    })
}

fn split_line(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let index = data.iter().position(|byte| *byte == b'\n')?;
    Some((&data[..index], &data[index + 1..]))
}
fn parse_canonical_positive(value: &[u8], field: &str) -> Result<i64, String> {
    let text = std::str::from_utf8(value).map_err(|_| format!("cache {field} is not ASCII"))?;
    let parsed = text
        .parse::<i64>()
        .map_err(|_| format!("cache {field} is invalid"))?;
    if parsed <= 0 || parsed.to_string() != text {
        return Err(format!("cache {field} is noncanonical"));
    }
    Ok(parsed)
}
fn parse_canonical_usize(value: &[u8], field: &str) -> Result<usize, String> {
    let text = std::str::from_utf8(value).map_err(|_| format!("cache {field} is not ASCII"))?;
    let parsed = text
        .parse::<usize>()
        .map_err(|_| format!("cache {field} is invalid"))?;
    if parsed.to_string() != text {
        return Err(format!("cache {field} is noncanonical"));
    }
    Ok(parsed)
}

fn captured_time(
    clock: &dyn Clock,
    operation: &'static str,
) -> Result<(SystemTime, i64), CacheError> {
    let now = clock.now();
    let elapsed = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CacheError::invalid(operation, "clock is before the Unix epoch"))?;
    if elapsed.is_zero() {
        return Err(CacheError::invalid(
            operation,
            "clock equals the Unix epoch reserved for missing timestamps",
        ));
    }
    let seconds = i64::try_from(elapsed.as_secs())
        .map_err(|_| CacheError::invalid(operation, "clock exceeds the cache timestamp range"))?;
    Ok((now, seconds))
}

fn is_owned_filename(name: &str) -> bool {
    name.len() == 74
        && name.starts_with("b9c-")
        && name.ends_with(".cache")
        && name[4..68]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
fn record_prune_failure(report: &mut PruneReport, path: PathBuf, error: io::Error) {
    report.failed += 1;
    report.issues.push(PruneIssue {
        path,
        detail: error.to_string(),
    });
}

/// Resolve the production cache root without creating it.
pub fn production_cache_root() -> Result<PathBuf, CacheError> {
    resolve_cache_root(dirs::cache_dir(), dirs::home_dir())
}

fn resolve_cache_root(
    platform: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf, CacheError> {
    platform
        .or_else(|| home.map(|path| path.join(".cache")))
        .map(|path| path.join("b9").join("api-cache"))
        .ok_or_else(|| {
            CacheError::invalid(
                "resolve cache root",
                "platform cache directory and home directory are unavailable",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;

    struct FixedClock;
    impl Clock for FixedClock {
        fn now(&self) -> SystemTime {
            UNIX_EPOCH + Duration::from_secs(100)
        }
    }

    #[test]
    fn root_resolution_and_private_failures_are_deterministic() {
        assert_eq!(
            resolve_cache_root(Some(PathBuf::from("cache")), Some(PathBuf::from("home"))).unwrap(),
            PathBuf::from("cache/b9/api-cache")
        );
        assert_eq!(
            resolve_cache_root(None, Some(PathBuf::from("home"))).unwrap(),
            PathBuf::from("home/.cache/b9/api-cache")
        );
        assert!(resolve_cache_root(None, None).is_err());
        let directory = tempdir().unwrap();
        let cache = DiskCache::at_with_clock(directory.path(), Arc::new(FixedClock));
        cache.put("mlb", "key", b"old").unwrap();
        for rejected in [
            WriteStage::BeforeCreate,
            WriteStage::AfterCreate,
            WriteStage::AfterWrite,
            WriteStage::AfterFileSync,
        ] {
            let error = cache.put_inner("mlb", "key", b"new", |stage| {
                if std::mem::discriminant(&stage) == std::mem::discriminant(&rejected) {
                    Err(CacheError::invalid("inject", "failure"))
                } else {
                    Ok(())
                }
            });
            assert!(error.is_err());
            assert!(
                matches!(cache.get("mlb", "key", Duration::from_secs(10)).unwrap(), CacheLookup::Hit(CacheEntry { payload, .. }) if payload == b"old")
            );
        }
        let error = cache.put_inner("mlb", "key", b"new", |stage| {
            if matches!(stage, WriteStage::AfterRename) {
                Err(CacheError::invalid("inject", "failure"))
            } else {
                Ok(())
            }
        });
        assert!(matches!(error, Err(CacheError::DurabilityUncertain { .. })));
        assert!(
            matches!(cache.get("mlb", "key", Duration::from_secs(10)).unwrap(), CacheLookup::Hit(CacheEntry { payload, .. }) if payload == b"new")
        );
        assert!(
            fs::read_dir(directory.path().join("mlb"))
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp"))
        );
    }
}
