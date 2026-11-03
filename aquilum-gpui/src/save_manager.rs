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
mod tests;
