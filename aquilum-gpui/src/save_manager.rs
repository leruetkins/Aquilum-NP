//! Adaptive save manager for vibecodium.
//!
//! Provides adaptive debouncing and deadline tracking based on content size,
//! preventing excessive disk writes for large documents while keeping small
//! notes instantly saved and safe.
//!
//! For files >= 10 MB, writes are made to a unique temporary file with `sync_all`
//! before replacing the target file, ensuring atomicity and preventing data
//! truncation. Writes to one path are serialized: two save tasks for the same
//! note can be dispatched before the first one finishes, and without the lock
//! they would share the temporary file and interleave their chunks.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

/// Threshold above which files are written atomically through a temporary file.
/// Files below 10 MB write directly with minimal filesystem overhead.
pub const ATOMIC_WRITE_THRESHOLD_BYTES: usize = 10 * 1024 * 1024;

/// SHA-256 of a rope's content, streaming chunk by chunk (no contiguous copy).
/// This is the buffer-side hash used for external-change detection.
pub fn hash_rope(rope: &crop::Rope) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    for chunk in rope.chunks() {
        hasher.update(chunk.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// SHA-256 of a file's content, streamed. `None` on any read error — callers
/// must treat a missing hash as "unknown, do the safe thing".
pub fn hash_file(path: &Path) -> Option<String> {
    use sha2::Digest;
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// File `(len, mtime)` stamp for the cheap pre-hash check; `None` when the
/// file is unreadable.
pub fn file_stamp(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// Returns `(debounce_duration, max_deadline)` based on file content length.
///
/// Uses a progressive non-linear square-root scaling model:
/// - Small (< 100 KB): fixed 500ms debounce, 5s max deadline.
/// - Files >= 100 KB:
///   `debounce = clamp(1.5 + 1.5 * sqrt(mb), 0.5s, 60.0s)`
///   `max_deadline = clamp(10.0 + 8.5 * sqrt(mb), 5.0s, 300.0s)`
pub fn adaptive_delays(content_len: usize) -> (Duration, Duration) {
    if content_len < 100 * 1024 {
        return (Duration::from_millis(500), Duration::from_secs(5));
    }

    let mb = content_len as f64 / (1024.0 * 1024.0);
    let sqrt_mb = mb.sqrt();

    let debounce_secs = (1.5 + 1.5 * sqrt_mb).clamp(0.5, 60.0);
    let deadline_secs = (10.0 + 8.5 * sqrt_mb).clamp(5.0, 300.0);

    (
        Duration::from_secs_f64(debounce_secs),
        Duration::from_secs_f64(deadline_secs),
    )
}

/// Lock guarding the writes to one path. Saves for the same note may be
/// dispatched before the previous one finished, and the executor runs them on a
/// pool, so without this lock two writers could pass the hash check together and
/// interleave their chunks in the temporary file.
fn path_lock(path: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    let mut locks = lock_unpoisoned(LOCKS.get_or_init(|| Mutex::new(HashMap::new())));
    locks.entry(path.to_path_buf()).or_default().clone()
}

/// What this process last wrote to a path.
///
/// `version` is the buffer version the written snapshot came from (monotonic per
/// document). A save task that reaches the disk after a newer snapshot must not
/// roll it back, and a hash that matches our own previous write must not be
/// mistaken for a foreign edit.
#[derive(Clone)]
struct Written {
    version: u64,
    hash: String,
}

fn written_saves() -> &'static Mutex<HashMap<PathBuf, Written>> {
    static WRITTEN: OnceLock<Mutex<HashMap<PathBuf, Written>>> = OnceLock::new();
    WRITTEN.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn recorded_save(path: &Path) -> Option<Written> {
    lock_unpoisoned(written_saves()).get(path).cloned()
}

fn remember_save(path: &Path, version: u64, hash: &str) {
    lock_unpoisoned(written_saves()).insert(
        path.to_path_buf(),
        Written {
            version,
            hash: hash.to_string(),
        },
    );
}

/// A temporary file next to `path` that no other write can share.
///
/// The per-write counter goes before the pid on purpose: the fs watcher
/// (`fs_watcher::classify::is_our_atomic_tmp`) reads the last dot-separated part
/// as the process id to tell our own atomic save from a user rename.
fn temp_sibling(path: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("note");
    let name = format!(
        ".{file_name}.{}.{}.tmp",
        SEQ.fetch_add(1, Ordering::Relaxed),
        std::process::id()
    );
    match path.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

fn write_and_remember(path: &Path, rope: &crop::Rope, version: u64) -> CheckedWrite {
    match write_rope(path, rope) {
        Ok(()) => {
            let hash = hash_rope(rope);
            remember_save(path, version, &hash);
            CheckedWrite::Written(hash)
        }
        Err(error) => CheckedWrite::Failed(error),
    }
}

/// Safely writes `rope` to `path` chunk-by-chunk without loading into a contiguous string.
pub fn safe_write_rope(path: &Path, rope: &crop::Rope) -> std::io::Result<()> {
    let lock = path_lock(path);
    let _guard = lock_unpoisoned(&lock);
    write_rope(path, rope)
}

/// The write itself, for callers that already hold the path lock.
fn write_rope(path: &Path, rope: &crop::Rope) -> std::io::Result<()> {
    if rope.byte_len() < ATOMIC_WRITE_THRESHOLD_BYTES {
        let mut file = std::fs::File::create(path)?;
        for chunk in rope.chunks() {
            file.write_all(chunk.as_bytes())?;
        }
        return file.flush();
    }

    let tmp_path = temp_sibling(path);
    let write_res = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp_path)?;
        for chunk in rope.chunks() {
            file.write_all(chunk.as_bytes())?;
        }
        file.flush()?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp_path, path)
    })();

    match write_res {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = std::fs::remove_file(&tmp_path);
            let mut file = std::fs::File::create(path)?;
            for chunk in rope.chunks() {
                file.write_all(chunk.as_bytes())?;
            }
            file.flush()
        }
    }
}

/// Outcome of the conflict-checked write (`safe_write_rope_checked`).
#[derive(Debug)]
pub enum CheckedWrite {
    /// Written; the hash is the new on-disk content hash to remember.
    Written(String),
    /// The file on disk diverged from the buffer's known hash since the load:
    /// **nothing was overwritten**. Our version has been preserved in a
    /// conflict copy next to the original; `disk_text` is the external content
    /// the buffer should adopt (already read here, on the background thread).
    Conflict { copy: PathBuf, disk_text: String },
    /// Write failed for an IO reason; nothing was changed.
    Failed(std::io::Error),
}

/// Writes `rope` to `path` only when the file on disk still matches
/// `expected_hash` (the hash the buffer was loaded with / last wrote).
///
/// A mismatch means another program changed the file while we were editing:
/// overwriting would silently destroy those changes. Instead the buffer's
/// version is preserved as a timestamped conflict copy and the external
/// content is returned for the buffer to adopt — the two-sided protocol of
/// the original Aquilum, reduced to its data-loss-free core.
///
/// `version` is the buffer version `rope` was captured from. Save tasks run on
/// a pool and can reach the disk out of dispatch order, so the check, the write
/// and the bookkeeping happen under one per-path lock, and a task whose snapshot
/// is older than what this process already wrote is a no-op.
///
/// Runs on a background thread in every call site; the reads and the hash
/// computations are IO work, not UI work.
pub fn safe_write_rope_checked(
    path: &Path,
    rope: &crop::Rope,
    expected_hash: Option<&str>,
    version: u64,
) -> CheckedWrite {
    let lock = path_lock(path);
    let _guard = lock_unpoisoned(&lock);

    let recorded = recorded_save(path);
    let actual = if path.exists() {
        hash_file(path)
    } else {
        None
    };

    // A newer snapshot from this process already reached the disk. Letting an
    // older one overwrite it would roll the file back.
    if let (Some(recorded), Some(actual)) = (&recorded, &actual) {
        if recorded.version > version && recorded.hash == *actual {
            return CheckedWrite::Written(recorded.hash.clone());
        }
    }

    // An unknown expected hash means the buffer was never reconciled with the
    // file (in-memory note, tests); writing without a check is the historical
    // behaviour for that case.
    let Some(expected) = expected_hash else {
        return write_and_remember(path, rope, version);
    };

    // Deleted externally while we were editing: writing recreates the file with
    // our content — that is the correct resolution (the deletion path already
    // handled the tab-close decision). An unreadable file takes the same route.
    let Some(actual) = actual else {
        return write_and_remember(path, rope, version);
    };

    // A queued save still carries the hash from before another save of this
    // process completed. That is not an external edit, so it is not a conflict.
    let ours_already = recorded.as_ref().is_some_and(|r| r.hash == actual);
    if actual == expected || ours_already {
        return write_and_remember(path, rope, version);
    }

    // Conflict: preserve our version, read the external one — unless the
    // on-disk content already equals what we are about to write (our own
    // earlier save won the race; rewriting would be a no-op and a
    // self-conflict would be pure noise).
    let ours_text = rope.to_string();
    if std::fs::read_to_string(path).ok().as_deref() == Some(ours_text.as_str()) {
        remember_save(path, version, &actual);
        return CheckedWrite::Written(actual);
    }
    let copy = match conflict_copy_path(path) {
        Some(candidate) => match safe_write_rope(&candidate, rope) {
            Ok(()) => candidate,
            Err(error) => return CheckedWrite::Failed(error),
        },
        None => return CheckedWrite::Failed(std::io::Error::other("cannot name a conflict copy")),
    };
    let disk_text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => return CheckedWrite::Failed(error),
    };
    CheckedWrite::Conflict { copy, disk_text }
}

/// Timestamped, collision-free name for a conflict copy next to `path`.
fn conflict_copy_path(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("md");
    // The label is UTC-based (no external crates); uniqueness comes from the
    // counter, recognisability from the timestamp.
    let label = format_utc_label(std::time::SystemTime::now());
    for attempt in 0..1000u32 {
        let name = if attempt == 0 {
            format!("{stem} (конфликт {label}).{ext}")
        } else {
            format!("{stem} (конфликт {label} {attempt}).{ext}")
        };
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// `YYYY-MM-DD HH-MM` from a `SystemTime`, in UTC (no external crates).
fn format_utc_label(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let (h, m) = (time_of_day / 3600, (time_of_day % 3600) / 60);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mth <= 2 { y + 1 } else { y };
    format!("{y:04}-{mth:02}-{d:02} {h:02}-{m:02}")
}

/// Safely writes `content` to `path`.
pub fn safe_write(path: &Path, content: &str) -> std::io::Result<()> {
    let lock = path_lock(path);
    let _guard = lock_unpoisoned(&lock);
    if content.len() < ATOMIC_WRITE_THRESHOLD_BYTES {
        return std::fs::write(path, content.as_bytes());
    }

    let tmp_path = temp_sibling(path);
    let write_res = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp_path, path)
    })();

    match write_res {
        Ok(()) => Ok(()),
        Err(_) => {
            // If atomic rename fails (e.g. file lock or cross-filesystem issue),
            // clean up the temp file and fall back to direct write.
            let _ = std::fs::remove_file(&tmp_path);
            std::fs::write(path, content.as_bytes())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str, content: &str) -> (std::path::PathBuf, crop::Rope) {
        let dir = std::env::temp_dir().join(format!("vibe_save_{}_{}", tag, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.md");
        std::fs::write(&path, content).unwrap();
        (path, crop::Rope::from(content.to_string()))
    }

    /// The core of the conflict protocol: a write against a stale expected
    /// hash must not overwrite the external content; our version lands in a
    /// conflict copy; the external text is returned for the buffer.
    #[test]
    fn checked_write_conflicts_into_a_copy_instead_of_overwriting() {
        let (path, _rope) = temp_file("conflict", "исходная версия");
        let loaded_hash = hash_file(&path).unwrap();

        // Another program rewrites the file while we were editing.
        std::fs::write(&path, "внешняя версия").unwrap();

        let ours = crop::Rope::from("наша версия".to_string());
        let outcome = safe_write_rope_checked(&path, &ours, Some(&loaded_hash), 1);

        match outcome {
            CheckedWrite::Conflict { copy, disk_text } => {
                // The disk keeps the external content.
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "внешняя версия");
                // Our version is intact in the copy.
                assert_eq!(std::fs::read_to_string(&copy).unwrap(), "наша версия");
                assert!(copy.to_string_lossy().contains("конфликт"));
                // The buffer is told to adopt the external text.
                assert_eq!(disk_text, "внешняя версия");
                // The copy sits next to the original, not elsewhere.
                assert_eq!(copy.parent(), path.parent());
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn checked_write_with_matching_hash_writes_and_returns_the_new_hash() {
        let (path, rope) = temp_file("match", "исходная версия");
        let loaded_hash = hash_file(&path).unwrap();

        let next = crop::Rope::from("новая версия".to_string());
        match safe_write_rope_checked(&path, &next, Some(&loaded_hash), 1) {
            CheckedWrite::Written(hash) => {
                assert_eq!(hash, hash_rope(&next));
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "новая версия");
            }
            other => panic!("expected a plain write, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn checked_write_with_unknown_hash_writes_directly() {
        let (path, _rope) = temp_file("unknown", "исходная версия");
        let next = crop::Rope::from("новая версия".to_string());
        match safe_write_rope_checked(&path, &next, None, 1) {
            CheckedWrite::Written(_) => {
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "новая версия");
            }
            other => panic!("expected a plain write, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn checked_write_recreates_a_externally_deleted_file() {
        let (path, _rope) = temp_file("deleted", "исходная версия");
        let loaded_hash = hash_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        let next = crop::Rope::from("наша версия".to_string());
        match safe_write_rope_checked(&path, &next, Some(&loaded_hash), 1) {
            CheckedWrite::Written(_) => {
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "наша версия");
            }
            other => panic!("expected a plain write, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Two save tasks for one path can reach the disk in either order. The older
    /// buffer version must not overwrite the newer one.
    #[test]
    fn a_late_older_save_does_not_roll_back_a_newer_one() {
        let (path, _rope) = temp_file("order", "исходная версия");
        let loaded_hash = hash_file(&path).unwrap();

        let newer = crop::Rope::from("новая версия".to_string());
        match safe_write_rope_checked(&path, &newer, Some(&loaded_hash), 2) {
            CheckedWrite::Written(hash) => assert_eq!(hash, hash_rope(&newer)),
            other => panic!("expected a plain write, got {other:?}"),
        }

        let older = crop::Rope::from("старая версия".to_string());
        match safe_write_rope_checked(&path, &older, Some(&loaded_hash), 1) {
            CheckedWrite::Written(hash) => assert_eq!(hash, hash_rope(&newer)),
            other => panic!("expected the newer save to stand, got {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "новая версия");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// A queued save still carries the hash from before another save of this
    /// process completed. That is not an external edit: no conflict copy.
    #[test]
    fn a_queued_save_recognises_our_own_write() {
        let (path, _rope) = temp_file("queued", "исходная версия");
        let loaded_hash = hash_file(&path).unwrap();

        let first = crop::Rope::from("первая версия".to_string());
        assert!(matches!(
            safe_write_rope_checked(&path, &first, Some(&loaded_hash), 1),
            CheckedWrite::Written(_)
        ));

        let second = crop::Rope::from("вторая версия".to_string());
        match safe_write_rope_checked(&path, &second, Some(&loaded_hash), 2) {
            CheckedWrite::Written(hash) => assert_eq!(hash, hash_rope(&second)),
            other => panic!("expected a plain write, got {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "вторая версия");
        let conflicts = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains("конфликт"))
            .count();
        assert_eq!(conflicts, 0, "our own write must not produce a conflict copy");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn atomic_temp_names_are_unique_and_keep_the_process_id_last() {
        let (path, _rope) = temp_file("tmpname", "x");
        let first = temp_sibling(&path);
        let second = temp_sibling(&path);
        assert_ne!(first, second, "two writes must not share a temporary file");
        let expected_suffix = format!(".{}.tmp", std::process::id());
        for tmp in [first, second] {
            let name = tmp.file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.starts_with(".note.md."), "unexpected tmp name: {name}");
            assert!(name.ends_with(&expected_suffix), "unexpected tmp name: {name}");
            assert!(crate::fs_watcher::classify::is_our_atomic_tmp(&tmp));
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The reported corruption: two overlapping saves of a big note shared one
    /// temporary file and interleaved their chunks, leaving bytes that are not
    /// valid UTF-8. Serialized writes always leave one whole version.
    #[test]
    fn overlapping_big_writes_never_interleave() {
        const LEN: usize = ATOMIC_WRITE_THRESHOLD_BYTES + 64 * 1024;
        let (path, _rope) = temp_file("overlap", "старт");

        let a = crop::Rope::from("a".repeat(LEN));
        let b = crop::Rope::from("b".repeat(LEN));
        std::thread::scope(|scope| {
            let (path_a, rope_a) = (&path, a.clone());
            scope.spawn(move || {
                let _ = safe_write_rope_checked(path_a, &rope_a, None, 1);
            });
            let (path_b, rope_b) = (&path, b.clone());
            scope.spawn(move || {
                let _ = safe_write_rope_checked(path_b, &rope_b, None, 2);
            });
        });

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), LEN);
        // Version 2 is the newest snapshot, so it must be the one on disk —
        // whichever task got the lock first.
        let text = String::from_utf8(bytes).expect("a mixed write would not be valid UTF-8");
        assert!(text.bytes().all(|c| c == b'b'), "the newest version must win");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn conflict_copy_names_are_collision_free() {
        let (path, _rope) = temp_file("collision", "исходная версия");
        let first = conflict_copy_path(&path).unwrap();
        std::fs::write(&first, "").unwrap();
        let second = conflict_copy_path(&path).unwrap();
        assert_ne!(first, second, "an occupied name must rotate to a numbered one");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn utc_label_is_wellformed() {
        let label = format_utc_label(std::time::SystemTime::UNIX_EPOCH);
        assert_eq!(label, "1970-01-01 00-00");
    }

    #[test]
    fn test_adaptive_delays_progressive() {
        // Small (< 100 KB) - fixed 500ms debounce, 5s deadline
        assert_eq!(adaptive_delays(0), (Duration::from_millis(500), Duration::from_secs(5)));
        assert_eq!(adaptive_delays(50 * 1024), (Duration::from_millis(500), Duration::from_secs(5)));
        assert_eq!(adaptive_delays(100 * 1024 - 1), (Duration::from_millis(500), Duration::from_secs(5)));

        // 100 KB (~0.098 MB) -> ~1.97s debounce, ~12.66s deadline
        let (d_100k, m_100k) = adaptive_delays(100 * 1024);
        assert!((d_100k.as_secs_f64() - 1.97).abs() < 0.05);
        assert!((m_100k.as_secs_f64() - 12.66).abs() < 0.1);

        // 1 MB -> exactly 3.0s debounce, 18.5s deadline
        let (d_1m, m_1m) = adaptive_delays(1024 * 1024);
        assert!((d_1m.as_secs_f64() - 3.0).abs() < 0.01);
        assert!((m_1m.as_secs_f64() - 18.5).abs() < 0.01);

        // 10 MB -> ~6.24s debounce, ~36.88s deadline
        let (d_10m, m_10m) = adaptive_delays(10 * 1024 * 1024);
        assert!((d_10m.as_secs_f64() - 6.24).abs() < 0.05);
        assert!((m_10m.as_secs_f64() - 36.88).abs() < 0.1);

        // 50 MB -> ~12.1s debounce, ~70.1s deadline
        let (d_50m, m_50m) = adaptive_delays(50 * 1024 * 1024);
        assert!((d_50m.as_secs_f64() - 12.1).abs() < 0.1);
        assert!((m_50m.as_secs_f64() - 70.1).abs() < 0.5);

        // 100 MB -> ~16.5s debounce, ~95.0s deadline
        let (d_100m, m_100m) = adaptive_delays(100 * 1024 * 1024);
        assert!((d_100m.as_secs_f64() - 16.5).abs() < 0.1);
        assert!((m_100m.as_secs_f64() - 95.0).abs() < 0.5);

        // 700 MB -> ~41.2s debounce, ~234.9s deadline
        let (d_700m, m_700m) = adaptive_delays(700 * 1024 * 1024);
        assert!((d_700m.as_secs_f64() - 41.2).abs() < 0.2);
        assert!((m_700m.as_secs_f64() - 234.9).abs() < 1.0);

        // Monotonic progression: larger content must always have >= debounce and >= deadline
        let sizes = [0, 50 * 1024, 100 * 1024, 500 * 1024, 1024 * 1024, 10 * 1024 * 1024, 100 * 1024 * 1024, 700 * 1024 * 1024, 2 * 1024 * 1024 * 1024];
        for pair in sizes.windows(2) {
            let (d1, m1) = adaptive_delays(pair[0]);
            let (d2, m2) = adaptive_delays(pair[1]);
            assert!(d2 >= d1, "debounce for {} must be >= {}", pair[1], pair[0]);
            assert!(m2 >= m1, "deadline for {} must be >= {}", pair[1], pair[0]);
        }
    }

    #[test]
    fn test_safe_write_small_file() {
        let temp_dir = std::env::temp_dir().join(format!("vibecodium_test_small_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let file_path = temp_dir.join("test_small.md");

        let content = "Hello world!";
        safe_write(&file_path, content).expect("safe_write small failed");

        let read_back = std::fs::read_to_string(&file_path).expect("read failed");
        assert_eq!(read_back, content);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_safe_write_large_file_atomic() {
        let temp_dir = std::env::temp_dir().join(format!("vibecodium_test_large_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let file_path = temp_dir.join("test_large.md");

        // Create > 10 MB content to trigger atomic path
        let content = "a".repeat(ATOMIC_WRITE_THRESHOLD_BYTES + 100);
        safe_write(&file_path, &content).expect("safe_write large failed");

        let read_back = std::fs::read_to_string(&file_path).expect("read failed");
        assert_eq!(read_back.len(), content.len());
        assert_eq!(read_back, content);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
