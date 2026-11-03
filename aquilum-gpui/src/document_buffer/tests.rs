    use super::*;

    #[test]
    fn test_transform_offset_rules() {
        // Insertion: "Hello World" -> insert "Beautiful " at 6
        let e_ins = Edit {
            start: 6,
            old_len: 0,
            new_len: 10,
        };

        // Before edit stays unchanged
        assert_eq!(transform_offset(0, e_ins, Bias::Left), 0);
        assert_eq!(transform_offset(5, e_ins, Bias::Left), 5);

        // Exactly at insertion point: Bias::Left stays at 6, Bias::Right moves to 16
        assert_eq!(transform_offset(6, e_ins, Bias::Left), 6);
        assert_eq!(transform_offset(6, e_ins, Bias::Right), 16);

        // After edit is shifted by +10
        assert_eq!(transform_offset(11, e_ins, Bias::Left), 21);
        assert_eq!(transform_offset(11, e_ins, Bias::Right), 21);

        // Deletion: delete 5 bytes starting at 6
        let e_del = Edit {
            start: 6,
            old_len: 5,
            new_len: 0,
        };

        // Inside deleted range collapses to start
        assert_eq!(transform_offset(7, e_del, Bias::Left), 6);
        assert_eq!(transform_offset(9, e_del, Bias::Right), 6);
        // After deleted range shifts by -5
        assert_eq!(transform_offset(15, e_del, Bias::Left), 10);
    }

    #[test]
    fn line_of_byte_rounds_a_stale_offset_down_to_a_char_boundary() {
        let doc = DocumentBuffer::new(
            DocumentId::new(),
            PathBuf::from("test.md"),
            "test".to_string(),
            "первая строка
вторая".to_string(),
            1,
        );
        // 9 is inside the two-byte 'а' of "первая".
        assert_eq!(doc.line_of_byte(9), 0);
        assert_eq!(doc.line_of_byte(29), 1);
    }

    #[test]
    fn test_transform_selection_range() {
        // Range 5..10, insert 4 chars at 7
        let edit = Edit {
            start: 7,
            old_len: 0,
            new_len: 4,
        };
        let res = transform_selection_range((5, 10), edit);
        assert_eq!(res, (5, 14));

        // Range 5..10, delete 7..9 (2 chars)
        let edit_del = Edit {
            start: 7,
            old_len: 2,
            new_len: 0,
        };
        let res_del = transform_selection_range((5, 10), edit_del);
        assert_eq!(res_del, (5, 8));
    }

    #[test]
    fn test_document_buffer_mutations_and_undo() {
        let doc_id = DocumentId::new();
        let mut buf = DocumentBuffer::new(
            doc_id,
            PathBuf::from("test.md"),
            "test".to_string(),
            "Hello World".to_string(),
            1,
        );

        assert_eq!(buf.content(), "Hello World");
        assert!(!buf.is_dirty);

        // Apply edit
        let (ev, _, _) = buf.record_change(6, "World".to_string(), "Rust".to_string(), 6, 10, None);
        match ev {
            BufferEvent::Edited { edit, version, .. } => {
                assert_eq!(edit.start, 6);
                assert_eq!(edit.old_len, 5);
                assert_eq!(edit.new_len, 4);
                assert_eq!(version, 2);
            }
            _ => panic!("Expected Edited event"),
        }

        assert_eq!(buf.content(), "Hello Rust");
        assert!(buf.is_dirty);

        // Undo
        let (sel, _, _) = buf.undo_step(None).expect("Undo should succeed");
        assert_eq!(sel.cursor, 6);
        assert_eq!(buf.content(), "Hello World");

        // Redo
        let (sel_redo, _, _) = buf.redo_step(None).expect("Redo should succeed");
        assert_eq!(sel_redo.cursor, 10);
        assert_eq!(buf.content(), "Hello Rust");
    }

    #[test]
    fn test_document_buffer_insert_delete_replace_and_line_index() {
        let doc_id = DocumentId::new();
        let mut buf = DocumentBuffer::new(
            doc_id,
            PathBuf::from("doc.md"),
            "doc".to_string(),
            "line 1\nline 2\nline 3".to_string(),
            1,
        );
        assert_eq!(buf.line_index.lines(), 3);
        assert_eq!(buf.version, 1);
        assert!(!buf.is_dirty);

        // 1. Insertion with newline
        buf.record_change(7, String::new(), "inserted\n".to_string(), 7, 16, None);
        assert_eq!(buf.content(), "line 1\ninserted\nline 2\nline 3");
        assert_eq!(buf.line_index.lines(), 4);
        assert_eq!(buf.version, 2);
        assert!(buf.is_dirty);

        // 2. Deletion across lines
        // Delete "inserted\n" (9 bytes at index 7)
        buf.record_change(7, "inserted\n".to_string(), String::new(), 7, 7, None);
        assert_eq!(buf.content(), "line 1\nline 2\nline 3");
        assert_eq!(buf.line_index.lines(), 3);
        assert_eq!(buf.version, 3);

        // 3. Replacement
        // Replace "line 1" with "header 1"
        buf.record_change(0, "line 1".to_string(), "header 1".to_string(), 0, 8, None);
        assert_eq!(buf.content(), "header 1\nline 2\nline 3");
        assert_eq!(buf.line_index.lines(), 3);
        assert_eq!(buf.version, 4);
    }

    #[test]
    fn test_document_buffer_multi_step_undo_redo_and_redo_clearing() {
        let doc_id = DocumentId::new();
        let mut buf = DocumentBuffer::new(
            doc_id,
            PathBuf::from("undo.md"),
            "undo".to_string(),
            "A".to_string(),
            1,
        );

        // Edit 1: A -> AB
        buf.record_change(1, String::new(), "B".to_string(), 1, 2, None);
        // Edit 2: AB -> ABC
        buf.record_change(2, String::new(), "C".to_string(), 2, 3, None);
        assert_eq!(buf.content(), "ABC");
        assert_eq!(buf.history.undo_len(), 2);
        assert_eq!(buf.history.redo_len(), 0);

        // Undo edit 2 -> AB
        let (sel1, _, _) = buf.undo_step(None).unwrap();
        assert_eq!(buf.content(), "AB");
        assert_eq!(sel1.cursor, 2);
        assert_eq!(buf.history.redo_len(), 1);

        // Undo edit 1 -> A
        let (sel0, _, _) = buf.undo_step(None).unwrap();
        assert_eq!(buf.content(), "A");
        assert_eq!(sel0.cursor, 1);
        assert_eq!(buf.history.redo_len(), 2);

        // Redo edit 1 -> AB
        let (sel_r1, _, _) = buf.redo_step(None).unwrap();
        assert_eq!(buf.content(), "AB");
        assert_eq!(sel_r1.cursor, 2);
        assert_eq!(buf.history.redo_len(), 1);

        // New edit while redo stack has items: AB -> AB!
        buf.record_change(2, String::new(), "!".to_string(), 2, 3, None);
        assert_eq!(buf.content(), "AB!");
        // Redo stack must be cleared
        assert_eq!(buf.history.redo_len(), 0);
        assert!(buf.redo_step(None).is_none());
    }

    #[test]
    fn test_document_buffer_stats_cache_invalidation() {
        let doc_id = DocumentId::new();
        let mut buf = DocumentBuffer::new(
            doc_id,
            PathBuf::from("stats.md"),
            "stats".to_string(),
            "hello world".to_string(),
            1,
        );

        let s1 = buf.text_stats();
        assert_eq!(s1, crate::state::TextStats::Exact { words: 2, chars: 11 });

        // Stats cache is now populated
        assert!(buf.stats_cache.get().is_some());

        // Edit changes content and increments version
        buf.record_change(11, String::new(), " test".to_string(), 11, 16, None);
        assert_eq!(buf.version, 2);

        let s2 = buf.text_stats();
        assert_eq!(s2, crate::state::TextStats::Exact { words: 3, chars: 16 });
    }

    #[test]
    fn test_document_buffer_set_path() {
        let doc_id = DocumentId::new();
        let mut buf = DocumentBuffer::new(
            doc_id,
            PathBuf::from("old_name.md"),
            "old_name".to_string(),
            "text".to_string(),
            1,
        );

        let ev = buf.set_path(PathBuf::from("new_name.md"));
        match ev {
            BufferEvent::PathChanged { old, new } => {
                assert_eq!(old, PathBuf::from("old_name.md"));
                assert_eq!(new, PathBuf::from("new_name.md"));
            }
            _ => panic!("Expected PathChanged event"),
        }
        assert_eq!(buf.path, PathBuf::from("new_name.md"));
    }

    #[test]
    fn test_forward_deletes_are_separate_undo_steps() {
        let mut buf = DocumentBuffer::new(
            DocumentId::new(),
            PathBuf::from("delete.md"),
            "delete".to_string(),
            "abcdef".to_string(),
            1,
        );

        buf.record_change(2, "c".to_string(), String::new(), 3, 2, None);
        buf.record_change(2, "d".to_string(), String::new(), 2, 2, None);

        assert_eq!(buf.history.undo_len(), 2, "no coalescing: each delete is a step");
        assert_eq!(buf.content(), "abef");

        buf.undo_step(None).expect("undo must succeed");
        assert_eq!(buf.content(), "abdef");
        buf.undo_step(None).expect("undo must succeed");
        assert_eq!(buf.content(), "abcdef");
    }

    #[test]
    fn test_selection_delete_undo_restores_text() {
        let mut buf = DocumentBuffer::new(
            DocumentId::new(),
            PathBuf::from("selection.md"),
            "selection".to_string(),
            "one **two** three".to_string(),
            1,
        );

        let removed = "**two** ".to_string();
        let at = 4;
        buf.record_change(at, removed.clone(), String::new(), at + removed.len(), at, None);
        assert_eq!(buf.content(), "one three");

        buf.undo_step(None).expect("undo must succeed");
        assert_eq!(buf.content(), "one **two** three");
    }

    #[test]
    fn test_undo_redo_updates_table_index_incrementally() {
        let text = "# T\n\n| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n\ntail\n";
        let mut buf = DocumentBuffer::new(
            DocumentId::new(),
            PathBuf::from("table.md"),
            "table".to_string(),
            text.to_string(),
            1,
        );

        let start = buf.line_start_byte(4);
        let old_len = buf.line_start_byte(5) - start;
        let removed = buf.slice_to_string(start..start + old_len);
        buf.record_change(start, removed, String::new(), start, start, None);

        {
            let idx = buf.ensure_table_index();
            assert_eq!(
                (idx.blocks[0].first_line, idx.blocks[0].last_line),
                (2, 4)
            );
        }

        buf.undo_step(None).expect("undo must succeed");
        {
            let idx = buf.ensure_table_index();
            assert_eq!(
                (idx.blocks[0].first_line, idx.blocks[0].last_line),
                (2, 5)
            );
        }

        buf.redo_step(None).expect("redo must succeed");
        {
            let idx = buf.ensure_table_index();
            assert_eq!(
                (idx.blocks[0].first_line, idx.blocks[0].last_line),
                (2, 4)
            );
        }
    }

    #[test]
    fn test_from_file_large() {
        let temp_dir = std::env::temp_dir().join(format!("test_large_from_file_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let p = temp_dir.join("test.txt");
        let text = "Привет мир! Hello world!\n".repeat(500_000); // ~18.5 MB
        std::fs::write(&p, &text).unwrap();
        let doc = DocumentBuffer::from_file(
            DocumentId::new(),
            p.clone(),
            "test".into(),
            1,
        ).unwrap();
        println!("Expected len: {}, actual doc len: {}", text.len(), doc.len());
        assert_eq!(doc.len(), text.len());

        let mut state = crate::state::AppState::new();
        state.open_note(p.clone());
        let active_doc = state.active_document().unwrap();
        assert_eq!(active_doc.len(), text.len());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_normalize_newlines() {
        use super::normalize_newlines;
        // No CR: borrowed as-is
        let s = "hello\nworld";
        assert_eq!(normalize_newlines(s), std::borrow::Cow::Borrowed("hello\nworld"));

        // Windows CRLF
        assert_eq!(normalize_newlines("hello\r\nworld\r\n"), "hello\nworld\n");

        // Classic Mac CR
        assert_eq!(normalize_newlines("hello\rworld\r"), "hello\nworld\n");

        // Mixed CRLF, CR, LF
        assert_eq!(normalize_newlines("a\r\nb\rc\nd"), "a\nb\nc\nd");

        // Consecutive CRs
        assert_eq!(normalize_newlines("a\r\r\rb"), "a\n\n\nb");

        // User's exact pattern: isolated CR between spaces
        let user_str = "после исправления -  проверь  ssl  еще раз  \r \r \r                            ";
        let expected = "после исправления -  проверь  ssl  еще раз  \n \n \n                            ";
        assert_eq!(normalize_newlines(user_str), expected);
    }
