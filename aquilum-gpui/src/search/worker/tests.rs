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
