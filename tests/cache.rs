use std::fs;
use std::fs::OpenOptions;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use b9::cache::{CacheLookup, DiskCache};
use b9::store::Clock;
use tempfile::tempdir;

#[derive(Clone)]
struct AdjustableClock(Arc<Mutex<SystemTime>>);

impl AdjustableClock {
    fn at(seconds: u64) -> Self {
        Self(Arc::new(Mutex::new(
            UNIX_EPOCH + Duration::from_secs(seconds),
        )))
    }
    fn set(&self, seconds: u64) {
        *self.0.lock().unwrap() = UNIX_EPOCH + Duration::from_secs(seconds);
    }
}

impl Clock for AdjustableClock {
    fn now(&self) -> SystemTime {
        *self.0.lock().unwrap()
    }
}

#[test]
fn lookup_uses_exact_bytes_ttl_boundaries_and_corruption_states() {
    let directory = tempdir().unwrap();
    let clock = AdjustableClock::at(100);
    let cache = DiskCache::at_with_clock(directory.path(), Arc::new(clock.clone()));
    assert_eq!(
        cache
            .get("mlb", "schedule", Duration::from_secs(10))
            .unwrap(),
        CacheLookup::Missing
    );
    let payload = b"\0raw\nbytes\xff";
    cache.put("mlb", "schedule", payload).unwrap();
    assert!(
        matches!(cache.get("mlb", "schedule", Duration::from_secs(10)).unwrap(), CacheLookup::Hit(entry) if entry.payload == payload)
    );
    clock.set(110);
    assert!(matches!(
        cache
            .get("mlb", "schedule", Duration::from_secs(10))
            .unwrap(),
        CacheLookup::Expired(_)
    ));
    clock.set(99);
    assert!(matches!(
        cache
            .get("mlb", "schedule", Duration::from_secs(10))
            .unwrap(),
        CacheLookup::Hit(_)
    ));
    let path = cache.entry_path("mlb", "schedule").unwrap();
    for corrupt in [
        b"bad".as_slice(),
        b"b9-cache-v2\n100\n0\n",
        b"b9-cache-v1\n0\n0\n",
        b"b9-cache-v1\n0100\n0\n",
        b"b9-cache-v1\n100\n01\nx",
        b"b9-cache-v1\n100\n2\nx",
        b"b9-cache-v1\n100\n0\nx",
    ] {
        fs::write(&path, corrupt).unwrap();
        assert!(matches!(
            cache
                .get("mlb", "schedule", Duration::from_secs(10))
                .unwrap(),
            CacheLookup::Corrupt { .. }
        ));
    }
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(32 * 1024 * 1024 + 129)
        .unwrap();
    assert!(matches!(
        cache
            .get("mlb", "schedule", Duration::from_secs(10))
            .unwrap(),
        CacheLookup::Corrupt { .. }
    ));
    assert!(path.exists());
}

#[test]
fn validation_precedes_clock_and_filesystem_access() {
    let directory = tempdir().unwrap();
    let clock = AdjustableClock::at(0);
    let cache = DiskCache::at_with_clock(directory.path(), Arc::new(clock));
    assert!(cache.get("../bad", "key", Duration::from_secs(1)).is_err());
    assert!(cache.get("..", "key", Duration::from_secs(1)).is_err());
    assert!(cache.entry_path(&"a".repeat(64), &"b".repeat(256)).is_ok());
    assert!(cache.entry_path(&"a".repeat(65), "key").is_err());
    assert!(cache.entry_path("mlb", &"b".repeat(257)).is_err());
    assert!(cache.entry_path("méxico", "key").is_err());
    assert!(cache.get("mlb", "key", Duration::ZERO).is_err());
    assert!(!directory.path().join("mlb").exists());
    assert!(cache.get("mlb", "key", Duration::from_secs(1)).is_err());
    assert!(
        cache
            .put("mlb", "key", &vec![0; 32 * 1024 * 1024 + 1])
            .is_err()
    );
}

#[test]
fn paths_are_hashed_private_and_stable() {
    let directory = tempdir().unwrap();
    let cache = DiskCache::at(directory.path());
    let first = cache.entry_path("yahoo", "league_123").unwrap();
    let second = cache.entry_path("yahoo", "league_124").unwrap();
    assert_eq!(first, cache.entry_path("yahoo", "league_123").unwrap());
    assert_ne!(first, second);
    let name = first.file_name().unwrap().to_str().unwrap();
    assert_eq!(
        name,
        "b9c-20a3639c5dac3801224ed7fdd6751905556e898ab6492398e347b7fd3de12b22.cache"
    );
    assert!(name.starts_with("b9c-") && name.ends_with(".cache"));
    assert!(!name.contains("league"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o777)).unwrap();
        fs::create_dir(directory.path().join("yahoo")).unwrap();
        fs::set_permissions(
            directory.path().join("yahoo"),
            fs::Permissions::from_mode(0o777),
        )
        .unwrap();
    }
    cache.put("yahoo", "league_123", b"data").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(directory.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(first).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn prune_is_explicit_bounded_and_deterministic() {
    let directory = tempdir().unwrap();
    let clock = AdjustableClock::at(100);
    let cache = DiskCache::at_with_clock(directory.path(), Arc::new(clock.clone()));
    cache.put("mlb", "old", b"old").unwrap();
    clock.set(100 + 24 * 60 * 60);
    cache.put("mlb", "young", b"young").unwrap();
    clock.set(100 + 48 * 60 * 60);
    cache.put("mlb", "future", b"future").unwrap();
    clock.set(100 + 24 * 60 * 60);
    let malformed = cache.entry_path("mlb", "malformed").unwrap();
    fs::write(&malformed, b"bad").unwrap();
    fs::write(directory.path().join("mlb/unrelated.txt"), b"keep").unwrap();
    fs::write(directory.path().join("mlb/.b9-cache-1-1.tmp"), b"keep").unwrap();
    fs::create_dir(directory.path().join("mlb/directory.cache")).unwrap();
    let report = cache.prune("mlb").unwrap();
    assert_eq!(report.removed, 1);
    assert_eq!(report.malformed, 1);
    assert_eq!(report.unrelated, 3);
    assert_eq!(report.failed, 0);
    assert!(cache.entry_path("mlb", "young").unwrap().exists());
    assert!(cache.entry_path("mlb", "future").unwrap().exists());
    assert!(malformed.exists());
}

#[cfg(unix)]
#[test]
fn symlink_targets_are_rejected() {
    use std::os::unix::fs::symlink;
    let directory = tempdir().unwrap();
    let outside = directory.path().join("outside");
    fs::write(&outside, b"outside").unwrap();
    let cache = DiskCache::at(directory.path().join("cache"));
    fs::create_dir_all(directory.path().join("cache/mlb")).unwrap();
    let target = cache.entry_path("mlb", "key").unwrap();
    symlink(&outside, &target).unwrap();
    assert!(cache.put("mlb", "key", b"replacement").is_err());
    assert_eq!(fs::read(outside).unwrap(), b"outside");

    let namespace_root = directory.path().join("namespace-cache");
    fs::create_dir_all(&namespace_root).unwrap();
    symlink(
        directory.path().join("cache/mlb"),
        namespace_root.join("mlb"),
    )
    .unwrap();
    let namespace_cache = DiskCache::at(&namespace_root);
    assert!(
        namespace_cache
            .get("mlb", "key", Duration::from_secs(1))
            .is_err()
    );
    assert!(namespace_cache.prune("mlb").is_err());
}

#[test]
fn concurrent_writers_leave_one_complete_entry() {
    let directory = tempdir().unwrap();
    let cache = Arc::new(DiskCache::at(directory.path()));
    let mut writers = Vec::new();
    for value in 0..8u8 {
        let cache = cache.clone();
        writers.push(std::thread::spawn(move || {
            cache.put("mlb", "shared", &vec![value; 4096]).unwrap();
        }));
    }
    for writer in writers {
        writer.join().unwrap();
    }
    let entry = match cache.get("mlb", "shared", Duration::from_secs(60)).unwrap() {
        CacheLookup::Hit(entry) => entry,
        other => panic!("unexpected lookup: {other:?}"),
    };
    assert_eq!(entry.payload.len(), 4096);
    assert!(entry.payload.iter().all(|byte| *byte == entry.payload[0]));
}

#[cfg(unix)]
#[test]
fn prune_preserves_symlinks_and_reports_permission_failures() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let directory = tempdir().unwrap();
    let clock = AdjustableClock::at(100);
    let cache = DiskCache::at_with_clock(directory.path(), Arc::new(clock.clone()));
    cache.put("mlb", "unreadable", b"old").unwrap();
    let unreadable = cache.entry_path("mlb", "unreadable").unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
    clock.set(100 + 24 * 60 * 60);
    let report = cache.prune("mlb").unwrap();
    assert_eq!(report.failed, 1);
    assert!(unreadable.exists());
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600)).unwrap();

    let link = cache.entry_path("mlb", "link").unwrap();
    symlink(&unreadable, &link).unwrap();
    let report = cache.prune("mlb").unwrap();
    assert_eq!(report.unrelated, 1);
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
}
