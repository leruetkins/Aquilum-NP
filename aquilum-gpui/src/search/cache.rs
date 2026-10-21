//! Per-session cache of file bodies for the search scans.
//!
//! Typing a query scans the same files over and over; re-reading every one of
//! them per keystroke is what makes the search stutter. Entries are reused
//! while the file's `(len, mtime)` stamp is unchanged, so a repeat keystroke
//! costs only one `metadata` call per file. The cache is owned by the search
//! worker (`search::worker`), so the UI thread never touches the filesystem.
//!
//! Eviction is LRU (least-recently-used file bodies go first). The previous
//! wholesale clear-on-overflow thrashed a vault whose bounded heads together
//! brushed the budget: every scan dropped the whole cache and re-read the
//! disk — that is what made every new search feel like a cold scan.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Files larger than this are searched through a bounded head instead of the
/// whole body.
pub(crate) const MAX_FILE_BYTES: u64 = 512 * 1024;

/// How much of an oversized file search sees (the full body of every file
/// within [`MAX_FILE_BYTES`], the first [`OVERSIZE_HEAD_BYTES`] of bigger
/// ones). Keeps multi-megabyte notes findable without reading or caching
/// gigabytes per keystroke.
pub(crate) const OVERSIZE_HEAD_BYTES: u64 = 1024 * 1024;

/// Diagnostics: file bodies actually read from disk (cache misses), ever.
pub static BODY_READS: AtomicU64 = AtomicU64::new(0);

/// Default cache budget.
const CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

/// The text search sees for `path`: the full body within [`MAX_FILE_BYTES`],
/// otherwise the bounded head, empty when unreadable. The index builder MUST
/// use this same function — index tokens are verified against this text.
pub(crate) fn body_for_search(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() <= MAX_FILE_BYTES => {
            BODY_READS.fetch_add(1, Ordering::Relaxed);
            std::fs::read_to_string(path).unwrap_or_default()
        }
        Ok(meta) if meta.len() > MAX_FILE_BYTES => {
            BODY_READS.fetch_add(1, Ordering::Relaxed);
            read_head(path, OVERSIZE_HEAD_BYTES)
        }
        _ => String::new(),
    }
}

/// First `cap` bytes of `path` as text (lossy UTF-8: the cut may split a
/// multi-byte char at the boundary).
fn read_head(path: &Path, cap: u64) -> String {
    let Ok(file) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut buf = Vec::with_capacity(cap as usize);
    if file.take(cap).read_to_end(&mut buf).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&buf).into_owned()
}

struct CachedContent {
    stamp: Option<(u64, std::time::SystemTime)>,
    /// `None` — no metadata (missing file, no permission); remembered so
    /// failures are not re-attempted on every keystroke. Present-but-invalid
    /// UTF-8 is lossily converted instead — bounded by the head cap.
    content: Option<Arc<String>>,
    bytes: usize,
}

/// LRU cache of file bodies shared by the search surfaces.
///
/// Bodies are stored as the ORIGINAL text (`Arc<String>`), not a case-folded
/// copy: case-sensitive search (`Aa`) matches against the original, and
/// snippets are cut from it directly — the folding matcher reports byte
/// ranges in the original anyway. Oversized files are stored as their bounded
/// head ([`OVERSIZE_HEAD_BYTES`]), so they stay body-searchable.
pub(crate) struct SearchContentCache {
    entries: HashMap<PathBuf, CachedContent>,
    /// Recency order, back = most recently used. Stale duplicates are
    /// compacted lazily (a hit pushes without removing the old position).
    order: VecDeque<PathBuf>,
    total_bytes: usize,
    budget: usize,
}

impl SearchContentCache {
    pub(crate) fn new() -> Self {
        Self::with_budget(CACHE_MAX_BYTES)
    }

    pub(crate) fn with_budget(budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            total_bytes: 0,
            budget,
        }
    }

    /// True when nothing is cached (tests: a cancelled search must leave it
    /// this way — it may not read a single body).
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn get_or_load(&mut self, path: &Path) -> Option<Arc<String>> {
        let stamp = std::fs::metadata(path).ok().and_then(|meta| {
            meta.modified().ok().map(|modified| (meta.len(), modified))
        });
        let hit = self
            .entries
            .get(path)
            .filter(|entry| entry.stamp == stamp)
            .and_then(|entry| entry.content.clone());
        if let Some(content) = hit {
            self.touch(path);
            return Some(content);
        }
        let content = stamp.is_some().then(|| Arc::new(body_for_search(path)));
        let bytes = content.as_ref().map_or(0, |s| s.len());
        self.insert(
            path.to_path_buf(),
            CachedContent {
                stamp,
                content: content.clone(),
                bytes,
            },
        );
        content
    }

    /// Marks `path` most recently used: its existing queue position moves to
    /// the back (no duplicates — a stale front duplicate of a hot entry
    /// would otherwise look like the LRU victim during eviction).
    fn touch(&mut self, path: &Path) {
        self.order.retain(|p| p != path);
        self.order.push_back(path.to_path_buf());
        if self.order.len() > self.entries.len() * 2 + 8 {
            let live: HashSet<PathBuf> = self.entries.keys().cloned().collect();
            self.order.retain(|p| live.contains(p));
        }
    }

    fn insert(&mut self, path: PathBuf, entry: CachedContent) {
        if let Some(old) = self.entries.remove(&path) {
            self.total_bytes = self.total_bytes.saturating_sub(old.bytes);
        }
        self.total_bytes += entry.bytes;
        self.entries.insert(path.clone(), entry);
        // Drop stale queue positions of this path, then the eviction pops
        // REAL victims only: a leftover duplicate position of a hot entry
        // would otherwise evict the live entry (front == stale dup).
        self.order.retain(|p| p != &path);
        self.order.push_back(path);
        // Evict least-recently-used entries until back under budget. Dead
        // positions (path re-touched later, slot already gone) are skipped:
        // only entries still in `entries` count as evicted.
        while self.total_bytes > self.budget && self.entries.len() > 1 {
            let Some(mut victim) = self.order.pop_front() else {
                break;
            };
            while !self.entries.contains_key(&victim) {
                match self.order.pop_front() {
                    Some(next) => victim = next,
                    None => return,
                }
            }
            if let Some(old) = self.entries.remove(&victim) {
                self.total_bytes = self.total_bytes.saturating_sub(old.bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("vibecodium_search_cache_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_file(dir: &Path, name: &str, fill: &str, target: usize) -> PathBuf {
        let path = dir.join(name);
        let mut content = String::new();
        while content.len() < target {
            content.push_str(fill);
        }
        std::fs::write(&path, &content).unwrap();
        path
    }

    /// THE regression for "every new search waits as if the index is dead":
    /// overflowing the budget must evict the LRU entry only — the rest of
    /// the cache (including the just-inserted hot entry) stays usable.
    #[test]
    fn lru_evicts_oldest_and_keeps_recent() {
        let dir = tmp_dir("lru");
        let a = make_file(&dir, "a.md", "alpha filler ", 600 * 1024);
        let b = make_file(&dir, "b.md", "beta filler ", 600 * 1024);
        let c = make_file(&dir, "c.md", "gamma filler ", 600 * 1024);
        // Budget fits A+B (1.2 MB) but not A+B+C (1.8 MB): touching A must
        // make B the eviction victim when C arrives.
        let mut cache = SearchContentCache::with_budget(1536 * 1024);
        let first_a = cache.get_or_load(&a).unwrap();
        let first_b = cache.get_or_load(&b).unwrap();
        cache.get_or_load(&a).unwrap(); // A becomes most recent
        let first_c = cache.get_or_load(&c).unwrap(); // evicts B, keeps A

        // A survived C's insertion; B was evicted.
        let reads_before = BODY_READS.load(Ordering::Relaxed);
        let warm_a = cache.get_or_load(&a).unwrap();
        eprintln!("entries left: {:?}", cache.entries.keys().map(|k| k.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>());
        eprintln!("order: {:?}", cache.order.iter().map(|k| k.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>());
        eprintln!("total_bytes: {}", cache.total_bytes);
        assert!(
            Arc::ptr_eq(&first_a, &warm_a),
            "recently used entry must survive the C insertion"
        );
        let _ = first_c;
        let reads_after_a = BODY_READS.load(Ordering::Relaxed);
        assert_eq!(
            reads_after_a, reads_before,
            "A must come from cache, not disk"
        );
        // Evicted B comes back from disk (fresh Arc).
        let warm_b = cache.get_or_load(&b).unwrap();
        assert!(!Arc::ptr_eq(&first_b, &warm_b), "B was evicted and re-read");
    }

    /// A file over the body limit is served as its bounded head and cached:
    /// a repeat call costs no disk read (BODY_READS unchanged).
    #[test]
    fn oversized_file_serves_head_and_caches() {
        let dir = tmp_dir("head");
        let path = dir.join("big.md");
        let mut content = String::from("bronto near the start\n");
        while content.len() < 600 * 1024 {
            content.push_str("filler lines without the term\n");
        }
        content.push_str("bronto near the end\n");
        std::fs::write(&path, &content).unwrap();
        assert!(content.len() > MAX_FILE_BYTES as usize, "test file must be oversized");

        let mut cache = SearchContentCache::new();
        let first = cache.get_or_load(&path).expect("oversized head must be served");
        assert!(first.contains("bronto near the start"));
        assert!(first.contains("bronto near the end"));
        let after_first = BODY_READS.load(Ordering::Relaxed);

        let second = cache.get_or_load(&path).expect("still served");
        assert!(Arc::ptr_eq(&first, &second), "cache hit, no re-read");
        assert_eq!(
            BODY_READS.load(Ordering::Relaxed),
            after_first,
            "cache hit must not read again"
        );
    }

    #[test]
    fn small_file_serves_full_body() {
        let dir = tmp_dir("full");
        let path = dir.join("small.md");
        std::fs::write(&path, "alpha body").unwrap();
        let mut cache = SearchContentCache::new();
        let text = cache.get_or_load(&path).expect("served");
        assert_eq!(&*text, "alpha body");
    }
}
