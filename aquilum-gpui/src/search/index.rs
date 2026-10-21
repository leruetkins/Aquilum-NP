//! In-memory inverted index (stage B, page-partitioned).
//!
//! Modeled on the reference architecture (`core/src/search/index_document`
//! + `hits/count.rs`, ideas only): a file is cut into line-bounded pages
//! (`search::pages`) and every page is indexed as its own document, so the
//! WHOLE file is searchable without ever holding it in memory — a
//! multi-hundred-megabyte note becomes a few hundred page-documents. Term
//! frequencies summed over a path's pages give the exact match count (the
//! reference reads them from tantivy postings; ours come from the same
//! postings idea in `query`). Bodies are still not stored — verification and
//! snippets stream the file (`search::locate`).
//!
//! Page ids pack `(path, page)` into one `u32`: low bits are the page
//! ordinal, the rest index the path table — postings stay plain `u32` keys.
//!
//! Building is preemptible: `Builder` keeps a file's page stream open across
//! `step_until` slices (a page is the atomic unit), so a 600 MB note cannot
//! stall the worker's search replies — searches are answered between slices
//! (see `search::worker`).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

use crate::search::pages::PageStream;
use crate::search::tokenize::{tokenize_document, ANALYZER_VERSION};

/// Low bits of a doc id hold the page ordinal (65k pages = ~16 GB of file).
const PAGE_BITS: u32 = 16;
const PAGE_MASK: u32 = (1 << PAGE_BITS) - 1;

/// `(mtime_ns, len)` of a file, or `None` without metadata.
pub(crate) fn fingerprint(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let nanos = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    Some((nanos, meta.len()))
}

/// Magic + format version of the persisted index file.
///
/// Version 2: fingerprints are recorded when a file's page stream COMPLETES
/// (v1 snapshots recorded them at stream start, so an interrupted build
/// persisted page-holed files that looked fully indexed).
const INDEX_MAGIC: &[u8; 4] = b"VBIX";
const INDEX_FORMAT: u32 = 2;

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn doc_id(path_ix: u32, page: u32) -> u32 {
    (path_ix << PAGE_BITS) | (page & PAGE_MASK)
}

pub(crate) fn doc_path_ix(doc: u32) -> u32 {
    doc >> PAGE_BITS
}

/// One indexed page: identity and token lengths (norms are per page — the
/// reference indexes pages as separate tantivy documents with the same
/// effect). No text is stored.
#[derive(Clone)]
pub struct Doc {
    pub path_ix: u32,
    pub page: u32,
    pub title_len: u32,
    pub path_len: u32,
    pub body_len: u32,
}

/// Per-term per-document counts, one slot per field `[title, path, body]`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Posting {
    pub doc: u32,
    pub counts: crate::search::tokenize::FieldCounts,
}

impl Posting {
    /// `(path index, page ordinal)` packed in this posting's doc id.
    #[cfg(test)]
    pub(crate) fn page_of(&self) -> Option<(u32, u32)> {
        Some((doc_path_ix(self.doc), self.doc & PAGE_MASK))
    }
}

pub struct Index {
    /// Vault root for relative-path tokens; set at build time.
    workspace: Option<PathBuf>,
    /// Distinct paths in file-list order; doc ids index into this table.
    paths: Vec<PathBuf>,
    by_path: HashMap<PathBuf, u32>,
    /// Pages keyed by packed doc id (`path_ix << PAGE_BITS | page`) — ids are
    /// sparse once a second path appears, so positional storage would lie.
    docs: HashMap<u32, Doc>,
    /// BTreeMap for prefix range scans (`terms.range(prefix..)`).
    terms: BTreeMap<String, Vec<Posting>>,
    /// `(mtime_ns, len)` at indexing time — the persisted index is verified
    /// against these on load; changed files are rebuilt, the rest load as-is.
    fingerprints: HashMap<PathBuf, (u64, u64)>,
    /// Total body tokens over all pages, for the body norm.
    total_body_len: u64,
    version: u32,
}

/// A cheap-enough clone for handing a loaded index both to search (live) and
/// to the builder (as its base): postings are the bulk and stay compact.
impl Clone for Index {
    fn clone(&self) -> Self {
        Self {
            workspace: self.workspace.clone(),
            paths: self.paths.clone(),
            by_path: self.by_path.clone(),
            docs: self.docs.clone(),
            terms: self.terms.clone(),
            fingerprints: self.fingerprints.clone(),
            total_body_len: self.total_body_len,
            version: self.version,
        }
    }
}

impl Index {
    /// An index with no content (fresh build base).
    pub(crate) fn empty(workspace: Option<PathBuf>) -> Self {
        Self {
            workspace,
            paths: Vec::new(),
            by_path: HashMap::new(),
            docs: HashMap::new(),
            terms: BTreeMap::new(),
            fingerprints: HashMap::new(),
            total_body_len: 0,
            version: ANALYZER_VERSION,
        }
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    /// Number of indexed PAGES (not paths — a paged file counts many times).
    pub fn doc_count(&self) -> usize {
        self.docs.len()
    }

    /// Whether `path` is FULLY indexed: its stream completed, so a
    /// fingerprint is recorded. A file mid-stream (interrupted build) is NOT
    /// contained — the fallback tail keeps scanning it and the next `sync`
    /// requeues it, healing the page hole.
    pub fn contains(&self, path: &Path) -> bool {
        self.fingerprints.contains_key(path)
    }

    /// Average body token count per PAGE over all pages (BM25 norm). Page
    /// norms keep scores comparable between a 300 KB note and a 600 MB one.
    pub fn avg_body_len(&self) -> f32 {
        let pages = self.docs.len().max(1) as f32;
        self.total_body_len as f32 / pages
    }

    /// Postings of an exact term.
    pub fn postings(&self, term: &str) -> Option<&Vec<Posting>> {
        self.terms.get(term)
    }

    /// Postings of the exact term plus every term starting with `prefix`,
    /// capped so a one-letter prefix cannot union the whole dictionary.
    /// Returns owned postings (cloned) so the iterator cannot outlive `&self`
    /// through the `range` prefix string.
    pub fn postings_with_prefix(&self, prefix: &str, cap: usize) -> Vec<Posting> {
        let mut out = Vec::new();
        for (term, postings) in self.terms.range(prefix.to_string()..) {
            if !term.starts_with(prefix) || out.len() >= cap {
                break;
            }
            out.extend_from_slice(postings);
        }
        out
    }

    /// Maps a doc id back to its path (candidates map to files).
    pub fn doc_path(&self, doc: u32) -> Option<&Path> {
        self.paths.get(doc_path_ix(doc) as usize).map(|p| p.as_path())
    }

    /// Number of distinct indexed paths.
    pub fn path_count(&self) -> usize {
        self.paths.len()
    }

    /// Path of a candidate's path index (`query::Candidate`).
    pub fn doc_path_for_ix(&self, path_ix: u32) -> &Path {
        self.paths
            .get(path_ix as usize)
            .map(|p| p.as_path())
            .unwrap_or_else(|| Path::new(""))
    }

    /// Token lengths of one page, `[title, path, body]` (BM25 norms).
    pub fn page_lengths(&self, doc: u32) -> Option<[u32; 3]> {
        self.docs
            .get(&doc)
            .map(|d| [d.title_len, d.path_len, d.body_len])
    }

    /// Lengths `[title, path, body]` of every FIRST page (files carry their
    /// title/path tokens there only).
    pub fn first_page_lengths(&self) -> impl Iterator<Item = [u32; 3]> + '_ {
        self.docs
            .values()
            .filter(|d| d.page == 0)
            .map(|d| [d.title_len, d.path_len, d.body_len])
    }

    /// Sums a term's body frequencies over every indexed page of `path`:
    /// the exact whole-file match count, straight from postings — no file
    /// read (the reference `hits/count.rs` model).
    ///
    /// Linear scan, NOT binary search: rebuilds (saved files, loaded-index
    /// refreshes) re-append a path's pages at the END of a term's posting
    /// list, so the list is NOT sorted by path id and a `partition_point`
    /// range would silently grab other paths' counts.
    pub fn body_count(&self, path: &Path, term: &str) -> usize {
        let Some(&path_ix) = self.by_path.get(path) else {
            return 0;
        };
        let Some(postings) = self.terms.get(term) else {
            return 0;
        };
        postings
            .iter()
            .filter(|p| doc_path_ix(p.doc) == path_ix)
            .map(|p| p.counts[2] as usize)
            .sum()
    }

    /// Page ordinals (ascending) of `path_ix`'s pages containing ALL
    /// `folded_terms` (exact posting keys plus prefix expansions, capped per
    /// term like `query::candidates`).
    ///
    /// This is what makes deep snippets cheap: the postings already know
    /// WHICH page carries a term, so the caller reads one 256 KB page instead
    /// of streaming a multi-hundred-MB file from its start hunting for the
    /// match (that stream was the multi-second search on big files).
    pub fn pages_with_all_terms(
        &self,
        path_ix: u32,
        folded_terms: &[String],
        cap_per_term: usize,
    ) -> Vec<u32> {
        if folded_terms.is_empty() {
            return Vec::new();
        }
        let mut pages: Option<std::collections::HashSet<u32>> = None;
        for term in folded_terms {
            let mut set: std::collections::HashSet<u32> = std::collections::HashSet::new();
            if let Some(postings) = self.terms.get(term.as_str()) {
                for p in postings.iter().filter(|p| doc_path_ix(p.doc) == path_ix) {
                    set.insert(p.doc & PAGE_MASK);
                }
            }
            for p in self.postings_with_prefix(term, cap_per_term) {
                if doc_path_ix(p.doc) == path_ix {
                    set.insert(p.doc & PAGE_MASK);
                }
            }
            if set.is_empty() {
                return Vec::new();
            }
            pages = Some(match pages {
                None => set,
                Some(acc) => acc.intersection(&set).copied().collect(),
            });
            if pages.as_ref().is_some_and(|s| s.is_empty()) {
                return Vec::new();
            }
        }
        let mut out: Vec<u32> = pages.unwrap_or_default().into_iter().collect();
        out.sort_unstable();
        out
    }

    /// Reserves (or returns the existing) path-table slot for `path`.
    pub(crate) fn ensure_path(&mut self, path: &Path) -> u32 {
        match self.by_path.get(path) {
            Some(&ix) => ix,
            None => {
                let ix = self.paths.len() as u32;
                self.paths.push(path.to_path_buf());
                self.by_path.insert(path.to_path_buf(), ix);
                ix
            }
        }
    }

    pub(crate) fn workspace_root(&self) -> Option<&Path> {
        self.workspace.as_deref()
    }

    /// The `(mtime_ns, len)` recorded when `path` was indexed.
    pub fn stored_fingerprint(&self, path: &Path) -> Option<(u64, u64)> {
        self.fingerprints.get(path).copied()
    }

    /// Every path with indexed pages (for staleness sweeps).
    pub fn stored_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.fingerprints.keys()
    }

    /// Drops every page posting of `path` (and empty lists). The path slot
    /// survives — doc ids of other paths must not shift.
    pub(crate) fn clear_path(&mut self, path: &Path) {
        let Some(&path_ix) = self.by_path.get(path) else {
            return;
        };
        for (_, postings) in self.terms.iter_mut() {
            postings.retain(|p| doc_path_ix(p.doc) != path_ix);
        }
        self.terms.retain(|_, p| !p.is_empty());
        self.total_body_len = self
            .docs
            .values()
            .filter(|d| d.path_ix != path_ix)
            .map(|d| d.body_len as u64)
            .sum();
        self.docs.retain(|_, d| d.path_ix != path_ix);
    }

    /// Indexes one page into the index. `title`/`rel` contribute tokens on
    /// the FIRST page only (the reference does the same), so short-field
    /// norms do not repeat across a file's pages.
    pub(crate) fn add_page(
        &mut self,
        path_ix: u32,
        title: &str,
        rel: &str,
        page_text: &str,
        page_ordinal: u32,
    ) {
        let first = page_ordinal == 0;
        let counts = tokenize_document(
            if first { title } else { "" },
            if first { rel } else { "" },
            page_text,
        );
        let mut lengths = [0u32; 3];
        for c in counts.values() {
            for (i, v) in c.iter().enumerate() {
                lengths[i] += *v as u32;
            }
        }
        self.total_body_len += lengths[2] as u64;
        let doc = doc_id(path_ix, page_ordinal);
        self.docs.insert(
            doc,
            Doc {
                path_ix,
                page: page_ordinal,
                title_len: lengths[0],
                path_len: lengths[1],
                body_len: lengths[2],
            },
        );
        for (term, counts) in counts {
            let postings = self.terms.entry(term).or_default();
            match postings.last_mut() {
                Some(last) if last.doc == doc => last.counts = counts,
                _ => postings.push(Posting { doc, counts }),
            }
        }
    }

    fn rel_of(&self, path: &Path) -> String {
        self.workspace_root()
            .and_then(|base| path.strip_prefix(base).ok())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned())
    }

    fn title_of(path: &Path) -> String {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Adds or replaces one file: streams it page by page and indexes every
    /// page. Used for whole-file reindexes (saves of small files); the
    /// builder streams preemptibly instead (see [`Builder`]). Unreadable
    /// files end up with zero pages — they stay name-matchable via the list.
    pub fn upsert(&mut self, path: &Path) {
        self.clear_path(path);
        if let Some(fp) = fingerprint(path) {
            self.fingerprints.insert(path.to_path_buf(), fp);
        }
        let path_ix = self.ensure_path(path);
        let title = Self::title_of(path);
        let rel = self.rel_of(path);
        let Ok(mut stream) = PageStream::open(path) else {
            return;
        };
        let mut ordinal = 0u32;
        while let Ok(Some(page)) = stream.next_page() {
            self.add_page(path_ix, &title, &rel, &page, ordinal);
            ordinal += 1;
        }
    }

    /// Persists the index (compact binary, atomic replace) so the next launch
    /// starts with a ready index — the reference keeps its index in SQLite for
    /// the same reason. `Err` — I/O trouble; the in-memory index stays valid.
    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        let mut out = Vec::new();
        out.extend_from_slice(INDEX_MAGIC);
        out.extend_from_slice(&INDEX_FORMAT.to_le_bytes());
        out.extend_from_slice(&self.version.to_le_bytes());
        put_u32(&mut out, self.paths.len() as u32);
        for p in &self.paths {
            let s = p.to_string_lossy();
            put_u32(&mut out, s.len() as u32);
            out.extend_from_slice(s.as_bytes());
        }
        for p in &self.paths {
            let fp = self.fingerprints.get(p).copied().unwrap_or((u64::MAX, u64::MAX));
            put_u64(&mut out, fp.0);
            put_u64(&mut out, fp.1);
        }
        put_u32(&mut out, self.docs.len() as u32);
        for (&id, d) in &self.docs {
            put_u32(&mut out, id);
            put_u32(&mut out, d.path_ix);
            put_u32(&mut out, d.page);
            put_u32(&mut out, d.title_len);
            put_u32(&mut out, d.path_len);
            put_u32(&mut out, d.body_len);
        }
        put_u64(&mut out, self.total_body_len);
        put_u32(&mut out, self.terms.len() as u32);
        for (term, postings) in &self.terms {
            put_u32(&mut out, term.len() as u32);
            out.extend_from_slice(term.as_bytes());
            put_u32(&mut out, postings.len() as u32);
            for p in postings {
                put_u32(&mut out, p.doc);
                for c in p.counts {
                    put_u16(&mut out, c);
                }
            }
        }
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = file.with_extension("idx.tmp");
        std::fs::write(&tmp, &out)?;
        std::fs::rename(&tmp, file)
    }

    /// Loads a persisted index; `None` on any mismatch (missing file, format
    /// or analyzer version, truncated data) — the caller rebuilds from scratch.
    /// Fingerprints are NOT verified here; the worker cross-checks them
    /// against current metadata and rebuilds stale files.
    pub fn load(file: &Path) -> Option<Index> {
        let bytes = std::fs::read(file).ok()?;
        let mut at = 0usize;
        macro_rules! take {
            ($n:expr) => {{
                let end = at + $n;
                let slice = bytes.get(at..end)?;
                at = end;
                slice
            }};
        }
        if take!(4) != INDEX_MAGIC {
            return None;
        }
        let format = u32::from_le_bytes(take!(4).try_into().ok()?);
        if format != INDEX_FORMAT {
            return None;
        }
        let version = u32::from_le_bytes(take!(4).try_into().ok()?);
        if version != ANALYZER_VERSION {
            return None;
        }
        let mut index = Index::empty(None);
        let path_count = u32::from_le_bytes(take!(4).try_into().ok()?) as usize;
        for _ in 0..path_count {
            let len = u32::from_le_bytes(take!(4).try_into().ok()?) as usize;
            let s = std::str::from_utf8(take!(len)).ok()?;
            index.paths.push(PathBuf::from(s));
        }
        for p in index.paths.iter().take(path_count) {
            index.by_path.insert(p.clone(), index.by_path.len() as u32);
        }
        for p in index.paths.iter().take(path_count) {
            let mtime = u64::from_le_bytes(take!(8).try_into().ok()?);
            let len = u64::from_le_bytes(take!(8).try_into().ok()?);
            if mtime != u64::MAX {
                index.fingerprints.insert(p.clone(), (mtime, len));
            }
        }
        let doc_count = u32::from_le_bytes(take!(4).try_into().ok()?) as usize;
        for _ in 0..doc_count {
            let id = u32::from_le_bytes(take!(4).try_into().ok()?);
            let path_ix = u32::from_le_bytes(take!(4).try_into().ok()?);
            let page = u32::from_le_bytes(take!(4).try_into().ok()?);
            let title_len = u32::from_le_bytes(take!(4).try_into().ok()?);
            let path_len = u32::from_le_bytes(take!(4).try_into().ok()?);
            let body_len = u32::from_le_bytes(take!(4).try_into().ok()?);
            if path_ix as usize >= path_count {
                return None;
            }
            index.docs.insert(
                id,
                Doc {
                    path_ix,
                    page,
                    title_len,
                    path_len,
                    body_len,
                },
            );
        }
        index.total_body_len = u64::from_le_bytes(take!(8).try_into().ok()?);
        let term_count = u32::from_le_bytes(take!(4).try_into().ok()?) as usize;
        for _ in 0..term_count {
            let len = u32::from_le_bytes(take!(4).try_into().ok()?) as usize;
            let term = std::str::from_utf8(take!(len)).ok()?.to_string();
            let postings_count = u32::from_le_bytes(take!(4).try_into().ok()?) as usize;
            let mut postings = Vec::with_capacity(postings_count);
            for _ in 0..postings_count {
                let doc = u32::from_le_bytes(take!(4).try_into().ok()?);
                let counts = [
                    u16::from_le_bytes(take!(2).try_into().ok()?),
                    u16::from_le_bytes(take!(2).try_into().ok()?),
                    u16::from_le_bytes(take!(2).try_into().ok()?),
                ];
                postings.push(Posting { doc, counts });
            }
            index.terms.insert(term, postings);
        }
        if at != bytes.len() {
            return None;
        }
        Some(index)
    }
}

/// One file being indexed across slices: the page stream stays open between
/// `step_until` calls, pages are the atomic unit of work.
struct Current {
    path: PathBuf,
    path_ix: u32,
    title: String,
    rel: String,
    stream: PageStream<std::io::BufReader<std::fs::File>>,
    next_page: u32,
}

/// Files up to this size are reindexed inline in the `Update` job (tens of
/// ms); bigger ones join the preemptible build queue instead so a save of a
/// huge file cannot stall searches.
pub(crate) const INLINE_UPSERT_BYTES: u64 = 2 * 1024 * 1024;

/// Chunked index builder.
///
/// `step(n)` (whole files, for tests and one-shot builds) and `step_until`
/// (time-budgeted slices for the worker loop) both advance the same cursor;
/// the worker interleaves slices with search replies. `sync` retargets the
/// queue to a fresh file list, keeping every already-indexed page.
pub struct Builder {
    index: Index,
    paths: Vec<PathBuf>,
    next: usize,
    current: Option<Current>,
    /// Set once anything was indexed — the worker saves the index only when
    /// work actually happened (a load-and-everything-fresh boot must not
    /// rewrite the file).
    worked: bool,
    /// Vault bytes total (sync) and bytes already indexed (complete files at
    /// sync + page bytes streamed since) — the progress percentage the UI
    /// shows next to the pending-search indicator.
    total_bytes: u64,
    indexed_bytes: u64,
    /// Kept so `take_index` can hand back a fresh `Index` for the (dropped)
    /// remainder of the build.
    index_workspace: Option<PathBuf>,
}

impl Builder {
    /// Continues from a pre-built (e.g. loaded from disk) index; the queue is
    /// filled by [`Builder::sync`].
    pub fn with_base(index: Index, workspace: Option<PathBuf>) -> Self {
        Self {
            index,
            paths: Vec::new(),
            next: 0,
            current: None,
            worked: false,
            total_bytes: 0,
            indexed_bytes: 0,
            index_workspace: workspace.clone(),
        }
    }

    pub fn new(paths: Vec<PathBuf>, workspace: Option<PathBuf>) -> Self {
        let mut b = Self::with_base(Index::empty(workspace.clone()), workspace);
        b.paths = paths;
        b
    }

    /// Retargets the queue to `files`: tree-vanished paths lose their pages,
    /// everything not already indexed with a current fingerprint joins the
    /// queue (ascending size — the index becomes useful fast, monster files
    /// stream last). Files currently streaming stay streamed unless removed.
    pub fn sync(&mut self, files: &[PathBuf]) {
        if let Some(cur) = &self.current {
            if !files.iter().any(|f| f == &cur.path) {
                self.current = None;
            }
        }
        let live: std::collections::HashSet<&PathBuf> = files.iter().collect();
        let stale: Vec<PathBuf> = self
            .index
            .stored_paths()
            .filter(|p| !live.contains(*p))
            .cloned()
            .collect();
        for p in stale {
            self.index.clear_path(&p);
        }
        let size_of = |p: &PathBuf| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        // A file counts as indexed only when its fingerprint matches — a file
        // interrupted mid-stream has NO fingerprint (see `start_file`) and is
        // queued again, healing the hole.
        let complete = |p: &PathBuf| self.index.stored_fingerprint(p) == fingerprint(p);
        let mut queue: Vec<PathBuf> = files.iter().filter(|f| !complete(f)).cloned().collect();
        queue.sort_by_key(|p| size_of(p));
        self.paths = queue;
        self.next = 0;
        // Progress basis: whole-vault bytes; already-complete files seed the
        // numerator (streamed pages add theirs as they land).
        self.total_bytes = files.iter().map(size_of).sum();
        self.indexed_bytes = files.iter().filter(|f| complete(*f)).map(|f| size_of(f)).sum();
    }

    /// Whether the queue is empty and nothing is mid-stream — the caller can
    /// finalize (take the index) without waiting for a slice.
    pub fn idle(&self) -> bool {
        self.current.is_none() && self.next >= self.paths.len()
    }

    /// Whether anything was indexed since this builder existed.
    pub fn did_work(&self) -> bool {
        self.worked
    }

    /// Build progress: `(bytes indexed, bytes total)`. `None` — nothing to
    /// build (empty vault or everything already indexed).
    pub fn progress(&self) -> Option<(u64, u64)> {
        (self.total_bytes > 0).then(|| {
            (self.indexed_bytes.min(self.total_bytes), self.total_bytes)
        })
    }

    /// Starts (or restarts) one file: clears its old pages, opens the page
    /// stream. Unreadable files leave `current` empty (zero pages).
    ///
    /// The path's fingerprint is REMOVED here and recorded only when its
    /// stream completes (`pump`): a snapshot taken mid-stream must leave the
    /// file fingerprint-less, so the next launch's `sync` requeues it instead
    /// of silently keeping a page-holed "indexed" file.
    fn start_file(&mut self, path: &Path) {
        self.index.clear_path(path);
        self.index.fingerprints.remove(path);
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        self.indexed_bytes = self.indexed_bytes.saturating_sub(size);
        let path_ix = self.index.ensure_path(path);
        let title = Index::title_of(path);
        let rel = self.index.rel_of(path);
        match PageStream::open(path) {
            Ok(stream) => {
                self.worked = true;
                self.current = Some(Current {
                    path: path.to_path_buf(),
                    path_ix,
                    title,
                    rel,
                    stream,
                    next_page: 0,
                });
            }
            Err(_) => self.current = None,
        }
    }

    /// Indexes up to `max` pages of the current file. `false` — the current
    /// file is finished (or failed); `true` — pages remain.
    fn pump(&mut self, max: usize) -> bool {
        let Some(mut cur) = self.current.take() else {
            return false;
        };
        for _ in 0..max {
            match cur.stream.next_page() {
                Ok(Some(page)) => {
                    self.index
                        .add_page(cur.path_ix, &cur.title, &cur.rel, &page, cur.next_page);
                    cur.next_page += 1;
                    self.indexed_bytes += page.len() as u64;
                }
                Ok(None) => {
                    // Stream finished: the file is fully indexed NOW — record
                    // its fingerprint (start_file cleared it).
                    if let Some(fp) = fingerprint(&cur.path) {
                        self.index.fingerprints.insert(cur.path.clone(), fp);
                    }
                    return false;
                }
                Err(_) => return false,
            }
        }
        self.current = Some(cur);
        true
    }

    /// Indexes `n` whole files. `true` — more work remains.
    pub fn step(&mut self, n: usize) -> bool {
        let mut completed = 0usize;
        while completed < n {
            if self.current.is_none() {
                if self.next >= self.paths.len() {
                    break;
                }
                let path = self.paths[self.next].clone();
                self.next += 1;
                self.start_file(&path);
                if self.current.is_none() {
                    completed += 1; // unreadable: nothing to stream
                    continue;
                }
            }
            if !self.pump(usize::MAX) {
                completed += 1;
            }
        }
        self.next < self.paths.len() || self.current.is_some()
    }

    /// Time-budgeted slice: indexes pages until `deadline`. `true` — more
    /// work remains. A runaway page (a 1 MB line) may overshoot slightly;
    /// that is the atomic unit and cannot be split.
    pub fn step_until(&mut self, deadline: Instant) -> bool {
        loop {
            if self.current.is_none() {
                if self.next >= self.paths.len() {
                    return false;
                }
                let path = self.paths[self.next].clone();
                self.next += 1;
                self.start_file(&path);
                if self.current.is_none() {
                    continue; // unreadable: next file
                }
            }
            self.pump(1);
            if Instant::now() >= deadline {
                return true;
            }
        }
    }

    /// Reindexes one file (saved file, stage B5). Small files reindex inline
    /// (fast enough to answer right away); big ones join the preemptible
    /// queue so the save cannot stall searches. A file currently streaming
    /// restarts from a clean slate either way.
    pub fn upsert(&mut self, path: &Path) {
        if let Some(cur) = &self.current {
            if cur.path == path {
                self.current = None;
            }
        }
        self.worked = true;
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if size <= INLINE_UPSERT_BYTES {
            self.index.upsert(path);
        } else if !self.paths.iter().any(|p| p == path) {
            // Old pages stay visible until the streamed rebuild lands.
            self.paths.push(path.to_path_buf());
        }
    }

    /// Takes the index built so far (remaining files simply stay unindexed),
    /// leaving a fresh empty index in the builder.
    /// Persists the index built SO FAR (preemptible build: the process may
    /// close before the whole vault is done — periodic snapshots keep the
    /// progress, so the next launch continues from here instead of zero).
    pub fn save_snapshot(&self, file: &Path) -> std::io::Result<()> {
        self.index.save(file)
    }

    pub fn take_index(&mut self) -> Index {
        std::mem::replace(
            &mut self.index,
            Index::empty(self.index_workspace.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_tmp(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("vibecodium_search_index_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn build_indexes_terms_and_prefix_works() {
        let dir = tmp_dir("build");
        let a = write_tmp(&dir, "alpha.md", "alpha beta alpha");
        let b = write_tmp(&dir, "alphabet.md", "alphabet soup");
        let mut builder = Builder::new(vec![a.clone(), b.clone()], Some(dir.clone()));
        assert!(builder.step(1), "one file left");
        assert!(!builder.step(1), "build done");
        let index = builder.take_index();
        assert!(index.doc_count() >= 2);
        // Exact postings on the first page: alpha = 1 title + 1 path + 2 body.
        let pa = index
            .postings("alpha")
            .and_then(|ps| ps.iter().find(|p| p.page_of() == Some((0, 0))))
            .unwrap();
        assert_eq!(pa.counts, [1, 1, 2]);
        // Prefix "alph" hits both paths.
        let paths: std::collections::HashSet<u32> = index
            .postings_with_prefix("alph", 100)
            .iter()
            .map(|p| doc_path_ix(p.doc))
            .collect();
        assert!(paths.contains(&0) && paths.contains(&1));
    }

    #[test]
    fn upsert_replaces_postings() {
        let dir = tmp_dir("upsert");
        let a = write_tmp(&dir, "alpha.md", "alpha beta");
        let mut builder = Builder::new(vec![a.clone()], Some(dir.clone()));
        builder.step(10);
        let mut index = builder.take_index();
        assert!(index.postings("beta").is_some());
        std::fs::write(&a, "gamma only now").unwrap();
        index.upsert(&a);
        assert!(index.postings("beta").is_none());
        assert!(index.postings("gamma").is_some());
    }

    /// Packed doc ids must resolve: the SECOND path's pages have large ids
    /// (path_ix << 16) that cannot index a positional table — their norms
    /// must be readable (regression for the HashMap-docs invariant).
    #[test]
    fn second_path_page_lengths_resolve() {
        let dir = tmp_dir("packed");
        let a = write_tmp(&dir, "alpha.md", "one two");
        let b = write_tmp(&dir, "beta.md", "three four five");
        let mut builder = Builder::new(vec![a, b], Some(dir.clone()));
        builder.step(10);
        let index = builder.take_index();
        let postings = index.postings("three").unwrap();
        assert!(!postings.is_empty());
        let doc = postings[0].doc;
        assert_eq!(doc_path_ix(doc), 1);
        assert!(
            index.page_lengths(doc).map(|l| l[2] > 0).unwrap_or(false),
            "page norms of the second path must resolve, doc id {doc}"
        );
    }

    /// The whole file is covered: a term that appears only past the first
    /// megabyte is indexed (pages), and its count is exact.
    #[test]
    fn whole_file_is_indexed_through_pages() {
        let dir = tmp_dir("pages");
        let big = dir.join("big.md");
        let mut f = std::fs::File::create(&big).unwrap();
        f.write_all(b"intro\n").unwrap();
        while f.metadata().unwrap().len() < 3 * 256 * 1024 {
            f.write_all(b"filler without the term\n").unwrap();
        }
        let mut expected = 0usize;
        for _ in 0..7 {
            f.write_all(b"rareterm\n").unwrap();
            expected += 1;
        }
        while f.metadata().unwrap().len() < 4 * 256 * 1024 {
            f.write_all(b"more filler\n").unwrap();
        }
        f.write_all(b"rareterm tail\n").unwrap();
        expected += 1;
        drop(f);

        let mut builder = Builder::new(vec![big.clone()], Some(dir.clone()));
        builder.step(10);
        let index = builder.take_index();
        assert!(index.doc_count() > 1, "a 1 MB+ file must span pages");
        assert_eq!(index.body_count(&big, "rareterm"), expected);
        assert_eq!(index.body_count(&big, "absent"), 0);
    }

    /// `step_until` interleave: a slice at a time, the file still ends up
    /// fully indexed, and the builder reports completion.
    #[test]
    fn step_until_completes_the_build() {
        let dir = tmp_dir("slices");
        let a = write_tmp(&dir, "alpha.md", "alpha beta");
        let mut builder = Builder::new(vec![a], Some(dir.clone()));
        let mut guard = 0;
        while builder.step_until(Instant::now() + std::time::Duration::from_millis(1)) {
            guard += 1;
            assert!(guard < 10_000, "build never finished");
        }
        let index = builder.take_index();
        assert!(index.postings("alpha").is_some());
    }

    /// THE regression for cross-file count bleed: reindexing a path with a
    /// LOW index after higher paths were indexed re-appends its postings at
    /// the END of the term's list — counts must stay per-path (a binary
    /// search over the now-unsorted list once grabbed other paths' pages:
    /// files with zero matches showed thousands of "matches").
    #[test]
    fn counts_stay_per_path_after_rebuild() {
        let dir = tmp_dir("bleed");
        let a = write_tmp(&dir, "aaa.md", "конец один конец два конец");
        let b = write_tmp(&dir, "zzz.md", "конец много раз конец конец конец конец");
        let mut builder = Builder::new(vec![a.clone(), b.clone()], Some(dir.clone()));
        builder.step(10);
        let mut index = builder.take_index();
        assert_eq!(index.body_count(&a, "конец"), 3);
        assert_eq!(index.body_count(&b, "конец"), 5);

        // Re-save the FIRST file: its postings re-append after B's.
        // "конец снова один конец два конец три" — 3 in-body occurrences;
        // the rebuilt postings must replace A's, not merge with B's.
        std::fs::write(&a, "конец снова один конец два конец три").unwrap();
        index.upsert(&a);
        assert_eq!(index.body_count(&a, "конец"), 3, "A's own count only");
        assert_eq!(index.body_count(&b, "конец"), 5, "B must not absorb A's pages");
    }

    /// Save + load round-trip preserves postings, counts and fingerprints.
    #[test]
    fn save_load_roundtrip() {
        let dir = tmp_dir("persist");
        let a = write_tmp(&dir, "alpha.md", "alpha beta alpha");
        let mut builder = Builder::new(vec![a.clone()], Some(dir.clone()));
        builder.step(10);
        let index = builder.take_index();
        let file = dir.join("vault.idx");
        index.save(&file).expect("save");
        let loaded = Index::load(&file).expect("load");
        assert_eq!(loaded.doc_count(), index.doc_count());
        assert_eq!(loaded.body_count(&a, "alpha"), 2, "alpha alpha in body");
        assert_eq!(loaded.stored_fingerprint(&a), index.stored_fingerprint(&a));
        // Same prefix behavior after the round-trip.
        let paths: std::collections::HashSet<u32> = loaded
            .postings_with_prefix("alph", 100)
            .iter()
            .map(|p| doc_path_ix(p.doc))
            .collect();
        assert!(!paths.is_empty());
        // Garbage and wrong versions fall back to a rebuild.
        std::fs::write(&file, b"not an index").unwrap();
        assert!(Index::load(&file).is_none());
    }
}
