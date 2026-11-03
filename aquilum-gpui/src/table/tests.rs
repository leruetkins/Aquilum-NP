    use super::*;

    fn doc(s: &str) -> Vec<&str> {
        s.split('\n').collect()
    }

    // The per-keystroke hot path relies on this: an incremental edit inside the
    // covered window must not shrink or shift `covered_range`, so the next
    // `scan_window` for the same window is a guard no-op instead of a rescan.
    #[test]
    fn scan_window_is_noop_while_edit_stays_inside_covered_range() {
        use crop::Rope;
        let mut text = String::new();
        for i in 0..6000 {
            text.push_str(&format!("line {}\n", i));
        }
        text.push_str("| a | b |\n|---|---|\n| 1 | 2 |\n");
        let rope = Rope::from(text);
        let mut idx = TableIndex::new();
        idx.scan_window(1, &rope, 1000, 5000);
        assert_eq!(idx.covered_range, (900, 5100), "coverage extends by the ±100 table-run margin");
        assert!(!idx.fully_scanned);
        // The table sits outside this window and must stay unindexed.
        assert!(idx.at(6000).is_none());

        // An incremental edit inside the covered range: coverage must not move.
        idx.apply_edit(2, 2000, 0, &rope);
        assert_eq!(
            idx.covered_range,
            (900, 5100),
            "apply_edit must not shrink or shift coverage for an in-window edit"
        );
        // The next keystroke's scan (same window, same rev) hits the guard.
        idx.scan_window(2, &rope, 1000, 5000);
        assert_eq!(idx.covered_range, (900, 5100));

        // An edit OUTSIDE the covered range extends coverage towards it.
        idx.apply_edit(3, 100, 0, &rope);
        assert_eq!(idx.covered_range.0, 100);
    }

    const T: &str = "intro\n\n| Имя | Qty |\n|:--|--:|\n| яблоко | 3 |\n| *pear* | 10 |\n\nконец";

    fn block(lines: &[&str]) -> TableBlock {
        find_tables(lines).into_iter().next().expect("table")
    }

    #[test]
    fn finds_table_bounds_and_aligns() {
        let l = doc(T);
        let t = find_tables(&l);
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].first_line, t[0].last_line), (2, 5));
        assert_eq!(t[0].aligns, vec![Align::Left, Align::Right]);
        assert_eq!(t[0].row_count(), 3);
        assert_eq!(t[0].row_of_line(2), Some(0));
        assert_eq!(t[0].row_of_line(3), None);
        assert_eq!(t[0].row_of_line(5), Some(2));
        assert_eq!(t[0].line_of_row(2), 5);
    }

    #[test]
    fn no_table_without_separator_or_for_plain_rule() {
        assert!(find_tables(&doc("a | b\nc | d")).is_empty());
        assert!(find_tables(&doc("text\n---\nmore")).is_empty());
    }

    #[test]
    fn table_in_blockquote_or_list_is_skipped() {
        assert!(find_tables(&doc("> | a |\n> |---|\n> | 1 |")).is_empty());
        assert!(find_tables(&doc("- | a |\n  |---|\n  | 1 |")).is_empty());
    }

    #[test]
    fn two_tables_and_local_lookup() {
        let l = doc("| a |\n|---|\n| 1 |\n\ntext\n\n| b | c |\n|---|---|\n| 2 | 3 |");
        let all = find_tables(&l);
        assert_eq!(all.len(), 2);
        assert_eq!((all[1].first_line, all[1].last_line), (6, 8));
        assert_eq!(table_at(&l, 7), Some(all[1].clone()));
        assert_eq!(table_at(&l, 4), None);
        assert_eq!(table_at(&l, 99), None);
    }

    #[test]
    fn table_index_refresh_and_lookup() {
        let l = doc(T);
        let mut idx = TableIndex::new();
        idx.refresh(1, &l);
        assert_eq!(idx.blocks.len(), 1);
        assert!(idx.at(2).is_some());
        assert!(idx.at(5).is_some());
        assert!(idx.at(0).is_none());
        assert!(idx.at(6).is_none());
    }

    #[test]
    fn split_row_variants() {
        let cs = split_row("| a | b |");
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].outer, 1..4);
        assert_eq!(cs[0].content, 2..3);

        assert_eq!(split_row("a | b").len(), 2);
        assert_eq!(split_row("| a | b").len(), 2);
        assert_eq!(split_row("a | b |").len(), 2);

        let cs = split_row(r"| a \| b | c |");
        assert_eq!(cs.len(), 2);
        assert_eq!(slice_chars(r"| a \| b | c |", &cs[0].content), r"a \| b");

        let cs = split_row("|  ||  |");
        assert_eq!(cs.len(), 3);
        assert!(cs.iter().all(|c| c.content.is_empty()));
    }

    #[test]
    fn split_row_uses_char_indices_not_bytes() {
        let line = "| яблоко | 3 |";
        let cs = split_row(line);
        assert_eq!(slice_chars(line, &cs[0].content), "яблоко");
        assert_eq!(cs[0].content, 2..8);
        assert_eq!(slice_chars(line, &cs[1].content), "3");
    }

    #[test]
    fn cell_index_boundaries() {
        let cs = split_row("| a | b |");
        assert_eq!(cell_index_at(&cs, 0), 0);
        assert_eq!(cell_index_at(&cs, 4), 0);
        assert_eq!(cell_index_at(&cs, 5), 1);
        assert_eq!(cell_index_at(&cs, 99), 1);
    }

    #[test]
    fn cell_rect_helpers() {
        let r = CellRect::new(2, 3, 0, 1);
        assert_eq!(r.row_start, 0);
        assert_eq!(r.row_end, 3);
        assert_eq!(r.col_start, 1);
        assert_eq!(r.col_end, 4);
        assert!(r.contains_cell(1, 2));
        assert!(!r.contains_cell(3, 2));
    }

    #[test]
    fn model_parse_and_padded_serialize() {
        let l = doc(T);
        let b = block(&l);
        let m = TableModel::parse(&b, &l);
        assert_eq!(m.rows[1], vec!["яблоко", "3"]);
        let out = m.serialize(true, &char_width);
        assert_eq!(
            out,
            vec![
                "| Имя    | Qty |",
                "| :----- | --: |",
                "| яблоко | 3   |",
                "| *pear* | 10  |",
            ]
        );
        let again: Vec<&str> = out.iter().map(|s| s.as_str()).collect();
        let b2 = block(&again);
        assert_eq!(TableModel::parse(&b2, &again), m);
    }

    #[test]
    fn short_and_long_rows_are_normalized() {
        let l = doc("| a | b |\n|---|---|\n| 1 |\n| 1 | 2 | 3 |");
        let b = block(&l);
        let m = TableModel::parse(&b, &l);
        assert_eq!(m.rows[1], vec!["1", ""]);
        assert_eq!(m.rows[2], vec!["1", "2"]);
    }

    #[test]
    fn row_and_col_ops() {
        let l = doc(T);
        let b = block(&l);
        let mut m = TableModel::parse(&b, &l);

        assert!(!m.insert_row(0));
        assert!(m.insert_row(1));
        assert_eq!(m.rows[1], vec!["", ""]);
        assert!(m.delete_row(1));
        assert!(!m.delete_row(0));

        assert!(m.insert_col(1));
        assert_eq!(m.rows[0], vec!["Имя", "", "Qty"]);
        assert_eq!(m.aligns, vec![Align::Left, Align::None, Align::Right]);
        assert!(m.move_col(2, 0));
        assert_eq!(m.rows[0], vec!["Qty", "Имя", ""]);
        assert_eq!(m.aligns[0], Align::Right);
        assert!(m.delete_col(2));
        assert!(m.delete_col(1));
        assert!(!m.delete_col(0));
        assert!(m.set_align(0, Align::Center));
    }

    #[test]
    fn sort_numeric_and_text_and_stability_of_header() {
        let l = doc("| n |\n|---|\n| 10 |\n| 2 |\n| 33 |");
        let b = block(&l);
        let mut m = TableModel::parse(&b, &l);
        assert!(m.sort_by_col(0, true));
        assert_eq!(m.rows.iter().map(|r| r[0].as_str()).collect::<Vec<_>>(), ["n", "2", "10", "33"]);
        assert!(m.sort_by_col(0, false));
        assert_eq!(m.rows[1][0], "33");

        let l = doc("| n |\n|---|\n| b |\n| 2 |\n| A |");
        let mut m = TableModel::parse(&block(&l), &l);
        assert!(m.sort_by_col(0, true));
        assert_eq!(m.rows.iter().map(|r| r[0].as_str()).collect::<Vec<_>>(), ["n", "2", "A", "b"]);
    }

    #[test]
    fn rewrite_replaces_exactly_the_table_lines() {
        let l = doc(T);
        let b = block(&l);
        let mut m = TableModel::parse(&b, &l);
        m.insert_row(3);
        let edit = rewrite(&b, &m, true, &char_width);
        let mut owned: Vec<String> = l.iter().map(|s| s.to_string()).collect();
        edit.apply(&mut owned);
        assert_eq!(owned.len(), l.len() + 1);
        assert_eq!(owned[0], "intro");
        assert_eq!(owned.last().unwrap(), "конец");
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        assert_eq!(block(&refs).last_line, b.last_line + 1);
    }

    #[test]
    fn tab_navigation() {
        let l = doc(T);
        let b = block(&l);
        assert_eq!(tab(&b, &l, 2, 3, false), Some(Nav::To { row: 0, cell: 1 }));
        assert_eq!(tab(&b, &l, 2, 9, false), Some(Nav::To { row: 1, cell: 0 }));
        assert_eq!(tab(&b, &l, 5, 14, false), Some(Nav::AppendRow { cell: 0 }));
        assert_eq!(tab(&b, &l, 2, 3, true), Some(Nav::Exit));
        assert_eq!(tab(&b, &l, 4, 3, true), Some(Nav::To { row: 0, cell: 1 }));
        assert_eq!(tab(&b, &l, 3, 3, false), None);
        assert_eq!(tab(&b, &l, 0, 0, false), None);
    }

    #[test]
    fn enter_navigation_and_cursor_placement() {
        let l = doc(T);
        let b = block(&l);
        assert_eq!(enter(&b, &l, 2, 3, false), Some(Nav::To { row: 1, cell: 0 }));
        assert_eq!(enter(&b, &l, 5, 3, false), Some(Nav::AppendRow { cell: 0 }));
        assert_eq!(enter(&b, &l, 2, 3, true), Some(Nav::Exit));

        assert_eq!(nav_cursor(&b, &l, 1, 0, Place::Start), (4, 2));
        assert_eq!(nav_cursor(&b, &l, 1, 0, Place::End), (4, 8));
        assert_eq!(nav_cursor(&b, &l, 1, 1, Place::End), (4, 12));
    }

    #[test]
    fn guard_protects_pipes_and_separator() {
        let l = doc(T);
        let b = block(&l);
        let line = l[4];
        assert_eq!(guard_edit(&b, 4, line, 3..4), EditGuard::Allow);
        assert_eq!(guard_edit(&b, 4, line, 5..5), EditGuard::Allow);
        assert_eq!(guard_edit(&b, 4, line, 8..8), EditGuard::Allow);
        assert_eq!(guard_edit(&b, 4, line, 1..2), EditGuard::Allow);
        assert_eq!(guard_edit(&b, 4, line, 0..1), EditGuard::Deny);
        assert_eq!(guard_edit(&b, 4, line, 9..10), EditGuard::Deny);
        assert_eq!(guard_edit(&b, 4, line, 5..11), EditGuard::Deny);
        assert_eq!(guard_edit(&b, 3, l[3], 3..3), EditGuard::Deny);
        assert_eq!(guard_edit(&b, 0, l[0], 0..0), EditGuard::Deny);
    }

    #[test]
    fn sanitize_input() {
        assert_eq!(sanitize_cell_input("a|b"), r"a\|b");
        assert_eq!(sanitize_cell_input(r"a\|b"), r"a\|b");
        assert_eq!(sanitize_cell_input("x\ny\r\nz"), "x<br>y<br>z");
    }

    #[test]
    fn tsv_roundtrip() {
        let mut m = TableModel::from_tsv("Имя\tQty\nяблоко\t3\nгруша\n").unwrap();
        assert_eq!(m.col_count(), 2);
        assert_eq!(m.rows[2], vec!["груша", ""]);
        assert_eq!(m.to_tsv(0..3, 0..2), "Имя\tQty\nяблоко\t3\nгруша\t");
        assert!(TableModel::from_tsv("  \n").is_none());
        let esc = TableModel::from_tsv("a|b\tc").unwrap();
        assert_eq!(esc.rows[0][0], r"a\|b");
        assert_eq!(esc.to_tsv(0..1, 0..1), "a|b");

        m.clear_rect(&CellRect::new(1, 0, 1, 1));
        assert_eq!(m.rows[1], vec!["", ""]);
    }

    #[test]
    fn serialized_output_is_recognised_by_the_parser() {
        let m = TableModel::from_tsv("x|y\tz\n1\t2").unwrap();
        let out = m.serialize(true, &char_width);
        let refs: Vec<&str> = out.iter().map(|s| s.as_str()).collect();
        let t = find_tables(&refs);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].col_count(), 2);
        assert_eq!(TableModel::parse(&t[0], &refs), m);
    }

    #[test]
    fn scan_rope_and_apply_edit_incremental() {
        use crop::Rope;

        let mut text = String::new();
        text.push_str("# Heading\n\n| H1 | H2 |\n|---|---|\n| a | b |\n\n");
        for i in 0..5000 {
            text.push_str(&format!("Line {}\n", i));
        }
        text.push_str("\n| ColA | ColB |\n|---|---|\n| 1 | 2 |\n");

        let mut rope = Rope::from(text);
        let mut idx = TableIndex::new();
        idx.scan_rope(1, &rope);

        assert_eq!(idx.blocks.len(), 2);
        assert_eq!(idx.blocks[0].first_line, 2);
        assert_eq!(idx.blocks[0].last_line, 4);
        assert_eq!(idx.blocks[1].first_line, 5007);
        assert_eq!(idx.blocks[1].last_line, 5009);

        // Relative lines indexing works
        let rel_lines = vec!["| H1 | H2 |", "|---|---|", "| a | b |"];
        let model = TableModel::parse(&idx.blocks[0], &rel_lines);
        assert_eq!(model.col_count(), 2);
        assert_eq!(model.rows[0], vec!["H1", "H2"]);
        assert_eq!(model.rows[1], vec!["a", "b"]);

        // Incremental line shift: insert 5 lines at line 100
        let insert_at = rope.byte_of_line(100);
        rope.insert(insert_at, "New 1\nNew 2\nNew 3\nNew 4\nNew 5\n");
        idx.apply_edit(2, 100, 5, &rope);

        assert_eq!(idx.blocks.len(), 2);
        assert_eq!(idx.blocks[0].first_line, 2);
        assert_eq!(idx.blocks[0].last_line, 4);
        assert_eq!(idx.blocks[1].first_line, 5012);
        assert_eq!(idx.blocks[1].last_line, 5014);

        // Edit inside table 0
        let cell_at = rope.byte_of_line(4);
        rope.replace(cell_at..cell_at + 9, "| a_modified | b |\n");
        idx.apply_edit(3, 4, 0, &rope);

        assert_eq!(idx.blocks.len(), 2);
        assert_eq!(idx.blocks[0].first_line, 2);
        assert_eq!(idx.blocks[0].last_line, 4);
        let rel_lines2 = vec!["| H1 | H2 |", "|---|---|", "| a_modified | b |"];
        let model2 = TableModel::parse(&idx.blocks[0], &rel_lines2);
        assert_eq!(model2.rows[1][0], "a_modified");
    }

    #[test]
    fn windowed_scan_does_not_hide_tables_from_full_scan() {
        use crop::Rope;

        let mut text = String::new();
        for i in 0..5000 {
            text.push_str(&format!("Line {}\n", i));
        }
        text.push_str("\n| A | B |\n|---|---|\n| 1 | 2 |\n");
        let rope = Rope::from(text);

        let mut idx = TableIndex::new();
        idx.scan_window(1, &rope, 0, 100);
        assert!(!idx.fully_scanned);
        assert!(idx.at(5001).is_none());

        idx.scan_rope(1, &rope);
        assert!(idx.fully_scanned);
        assert_eq!(idx.blocks.len(), 1);
        assert!(idx.at(5001).is_some());
    }
