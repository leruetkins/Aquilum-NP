//! Adaptive save manager for vibecodium.
//!
//! Provides adaptive debouncing and deadline tracking based on content size,
//! preventing excessive disk writes for large documents while keeping small
//! notes instantly saved and safe.
//!
//! For files >= 10 MB, writes are made to a temporary file with `sync_all` before
//! replacing the target file, ensuring atomicity and preventing data truncation.

use std::path::Path;
use std::time::Duration;

/// Threshold above which files are written atomically through a temporary file.
/// Files below 10 MB write directly with minimal filesystem overhead.
pub const ATOMIC_WRITE_THRESHOLD_BYTES: usize = 10 * 1024 * 1024;

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

/// Safely writes `rope` to `path` chunk-by-chunk without loading into a contiguous string.
pub fn safe_write_rope(path: &Path, rope: &crop::Rope) -> std::io::Result<()> {
    use std::io::Write;
    if rope.byte_len() < ATOMIC_WRITE_THRESHOLD_BYTES {
        let mut file = std::fs::File::create(path)?;
        for chunk in rope.chunks() {
            file.write_all(chunk.as_bytes())?;
        }
        return file.flush();
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("note");
    let pid = std::process::id();
    let tmp_name = format!(".{file_name}.{pid}.tmp");
    let tmp_path = match path.parent() {
        Some(parent) => parent.join(tmp_name),
        None => std::path::PathBuf::from(tmp_name),
    };

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

/// Safely writes `content` to `path`.
pub fn safe_write(path: &Path, content: &str) -> std::io::Result<()> {
    if content.len() < ATOMIC_WRITE_THRESHOLD_BYTES {
        return std::fs::write(path, content.as_bytes());
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("note");
    let pid = std::process::id();
    let tmp_name = format!(".{file_name}.{pid}.tmp");
    let tmp_path = match path.parent() {
        Some(parent) => parent.join(tmp_name),
        None => std::path::PathBuf::from(tmp_name),
    };

    let write_res = (|| -> std::io::Result<()> {
        use std::io::Write;
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
