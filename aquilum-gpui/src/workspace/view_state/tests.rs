    use super::*;
    use super::test_support::scratch_doc;

    #[test]
    fn test_insert_text_with_cr_and_crlf_breaks_into_multiple_lines() {
        let mut doc = scratch_doc();
        let mut view = EditorViewState::new(TabId::new(), doc.id);
        let input = "после исправления -  проверь  ssl  еще раз  \r \r \r                            ";
        view.insert_text(&mut doc, input);

        assert_eq!(doc.line_index.lines(), 4);
        assert_eq!(doc.line_text(0), "после исправления -  проверь  ssl  еще раз  ");
        assert_eq!(doc.line_text(1), " ");
        assert_eq!(doc.line_text(2), " ");
        assert_eq!(doc.line_text(3), "                            ");
    }

    #[test]
    fn test_viewport_stability_when_edit_is_above() {
        let text_initial = "line 0\nline 1\nline 2\nline 3\nline 4\nline 5\n";

        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        // Viewport anchored at "line 3" (offset: 7*3 = 21)
        let line_3_offset = 21;
        view.viewport.offset = line_3_offset;
        view.scroll_top = 3.0 * 24.0;
        view.above_lines = 3;
        view.above_h = 3.0 * 24.0;

        // Another view inserts 100 new lines at line 0 (offset 0)
        let inserted = "inserted\n".repeat(100);
        let edit = Edit {
            start: 0,
            old_len: 0,
            new_len: inserted.len(),
        };

        view.apply_external_edit(edit, 100, 0);

        // The viewport anchor offset shifted by new_len
        assert_eq!(view.viewport.offset, line_3_offset + inserted.len());

        let text_after = format!("{}{}", inserted, text_initial);
        let line_slice = &text_after[view.viewport.offset..view.viewport.offset + 6];
        assert_eq!(line_slice, "line 3");

        // scroll_top increased by 100 lines * 24px
        assert_eq!(view.scroll_top, (3.0 + 100.0) * 24.0);
    }

    #[test]
    fn test_viewport_stability_when_edit_is_below() {
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        // Viewport anchored at "line 1" (offset 7)
        let line_1_offset = 7;
        view.viewport.offset = line_1_offset;
        view.scroll_top = 24.0;

        // Edit happens at line 4 (below viewport, offset 28)
        let line_4_offset = 28;
        let inserted = "hello\nworld\n";
        let edit = Edit {
            start: line_4_offset,
            old_len: 0,
            new_len: inserted.len(),
        };

        view.apply_external_edit(edit, 2, 0);

        // Viewport anchor and scroll_top did not change at all!
        assert_eq!(view.viewport.offset, line_1_offset);
        assert_eq!(view.scroll_top, 24.0);
    }

    #[test]
    fn test_multi_window_viewport_stability_with_window_shift() {
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        let start_line = 997_000;
        let end_line = 1_000_000;
        let anchor_line = 1_000_000;
        view.viewport.offset = anchor_line;
        view.cursor_offset = 50_000_000;

        let lines_vec: Vec<String> = (start_line..end_line).map(|i| format!("line {i}")).collect();
        let visual_rows: Vec<crate::markdown::VisualRow> = (start_line..end_line)
            .map(|i| crate::markdown::VisualRow {
                line_idx: i,
                sub_idx: 0,
                is_last_sub: true,
                char_start: 0,
                char_end: 10,
            })
            .collect();
        let height_index = crate::markdown::VisualRowHeightIndex::new(vec![24.0; visual_rows.len()]);

        view.window = EditorWindow {
            start_line,
            end_line,
            start_byte: 40_000_000,
            end_byte: 50_000_000,
            lines: std::sync::Arc::new(crate::editor_window::WindowLines::new(start_line, lines_vec)),
            visual_rows: std::sync::Arc::new(visual_rows),
            code_states: std::sync::Arc::new(crate::editor_window::WindowCodeStates::default()),
            height_index: std::sync::Arc::new(height_index),
            word_wrap: false,
            wrap_cols: 80,
        };

        view.above_lines = start_line;
        view.est_rows_per_line = 1.0;
        view.est_row_h = 24.0;
        view.above_h = start_line as f64 * 24.0;
        view.scroll_top = view.above_h + 100.0;

        let initial_local_scroll = view.scroll_top - view.above_h;
        assert_eq!(initial_local_scroll, 100.0);

        // View 1 at line 10 inserts 10 lines
        let edit_event = crate::state::NoteEditEvent {
            edit_line: 10,
            at: 100,
            old_len: 0,
            new_len: 10,
            line_diff: 10,
        };

        view.apply_external_edit_detailed(&scratch_doc(), &edit_event);

        // 1. Invariant: local scroll (screen position of visible text) must be 100% UNCHANGED
        let new_local_scroll = view.scroll_top - view.above_h;
        assert_eq!(new_local_scroll, initial_local_scroll);

        // 2. Window lines shifted: 997,000 -> 997,010; 1,000,000 -> 1,000,010
        assert_eq!(view.window.start_line, 997_010);
        assert_eq!(view.window.end_line, 1_000_010);
        assert_eq!(view.window.lines.start(), 997_010);

        // 3. Line numbers in the visual rows incremented by 10 (gutter line numbers update!)
        assert_eq!(view.window.visual_rows.last().unwrap().line_idx, 1_000_009);

        // 4. Cursor shifted by new_len
        assert_eq!(view.cursor_offset, 50_000_010);

        // 5. Anchor line shifted by 10
        assert_eq!(view.viewport.offset, 1_000_010);

        // 6. The text inside the window is preserved exactly
        assert_eq!(view.window.line(997_010).unwrap(), "line 997000");

        // 7. Window remains non-empty and cached (zero rebuild cost on render/typing)
        assert!(!view.window.is_empty());
        assert!(!view.visual_cache_dirty);
    }

    #[test]
    fn test_view_state_resize_and_wrap_cols_anchoring() {
        let tab_id = TabId::new();
        let doc_id = DocumentId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        let anchor_line = 500;
        view.viewport.offset = anchor_line;
        view.viewport.px_in_line = 5.0;

        let lines_vec: Vec<String> = (0..1000).map(|i| format!("line {i} with some content that can wrap when resized")).collect();
        let mut heights = Vec::new();
        let mut visual_rows = Vec::new();
        for i in 0..1000 {
            heights.push(24.0);
            visual_rows.push(crate::markdown::VisualRow {
                line_idx: i,
                sub_idx: 0,
                is_last_sub: true,
                char_start: 0,
                char_end: lines_vec[i].len(),
            });
        }
        let height_index = crate::markdown::VisualRowHeightIndex::new(heights);

        view.window = crate::editor_window::EditorWindow {
            start_line: 0,
            end_line: 1000,
            start_byte: 0,
            end_byte: 50_000,
            lines: std::sync::Arc::new(crate::editor_window::WindowLines::new(0, lines_vec.clone())),
            visual_rows: std::sync::Arc::new(visual_rows),
            code_states: std::sync::Arc::new(crate::editor_window::WindowCodeStates::default()),
            height_index: std::sync::Arc::new(height_index),
            word_wrap: true,
            wrap_cols: 80,
        };
        view.above_lines = 0;
        view.above_h = 0.0;
        let first_v = view.window.first_row_of_line(anchor_line).unwrap();
        view.scroll_top = view.above_h + view.window.height_index.top_of(first_v) + view.viewport.px_in_line as f64;

        // When resizing (dragging split border), wrap_cols changes:
        // Say wrap_cols changes to 40, increasing rows for wrapped lines
        let new_wrap_cols = 40;
        assert!(view.window.should_move_to(view.viewport.offset, 1000, true, new_wrap_cols));

        // Rebuilt window with new_wrap_cols
        let mut new_heights = Vec::new();
        let mut new_visual_rows = Vec::new();
        for i in 0..1000 {
            // Some lines wrap into 2 rows at wrap_cols 40
            let rows_count = if i % 2 == 0 { 2 } else { 1 };
            for sub in 0..rows_count {
                new_heights.push(24.0);
                new_visual_rows.push(crate::markdown::VisualRow {
                    line_idx: i,
                    sub_idx: sub,
                    is_last_sub: sub + 1 == rows_count,
                    char_start: 0,
                    char_end: lines_vec[i].len(),
                });
            }
        }
        let new_height_index = crate::markdown::VisualRowHeightIndex::new(new_heights);
        let new_window = crate::editor_window::EditorWindow {
            start_line: 0,
            end_line: 1000,
            start_byte: 0,
            end_byte: 50_000,
            lines: std::sync::Arc::new(crate::editor_window::WindowLines::new(0, lines_vec)),
            visual_rows: std::sync::Arc::new(new_visual_rows),
            code_states: std::sync::Arc::new(crate::editor_window::WindowCodeStates::default()),
            height_index: std::sync::Arc::new(new_height_index),
            word_wrap: true,
            wrap_cols: new_wrap_cols,
        };

        // Anchor recalculation
        let new_first_v = new_window.first_row_of_line(view.viewport.offset).unwrap();
        let new_line_local_y = new_window.height_index.top_of(new_first_v);
        let new_scroll_top = view.above_h + new_line_local_y + view.viewport.px_in_line as f64;
        view.scroll_top = new_scroll_top;
        view.window = new_window;

        // Invariant: local_scroll = scroll_top - above_h lands exactly on anchor line + px_in_line
        let local_scroll = view.scroll_top - view.above_h;
        assert_eq!(local_scroll, new_line_local_y + 5.0);
        let vr = view.window.height_index.visible_range(local_scroll, 600.0, 300.0);
        assert!(vr.start <= new_first_v);
        assert!(vr.end > new_first_v);
    }

    #[test]
    fn test_apply_external_edit_detailed_cursor_and_selection_cases() {
        let tab_id = TabId::new();
        let doc_id = DocumentId::new();

        // 1. Edit strictly above selection: [20..30], edit at 5 (insert 10 bytes)
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 30;
        view.selection_anchor = Some(20);
        let edit = crate::state::NoteEditEvent {
            edit_line: 0,
            at: 5,
            old_len: 0,
            new_len: 10,
            line_diff: 0,
        };
        view.apply_external_edit_detailed(&scratch_doc(), &edit);
        assert_eq!(view.cursor_offset, 40);
        assert_eq!(view.selection_anchor, Some(30));

        // 2. Edit strictly below selection: [20..30], edit at 50 (insert 10 bytes)
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 30;
        view.selection_anchor = Some(20);
        let edit = crate::state::NoteEditEvent {
            edit_line: 5,
            at: 50,
            old_len: 0,
            new_len: 10,
            line_diff: 0,
        };
        view.apply_external_edit_detailed(&scratch_doc(), &edit);
        assert_eq!(view.cursor_offset, 30);
        assert_eq!(view.selection_anchor, Some(20));

        // 3. Edit inside selection: [20..30], edit at 25 (insert 5 bytes)
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 30;
        view.selection_anchor = Some(20);
        let edit = crate::state::NoteEditEvent {
            edit_line: 2,
            at: 25,
            old_len: 0,
            new_len: 5,
            line_diff: 0,
        };
        view.apply_external_edit_detailed(&scratch_doc(), &edit);
        // Start (20) unchanged, end (30) expanded to 35
        assert_eq!(view.selection_anchor, Some(20));
        assert_eq!(view.cursor_offset, 35);

        // 4. Edit at start boundary: [20..30], insert at 20
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 30;
        view.selection_anchor = Some(20);
        let edit = crate::state::NoteEditEvent {
            edit_line: 2,
            at: 20,
            old_len: 0,
            new_len: 5,
            line_diff: 0,
        };
        view.apply_external_edit_detailed(&scratch_doc(), &edit);
        // Start mapped with Bias::Left stays at 20, end mapped with Bias::Right shifts to 35
        assert_eq!(view.selection_anchor, Some(20));
        assert_eq!(view.cursor_offset, 35);

        // 5. Cursor without selection at boundary of insert: cursor=20, insert at 20
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 20;
        view.selection_anchor = None;
        let edit = crate::state::NoteEditEvent {
            edit_line: 2,
            at: 20,
            old_len: 0,
            new_len: 5,
            line_diff: 0,
        };
        view.apply_external_edit_detailed(&scratch_doc(), &edit);
        // Single cursor with Bias::Left stays at 20
        assert_eq!(view.cursor_offset, 20);
    }

    #[test]
    fn test_multi_window_switch_stability_no_drift() {
        let text = (0..500)
            .map(|i| format!("Line {:03}: some meaningful content here for testing.\n", i))
            .collect::<String>();
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text,
            1,
        );
        let lines_total = doc.line_index.lines();

        let tab1 = TabId::new();
        let tab2 = TabId::new();
        let mut view1 = EditorViewState::new(tab1, doc.id);
        let mut view2 = EditorViewState::new(tab2, doc.id);

        let wrap_cols = 60;
        view1.rebuild_visual_cache_with_headings(&mut doc, true, wrap_cols, None, 100, false);
        view2.rebuild_visual_cache_with_headings(&mut doc, true, wrap_cols, None, 50, false);

        let initial_scroll_1 = view1.doc_y_of_line(lines_total, 100);
        let initial_scroll_2 = view2.doc_y_of_line(lines_total, 50);

        view1.scroll_top = initial_scroll_1;
        view1.viewport.offset = view1.line_at_doc_y(lines_total, initial_scroll_1);
        let line_top1 = view1.doc_y_of_line(lines_total, view1.viewport.offset);
        view1.viewport.px_in_line = ((initial_scroll_1 - line_top1) as f32).max(0.0);

        view2.scroll_top = initial_scroll_2;
        view2.viewport.offset = view2.line_at_doc_y(lines_total, initial_scroll_2);
        let line_top2 = view2.doc_y_of_line(lines_total, view2.viewport.offset);
        view2.viewport.px_in_line = ((initial_scroll_2 - line_top2) as f32).max(0.0);

        // Simulate 20 alternating switch cycles between Split 1 and Split 2
        for _ in 0..20 {
            // Switch 1 -> 2:
            // 1) Snapshot outgoing (view1)
            view1.viewport.offset = view1.line_at_doc_y(lines_total, view1.scroll_top);
            let top1 = view1.doc_y_of_line(lines_total, view1.viewport.offset);
            view1.viewport.px_in_line = ((view1.scroll_top - top1) as f32).max(0.0);

            // 2) Restore incoming (view2)
            view2.ensure_visual_cache_with_headings(&mut doc, true, wrap_cols, None, view2.viewport.offset, false);
            let restore_y2 = if view2.window.wrap_cols == wrap_cols {
                view2.scroll_top
            } else {
                let top2 = view2.doc_y_of_line(lines_total, view2.viewport.offset);
                top2 + view2.viewport.px_in_line as f64
            };
            view2.scroll_top = restore_y2;

            // Switch 2 -> 1:
            // 1) Snapshot outgoing (view2)
            view2.viewport.offset = view2.line_at_doc_y(lines_total, view2.scroll_top);
            let top2 = view2.doc_y_of_line(lines_total, view2.viewport.offset);
            view2.viewport.px_in_line = ((view2.scroll_top - top2) as f32).max(0.0);

            // 2) Restore incoming (view1)
            view1.ensure_visual_cache_with_headings(&mut doc, true, wrap_cols, None, view1.viewport.offset, false);
            let restore_y1 = if view1.window.wrap_cols == wrap_cols {
                view1.scroll_top
            } else {
                let top1 = view1.doc_y_of_line(lines_total, view1.viewport.offset);
                top1 + view1.viewport.px_in_line as f64
            };
            view1.scroll_top = restore_y1;
        }

        assert_eq!(view1.scroll_top, initial_scroll_1, "Split 1 scroll_top must have zero drift after repeated switches");
        assert_eq!(view1.viewport.offset, 100, "Split 1 anchor line must remain line 100");
        assert_eq!(view2.scroll_top, initial_scroll_2, "Split 2 scroll_top must have zero drift after repeated switches");
        assert_eq!(view2.viewport.offset, 50, "Split 2 anchor line must remain line 50");
    }

    #[test]
    fn test_grid_table_at_and_raw_table_reset() {
        let text = "Intro\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nMiddle\n\n| C | D |\n|---|---|\n| 3 | 4 |\n\nOutro";
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text.to_string(),
            1,
        );
        let tab_id = TabId::new();
        let mut view = TabSession::new(tab_id, doc.id);

        let (tbl1_fl, tbl2_fl) = {
            let tables = doc.ensure_table_index();
            assert_eq!(tables.blocks.len(), 2);
            assert_eq!(tables.blocks[0].first_line, 2);
            assert_eq!(tables.blocks[0].last_line, 4);
            assert_eq!(tables.blocks[1].first_line, 8);
            assert_eq!(tables.blocks[1].last_line, 10);
            (tables.blocks[0].first_line, tables.blocks[1].first_line)
        };

        // Initially both tables return Some (grid mode)
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 2, 2, false).is_some());
            assert!(view.grid_table_at(&doc, &tables, 8, 8, false).is_some());
        }

        // Toggle Table 1 into raw mode
        view.toggle_raw_table_at(tbl1_fl);
        assert!(view.is_table_raw(tbl1_fl));
        assert!(!view.is_table_raw(tbl2_fl));

        // Table 1 is in raw mode (None), Table 2 remains in grid mode (Some)
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 2, 2, false).is_none());
            assert!(view.grid_table_at(&doc, &tables, 4, 3, false).is_none());
            assert!(view.grid_table_at(&doc, &tables, 8, 8, false).is_some());
            assert!(view.grid_table_at(&doc, &tables, 10, 8, false).is_some());
        }

        // When cursor moves outside, both tables display as grid tables, but Table 1 remembers it is raw
        view.cursor_offset = 0; // outside tables
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        assert!(view.is_table_raw(tbl1_fl), "Table 1 raw setting persists");
        {
            let tables2 = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables2, 2, 0, false).is_some(), "Table 1 displays as grid mode when cursor outside");
            assert!(view.grid_table_at(&doc, &tables2, 8, 0, false).is_some(), "Table 2 displays as grid mode");
        }

        // When cursor moves back to Table 1, Table 1 is in raw code mode
        view.cursor_offset = doc.line_start_byte(2);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables3 = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables3, 2, 2, false).is_none(), "Table 1 enters raw mode when cursor returns");
            assert!(view.grid_table_at(&doc, &tables3, 8, 2, false).is_some(), "Table 2 remains in grid mode");
        }

        // When cursor moves to Table 2 (which is in inline mode), Table 2 stays in inline mode
        view.cursor_offset = doc.line_start_byte(8);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables4 = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables4, 2, 8, false).is_some(), "Table 1 displays as grid mode");
            assert!(view.grid_table_at(&doc, &tables4, 8, 8, false).is_some(), "Table 2 displays as inline grid mode");
        }
    }

    #[test]
    fn test_selection_reveals_table_as_source() {
        let text = "Intro\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nMiddle\n\n| C | D |\n|---|---|\n| 3 | 4 |\n\nOutro";
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text.to_string(),
            1,
        );
        let mut view = TabSession::new(TabId::new(), doc.id);

        // Selection from "Middle" through the second table reveals its source.
        view.selection_anchor = Some(doc.line_start_byte(6));
        view.cursor_offset = doc.line_start_byte(10) + 3;
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 8, 10, false).is_none());
            assert!(view.grid_table_at(&doc, &tables, 9, 10, false).is_none());
            assert!(view.grid_table_at(&doc, &tables, 10, 10, false).is_none());
            assert!(view.grid_table_at(&doc, &tables, 2, 10, false).is_some());
            assert_eq!(view.revealed_selection, Some((6, 6, 10)));

            let header_row = view.window.first_row_of_line(8).unwrap();
            assert_eq!(view.window.height_index.height_of(header_row), 24.0);
            assert!(!view.is_table_raw(8), "reveal must not persist as raw mode");
        }

        // A drag that starts inside a cell keeps the grid and table copy mode,
        // even when it is extended above the table.
        view.selection_anchor = Some(doc.line_start_byte(10) + 3);
        view.cursor_offset = doc.line_start_byte(6);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 8, 6, false).is_some());
            assert_eq!(view.revealed_selection, Some((10, 6, 10)));
        }
        view.selection_anchor = Some(doc.line_start_byte(6));
        view.cursor_offset = doc.line_start_byte(10) + 3;
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);

        // Ctrl/preview selection keeps the grid layout.
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, true);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 8, 10, true).is_some());
            assert_eq!(view.revealed_selection, None);
            let header_row = view.window.first_row_of_line(8).unwrap();
            assert_eq!(view.window.height_index.height_of(header_row), 42.0);
        }

        // A selection that ends above the table keeps the grid.
        view.selection_anchor = Some(doc.line_start_byte(0));
        view.cursor_offset = doc.line_start_byte(6) + 2;
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 8, 6, false).is_some());
            let header_row = view.window.first_row_of_line(8).unwrap();
            assert_eq!(view.window.height_index.height_of(header_row), 42.0);
        }

        // Collapsing the selection returns the grid.
        view.selection_anchor = Some(view.cursor_offset);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 8, 6, false).is_some());
        }
    }

    #[test]
    fn an_upward_drag_holds_a_revealed_image_against_the_reflow() {
        let text = "Intro

![[pic.png]]

Middle";
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text.to_string(),
            1,
        );
        let mut view = TabSession::new(TabId::new(), doc.id);
        view.cursor_offset = doc.line_start_byte(4);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        view.selection_anchor = Some(doc.line_start_byte(4));
        view.cursor_offset = doc.line_start_byte(2);
        view.hold_drag_revealed_images(&doc, false);
        assert!(view.drag_revealed_images.contains(&2));

        view.cursor_offset = doc.line_start_byte(3);
        assert!(view.drag_revealed_images.contains(&2));
        view.drag_revealed_images.clear();
    }

    #[test]
    fn an_upward_drag_holds_a_revealed_table_against_the_reflow() {
        let text = "Intro\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nMiddle";
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text.to_string(),
            1,
        );
        let mut view = TabSession::new(TabId::new(), doc.id);
        view.cursor_offset = doc.line_start_byte(6);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);

        // Drag from "Middle" up into the table's last row.
        view.selection_anchor = Some(doc.line_start_byte(6));
        view.cursor_offset = doc.line_start_byte(4);
        view.hold_drag_revealed_tables(&doc, false);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);

        // The raw rows are shorter than the grid, so the same pointer now
        // resolves to the blank line above the anchor. The table must stay
        // source instead of flipping back to the grid at its edge.
        view.cursor_offset = doc.line_start_byte(5);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 2, 5, false).is_none());
        }

        // Releasing the drag drops the hold: with no selection over it the
        // table returns to the grid.
        view.drag_revealed_tables.clear();
        view.visual_cache_dirty = true;
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 2, 5, false).is_some());
        }
    }

    #[test]
    fn test_selection_span_handles_multibyte_boundaries() {
        let text = "Начало\n\n| Колонка |\n|---|\n| значение";
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text.to_string(),
            1,
        );
        let mut view = TabSession::new(TabId::new(), doc.id);

        view.selection_anchor = Some(0);
        view.cursor_offset = doc.len();
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);

        assert_eq!(view.revealed_selection, Some((0, 0, 4)));
        let tables = doc.ensure_table_index();
        assert!(view.grid_table_at(&doc, &tables, 2, 4, false).is_none());
    }

    #[test]
    fn test_table_cell_selection_column_drag() {
        let mut sel = TableCellSelection::new(2, 6, 2); // anchor: row 6, col 2
        assert!(!sel.is_multi_cell());
        assert!(sel.contains(6, 2));
        assert!(!sel.contains(5, 2));

        // Dragging upwards in the same column to row 1
        sel.head_row = 1;
        sel.head_col = 2;
        assert!(sel.is_multi_cell());
        let rect = sel.rect();
        assert_eq!(rect.row_start, 1);
        assert_eq!(rect.row_end, 7);
        assert_eq!(rect.col_start, 2);
        assert_eq!(rect.col_end, 3);

        // All rows 1..=6 in col 2 are in the selection
        for r in 1..=6 {
            assert!(sel.contains(r, 2), "row {} in col 2 must be selected", r);
            assert!(!sel.contains(r, 0), "col 0 must not be selected");
            assert!(!sel.contains(r, 1), "col 1 must not be selected");
            assert!(!sel.contains(r, 3), "col 3 must not be selected");
        }
        assert!(!sel.contains(0, 2), "row 0 not selected");
        assert!(!sel.contains(7, 2), "row 7 not selected");
    }

    fn rows_of_line(view: &EditorViewState, line: usize) -> usize {
        view.window.visual_rows.iter().filter(|r| r.line_idx == line).count()
    }

    #[test]
    fn caret_line_rewraps_for_revealed_markup_only_while_it_is_active() {
        // The link label is short, the URL is long: hidden markup fits one row,
        // the revealed `[label](url)` needs more.
        let text = format!(
            "prefix [label](https://example.com/{}) tail\nsecond line",
            "a".repeat(24)
        );
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text,
            1,
        );
        let mut view = TabSession::new(TabId::new(), doc.id);
        let wrap_cols = 40;

        // Caret on the next line: canonical wrap, markup hidden.
        view.cursor_offset = doc.len();
        view.ensure_visual_cache_with_headings(&mut doc, true, wrap_cols, None, 0, false);
        let canonical_rows = rows_of_line(&view, 0);
        assert_eq!(canonical_rows, 1, "hidden link must fit one row");
        assert!(view.line_paints.is_empty());

        // Caret inside the link label: the element is revealed and the line is
        // wrapped with its marker cells paid for, so it needs more rows.
        view.cursor_offset = 10;
        view.ensure_visual_cache_with_headings(&mut doc, true, wrap_cols, None, 0, false);
        assert!(
            rows_of_line(&view, 0) > canonical_rows,
            "revealed link must take more rows than the hidden one"
        );
        assert_eq!(view.line_paints.len(), 1);

        // The caret leaves the line: the exception is dropped and the rows go
        // back to the canonical (markup hidden) wrap.
        view.cursor_offset = doc.len();
        view.ensure_visual_cache_with_headings(&mut doc, true, wrap_cols, None, 0, false);
        assert_eq!(rows_of_line(&view, 0), canonical_rows);
        assert!(view.line_paints.is_empty());

        // Preview and Ctrl-selection keep that canonical wrap even with the
        // caret back on the line.
        view.cursor_offset = 10;
        view.ensure_visual_cache_with_headings(&mut doc, true, wrap_cols, None, 0, true);
        assert_eq!(rows_of_line(&view, 0), canonical_rows);
        assert!(view.line_paints.is_empty());

        // Editing again restores the revealed wrap.
        view.ensure_visual_cache_with_headings(&mut doc, true, wrap_cols, None, 0, false);
        assert!(rows_of_line(&view, 0) > canonical_rows);
    }

    #[test]
    fn incremental_edit_keeps_the_configured_row_height() {
        let text = "one\ntwo\nthree\n";
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text.to_string(),
            1,
        );
        let mut view = TabSession::new(TabId::new(), doc.id);
        let editor = crate::settings::EditorSettings {
            line_height: 1.8,
            font: crate::settings::FontSettings {
                font_size_base: 18,
                ..Default::default()
            },
            ..Default::default()
        };
        let typography = crate::settings::EditorTypography::from_editor(&editor);
        assert_eq!(typography.row_height, 32.0);
        view.rebuild_visual_cache_with_headings(&mut doc, true, 80, Some(&typography), 0, false);

        // The same-line path must recompute heights against the snapshot the
        // window was built with, not against a default typography.
        let cached = view.typography_cache.clone();
        assert!(view.window.update_single_line(1, "two!".to_string(), 1, Some(&cached), |_, _| None));
        for row in 0..view.window.visual_rows.len() {
            assert_eq!(
                view.window.height_index.height_of(row),
                32.0,
                "row {row} lost the configured height"
            );
        }
    }

    #[test]
    fn source_mode_disables_markdown_formatting_and_keeps_uniform_heights() {
        let text = "# Heading 1\n| col 1 | col 2 |\n| --- | --- |\n| val 1 | val 2 |\n```rust\nfn main() {}\n```\n- [ ] Task\n> [!NOTE]\n> Callout\n";
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text.to_string(),
            1,
        );
        let mut view = TabSession::new(TabId::new(), doc.id);
        view.source_mode = true;
        let typography = crate::settings::EditorTypography::reference();
        view.rebuild_visual_cache_with_headings(&mut doc, true, 80, Some(&typography), 0, false);

        // Every row in the window must have height 24.0 in source mode
        for i in 0..view.window.visual_rows.len() {
            assert_eq!(view.window.height_index.height_of(i), 24.0, "row {} must have uniform height 24.0 in source mode", i);
        }

        // Tables must not be shaped into visual grid rows
        let tables = doc.ensure_table_index_for_window(0, 10);
        assert!(view.grid_table_at(&doc, &tables, 1, 0, false).is_none(), "table must not be grid table in source mode");
    }
