//! Background search worker.
//!
//! Owns the body cache, the file list and (stage B) the in-memory index; the
//! UI thread only sends requests and drains replies (stages A5 and B4 in
//! `TODO/search-index-plan.md`). A scan never blocks typing: `SearchWorker::ask`
//! cancels the in-flight scan through the shared `Generation` counter before
//! queueing the new request, and the scan engine checks its `Cancel` token
//! between files.
//!
//! Search replies have absolute priority over index building: pending jobs
//! are drained before every build slice, and a slice is bounded (~10 ms,
//! page-granular — see `search::index::Builder`), so a multi-hundred-MB file
//! indexing in the background cannot delay a search by more than one slice.
//! The build order is ascending file size, so the index becomes useful fast
//! and the monster files stream in last. While the index is incomplete the
//! worker answers through the stage-A fallback scan — search never goes
//! quiet.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::search::bm25::Bm25;
use crate::search::cache::SearchContentCache;
use crate::search::cancel::{Cancel, Generation};
use crate::search::index::{Builder, Index};
use crate::settings::SettingsManager;
use crate::views::goto_search::{
    search_goto_files_opts, search_with_index, GotoResult, GotoSearchParams, SearchOp,
};

/// Which UI surface a request came from; replies are tagged the same way, so
/// each surface applies only its own still-current results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchConsumer {
    /// The "Go to file" dialog.
    Dialog,
    /// The sidebar search panel.
    Panel,
}

pub struct SearchRequest {
    pub consumer: SearchConsumer,
    /// UI-side sequence of the request; replies with an older number are
    /// stale and dropped by the consumer.
    pub seq: u64,
    pub query: String,
    pub op: SearchOp,
    pub case_sensitive: bool,
    pub limit: usize,
}

pub struct SearchReply {
    pub consumer: SearchConsumer,
    pub seq: u64,
    /// The query these results answer. The UI keeps it next to the results
    /// and highlights with ITS terms — while a newer query is in flight the
    /// displayed results (and their highlights) must stay consistent with
    /// the query that produced them, not the one being typed.
    pub query: String,
    pub results: Vec<GotoResult>,
    /// Index-build progress at reply time, percent 0..=99; `None` when the
    /// build is done (or there was nothing to build). Shown next to the
    /// pending-search indicator so "waiting" is explainable.
    pub index_progress: Option<u8>,
}

enum Job {
    SetFiles(Vec<PathBuf>),
    Reset(Option<PathBuf>),
    /// Reindex these paths after saves (stage B5).
    Update(Vec<PathBuf>),
    Search { req: SearchRequest, cancel: Cancel },
}

/// Idle wait when no build is running.
const IDLE_WAIT: Duration = Duration::from_millis(30);
/// Wait between build slices — search jobs preempt at the next loop turn.
const BUILD_WAIT: Duration = Duration::from_millis(2);
/// Wall-clock budget of one index-build slice (page-granular). Generous on
/// purpose: the build is what stands between a cold boot and full counts,
/// and a search job still preempts it within one slice.
const BUILD_SLICE: Duration = Duration::from_millis(40);
/// A saved file's reindex reaches the disk this long after the last update.
const SAVE_DEBOUNCE: Duration = Duration::from_secs(3);
/// Wall-clock spacing of build-progress snapshots (a long first build is
/// persisted while running, so an early close keeps the progress). Spaced
/// out: serializing a big index costs tens of ms and must not compete with
/// the build itself.
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(15);

fn path_exists(p: &Path) -> bool {
    p.exists()
}

/// Mutable worker state, shared by the drain loop and the wait loop.
struct WorkerState {
    workspace: Option<PathBuf>,
    /// Persisted-index file for the current vault (app-data, per workspace).
    index_file: Option<PathBuf>,
    cache: SearchContentCache,
    files: Vec<PathBuf>,
    index: Option<Index>,
    builder: Option<Builder>,
    /// A save is pending after file-update reindexes (debounced).
    dirty_since: Option<Instant>,
    /// Last periodic build-snapshot write (throttles disk writes during a
    /// long build).
    last_snapshot: Option<Instant>,
}

impl WorkerState {
    fn handle(
        &mut self,
        job: Job,
        settings: &Arc<SettingsManager>,
        replies: &Sender<SearchReply>,
    ) {
        match job {
            Job::SetFiles(new_files) => {
                // Tree reloads fire on every watcher batch; identical lists
                // change nothing.
                if new_files != self.files {
                    self.files = new_files;
                    self.files.sort_by_key(|p| {
                        std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
                    });
                    let live: std::collections::HashSet<&PathBuf> =
                        self.files.iter().collect();

                    // First list after a vault open: try the persisted index.
                    // It goes into SEARCH IMMEDIATELY (a snapshot from a
                    // previous session may be partial, but everything it
                    // covers answers with full counts — no fallback-reading
                    // of the whole vault), while the builder fills the gaps.
                    if self.index.is_none() {
                        if let Some(file) = self.index_file.clone() {
                            if let Some(mut loaded) = Index::load(&file) {
                                let stale: Vec<PathBuf> = loaded
                                    .stored_paths()
                                    .filter(|p| !live.contains(*p))
                                    .cloned()
                                    .collect();
                                for p in stale {
                                    loaded.clear_path(&p);
                                }
                                self.index = Some(loaded);
                            }
                        }
                    }

                    // Queue whatever is missing or stale; keep the rest. The
                    // index EXISTS from now on even when there was nothing to
                    // load: body search answers from the (partial) index
                    // instantly, like the reference — a full fallback scan of
                    // an uncovered vault would read everything per keystroke.
                    let had_index = self.index.is_some();
                    let base = self
                        .index
                        .clone()
                        .unwrap_or_else(|| Index::empty(self.workspace.clone()));
                    if !had_index {
                        self.index = Some(base.clone());
                    }
                    if let Some(builder) = self.builder.as_mut() {
                        builder.sync(&self.files);
                    } else {
                        let mut builder = Builder::with_base(base, self.workspace.clone());
                        builder.sync(&self.files);
                        if builder.idle() {
                            // Everything already indexed — finalize right here
                            // so the very next search uses the index.
                            self.index = Some(builder.take_index());
                        } else {
                            self.builder = Some(builder);
                        }
                    }
                }
            }
            Job::Reset(ws) => {
                self.workspace = ws.clone();
                self.index_file = ws.as_ref().map(|p| index_file_for(p));
                self.files.clear();
                self.cache = SearchContentCache::new();
                self.index = None;
                self.builder = None;
                self.dirty_since = None;
            }
            Job::Update(paths) => {
                // Small saved files reindex INLINE into the live index (the
                // term is searchable at once) AND into the running builder —
                // so the live index stays authoritative while the build
                // completes; big files go through the builder only.
                for path in paths.iter().filter(|p| path_exists(p)) {
                    let small = std::fs::metadata(path)
                        .map(|m| m.len() <= crate::search::index::INLINE_UPSERT_BYTES)
                        .unwrap_or(false);
                    if small {
                        if let Some(index) = self.index.as_mut() {
                            index.upsert(path);
                        }
                    }
                }
                if self.builder.is_none() {
                    let base = self
                        .index
                        .clone()
                        .unwrap_or_else(|| Index::empty(self.workspace.clone()));
                    self.builder = Some(Builder::with_base(base, self.workspace.clone()));
                }
                if let Some(builder) = self.builder.as_mut() {
                    for path in paths {
                        if path.exists() {
                            builder.upsert(&path);
                        }
                    }
                }
                self.dirty_since = Some(Instant::now());
            }
            Job::Search { req, cancel: token } => {
                // Superseded while queued: skip instead of scanning for nobody.
                if token.is_cancelled() {
                    return;
                }
                let config = settings.get_config();
                let rel_base = self.workspace.as_deref().map(|p| p.to_path_buf());
                let params = GotoSearchParams {
                    case_sensitive: req.case_sensitive,
                    rel_base: rel_base.as_deref(),
                    cancel: token.clone(),
                };
                let results = match (&req.op, self.index.as_ref()) {
                    // Name/path operators never consult the index: names
                    // are matched as substrings against the file list.
                    (SearchOp::File, _) | (SearchOp::Path, _) => {
                        self.scan(&req, &params)
                    }
                    // Case-sensitive queries cannot come from a folded
                    // index — exact stage-A behavior through the scan.
                    (_, Some(_)) if req.case_sensitive => self.scan(&req, &params),
                    (_, Some(index)) => search_with_index(
                        &self.files,
                        &req.query,
                        req.op,
                        &params,
                        req.limit,
                        &mut self.cache,
                        index,
                        config.search.candidate_pool_size,
                        config.search.max_query_terms,
                        Bm25::from_settings(&config.analysis.bm25f_params),
                    ),
                    // No index at all can only happen before the first file
                    // list arrives (no searches happen then); kept as a
                    // safety net, never on the user-visible path.
                    (_, None) => self.scan(&req, &params),
                };
                // Superseded mid-scan: the (partial) result is stale, drop it.
                if token.is_cancelled() {
                    return;
                }
                let index_progress = self
                    .builder
                    .as_ref()
                    .and_then(|b| b.progress())
                    .map(|(done, total)| ((done * 100 / total.max(1)) as u8).min(99));
                let _ = replies.send(SearchReply {
                    consumer: req.consumer,
                    seq: req.seq,
                    query: req.query.clone(),
                    results,
                    index_progress,
                });
            }
        }
    }

    /// Persists the current index (no-op without a vault or an index).
    fn save_index(&self) {
        if let (Some(file), Some(index)) = (&self.index_file, &self.index) {
            let _ = index.save(file);
        }
    }

    fn scan(&mut self, req: &SearchRequest, params: &GotoSearchParams) -> Vec<GotoResult> {
        search_goto_files_opts(&self.files, &req.query, req.op, params, req.limit, &mut self.cache)
    }
}

/// Handle to the background scanner. `AppState` owns one; the UI thread only
/// touches the channels, never the filesystem or the cache.
pub struct SearchWorker {
    jobs: Sender<Job>,
    replies: Receiver<SearchReply>,
    /// Per-consumer counters backing per-request `Cancel` tokens: a new
    /// request from one surface never cancels the other surface's scan
    /// (shared worker, separate generations — `TODO/search-index-plan.md`,
    /// risk "two consumers, one worker").
    dialog_cancel: Generation,
    panel_cancel: Generation,
    /// LIVE index-build progress, percent 0..=99; `None` while no build is
    /// running. The worker rewrites it every loop pass (each ~2 ms during a
    /// build), so the UI reads the CURRENT state — unlike
    /// `SearchReply::index_progress`, which is frozen at reply time and
    /// therefore invisible on a UI that renders between replies.
    build_progress: Arc<Mutex<Option<u8>>>,
    /// The persisted-index file the worker writes for the current vault
    /// (readable from the UI thread without touching the worker state).
    index_file: Arc<Mutex<Option<PathBuf>>>,
}

impl SearchWorker {
    pub fn spawn(workspace: Option<PathBuf>, settings: Arc<SettingsManager>) -> Self {
        let (jobs_tx, jobs_rx) = std::sync::mpsc::channel::<Job>();
        let (replies_tx, replies_rx) = std::sync::mpsc::channel();
        let build_progress: Arc<Mutex<Option<u8>>> = Arc::new(Mutex::new(None));
        let index_file: Arc<Mutex<Option<PathBuf>>> =
            Arc::new(Mutex::new(workspace.as_ref().map(|p| index_file_for(p))));
        std::thread::Builder::new()
            .name("search-worker".into())
            .spawn({
                let build_progress = Arc::clone(&build_progress);
                move || {
                    run_worker(jobs_rx, replies_tx, workspace, settings, build_progress);
                }
            })
            .expect("spawn search worker");
        Self {
            jobs: jobs_tx,
            replies: replies_rx,
            dialog_cancel: Generation::default(),
            panel_cancel: Generation::default(),
            build_progress,
            index_file,
        }
    }

    /// Replaces the scanned file list (after a tree reload) and restarts the
    /// index build.
    pub fn set_files(&self, files: Vec<PathBuf>) {
        let _ = self.jobs.send(Job::SetFiles(files));
    }

    /// Vault switch: retargets relative paths and drops the body cache.
    /// Also retargets the visible index-file path.
    pub fn reset(&self, workspace: Option<PathBuf>) {
        *self.index_file.lock().unwrap() = workspace.as_ref().map(|p| index_file_for(p));
        let _ = self.jobs.send(Job::Reset(workspace));
    }

    /// Reindexes saved files (stage B5). Files not yet indexed are picked up
    /// by the running build (it reads from disk when it reaches them).
    pub fn update_paths(&self, paths: Vec<PathBuf>) {
        let _ = self.jobs.send(Job::Update(paths));
    }

    /// Cancels any in-flight scan for `consumer` WITHOUT queueing a new
    /// request — called on every keystroke, so a scan for a query the user
    /// has already moved past stops eating I/O and can never surface.
    pub fn cancel(&self, consumer: SearchConsumer) {
        match consumer {
            SearchConsumer::Dialog => {
                self.dialog_cancel.next();
            }
            SearchConsumer::Panel => {
                self.panel_cancel.next();
            }
        }
    }

    /// Queues a request, cancelling any scan still in flight FOR THIS
    /// CONSUMER (the other surface's scan keeps running). The reply lands in
    /// the reply channel; the UI applies it only while `req.seq` is still
    /// current for `req.consumer`.
    pub fn ask(&self, req: SearchRequest) {
        let cancel = match req.consumer {
            SearchConsumer::Dialog => self.dialog_cancel.next(),
            SearchConsumer::Panel => self.panel_cancel.next(),
        };
        let _ = self.jobs.send(Job::Search { req, cancel });
    }

    /// Drains one finished reply, if any.
    pub fn try_recv(&self) -> Option<SearchReply> {
        self.replies.try_recv().ok()
    }

    /// LIVE index-build progress (percent 0..=99); `None` when no build is
    /// running (done, or nothing to build). Cheap enough for a render path —
    /// one mutex read.
    pub fn build_progress(&self) -> Option<u8> {
        *self.build_progress.lock().unwrap()
    }

    /// The persisted-index file for the current vault (for the status bar:
    /// click copies the path so the user can see/back up the index).
    pub fn index_file_path(&self) -> Option<PathBuf> {
        self.index_file.lock().unwrap().clone()
    }
}

fn run_worker(
    jobs: Receiver<Job>,
    replies: Sender<SearchReply>,
    workspace: Option<PathBuf>,
    settings: Arc<SettingsManager>,
    build_progress: Arc<Mutex<Option<u8>>>,
) {
    let mut state = WorkerState {
        index_file: workspace.as_ref().map(|p| index_file_for(p)),
        workspace,
        cache: SearchContentCache::new(),
        files: Vec::new(),
        index: None,
        builder: None,
        dirty_since: None,
        last_snapshot: None,
    };
    loop {
        // 1) Pending jobs first — a queued search must not wait for the
        // build. Drain everything arrived so far.
        loop {
            match jobs.try_recv() {
                Ok(job) => state.handle(job, &settings, &replies),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        // 2) One bounded build slice; the finished build replaces the live
        // index wholesale.
        if let Some(builder) = state.builder.as_mut() {
            let deadline = Instant::now() + BUILD_SLICE;
            if !builder.step_until(deadline) {
                let worked = builder.did_work();
                state.index = Some(builder.take_index());
                state.builder = None;
                state.last_snapshot = None;
                if worked {
                    state.save_index();
                    state.dirty_since = None;
                }
            } else {
                // A long build is snapshotted periodically: closing the app
                // mid-build keeps the progress, the next launch continues
                // from there instead of starting over (that is what made
                // every boot look like a full reindex).
                let due = state
                    .last_snapshot
                    .map_or(true, |at| at.elapsed() >= SNAPSHOT_INTERVAL);
                if due && builder.did_work() {
                    if let Some(file) = state.index_file.clone() {
                        if builder.save_snapshot(&file).is_ok() {
                            state.last_snapshot = Some(Instant::now());
                        }
                    }
                }
            }
        }
        // 3) Debounced save after file-update reindexes.
        if let Some(at) = state.dirty_since {
            if at.elapsed() >= SAVE_DEBOUNCE && state.builder.is_none() {
                state.save_index();
                state.dirty_since = None;
            }
        }
        // 4) Publish LIVE build progress (a build starting in SetFiles,
        // finishing here, or dropped by Reset all lands within one pass).
        *build_progress.lock().unwrap() = state
            .builder
            .as_ref()
            .and_then(|b| b.progress())
            .map(|(done, total)| ((done * 100 / total.max(1)) as u8).min(99));
        // 5) Wait for the next job; briefly while a build is running.
        let wait = if state.builder.is_some() { BUILD_WAIT } else { IDLE_WAIT };
        match jobs.recv_timeout(wait) {
            Ok(job) => state.handle(job, &settings, &replies),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Persisted-index file for a vault: `<app-data>/search-index/<path-hash>.idx`.
fn index_file_for(workspace: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    workspace.to_string_lossy().hash(&mut hasher);
    crate::state::app_data_dir()
        .join("search-index")
        .join(format!("{:016x}.idx", hasher.finish()))
}

#[cfg(test)]
mod tests;
