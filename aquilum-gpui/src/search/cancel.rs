//! Cancellation for background searches.
//!
//! Modeled on the reference `core/src/text/cancel.rs` (idea only): a shared
//! counter on an `AtomicU64`. Asking for a new search bumps the counter, so
//! every scan still running under an older `Cancel` token sees a mismatch and
//! exits early instead of finishing work nobody will read.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Diagnostics: scans that exited early because a newer request superseded
/// them (see `search_goto_files_opts`).
pub static CANCELLED_SCANS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Default)]
pub struct Generation(Arc<AtomicU64>);

impl Generation {
    /// Issues a `Cancel` token for a new request: all tokens issued before
    /// become cancelled.
    pub fn next(&self) -> Cancel {
        let mine = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        Cancel(Some((Arc::clone(&self.0), mine)))
    }

    pub fn current(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Default)]
pub struct Cancel(Option<(Arc<AtomicU64>, u64)>);

impl Cancel {
    pub fn generation(&self) -> u64 {
        self.0.as_ref().map_or(0, |(_, mine)| *mine)
    }

    /// True once a newer token has been issued from the same [`Generation`].
    pub fn is_cancelled(&self) -> bool {
        self.0
            .as_ref()
            .is_some_and(|(latest, mine)| latest.load(Ordering::Relaxed) != *mine)
    }
}

#[cfg(test)]
mod tests {
    use super::Generation;
    use super::Cancel as _C;

    #[test]
    fn a_newer_generation_cancels_the_older_one() {
        let generation = Generation::default();
        let first = generation.next();
        assert!(!first.is_cancelled());
        let second = generation.next();
        assert!(first.is_cancelled() && !second.is_cancelled());
        assert_eq!(second.generation(), generation.current());
    }
}
