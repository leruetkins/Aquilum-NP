//! Dropbox synchronization for vibecodium.
//!
//! This module is a port of the Aquilum Dropbox sync engine adapted to the
//! GPUI app model. The engine is deliberately file-based: Dropbox stores plain
//! files, and the local editor remains the owner of the open document buffer.
//! Remote writes land on disk; the existing fs watcher reloads clean open
//! documents and reports external changes on dirty ones.
//!
//! Design invariants:
//! * a local `records` row is the last agreed base for one path;
//! * local change detection is SHA-256 over the current file;
//! * cross-side equality uses Dropbox's own `content_hash`;
//! * a pull never overwrites a file whose hash no longer matches the base;
//! * deletions are guarded against mass deletion and an empty cloud folder;
//! * files are written through the same per-path lock as the editor's saves.

pub mod account;
pub mod apply;
pub mod conflict;
pub mod cycle;
pub mod decide;
pub mod dropbox;
pub mod halt;
pub mod hash;
pub mod identity;
pub mod measure;
pub mod merge;
pub mod oauth;
pub mod progress;
pub mod records;
pub mod remote;
pub mod scan;
pub mod service;
pub mod shared;
pub mod state;
pub mod transfer;
pub mod view;
pub mod worker;
pub mod write;

#[cfg(test)]
mod tests;

pub use records::{Large, Memory, Records, Size};
pub use remote::{Content, Delta, Remote, RemoteError, RemoteFile, RemoteResult};
pub use service::SyncService;
pub use state::SyncStatus;

use std::fmt;

#[derive(Debug)]
pub enum SyncError {
    Store(rusqlite::Error),
    Remote(RemoteError),
    File(std::io::Error),
    Changed,
    Deletions { planned: usize, synced: usize },
}

impl fmt::Display for SyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "sync database: {error}"),
            Self::Remote(error) => write!(formatter, "{error}"),
            Self::File(error) => write!(formatter, "{error}"),
            Self::Changed => formatter.write_str("the file changed during synchronization"),
            Self::Deletions { planned, synced } => {
                write!(formatter, "deleting {planned} of {synced} files; sync stopped")
            }
        }
    }
}

impl std::error::Error for SyncError {}

impl From<rusqlite::Error> for SyncError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Store(error)
    }
}

impl From<RemoteError> for SyncError {
    fn from(error: RemoteError) -> Self {
        Self::Remote(error)
    }
}

impl From<std::io::Error> for SyncError {
    fn from(error: std::io::Error) -> Self {
        Self::File(error)
    }
}
