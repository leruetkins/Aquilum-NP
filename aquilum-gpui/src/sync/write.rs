use crate::save_manager::{hash_file, path_lock, temp_sibling};
use crate::sync::remote::Content;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub enum WriteOutcome {
    Written(String),
    /// The file on disk no longer matches the base hash. Nothing was written.
    Conflict { actual: Option<String> },
    Failed(io::Error),
}

/// Streams `content` to a temporary sibling and renames it over `path` after
/// checking `expected` against the current file. The same per-path lock as the
/// editor's saves is used, so a sync write and a user save cannot interleave.
pub fn write_content_checked(
    path: &Path,
    content: &dyn Content,
    expected: Option<&str>,
) -> WriteOutcome {
    let lock = path_lock(path);
    let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());

    let actual = if path.exists() { hash_file(path) } else { None };
    if let Some(expected) = expected {
        if let Some(actual) = &actual {
            if actual != expected {
                return WriteOutcome::Conflict { actual: Some(actual.clone()) };
            }
        }
    }

    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return WriteOutcome::Failed(error);
        }
    }
    let temp = temp_sibling(path);
    match write_temp(&temp, content) {
        Ok(hash) => match fs::rename(&temp, path) {
            Ok(()) => WriteOutcome::Written(hash),
            Err(error) => {
                let _ = fs::remove_file(&temp);
                WriteOutcome::Failed(error)
            }
        },
        Err(error) => {
            let _ = fs::remove_file(&temp);
            WriteOutcome::Failed(error)
        }
    }
}

/// Creates a file only when it does not exist yet; a concurrent creator wins
/// and the caller treats it as `Changed` on the next pass.
pub fn create_content(path: &Path, content: &dyn Content) -> WriteOutcome {
    let lock = path_lock(path);
    let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return WriteOutcome::Failed(error);
        }
    }
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return WriteOutcome::Conflict { actual: hash_file(path) };
        }
        Err(error) => return WriteOutcome::Failed(error),
    };
    let mut hasher = Sha256::new();
    let result = content
        .read(0..content.byte_len(), &mut |bytes| {
            hasher.update(bytes);
            file.write_all(bytes)
        })
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all());
    match result {
        Ok(()) => WriteOutcome::Written(format!("{:x}", hasher.finalize())),
        Err(error) => {
            drop(file);
            let _ = fs::remove_file(path);
            WriteOutcome::Failed(error)
        }
    }
}

pub fn create_bytes(path: &Path, bytes: &[u8]) -> WriteOutcome {
    let lock = path_lock(path);
    let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return WriteOutcome::Failed(error);
        }
    }
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return WriteOutcome::Conflict { actual: hash_file(path) };
        }
        Err(error) => return WriteOutcome::Failed(error),
    };
    let result = file
        .write_all(bytes)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all());
    match result {
        Ok(()) => WriteOutcome::Written(crate::sync::hash::hash_bytes(bytes)),
        Err(error) => {
            drop(file);
            let _ = fs::remove_file(path);
            WriteOutcome::Failed(error)
        }
    }
}

pub fn write_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {    let lock = path_lock(path);
    let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = temp_sibling(path);
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_temp(path: &Path, content: &dyn Content) -> io::Result<String> {
    let mut file = fs::File::create(path)?;
    let mut hasher = Sha256::new();
    content.read(0..content.byte_len(), &mut |bytes| {
        hasher.update(bytes);
        file.write_all(bytes)
    })?;
    file.flush()?;
    file.sync_all()?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn sibling_path(path: &Path, label: &str, extension: &str, attempt: usize) -> PathBuf {
    let name = if attempt == 0 {
        format!("{label}{extension}")
    } else {
        format!("{label} {}{extension}", attempt + 1)
    };
    path.with_file_name(name)
}
