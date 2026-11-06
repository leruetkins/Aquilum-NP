//! Shared vault-trash operations.
//!
//! The editor and the sync engine must move files into `.trash` the same way:
//! the same collision suffix, the same parent directories and the same
//! attachment handling. Keeping this in one module prevents the two paths
//! from drifting apart.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const FOLDER: &str = ".trash";

/// A destination that never overwrites an earlier copy with the same path.
pub fn destination(root: &Path, relative: &str) -> PathBuf {
    let trash_root = root.join(FOLDER);
    let dest = trash_root.join(relative);
    if !dest.exists() {
        return dest;
    }
    let parent = dest.parent().map(Path::to_path_buf).unwrap_or(trash_root);
    let stem = dest
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = dest.extension().map(|extension| extension.to_string_lossy().into_owned());
    let mut attempt = 1u32;
    loop {
        let name = match &extension {
            Some(extension) => format!("{stem} {attempt}.{extension}"),
            None => format!("{stem} {attempt}"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
    }
}

/// Moves one file into `.trash/<relative>`, creating parents as needed.
pub fn move_to_trash(root: &Path, source: &Path, relative: &str) -> io::Result<PathBuf> {
    if !source.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "source is gone"));
    }
    let target = destination(root, relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(source, &target)?;
    Ok(target)
}

/// Moves a note and any attachments that become unreferenced with it.
pub fn trash_note(root: &Path, source: &Path, relative: &str) -> io::Result<PathBuf> {
    let content = note_content(source);
    let target = move_to_trash(root, source, relative)?;
    if let Some(content) = content {
        crate::images::trash_note_attachments(
            root,
            source,
            &content,
            &crate::images::folder(),
            &[source],
        );
    }
    Ok(target)
}

/// Reads the text of a note-like file; binary files return `None`.
pub fn note_content(path: &Path) -> Option<String> {
    let is_note = matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("md") | Some("markdown") | Some("txt")
    );
    if !is_note {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// Removes files from `.trash` older than `retention_days`. Zero means keep
/// everything. Returns the number of removed files.
pub fn purge(root: &Path, retention_days: u32) -> io::Result<usize> {
    if retention_days == 0 {
        return Ok(0);
    }
    let Some(cutoff) = SystemTime::now().checked_sub(Duration::from_secs(u64::from(retention_days) * 86_400)) else {
        return Ok(0);
    };
    let trash = root.join(FOLDER);
    if !trash.is_dir() {
        return Ok(0);
    }
    let mut removed = 0;
    let mut stack = vec![trash];
    while let Some(directory) = stack.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                let modified = entry.metadata()?.modified().unwrap_or(SystemTime::now());
                if modified < cutoff {
                    fs::remove_file(&path)?;
                    removed += 1;
                }
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    #[test]
    fn collisions_get_a_numbered_name() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        write(&root.join(".trash/notes/a.md"), "old");
        assert_eq!(destination(root, "notes/a.md"), root.join(".trash/notes/a 1.md"));
        write(&root.join(".trash/notes/a 1.md"), "older");
        assert_eq!(destination(root, "notes/a.md"), root.join(".trash/notes/a 2.md"));
    }

    #[test]
    fn move_creates_parents_and_purge_removes_only_old_files() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let source = root.join("notes/a.md");
        write(&source, "text");
        let target = move_to_trash(root, &source, "notes/a.md").unwrap();
        assert!(!source.exists());
        assert!(target.exists());

        let old = target.clone();
        let file = fs::File::options().write(true).open(&old).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(86_400)).unwrap();
        drop(file);
        let fresh = root.join(".trash/notes/fresh.md");
        write(&fresh, "fresh");

        assert_eq!(purge(root, 30).unwrap(), 1);
        assert!(!old.exists());
        assert!(fresh.exists());
        assert_eq!(purge(root, 0).unwrap(), 0);
    }

    #[test]
    fn missing_source_is_not_an_error_for_the_caller_to_mask() {
        let directory = tempfile::tempdir().unwrap();
        let error = move_to_trash(directory.path(), &directory.path().join("gone.md"), "gone.md")
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
