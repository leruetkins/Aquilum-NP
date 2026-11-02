//! Batching of raw watcher events: an 80 ms window merges the burst of
//! events one user-visible operation produces (a bulk save, a folder move)
//! into a single `FsBatch` before it reaches the UI thread.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Coalesce window: long enough to swallow the burst of one operation, short
/// enough to keep the perceived reaction latency well under a quarter second.
pub const BATCH_WINDOW: Duration = Duration::from_millis(80);

/// One coalesced watcher delivery. Kinds are kept in separate buckets — they
/// drive per-path actions (delete a tab vs reload a note), so merging paths
/// across kinds would misattribute actions. `rescan` subsumes everything:
/// the consumer reconciles the whole workspace and ignores the buckets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FsBatch {
    pub deleted: Vec<PathBuf>,
    pub renamed: Vec<(PathBuf, PathBuf)>,
    pub modified: Vec<PathBuf>,
    pub rescan: bool,
    /// A non-note file (an image attachment) was created, deleted or renamed.
    /// The editor drops its attachment resolution caches and repaints.
    pub attachments_touched: bool,
}

impl FsBatch {
    pub fn rescan() -> Self {
        Self {
            rescan: true,
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.rescan
            && !self.attachments_touched
            && self.deleted.is_empty()
            && self.renamed.is_empty()
            && self.modified.is_empty()
    }

    pub fn absorb(&mut self, other: FsBatch) {
        self.rescan |= other.rescan;
        self.attachments_touched |= other.attachments_touched;
        self.deleted.extend(other.deleted);
        self.renamed.extend(other.renamed);
        self.modified.extend(other.modified);
    }

    fn dedup(&mut self) {
        self.deleted.sort();
        self.deleted.dedup();
        self.modified.sort();
        self.modified.dedup();
        self.renamed.sort();
        self.renamed.dedup();
    }
}

/// Merges `first` with every batch arriving within [`BATCH_WINDOW`].
/// Returns as soon as the window closes; a disconnected or idle channel ends
/// the wait the same way.
pub fn coalesce(first: FsBatch, receiver: &mpsc::Receiver<FsBatch>) -> FsBatch {
    let deadline = Instant::now() + BATCH_WINDOW;
    let mut batch = first;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(remaining) {
            Ok(next) => batch.absorb(next),
            Err(mpsc::RecvTimeoutError::Timeout) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                break
            }
        }
    }
    batch.dedup();
    batch
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send_all(sender: &mpsc::Sender<FsBatch>, batches: Vec<FsBatch>) {
        for batch in batches {
            sender.send(batch).unwrap();
        }
    }

    #[test]
    fn merges_paths_into_their_own_buckets_and_dedups() {
        let (tx, rx) = mpsc::channel();
        send_all(
            &tx,
            vec![
                FsBatch {
                    deleted: vec![PathBuf::from("b.md"), PathBuf::from("a.md")],
                    modified: vec![PathBuf::from("c.md"), PathBuf::from("c.md")],
                    ..Default::default()
                },
                FsBatch {
                    renamed: vec![(PathBuf::from("x.md"), PathBuf::from("y.md"))],
                    ..Default::default()
                },
            ],
        );
        drop(tx);

        let merged = coalesce(FsBatch::default(), &rx);
        assert_eq!(merged.deleted, vec![PathBuf::from("a.md"), PathBuf::from("b.md")]);
        assert_eq!(merged.modified, vec![PathBuf::from("c.md")]);
        assert_eq!(merged.renamed, vec![(PathBuf::from("x.md"), PathBuf::from("y.md"))]);
        assert!(!merged.rescan);
    }

    #[test]
    fn rescan_is_sticky_across_the_window() {
        let (tx, rx) = mpsc::channel();
        send_all(
            &tx,
            vec![
                FsBatch {
                    modified: vec![PathBuf::from("a.md")],
                    ..Default::default()
                },
                FsBatch::rescan(),
            ],
        );
        drop(tx);

        let merged = coalesce(FsBatch::default(), &rx);
        assert!(merged.rescan);
        assert_eq!(merged.modified, vec![PathBuf::from("a.md")]);
    }

    #[test]
    fn returns_the_first_batch_when_nothing_else_arrives() {
        let (_tx, rx) = mpsc::channel::<FsBatch>();
        let mut first = FsBatch::default();
        first.modified.push(PathBuf::from("a.md"));
        let merged = coalesce(first, &rx);
        assert_eq!(merged.modified, vec![PathBuf::from("a.md")]);
    }
}
