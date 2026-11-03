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
