    use super::{EditorWindow, FOLLOW_SLACK, MARGIN_LINES};
    use crate::line_index::LineIndex;

    fn doc(lines: usize) -> String {
        let mut text = String::new();
        for i in 0..lines {
            text.push_str(&format!("line {i}\n"));
        }
        text
    }

    /// The window holds exactly the lines it claims, in document order, and
    /// every other array lines up with it.
    #[test]
    fn window_lines_and_rows_agree_with_the_document() {
        let text = doc(9000);
        let index = LineIndex::new(&text);
        let w = EditorWindow::build(&text, &index, 4000, 7000, false, 0, None, crate::markdown::CodeScanState::default());

        assert_eq!(w.start_line, 4000);
        assert_eq!(w.end_line, 7000);
        assert_eq!(w.line_count(), 3000);
        assert_eq!(w.lines.len(), 3000);
        assert_eq!(w.code_states.len(), 3000);
        for line in 4000..7000 {
            assert_eq!(w.line(line), Some(format!("line {line}").as_str()));
        }
        // Byte offsets are the ones `LineIndex` reports for those lines.
        assert_eq!(w.start_byte, index.start(&text, 4000));
        assert_eq!(w.end_byte, index.end(&text, 6999));
        assert_eq!(&text[w.start_byte..w.end_byte].split('\n').count(), &3000);
    }

    /// Rows carry document line numbers, so a row found by index can be used
    /// directly against a global line — that is the contract hit-testing and
    /// the caret rely on.
    #[test]
    fn visual_rows_carry_global_line_numbers() {
        let text = doc(5000);
        let index = LineIndex::new(&text);
        let w = EditorWindow::build(&text, &index, 2000, 2600, false, 0, None, crate::markdown::CodeScanState::default());

        assert!(!w.visual_rows.is_empty());
        assert_eq!(w.visual_rows.first().unwrap().line_idx, 2000);
        assert_eq!(w.visual_rows.last().unwrap().line_idx, 2599);
        // Row order is by document line, so the global index is monotone.
        let mut prev = 0;
        for row in w.visual_rows.iter() {
            assert!(row.line_idx >= prev, "row order broke at {}", row.line_idx);
            prev = row.line_idx;
        }
        // And the row geometry is window-local: 600 rows of 24 px.
        assert_eq!(w.height_index.len(), 600);
        assert_eq!(w.height_index.total(), 600.0 * 24.0);
        assert_eq!(w.height_index.top_of(0), 0.0);
        assert_eq!(w.height_index.top_of(600), 600.0 * 24.0);
        assert_eq!(w.first_row_of_line(2000), Some(0));
        assert_eq!(w.first_row_of_line(2599), Some(599));
        assert_eq!(w.first_row_of_line(1999), None);
        assert_eq!(w.first_row_of_line(2600), None);
    }

    /// Wrapping is measured inside the window only, and rows of one line stay
    /// contiguous so `find_visual_row_for_cursor` still works on global lines.
    #[test]
    fn wrapped_window_keeps_lines_contiguous() {
        let text = "aaaa\nbbbbbbbbbb\ncccc\n";
        let index = LineIndex::new(text);
        let w = EditorWindow::build(text, &index, 0, 4, true, 4, None, crate::markdown::CodeScanState::default());
        // "aaaa" (1 row), "bbbbbbbbbb" (3 rows), "cccc" (1 row), "" (1 row).
        assert_eq!(w.visual_rows.len(), 6);
        let line_of = |v: usize| w.visual_rows[v].line_idx;
        assert_eq!((line_of(0), line_of(1), line_of(2), line_of(3)), (0, 1, 1, 1));
        assert_eq!((line_of(4), line_of(5)), (2, 3));
        let found = crate::markdown::find_visual_row_for_cursor(&w.visual_rows, 1, 7);
        assert_eq!(found, Some((2, 3)));
    }

    /// Single-line updates must not wrap table rows and must keep their table
    /// heights when any other line is edited.
    #[test]
    fn single_line_update_keeps_table_rows_unwrapped_and_measured() {
        let text = "intro intro intro\n| a | b |\n|---|---|\n| a very long cell that exceeds the wrap width | b |";
        let table_kind = |line: usize, _: &str| match line {
            1 => Some(crate::markdown::TableRowVisualKind::Header { lines: 1 }),
            2 => Some(crate::markdown::TableRowVisualKind::Delimiter),
            3 => Some(crate::markdown::TableRowVisualKind::LastRow { lines: 1 }),
            _ => None,
        };
        let mut w = EditorWindow::build_from_slice_with_tables(
            text,
            0,
            4,
            0,
            text.len(),
            true,
            10,
            None,
            false,
            crate::markdown::CodeScanState::default(),
            table_kind,
            |_, _| None,
        );
        let rows_of = |w: &EditorWindow, line: usize| w.visual_rows.iter().filter(|r| r.line_idx == line).count();
        assert_eq!(rows_of(&w, 3), 1, "a table row must never wrap");
        assert_eq!(w.height_index.height_of(w.first_row_of_line(1).unwrap()), 42.0);
        assert_eq!(w.height_index.height_of(w.first_row_of_line(2).unwrap()), 1.0);
        assert_eq!(w.height_index.height_of(w.first_row_of_line(3).unwrap()), 42.0);

        let edited = "| a very long cell that exceeds the wrap width by far | b |".to_string();
        assert!(w.update_single_line(3, edited.clone(), 0, None, table_kind));
        assert_eq!(rows_of(&w, 3), 1);
        let row = w.visual_rows.iter().find(|r| r.line_idx == 3).unwrap();
        assert_eq!(row.char_end, edited.chars().count());

        // Editing an ordinary line must keep the table rows' exact heights.
        assert!(w.update_single_line(0, "intro intro intro intro intro".to_string(), 0, None, table_kind));
        assert_eq!(w.height_index.height_of(w.first_row_of_line(1).unwrap()), 42.0);
        assert_eq!(w.height_index.height_of(w.first_row_of_line(2).unwrap()), 1.0);
        assert_eq!(w.height_index.height_of(w.first_row_of_line(3).unwrap()), 42.0);
    }

    /// At the document edges the window is one-sided instead of out of range.
    #[test]
    fn bounds_clamp_at_the_document_edges() {
        assert_eq!(EditorWindow::bounds_for(0, 10), (0, 10));
        assert_eq!(EditorWindow::bounds_for(5, 10), (0, 10));
        assert_eq!(
            EditorWindow::bounds_for(1_000_000, 1_000_001),
            (1_000_000 - MARGIN_LINES, 1_000_001)
        );
        assert_eq!(EditorWindow::bounds_for(0, 0), (0, 0));
        assert_eq!(EditorWindow::bounds_for(999, 1), (0, 1));
    }

    /// A note that fits inside two margins is never windowed, so ordinary
    /// documents keep an exact height and no estimate at all.
    #[test]
    fn a_small_document_is_never_windowed() {
        let fits = 2 * MARGIN_LINES + 1;
        for total in [1, 2, 1000, fits] {
            assert_eq!(
                EditorWindow::bounds_for(0, total),
                (0, total),
                "total {total} must stay whole"
            );
            assert_eq!(
                EditorWindow::bounds_for(total - 1, total),
                (0, total),
                "total {total} must stay whole from the end too"
            );
        }
        // One line past that it does start windowing.
        let (s, e) = EditorWindow::bounds_for(fits, fits + 1);
        assert!(s > 0 || e < fits + 1);
    }

    /// Re-placing must be idempotent and must stop once the window already
    /// reaches the end of the document — otherwise the last screenful would be
    /// rebuilt on every frame.
    #[test]
    fn follow_is_idempotent_and_settles_at_the_end() {
        let text = doc(10_000);
        let index = LineIndex::new(&text);
        let total = index.lines();

        // Start where `bounds_for` would put the window, not anywhere we like:
        // re-placing is defined relative to that function.
        let (s, e) = EditorWindow::bounds_for(0, total);
        let w = EditorWindow::build(&text, &index, s, e, false, 0, None, crate::markdown::CodeScanState::default());
        assert_eq!((w.start_line, w.end_line), (0, MARGIN_LINES + 1));
        assert!(!w.should_move_to(0, total, false, 0));
        assert!(!w.should_move_to(1, total, false, 0));
        assert!(!w.should_move_to(100, total, false, 0));
        assert!(!w.should_move_to(FOLLOW_SLACK, total, false, 0));
        // Far enough down to want a new window: it may move.
        assert!(w.should_move_to(total - 1, total, false, 0));

        // At the end the bounds cannot grow, so asking again changes nothing.
        let (s, e) = EditorWindow::bounds_for(total - 1, total);
        let w2 = EditorWindow::build(&text, &index, s, e, false, 0, None, crate::markdown::CodeScanState::default());
        assert_eq!(w2.end_line, total);
        assert!(!w2.should_move_to(total - 1, total, false, 0));
        assert!(!w2.should_move_to(total - 2, total, false, 0));
        assert!(!w2.should_move_to(total - 100, total, false, 0));
        assert!(!w2.should_move_to(total - 1 - FOLLOW_SLACK, total, false, 0));

        // Scrolling back up eventually asks for a new window, and that window
        // is the same one `bounds_for` describes.
        assert!(w2.should_move_to(0, total, false, 0));
        let (s3, e3) = EditorWindow::bounds_for(0, total);
        let w3 = EditorWindow::build(&text, &index, s3, e3, false, 0, None, crate::markdown::CodeScanState::default());
        assert_eq!((w3.start_line, w3.end_line), (0, MARGIN_LINES + 1));
        assert!(!w3.should_move_to(0, total, false, 0));
    }

    /// Changing the wrap settings always re-places, because row geometry is
    /// only valid for the settings it was measured with.
    #[test]
    fn wrap_change_forces_a_rebuild() {
        let text = doc(100);
        let index = LineIndex::new(&text);
        let total = index.lines();
        let w = EditorWindow::build(&text, &index, 0, total, false, 0, None, crate::markdown::CodeScanState::default());
        assert!(w.should_move_to(0, total, true, 80));
        assert!(w.should_move_to(0, total, false, 40));
        assert!(!w.should_move_to(0, total, false, 0));
    }

    /// Lookups take document lines, not window offsets. Both are `usize`, so
    /// mixing them is exactly the mistake these types exist to prevent.
    #[test]
    fn lines_and_states_are_addressed_by_document_line() {
        let text = doc(9000);
        let index = LineIndex::new(&text);
        let w = EditorWindow::build(&text, &index, 4000, 7000, false, 0, None, crate::markdown::CodeScanState::default());

        assert_eq!(w.lines[4000], "line 4000");
        assert_eq!(w.lines[6999], "line 6999");
        assert_eq!(w.lines.start(), 4000);
        assert_eq!(w.lines.end(), 7000);
        assert_eq!(w.lines.get(3999), None);
        assert_eq!(w.lines.get(7000), None);
        assert!(w.code_states[4000].is_none());
        assert_eq!(w.code_states.get(3999), None);
        assert_eq!(w.code_states.start(), 4000);
        // `local` is the window offset, and it is what the arrays do not take.
        assert_eq!(w.local(4000), 0);
        assert_eq!(w.local(6999), 2999);
    }

    /// Addressing a line the window does not hold is a panic, not a silent read
    /// of whichever line happens to sit at that offset.
    #[test]
    #[should_panic(expected = "outside the window")]
    fn indexing_outside_the_window_panics() {
        let text = doc(9000);
        let index = LineIndex::new(&text);
        let w = EditorWindow::build(&text, &index, 4000, 7000, false, 0, None, crate::markdown::CodeScanState::default());
        let _ = &w.lines[0];
    }

    /// The empty window is what a note has before its first frame, and it must
    /// answer every question instead of panicking.
    #[test]
    fn empty_window_is_inert() {
        let w = EditorWindow::default();
        assert!(w.is_empty());
        assert_eq!(w.line_count(), 0);
        assert_eq!(w.line(0), None);
        assert_eq!(w.code_state(0), None);
        assert_eq!(w.first_row_of_line(0), None);
        assert_eq!(w.total_height(), 0.0);
        assert_eq!(w.bytes(), 0);
        assert!(w.should_move_to(0, 10, false, 0));
    }

    #[test]
    fn test_typing_stability_near_boundaries_and_middle() {
        let text = doc(20_000);
        let index = LineIndex::new(&text);
        let total = index.lines();

        // Window anchored at top (line 0)
        let (s, e) = EditorWindow::bounds_for(0, total);
        let w = EditorWindow::build(&text, &index, s, e, false, 0, None, crate::markdown::CodeScanState::default());
        assert_eq!((w.start_line, w.end_line), (0, MARGIN_LINES + 1));

        // When typing on line 1, 10, 50, 500, or 1000, should_move_to must NOT trigger
        for anchor in [0, 1, 10, 50, 500, 1000, 1499] {
            assert!(!w.should_move_to(anchor, total, false, 0), "anchor {anchor} should not move window");
        }

        // Window anchored at middle (line 10_000)
        let (s2, e2) = EditorWindow::bounds_for(10_000, total);
        let w2 = EditorWindow::build(&text, &index, s2, e2, false, 0, None, crate::markdown::CodeScanState::default());

        // When typing within FOLLOW_SLACK of 10_000, window must not move
        for anchor in [9000, 9500, 10_000, 10_500, 11_000] {
            assert!(!w2.should_move_to(anchor, total, false, 0), "middle anchor {anchor} should not move window");
        }
    }

    #[test]
    fn test_code_states_and_lines_bounds_with_distant_selection() {
        let text = doc(50_000);
        let index = LineIndex::new(&text);
        let total = index.lines();

        let (s, e) = EditorWindow::bounds_for(10_000, total);
        let w = EditorWindow::build(&text, &index, s, e, false, 0, None, crate::markdown::CodeScanState::default());

        // A selection far outside the window (e.g. lines 40_000..45_000)
        let sel_start_line = 40_000;
        let sel_end_line = 45_000;

        let win_start = w.code_states.start();
        let win_end = w.code_states.end();
        let scan_start = sel_start_line.max(win_start);
        let scan_end = sel_end_line.min(win_end.saturating_sub(1));

        // Clamped range must be empty when selection is entirely outside
        assert!(scan_start > scan_end);

        // Safe query via get() returns None without panic
        assert_eq!(w.lines.get(sel_start_line), None);
        assert_eq!(w.code_states.get(sel_start_line), None);
    }
