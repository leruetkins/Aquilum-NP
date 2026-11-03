    use super::*;

    fn temp_file(tag: &str, content: &str) -> (std::path::PathBuf, crop::Rope) {
        let dir = std::env::temp_dir().join(format!("vibe_save_{}_{}", tag, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.md");
        std::fs::write(&path, content).unwrap();
        (path, crop::Rope::from(content.to_string()))
    }

    /// The core of the conflict protocol: a write against a stale expected
    /// hash must not overwrite the external content; our version lands in a
    /// conflict copy; the external text is returned for the buffer.
    #[test]
    fn checked_write_conflicts_into_a_copy_instead_of_overwriting() {
        let (path, _rope) = temp_file("conflict", "исходная версия");
        let loaded_hash = hash_file(&path).unwrap();

        // Another program rewrites the file while we were editing.
        std::fs::write(&path, "внешняя версия").unwrap();

        let ours = crop::Rope::from("наша версия".to_string());
        let outcome = safe_write_rope_checked(&path, &ours, Some(&loaded_hash), 1);

        match outcome {
            CheckedWrite::Conflict { copy, disk_text } => {
                // The disk keeps the external content.
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "внешняя версия");
                // Our version is intact in the copy.
                assert_eq!(std::fs::read_to_string(&copy).unwrap(), "наша версия");
                assert!(copy.to_string_lossy().contains("конфликт"));
                // The buffer is told to adopt the external text.
                assert_eq!(disk_text, "внешняя версия");
                // The copy sits next to the original, not elsewhere.
                assert_eq!(copy.parent(), path.parent());
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn checked_write_with_matching_hash_writes_and_returns_the_new_hash() {
        let (path, rope) = temp_file("match", "исходная версия");
        let loaded_hash = hash_file(&path).unwrap();

        let next = crop::Rope::from("новая версия".to_string());
        match safe_write_rope_checked(&path, &next, Some(&loaded_hash), 1) {
            CheckedWrite::Written(hash) => {
                assert_eq!(hash, hash_rope(&next));
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "новая версия");
            }
            other => panic!("expected a plain write, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn checked_write_with_unknown_hash_writes_directly() {
        let (path, _rope) = temp_file("unknown", "исходная версия");
        let next = crop::Rope::from("новая версия".to_string());
        match safe_write_rope_checked(&path, &next, None, 1) {
            CheckedWrite::Written(_) => {
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "новая версия");
            }
            other => panic!("expected a plain write, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn checked_write_recreates_a_externally_deleted_file() {
        let (path, _rope) = temp_file("deleted", "исходная версия");
        let loaded_hash = hash_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        let next = crop::Rope::from("наша версия".to_string());
        match safe_write_rope_checked(&path, &next, Some(&loaded_hash), 1) {
            CheckedWrite::Written(_) => {
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "наша версия");
            }
            other => panic!("expected a plain write, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Two save tasks for one path can reach the disk in either order. The older
    /// buffer version must not overwrite the newer one.
    #[test]
    fn a_late_older_save_does_not_roll_back_a_newer_one() {
        let (path, _rope) = temp_file("order", "исходная версия");
        let loaded_hash = hash_file(&path).unwrap();

        let newer = crop::Rope::from("новая версия".to_string());
        match safe_write_rope_checked(&path, &newer, Some(&loaded_hash), 2) {
            CheckedWrite::Written(hash) => assert_eq!(hash, hash_rope(&newer)),
            other => panic!("expected a plain write, got {other:?}"),
        }

        let older = crop::Rope::from("старая версия".to_string());
        match safe_write_rope_checked(&path, &older, Some(&loaded_hash), 1) {
            CheckedWrite::Written(hash) => assert_eq!(hash, hash_rope(&newer)),
            other => panic!("expected the newer save to stand, got {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "новая версия");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// A queued save still carries the hash from before another save of this
    /// process completed. That is not an external edit: no conflict copy.
    #[test]
    fn a_queued_save_recognises_our_own_write() {
        let (path, _rope) = temp_file("queued", "исходная версия");
        let loaded_hash = hash_file(&path).unwrap();

        let first = crop::Rope::from("первая версия".to_string());
        assert!(matches!(
            safe_write_rope_checked(&path, &first, Some(&loaded_hash), 1),
            CheckedWrite::Written(_)
        ));

        let second = crop::Rope::from("вторая версия".to_string());
        match safe_write_rope_checked(&path, &second, Some(&loaded_hash), 2) {
            CheckedWrite::Written(hash) => assert_eq!(hash, hash_rope(&second)),
            other => panic!("expected a plain write, got {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "вторая версия");
        let conflicts = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains("конфликт"))
            .count();
        assert_eq!(conflicts, 0, "our own write must not produce a conflict copy");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn atomic_temp_names_are_unique_and_keep_the_process_id_last() {
        let (path, _rope) = temp_file("tmpname", "x");
        let first = temp_sibling(&path);
        let second = temp_sibling(&path);
        assert_ne!(first, second, "two writes must not share a temporary file");
        let expected_suffix = format!(".{}.tmp", std::process::id());
        for tmp in [first, second] {
            let name = tmp.file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.starts_with(".note.md."), "unexpected tmp name: {name}");
            assert!(name.ends_with(&expected_suffix), "unexpected tmp name: {name}");
            assert!(crate::fs_watcher::classify::is_our_atomic_tmp(&tmp));
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The reported corruption: two overlapping saves of a big note shared one
    /// temporary file and interleaved their chunks, leaving bytes that are not
    /// valid UTF-8. Serialized writes always leave one whole version.
    #[test]
    fn overlapping_big_writes_never_interleave() {
        const LEN: usize = ATOMIC_WRITE_THRESHOLD_BYTES + 64 * 1024;
        let (path, _rope) = temp_file("overlap", "старт");

        let a = crop::Rope::from("a".repeat(LEN));
        let b = crop::Rope::from("b".repeat(LEN));
        std::thread::scope(|scope| {
            let (path_a, rope_a) = (&path, a.clone());
            scope.spawn(move || {
                let _ = safe_write_rope_checked(path_a, &rope_a, None, 1);
            });
            let (path_b, rope_b) = (&path, b.clone());
            scope.spawn(move || {
                let _ = safe_write_rope_checked(path_b, &rope_b, None, 2);
            });
        });

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), LEN);
        // Version 2 is the newest snapshot, so it must be the one on disk —
        // whichever task got the lock first.
        let text = String::from_utf8(bytes).expect("a mixed write would not be valid UTF-8");
        assert!(text.bytes().all(|c| c == b'b'), "the newest version must win");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn conflict_copy_names_are_collision_free() {
        let (path, _rope) = temp_file("collision", "исходная версия");
        let first = conflict_copy_path(&path).unwrap();
        std::fs::write(&first, "").unwrap();
        let second = conflict_copy_path(&path).unwrap();
        assert_ne!(first, second, "an occupied name must rotate to a numbered one");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn utc_label_is_wellformed() {
        let label = format_utc_label(std::time::SystemTime::UNIX_EPOCH);
        assert_eq!(label, "1970-01-01 00-00");
    }

    #[test]
    fn test_adaptive_delays_progressive() {
        // Small (< 100 KB) - fixed 500ms debounce, 5s deadline
        assert_eq!(adaptive_delays(0), (Duration::from_millis(500), Duration::from_secs(5)));
        assert_eq!(adaptive_delays(50 * 1024), (Duration::from_millis(500), Duration::from_secs(5)));
        assert_eq!(adaptive_delays(100 * 1024 - 1), (Duration::from_millis(500), Duration::from_secs(5)));

        // 100 KB (~0.098 MB) -> ~1.97s debounce, ~12.66s deadline
        let (d_100k, m_100k) = adaptive_delays(100 * 1024);
        assert!((d_100k.as_secs_f64() - 1.97).abs() < 0.05);
        assert!((m_100k.as_secs_f64() - 12.66).abs() < 0.1);

        // 1 MB -> exactly 3.0s debounce, 18.5s deadline
        let (d_1m, m_1m) = adaptive_delays(1024 * 1024);
        assert!((d_1m.as_secs_f64() - 3.0).abs() < 0.01);
        assert!((m_1m.as_secs_f64() - 18.5).abs() < 0.01);

        // 10 MB -> ~6.24s debounce, ~36.88s deadline
        let (d_10m, m_10m) = adaptive_delays(10 * 1024 * 1024);
        assert!((d_10m.as_secs_f64() - 6.24).abs() < 0.05);
        assert!((m_10m.as_secs_f64() - 36.88).abs() < 0.1);

        // 50 MB -> ~12.1s debounce, ~70.1s deadline
        let (d_50m, m_50m) = adaptive_delays(50 * 1024 * 1024);
        assert!((d_50m.as_secs_f64() - 12.1).abs() < 0.1);
        assert!((m_50m.as_secs_f64() - 70.1).abs() < 0.5);

        // 100 MB -> ~16.5s debounce, ~95.0s deadline
        let (d_100m, m_100m) = adaptive_delays(100 * 1024 * 1024);
        assert!((d_100m.as_secs_f64() - 16.5).abs() < 0.1);
        assert!((m_100m.as_secs_f64() - 95.0).abs() < 0.5);

        // 700 MB -> ~41.2s debounce, ~234.9s deadline
        let (d_700m, m_700m) = adaptive_delays(700 * 1024 * 1024);
        assert!((d_700m.as_secs_f64() - 41.2).abs() < 0.2);
        assert!((m_700m.as_secs_f64() - 234.9).abs() < 1.0);

        // Monotonic progression: larger content must always have >= debounce and >= deadline
        let sizes = [0, 50 * 1024, 100 * 1024, 500 * 1024, 1024 * 1024, 10 * 1024 * 1024, 100 * 1024 * 1024, 700 * 1024 * 1024, 2 * 1024 * 1024 * 1024];
        for pair in sizes.windows(2) {
            let (d1, m1) = adaptive_delays(pair[0]);
            let (d2, m2) = adaptive_delays(pair[1]);
            assert!(d2 >= d1, "debounce for {} must be >= {}", pair[1], pair[0]);
            assert!(m2 >= m1, "deadline for {} must be >= {}", pair[1], pair[0]);
        }
    }

    #[test]
    fn test_safe_write_small_file() {
        let temp_dir = std::env::temp_dir().join(format!("vibecodium_test_small_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let file_path = temp_dir.join("test_small.md");

        let content = "Hello world!";
        safe_write(&file_path, content).expect("safe_write small failed");

        let read_back = std::fs::read_to_string(&file_path).expect("read failed");
        assert_eq!(read_back, content);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_safe_write_large_file_atomic() {
        let temp_dir = std::env::temp_dir().join(format!("vibecodium_test_large_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let file_path = temp_dir.join("test_large.md");

        // Create > 10 MB content to trigger atomic path
        let content = "a".repeat(ATOMIC_WRITE_THRESHOLD_BYTES + 100);
        safe_write(&file_path, &content).expect("safe_write large failed");

        let read_back = std::fs::read_to_string(&file_path).expect("read failed");
        assert_eq!(read_back.len(), content.len());
        assert_eq!(read_back, content);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
