//! Event-driven filesystem watching for the workspace root (plan:
//! `TODO/fs-watcher-plan.md`). Mirrors the scheme of the original Aquilum
//! without copying its AGPL code: `notify` events are classified into
//! per-kind buckets, coalesced on a dedicated delivery thread, and drained by
//! the UI thread through a channel — the UI thread never polls the filesystem
//! itself.
//!
//! This module holds no GPUI types and no app state: the `AppState` owns the
//! `Watcher` and the receiving end of the batch channel.

pub mod batch;
pub mod classify;

pub use batch::{coalesce, FsBatch};

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Owns the notify watcher and its delivery thread. Watching the same root
/// again is a no-op; watching a new root stops the previous watcher first
/// (the retarget semantics of the original's `WorkspaceWatcher`).
pub struct Watcher {
    inner: Mutex<Option<Running>>,
}

struct Running {
    root: PathBuf,
    stopping: Arc<std::sync::atomic::AtomicBool>,
    /// Injects the stop wake-up into the delivery thread's input channel.
    wake: Sender<FsBatch>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Watcher {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Starts (or retargets) recursive watching of `root`; coalesced batches
    /// land on `events`.
    pub fn watch(&self, root: &Path, events: Sender<FsBatch>) -> Result<(), String> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(active) = guard.as_ref() {
            if active.root == root {
                return Ok(());
            }
        }
        if let Some(previous) = guard.take() {
            previous.stop();
        }

        let (raw_tx, raw_rx) = std::sync::mpsc::channel::<FsBatch>();
        let stopping = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let watched = root.to_path_buf();
        let classify_tx = raw_tx.clone();
        let watcher_result: notify::Result<Box<dyn notify::Watcher + Send>> =
            match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Some(batch) = classify::classify(result, &watched) {
                    let _ = classify_tx.send(batch);
                }
            }) {
                Ok(watcher) => Ok(Box::new(watcher)),
                Err(error) => {
                    // Network drives and similar mounts may lack a native
                    // watcher; a slow poller keeps the same event API without
                    // any UI-thread filesystem calls.
                    let watched = root.to_path_buf();
                    let poll_tx = raw_tx.clone();
                    let config = notify::Config::default()
                        .with_poll_interval(Duration::from_secs(2))
                        .with_compare_contents(false);
                    match notify::PollWatcher::new(
                        move |result: notify::Result<notify::Event>| {
                            if let Some(batch) = classify::classify(result, &watched) {
                                let _ = poll_tx.send(batch);
                            }
                        },
                        config,
                    ) {
                        Ok(poll) => Ok(Box::new(poll)),
                        Err(_) => Err(error),
                    }
                }
            };
        let mut watcher = match watcher_result {
            Ok(watcher) => watcher,
            Err(error) => return Err(format!("failed to create watcher: {error}")),
        };
        watcher
            .watch(root, notify::RecursiveMode::Recursive)
            .map_err(|error| format!("failed to watch {}: {error}", root.display()))?;

        // The delivery thread owns the notify watcher — dropping it on exit
        // unregisters the watches — and merges raw events into coalesced
        // batches for the UI.
        let out = events;
        let thread_stopping = Arc::clone(&stopping);
        let thread = std::thread::Builder::new()
            .name("aquilum-fs-watch".to_owned())
            .spawn(move || {
                let _watcher = watcher;
                while let Ok(first) = raw_rx.recv() {
                    if thread_stopping.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    let batch = batch::coalesce(first, &raw_rx);
                    if thread_stopping.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    if out.send(batch).is_err() {
                        break;
                    }
                }
                // `_watcher` drops here.
            })
            .map_err(|error| format!("failed to spawn the watcher thread: {error}"))?;

        *guard = Some(Running {
            root: root.to_path_buf(),
            stopping,
            wake: raw_tx,
            thread: Some(thread),
        });
        Ok(())
    }

    pub fn stop(&self) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(previous) = guard.take() {
            previous.stop();
        }
    }

    pub fn watched_root(&self) -> Option<PathBuf> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|r| r.root.clone())
    }
}

impl Running {
    fn stop(self) {
        self.stopping
            .store(true, std::sync::atomic::Ordering::Relaxed);
        // Wake the delivery thread out of `recv`; it observes the flag and
        // exits without delivering this synthetic batch.
        let _ = self.wake.send(FsBatch::rescan());
        if let Some(thread) = self.thread {
            let _ = thread.join();
        }
    }
}

impl Default for Watcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vibe_fs_watch_{tag}_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn delivers_a_real_file_change_to_the_channel() {
        let dir = temp_dir("deliver");
        let (tx, rx) = mpsc::channel();
        let watcher = Watcher::new();
        watcher.watch(&dir, tx).expect("watch must start");

        std::fs::write(dir.join("Заметка.md"), "текст").unwrap();

        let batch = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the watcher must deliver the file change");
        assert!(
            batch.modified.iter().any(|p| p.file_name().is_some_and(|n| n == "Заметка.md")),
            "a write to a note must arrive as a modification: {batch:?}"
        );
        watcher.stop();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn watching_the_same_root_again_is_a_noop() {
        let dir = temp_dir("noop");
        let (tx, _rx) = mpsc::channel();
        let watcher = Watcher::new();
        watcher.watch(&dir, tx.clone()).unwrap();
        assert_eq!(watcher.watched_root(), Some(dir.clone()));
        // Same root: must not stop/restart (and must not error).
        watcher.watch(&dir, tx).unwrap();
        assert_eq!(watcher.watched_root(), Some(dir.clone()));
        watcher.stop();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retargeting_moves_the_watch_to_the_new_root() {
        let first = temp_dir("retarget_a");
        let second = temp_dir("retarget_b");
        let (tx, rx) = mpsc::channel();
        let watcher = Watcher::new();
        watcher.watch(&first, tx.clone()).unwrap();
        assert_eq!(watcher.watched_root(), Some(first.clone()));

        // Retarget: the previous root is stopped, the new one is watched.
        watcher.watch(&second, tx).unwrap();
        assert_eq!(watcher.watched_root(), Some(second.clone()));

        std::fs::write(second.join("Вторая.md"), "текст").unwrap();
        let batch = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the new root must be watched");
        assert!(
            batch.modified.iter().any(|p| p.file_name().is_some_and(|n| n == "Вторая.md")),
            "the new root's write must arrive: {batch:?}"
        );

        // The old root is no longer observed: writes there never arrive.
        std::fs::write(first.join("Первая.md"), "текст").unwrap();
        assert!(
            rx.recv_timeout(Duration::from_millis(1200)).is_err(),
            "the old root must not be watched after retarget"
        );

        watcher.stop();
        let _ = std::fs::remove_dir_all(&first);
        let _ = std::fs::remove_dir_all(&second);
    }

    #[test]
    fn stop_stops_delivering() {
        let dir = temp_dir("stop");
        let (tx, rx) = mpsc::channel();
        let watcher = Watcher::new();
        watcher.watch(&dir, tx).unwrap();
        // Let the initial directory scan events (if any) drain out.
        std::thread::sleep(Duration::from_millis(200));
        while rx.try_recv().is_ok() {}

        watcher.stop();
        std::fs::write(dir.join("После.md"), "текст").unwrap();

        assert!(
            rx.recv_timeout(Duration::from_millis(800)).is_err(),
            "nothing may be delivered after stop"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
