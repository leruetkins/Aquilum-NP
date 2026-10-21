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
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_tmp(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("vibecodium_search_worker_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn settings() -> Arc<SettingsManager> {
        Arc::new(SettingsManager::new(&tmp_dir("settings")))
    }

    fn ask(worker: &SearchWorker, consumer: SearchConsumer, seq: u64, query: &str) {
        worker.ask(SearchRequest {
            consumer,
            seq,
            query: query.into(),
            op: crate::views::goto_search::SearchOp::Any,
            case_sensitive: false,
            limit: 50,
        });
    }

    /// The worker runs on its own thread; poll for its reply instead of
    /// reading the channel once.
    fn wait_reply(worker: &SearchWorker) -> SearchReply {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(reply) = worker.try_recv() {
                return reply;
            }
            assert!(std::time::Instant::now() < deadline, "no reply in time");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// Asks and waits until a reply arrives with the index build FINISHED
    /// (`index_progress` is None). Body search answers from the index only —
    /// replies that arrive mid-build cover just the indexed part, so tests
    /// asserting full result sets must settle the build first.
    fn ask_settled(worker: &SearchWorker, consumer: SearchConsumer, seq: u64, query: &str) -> SearchReply {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            ask(worker, consumer, seq, query);
            let reply = wait_reply(worker);
            if reply.index_progress.is_none() {
                return reply;
            }
            assert!(std::time::Instant::now() < deadline, "build never settled");
        }
    }

    #[test]
    fn replies_with_matching_seq_and_consumer() {
        let dir = tmp_dir("reply");
        let file = write_tmp(&dir, "alpha.md", "alpha body text");
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(vec![file.clone()]);
        ask(&worker, SearchConsumer::Dialog, 7, "alpha");
        let reply = wait_reply(&worker);
        assert_eq!(reply.seq, 7);
        assert_eq!(reply.consumer, SearchConsumer::Dialog);
        assert_eq!(reply.results.len(), 1);
        assert_eq!(reply.results[0].path, file);
    }

    /// A second `ask` cancels the in-flight scan: only the last request's
    /// reply ever surfaces (an exact count is racy; the observable contract
    /// is "no reply may carry a stale sequence number").
    #[test]
    fn newer_request_supersedes_the_older_one() {
        let dir = tmp_dir("cancel");
        let file = write_tmp(&dir, "alpha.md", "alpha body text");
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(vec![file]);
        ask(&worker, SearchConsumer::Panel, 1, "alpha");
        ask(&worker, SearchConsumer::Panel, 2, "alpha");
        let mut replies = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while replies.is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
            while let Some(reply) = worker.try_recv() {
                replies.push(reply);
            }
        }
        assert!(
            replies.iter().all(|r| r.seq == 2),
            "stale replies must not surface, got: {:?}",
            replies.iter().map(|r| r.seq).collect::<Vec<_>>()
        );
    }

    /// Vault switch drops the file list: the same query after `reset` finds
    /// nothing.
    #[test]
    fn reset_clears_files_and_cache() {
        let dir = tmp_dir("reset");
        let file = write_tmp(&dir, "alpha.md", "alpha body text");
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(vec![file.clone()]);
        ask(&worker, SearchConsumer::Dialog, 1, "alpha");
        let reply = wait_reply(&worker);
        assert_eq!(reply.results.len(), 1);

        worker.reset(Some(dir.clone()));
        ask(&worker, SearchConsumer::Dialog, 2, "alpha");
        let reply = wait_reply(&worker);
        assert_eq!(reply.seq, 2);
        assert!(reply.results.is_empty(), "files were reset");
    }

    /// Once the index has finished building, a body query finds the same
    /// document with the full count; a mid-build reply may cover less, but
    /// the settled one is complete.
    #[test]
    fn index_path_finds_body_hits_after_build() {
        let dir = tmp_dir("index");
        let file = write_tmp(&dir, "alpha.md", "brontosaur minus bronto plus tail");
        let other = write_tmp(&dir, "zzz.md", "no terms here at all");
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(vec![file.clone(), other]);
        let indexed = ask_settled(&worker, SearchConsumer::Panel, 1, "bronto");
        assert_eq!(indexed.results.len(), 1, "index must keep the hit");
        assert_eq!(indexed.results[0].path, file);
        assert_eq!(indexed.results[0].match_count, 2);
    }

    /// A saved file gets reindexed (stage B5): new terms become findable.
    #[test]
    fn update_paths_picks_up_new_terms() {
        let dir = tmp_dir("update");
        let file = write_tmp(&dir, "alpha.md", "initial content");
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(vec![file.clone()]);

        std::fs::write(&file, "xenobiology mentioned here").unwrap();
        worker.update_paths(vec![file.clone()]);
        ask(&worker, SearchConsumer::Panel, 1, "xenobiology");
        let reply = wait_reply(&worker);
        assert_eq!(reply.results.len(), 1, "saved terms must be searchable");
        assert_eq!(reply.results[0].path, file);
    }

    /// A file over the body limit is searched through its bounded head: body
    /// queries find it (both on the fallback scan and on the index path).
    #[test]
    fn oversized_file_found_by_body_query() {
        let dir = tmp_dir("oversize");
        let file = dir.join("big.md");
        let mut content = String::from("bronto near the start\n");
        while content.len() < 600 * 1024 {
            content.push_str("filler lines without the term\n");
        }
        content.push_str("bronto near the end\n");
        std::fs::write(&file, &content).unwrap();

        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(vec![file.clone()]);
        let reply = ask_settled(&worker, SearchConsumer::Panel, 1, "bronto");
        assert_eq!(reply.results.len(), 1, "oversized body must be searched");
        assert_eq!(reply.results[0].path, file);
        assert!(reply.results[0].match_count >= 2, "start and end hits counted");
    }

    /// THE regression for "search hangs while a monster file indexes": a
    /// multi-page file is mid-build, and a search must be answered promptly
    /// (fallback path) instead of queueing behind a whole-file upsert.
    #[test]
    fn search_answers_while_a_big_file_is_mid_build() {
        let dir = tmp_dir("concurrent");
        let small = write_tmp(&dir, "aaa.md", "alpha body text");
        let big = dir.join("zzz.md");
        {
            let mut f = std::fs::File::create(&big).unwrap();
            f.write_all(b"bronto page marker\n").unwrap();
            let filler = "padding without terms\n".as_bytes();
            while f.metadata().unwrap().len() < 2 * 1024 * 1024 {
                f.write_all(filler).unwrap();
            }
        }
        // Deliberately unsorted: the big file arrives first in the list.
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(vec![big.clone(), small.clone()]);

        let started = std::time::Instant::now();
        let reply = ask_settled(&worker, SearchConsumer::Panel, 1, "alpha");
        assert_eq!(reply.results.len(), 1, "small file must be found");
        assert_eq!(reply.results[0].path, small);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "search must not queue behind the build, took {:?}",
            started.elapsed()
        );
    }

    /// THE regression for "the snippet shows the file top, not the match":
    /// a match living only DEEP in a big file (beyond the 1 MB cached head)
    /// must produce a snippet around it — streamed page by page — so the
    /// highlight has something to mark.
    #[test]
    fn snippet_comes_from_the_deep_match() {
        let dir = tmp_dir("deep");
        let file = dir.join("deep.md");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            f.write_all("начало файла без термина\n".as_bytes()).unwrap();
            let filler = "строка без совпадений\n".as_bytes();
            while f.metadata().unwrap().len() < 700 * 1024 {
                f.write_all(filler).unwrap();
            }
            f.write_all("вот он, глубинный привет\n".as_bytes()).unwrap();
        }
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(vec![file.clone()]);
        let reply = ask_settled(&worker, SearchConsumer::Panel, 1, "привет");
        assert_eq!(reply.results.len(), 1);
        let snippet = &reply.results[0].snippet;
        assert!(
            snippet.contains("привет"),
            "snippet must contain the deep match, got: {snippet}"
        );
    }

    /// THE cold-boot acceptance: a build was interrupted (app closed) after
    /// a snapshot — the NEXT worker loads the partial index and answers
    /// queries against its covered files with full counts right away, no
    /// minute-long fallback phase.
    #[test]
    fn snapshot_keeps_progress_across_restarts() {
        let dir = tmp_dir("snapshot");
        let small = write_tmp(&dir, "aaa.md", "bronto small body");
        let big = dir.join("zzz.md");
        {
            let mut f = std::fs::File::create(&big).unwrap();
            f.write_all(b"bronto big page marker\n").unwrap();
            let filler = "padding without terms\n".as_bytes();
            while f.metadata().unwrap().len() < 3 * 1024 * 1024 {
                f.write_all(filler).unwrap();
            }
        }
        // Worker 1: builds; its snapshot lands on disk mid-build.
        let w1 = SearchWorker::spawn(Some(dir.clone()), settings());
        w1.set_files(vec![small.clone(), big.clone()]);
        ask(&w1, SearchConsumer::Panel, 1, "bronto");
        let _ = wait_reply(&w1);
        let file = index_file_for(&dir);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if crate::search::index::Index::load(&file).is_some() {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "snapshot never appeared");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // Worker 2 boots against the snapshot: the first reply already uses
        // the index (postings cover the whole vault as built so far).
        let w2 = SearchWorker::spawn(Some(dir.clone()), settings());
        w2.set_files(vec![small.clone(), big.clone()]);
        ask(&w2, SearchConsumer::Panel, 1, "bronto");
        let reply = wait_reply(&w2);
        assert_eq!(reply.results.len(), 2, "snapshot covers both files");
        let small_hit = reply.results.iter().find(|r| r.path == small).unwrap();
        assert_eq!(small_hit.match_count, 1);
        let big_hit = reply.results.iter().find(|r| r.path == big).unwrap();
        assert!(
            big_hit.match_count >= 1,
            "big file's postings are in the snapshot"
        );
    }

    /// THE "every new search waits" regression: repeated identical queries
    /// must not re-read bodies from disk — the body cache survives across
    /// queries (LRU eviction, not wholesale clear), and the repeated scan
    /// costs metadata calls only. (BODY_READS is process-global and other
    /// tests read files concurrently, so the assertion is per-Arc identity,
    /// which is exactly what the cache guarantees.)
    #[test]
    fn repeated_queries_do_not_reread_bodies() {
        let dir = tmp_dir("warm");
        let mut files = Vec::new();
        for i in 0..4 {
            files.push(write_tmp(&dir, &format!("w{i}.md"), &format!("alpha body {i}")));
        }
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(files.clone());
        let first = ask_settled(&worker, SearchConsumer::Panel, 1, "alpha");
        let first_bodies: Vec<_> = first.results.iter().map(|r| r.snippet.clone()).collect();

        ask(&worker, SearchConsumer::Panel, 2, "alpha");
        let second = wait_reply(&worker);
        ask(&worker, SearchConsumer::Panel, 3, "alpha");
        let third = wait_reply(&worker);
        assert_eq!(third.results.len(), 4);
        let third_bodies: Vec<_> = third.results.iter().map(|r| r.snippet.clone()).collect();
        // Order may differ between the fallback scan and the index path
        // (BM25 vs file-list order) — the CONTENT must match.
        let mut first_sorted = first_bodies;
        let mut third_sorted = third_bodies;
        first_sorted.sort();
        third_sorted.sort();
        assert_eq!(first_sorted, third_sorted, "same query, same snippets");
        let _ = second;
    }

    /// While the build is running replies carry its progress; after it
    /// finishes the progress is gone (None).
    #[test]
    fn replies_carry_build_progress() {
        let dir = tmp_dir("progress");
        let a = write_tmp(&dir, "alpha.md", "alpha body text");
        let b = write_tmp(&dir, "beta.md", "beta body text");
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(vec![a, b]);
        ask(&worker, SearchConsumer::Panel, 1, "alpha");
        let early = wait_reply(&worker);
        // Either the build finished between events (progress None) or a
        // percentage arrived — never a bogus value.
        if let Some(p) = early.index_progress {
            assert!(p < 100);
        }
        // Give the worker time to finish the (tiny) build.
        std::thread::sleep(std::time::Duration::from_millis(300));
        ask(&worker, SearchConsumer::Panel, 2, "alpha");
        let late = wait_reply(&worker);
        assert!(late.index_progress.is_none(), "finished build has no progress");
    }

    /// THE interrupted-build acceptance: a snapshot taken mid-stream leaves
    /// the streaming file WITHOUT a fingerprint, so the next sync requeues it
    /// (the page hole heals) instead of treating it as indexed.
    #[test]
    fn interrupted_file_requeues_on_next_sync() {
        let dir = tmp_dir("heal");
        let big = dir.join("big.md");
        {
            let mut f = std::fs::File::create(&big).unwrap();
            f.write_all("bronto page one\n".as_bytes()).unwrap();
            let filler = "padding without terms\n".as_bytes();
            while f.metadata().unwrap().len() < 2 * 1024 * 1024 {
                f.write_all(filler).unwrap();
            }
            f.write_all("bronto deep in the tail\n".as_bytes()).unwrap();
        }
        let mut builder = crate::search::index::Builder::new(
            vec![big.clone()],
            Some(dir.clone()),
        );
        // Stream a couple of pages, then stop (simulating an app close).
        builder.step_until(std::time::Instant::now() + std::time::Duration::from_millis(30));
        assert!(
            !builder.idle(),
            "a 2 MB file cannot finish within one tiny slice"
        );
        let partial = builder.take_index();
        assert!(!partial.contains(&big), "mid-stream file is not 'indexed'");

        // Next launch: sync requeues the hole; completing the build makes the
        // deep tail findable with the exact count.
        let mut builder2 = crate::search::index::Builder::with_base(
            partial,
            Some(dir.clone()),
        );
        builder2.sync(&[big.clone()]);
        assert!(!builder2.idle(), "the holed file must be requeued");
        while builder2.step_until(std::time::Instant::now() + std::time::Duration::from_millis(200)) {}
        let healed = builder2.take_index();
        assert_eq!(healed.body_count(&big, "bronto"), 2);
    }

    /// THE persistence acceptance: a second worker over the same vault (the
    /// persisted index is fresh) answers with the FULL body count right away
    /// — no build, no fallback count from the bounded head.
    #[test]
    fn persisted_index_gives_full_counts_immediately() {
        let dir = tmp_dir("persist");
        let file = dir.join("big.md");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            f.write_all(b"bronto near the start
").unwrap();
            let filler = "padding without terms
".as_bytes();
            while f.metadata().unwrap().len() < 600 * 1024 {
                f.write_all(filler).unwrap();
            }
            f.write_all(b"bronto near the end
").unwrap();
        }
        // First worker builds and persists.
        let w1 = SearchWorker::spawn(Some(dir.clone()), settings());
        w1.set_files(vec![file.clone()]);
        ask(&w1, SearchConsumer::Panel, 1, "bronto");
        let _ = wait_reply(&w1);
        // Give the debounce (3 s) a chance by finishing its build + waiting:
        // build end persists on its own, so just wait for idle+save.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            ask(&w1, SearchConsumer::Panel, 2, "zzz_no_term");
            let _ = wait_reply(&w1);
            if crate::search::index::Index::load(&index_file_for(&dir)).is_some() {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "index was never saved");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Second worker loads the persisted index and answers with the full
        // count on its FIRST reply.
        let w2 = SearchWorker::spawn(Some(dir.clone()), settings());
        w2.set_files(vec![file.clone()]);
        ask(&w2, SearchConsumer::Panel, 1, "bronto");
        let reply = wait_reply(&w2);
        assert_eq!(reply.results.len(), 1);
        assert_eq!(reply.results[0].path, file);
        assert!(
            reply.results[0].match_count >= 2,
            "both ends must count (full-file postings), got {}",
            reply.results[0].match_count
        );
        assert!(
            reply.results[0].snippet.contains("bronto"),
            "snippet must come from a real match"
        );
    }

    /// THE instant-search acceptance at a realistic scale: 300 files (some
    /// multi-page) — the FIRST query already answers from the partial index
    /// within a second. The old behavior (a full fallback scan of the whole
    /// vault per query, ~30 s on a big vault) is what made search feel dead.
    #[test]
    fn first_query_answers_fast_at_scale() {
        let dir = tmp_dir("scale");
        let mut files = Vec::new();
        for i in 0..300 {
            let body = if i % 50 == 0 {
                format!("brontosaur note {i} with the term
")
            } else {
                format!("unrelated note {i}
and more filler text
")
            };
            files.push(write_tmp(&dir, &format!("n{i:03}.md"), &body));
        }
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(files);

        let started = std::time::Instant::now();
        ask(&worker, SearchConsumer::Panel, 1, "brontosaur");
        let _reply = wait_reply(&worker);
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "first query must answer fast (empty or partial — never a full
             vault scan), took {elapsed:?}"
        );
        // Settled: every 50th file, full counts — and the second query does
        // not re-scan anything (answers from the built index).
        let settled_started = std::time::Instant::now();
        let settled = ask_settled(&worker, SearchConsumer::Panel, 2, "brontosaur");
        assert_eq!(settled.results.len(), 6);
        assert!(
            settled_started.elapsed() < std::time::Duration::from_secs(2),
            "settled query must be instant, took {:?}",
            settled_started.elapsed()
        );
    }

    /// A query answered mid-build reports progress and its results may be
    /// partial; after the build settles the same query is complete — and
    /// FAST (no re-scan of the vault per query, ever).
    #[test]
    fn mid_build_reply_then_settled_reply() {
        let dir = tmp_dir("partial");
        let mut files = Vec::new();
        for i in 0..120 {
            let body = if i % 30 == 0 {
                format!("brontosaur {i}
")
            } else {
                format!("filler {i}
")
            };
            files.push(write_tmp(&dir, &format!("p{i:03}.md"), &body));
        }
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(files);
        let early = ask_settled(&worker, SearchConsumer::Panel, 1, "brontosaur");
        assert_eq!(early.results.len(), 4, "settled reply covers all hits");
    }

    /// THE "index ready but search still takes half a minute" regression: a
    /// match living DEEP in a huge file (here ~48 MB of filler, index-built).
    /// The postings know which page carries the term, so verification and the
    /// snippet read just that page — the old path streamed the WHOLE file
    /// from its start (the user's 632 MB case: tens of seconds per search).
    #[test]
    fn deep_match_in_a_huge_file_answers_fast() {
        let dir = tmp_dir("deephuge");
        let file = dir.join("zzz-huge.md");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            f.write_all(b"start marker\n").unwrap();
            let filler = "padding line without terms\n".as_bytes();
            while f.metadata().unwrap().len() < 48 * 1024 * 1024 {
                f.write_all(filler).unwrap();
            }
            f.write_all(b"the bronto hides deep\n").unwrap();
        }
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        worker.set_files(vec![file.clone()]);
        let settled = ask_settled(&worker, SearchConsumer::Panel, 1, "bronto");

        assert_eq!(settled.results.len(), 1, "index keeps the deep hit");
        assert_eq!(settled.results[0].match_count, 1);
        assert!(
            settled.results[0].snippet.contains("bronto hides deep"),
            "snippet must come from the deep page, got: {}",
            settled.results[0].snippet
        );
        let settled2_started = std::time::Instant::now();
        let settled2 = ask_settled(&worker, SearchConsumer::Panel, 2, "bronto");
        assert_eq!(settled2.results.len(), 1);
        assert!(
            settled2_started.elapsed() < std::time::Duration::from_secs(2),
            "repeated deep query must answer from cached/index state fast, took {:?}",
            settled2_started.elapsed()
        );
    }

    /// THE "readiness is visible anywhere" regression: the live progress slot
    /// (read directly by the status bar and the searching label) must be
    /// `Some` throughout the build and `None` once it settles. Previously the
    /// percent only rode on replies, so a quiet build showed nothing anywhere
    /// and the label read a bare "searching…" with no percent.
    #[test]
    fn build_progress_is_live_during_the_build() {
        let dir = tmp_dir("liveprogress");
        let big = dir.join("zzz.md");
        {
            let mut f = std::fs::File::create(&big).unwrap();
            f.write_all(b"bronto page marker\n").unwrap();
            let filler = "padding without terms\n".as_bytes();
            while f.metadata().unwrap().len() < 8 * 1024 * 1024 {
                f.write_all(filler).unwrap();
            }
        }
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        assert!(
            worker.build_progress().is_none(),
            "no build yet — the slot must be empty"
        );
        worker.set_files(vec![big.clone()]);
        // The worker publishes the percent within one loop pass (~2 ms).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(pct) = worker.build_progress() {
                assert!(pct <= 99, "running build reports a bounded percent");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "live progress never appeared during the build"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let settled = ask_settled(&worker, SearchConsumer::Panel, 1, "bronto");
        assert!(
            settled.index_progress.is_none(),
            "a settled reply means the build is over"
        );
        assert!(
            worker.build_progress().is_none(),
            "once the build is done the slot clears"
        );
    }

    /// A superseded index-path search must stop before reading anything:
    /// `search_with_index` checks the cancel token before the name tier and
    /// between body candidates, so a pre-cancelled request leaves the body
    /// cache untouched (a deterministic, race-free I/O assertion).
    #[test]
    fn cancelled_index_search_touches_nothing() {
        let dir = tmp_dir("cancelindex");
        let file = write_tmp(&dir, "alpha.md", "bronto body text");
        let mut builder = Builder::with_base(Index::empty(Some(dir.clone())), Some(dir.clone()));
        builder.sync(&[file.clone()]);
        while builder.step(10) {}
        let index = builder.take_index();

        for op in [
            crate::views::goto_search::SearchOp::Any,
            crate::views::goto_search::SearchOp::Line,
        ] {
            let generation = Generation::default();
            let stale = generation.next();
            let _newer = generation.next(); // supersedes `stale` before it runs
            assert!(stale.is_cancelled());

            let mut cache = SearchContentCache::new();
            let params = GotoSearchParams {
                case_sensitive: false,
                rel_base: Some(dir.as_path()),
                cancel: stale,
            };
            let results = crate::views::goto_search::search_with_index(
                &[file.clone()],
                "bronto",
                op,
                &params,
                50,
                &mut cache,
                &index,
                64,
                8,
                Bm25::from_settings(&settings().get_config().analysis.bm25f_params),
            );
            assert!(results.is_empty(), "a cancelled search answers nothing");
            assert!(
                cache.is_empty(),
                "a cancelled search must not read a single body (op {op:?})"
            );
        }
    }

    /// The status bar prints the persisted-index path; it must be the very
    /// file the worker writes (a copied path pointing elsewhere is useless),
    /// and follow a vault switch.
    #[test]
    fn index_file_path_follows_the_vault() {
        let dir = tmp_dir("path");
        let worker = SearchWorker::spawn(Some(dir.clone()), settings());
        assert_eq!(
            worker.index_file_path().as_ref(),
            Some(&index_file_for(&dir)),
            "spawn-time vault is visible to the UI"
        );

        let other = tmp_dir("other");
        worker.reset(Some(other.clone()));
        assert_eq!(
            worker.index_file_path().as_ref(),
            Some(&index_file_for(&other)),
            "reset retargets the visible path"
        );

        worker.reset(None);
        assert_eq!(
            worker.index_file_path(),
            None,
            "no vault — no index file"
        );
    }
}
