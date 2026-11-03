    use super::*;

    #[test]
    fn table_cell_wraps_by_words_and_keeps_raw_offsets() {
        let line = "| id | a long description made of several words here | x |";
        let cells = crate::table::split_row(line);
        let content = cells[1].content.clone();
        let headings = crate::settings::HeadingsSettings::default();
        let lines = table_cell_wrap_lines(
            line,
            content.clone(),
            false,
            &InlineReveal::None,
            &headings,
            12,
        );
        assert!(lines.len() > 1, "a cell wider than its budget must wrap");
        // Every sub-line is non-empty, in order, and lands on real line chars.
        let mut prev = content.start;
        for l in &lines {
            assert!(l.painted_end > l.painted_start);
            assert!(l.raw_start >= prev);
            assert!(l.raw_end <= content.end);
            prev = l.raw_end;
        }
        assert_eq!(lines.last().unwrap().raw_end, content.end);
        // No sub-line is longer than the budget.
        for l in &lines {
            assert!(
                l.painted_end - l.painted_start <= 12,
                "line {:?} exceeds the column budget",
                l
            );
        }
    }

    #[test]
    fn table_row_height_grows_with_wrapped_lines() {
        assert_eq!(
            TableRowVisualKind::Body { lines: 1 }.height(),
            28.0
        );
        assert_eq!(
            TableRowVisualKind::Body { lines: 3 }.height(),
            68.0
        );
        assert_eq!(
            TableRowVisualKind::Header { lines: 2 }.height(),
            62.0
        );
        assert_eq!(
            TableRowVisualKind::LastRow { lines: 4 }.height(),
            102.0
        );
        assert_eq!(TableRowVisualKind::Delimiter.height(), 1.0);
    }

    #[test]
    fn table_row_wrap_lines_uses_the_widest_cell() {
        let line = "| short | a much longer cell that needs to wrap |";
        let cells = crate::table::split_row(line);
        let ranges: Vec<std::ops::Range<usize>> =
            cells.iter().map(|c| c.content.clone()).collect();
        let headings = crate::settings::HeadingsSettings::default();
        // Generous budget: one line everywhere.
        assert_eq!(
            table_row_wrap_lines(line, &ranges, &[40, 40], None, &InlineReveal::None, &headings),
            1
        );
        // Tight budget: the long cell decides the row height.
        let lines = table_row_wrap_lines(
            line,
            &ranges,
            &[40, 8],
            None,
            &InlineReveal::None,
            &headings,
        );
        assert!(lines >= 4, "expected several wrapped lines, got {lines}");
    }

    #[test]
    fn test_selection_chars_in_line_unicode_boundary_safety() {
        // The six-char test word is 12 bytes: each char takes 2 bytes (0..2, 2..4, ..., 10..12)
        // With line_start = 20, the 4th char sits at bytes 26..28. Byte 27 is strictly inside it.
        let line = "Привет мир";
        let line_start = 20;

        // Selection ends at byte 27 (inside the 4th char at relative byte 7)
        let sel = 20..27;
        let res = selection_chars_in_line(line, line_start, Some(&sel));
        assert!(res.is_some());
        let (start_ch, end_ch) = res.unwrap();
        assert_eq!(start_ch, 0);
        // Clamps to byte 6 (the 3rd char), so 3 characters: chars 1, 2, 3
        assert_eq!(end_ch, 3);

        // Selection starts inside the 2nd char (relative byte 3) and ends after the 8th char (relative byte 15)
        let sel2 = 23..35;
        let res2 = selection_chars_in_line(line, line_start, Some(&sel2));
        assert!(res2.is_some());
        let (start_ch2, end_ch2) = res2.unwrap();
        // Clamps start from 23 to 22 (start of the 1st char), counting 1 char
        assert_eq!(start_ch2, 1);
        // Byte 35 - 20 = 15 bytes: the test word plus a space (13 bytes, 7 chars) + the 8th char (2 bytes, 1 char) = 8 chars
        assert_eq!(end_ch2, 8);

        // Selection extends beyond end of line
        let sel3 = 20..50;
        let res3 = selection_chars_in_line(line, line_start, Some(&sel3));
        assert!(res3.is_some());
        let (start_ch3, end_ch3) = res3.unwrap();
        assert_eq!(start_ch3, 0);
        assert_eq!(end_ch3, 10);
    }

    #[test]
    fn test_classify_all_headings() {
        assert_eq!(classify_line("# H1", None), LineKind::Heading1("H1"));
        assert_eq!(classify_line("## H2", None), LineKind::Heading2("H2"));
        assert_eq!(classify_line("### H3", None), LineKind::Heading3("H3"));
        assert_eq!(classify_line("#### H4", None), LineKind::Heading4("H4"));
        assert_eq!(classify_line("##### H5", None), LineKind::Heading5("H5"));
        assert_eq!(classify_line("###### H6", None), LineKind::Heading6("H6"));
        assert_eq!(classify_line("---", None), LineKind::HorizontalRule);
    }

    #[test]
    fn gfm_task_markers_classify_as_tasks() {
        let unchecked = LineKind::Task { checked: false, text: "a" };
        let checked = LineKind::Task { checked: true, text: "a" };
        assert_eq!(classify_line("- [ ] a", None), unchecked);
        assert_eq!(classify_line("* [ ] a", None), unchecked);
        assert_eq!(classify_line("+ [ ] a", None), unchecked);
        assert_eq!(classify_line("1. [ ] a", None), unchecked);
        assert_eq!(classify_line("12. [ ] a", None), unchecked);
        assert_eq!(classify_line("- [x] a", None), checked);
        assert_eq!(classify_line("- [X] a", None), checked);
        assert_eq!(classify_line("+ [X] a", None), checked);
        assert_eq!(classify_line("2. [x] a", None), checked);
        // A nested task keeps its indent in the marker, which the classification
        // itself does not carry.
        assert_eq!(classify_line("  - [ ] a", None), unchecked);
        // A checkbox needs the space after the bracket, and the list marker needs
        // its own space, otherwise the line is an ordinary list item.
        assert_eq!(
            classify_line("- [ ]a", None),
            LineKind::BulletList { indent: 0, text: "[ ]a" },
        );
        assert_eq!(
            classify_line("1. [x]a", None),
            LineKind::NumberedList { num: "1", text: "[x]a" },
        );
    }

    #[test]
    fn task_prefix_spans_indent_and_marker() {
        let task = task_prefix("  - [X] done").unwrap();
        assert_eq!(task.indent_chars, 2);
        assert_eq!(task.indent_bytes, 2);
        assert_eq!(task.list_marker, "- ");
        assert!(!task.ordered);
        assert_eq!(task.checkbox, "[X]");
        assert!(task.checked);
        assert_eq!(task.mark, 5..6);
        assert!(task.space_after);
        assert_eq!(task.chars, 8);
        assert_eq!(task.bytes, 8);

        let task = task_prefix("12. [ ] x").unwrap();
        assert_eq!(task.indent_chars, 0);
        assert_eq!(task.list_marker, "12. ");
        assert!(task.ordered);
        assert_eq!(task.checkbox, "[ ]");
        assert!(!task.checked);
        assert_eq!(task.chars, 8);

        // Any single mark counts, a line may end right after the checkbox, and a
        // bullet that merely starts with a link is not a task.
        assert!(task_prefix("- [✓] done").unwrap().checked);
        let end = task_prefix("- [ ]").unwrap();
        assert!(!end.checked);
        assert!(!end.space_after);
        assert_eq!(end.chars, 5);
        assert!(task_prefix("- [link](https://example.com)").is_none());

        assert!(task_prefix("- [ ]x").is_none());
        assert!(task_prefix("- [ab] two").is_none());
        assert!(task_prefix("1.[ ] x").is_none());
        assert!(task_prefix("- just a bullet").is_none());
    }

    #[test]
    fn task_toggle_flips_only_the_mark() {
        assert_eq!(task_toggle("- [ ] x"), Some((3..4, "x")));
        assert_eq!(task_toggle("  - [✓] x"), Some((5..8, " ")));
        assert_eq!(task_toggle("1. [x] x"), Some((4..5, " ")));
        assert_eq!(task_toggle("- [x]x"), None);
        assert_eq!(task_toggle("plain text"), None);
    }

    #[test]
    fn painted_task_prefix_is_as_wide_as_the_source() {
        let line = "  - [x] done";
        let task = task_prefix(line).unwrap();
        let idle = task_paint_prefix(line, &task, false);
        // The bullet keeps its dot and the bracket marker becomes blank box cells.
        assert_eq!(idle, format!("  {}    ", RENDERED_BULLET));
        assert_eq!(idle.chars().count(), task.chars);
        assert_eq!(task_paint_prefix(line, &task, true), "  - [x] ");

        let ordered = "1. [ ] done";
        let task = task_prefix(ordered).unwrap();
        assert_eq!(task_paint_prefix(ordered, &task, false), "1.     ");
        assert_eq!(task_paint_prefix(ordered, &task, false).chars().count(), task.chars);
    }

    #[test]
    fn test_parse_link_and_underline_and_highlight() {
        let input = "Обычный абзац с **жирным**, <u>HTML-подчёркиванием</u> и [ссылкой](https://example.com).";
        let spans = parse_inline_syntax_spans(input);

        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Bold(b) if b == "жирным")));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Underline(u) if u == "HTML-подчёркиванием")));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Link { text, url } if text == "ссылкой" && url == "https://example.com")));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Marker(m) if m == "](https://example.com)")));
    }

    #[test]
    fn bare_and_angle_urls_parse_as_links() {
        let input = "см. https://example.com/a_(b), www.site.ru. и <https://x.org>, <mail@example.com>";
        let spans = parse_inline_syntax_spans(input);
        let links: Vec<(&str, &str)> = spans
            .iter()
            .filter_map(|span| match span {
                SyntaxSpan::Link { text, url } => Some((text.as_str(), url.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            links,
            vec![
                ("https://example.com/a_(b)", "https://example.com/a_(b)"),
                ("www.site.ru", "https://www.site.ru"),
                ("https://x.org", "https://x.org"),
                ("mail@example.com", "mailto:mail@example.com"),
            ]
        );
    }

    #[test]
    fn bare_url_detection_skips_code_markdown_links_and_word_tails() {
        let input = "`https://code.io` nothttps://x.io, [https://label.io](https://target.io)";
        let spans = parse_inline_syntax_spans(input);
        let links: Vec<(&str, &str)> = spans
            .iter()
            .filter_map(|span| match span {
                SyntaxSpan::Link { text, url } => Some((text.as_str(), url.as_str())),
                _ => None,
            })
            .collect();
        // The code span keeps its URL literal, a prefix glued to a word is not a
        // link, and the markdown link is not linked a second time.
        assert_eq!(links, vec![("https://label.io", "https://target.io")]);
    }

    #[test]
    fn painted_bare_url_keeps_source_columns() {
        let line = "см. https://example.com/путь и www.site.ru";
        let kind = classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let v = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: line.chars().count(),
        };
        // An autolink has no hidden markers: every source char keeps its cell.
        let painted = row_painted_text(line, &kind, &v, false, &InlineReveal::None, &headings);
        assert_eq!(painted.columns(), line.chars().count());
        let text: String = painted.segments.iter().map(|segment| segment.text.as_str()).collect();
        assert_eq!(text, line);
    }

    #[test]
    fn angle_autolink_hides_its_brackets_like_a_markdown_link() {
        let line = "см. <https://example.com> конец";
        let kind = classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let v = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: line.chars().count(),
        };
        let idle = row_painted_text(line, &kind, &v, false, &InlineReveal::None, &headings);
        let idle_text: String = idle.segments.iter().map(|segment| segment.text.as_str()).collect();
        assert_eq!(idle_text, "см. https://example.com конец");
        let raw = row_painted_text(line, &kind, &v, false, &InlineReveal::All, &headings);
        let raw_text: String = raw.segments.iter().map(|segment| segment.text.as_str()).collect();
        assert_eq!(raw_text, line);
    }

    #[test]
    fn test_strip_inline_markdown_links() {
        let input = "Ссылка с заголовком: [Example](https://example.com \"Example Site\")";
        assert_eq!(strip_inline_markdown(input), "Ссылка с заголовком: Example");

        let partial = "Ссылка с заголовком: [Example";
        assert_eq!(strip_inline_markdown(partial), "Ссылка с заголовком: Example");

        let only_link = "[Example";
        assert_eq!(strip_inline_markdown(only_link), "Example");

        let mid_slice = "Example](https://example.com)";
        assert_eq!(strip_inline_markdown(mid_slice), "Example");
    }

    #[test]
    fn test_clean_link_url() {
        assert_eq!(clean_link_url("https://example.com \"Example Site\""), "https://example.com");
        assert_eq!(clean_link_url("<https://example.com>"), "https://example.com");
        assert_eq!(clean_link_url("https://example.com"), "https://example.com");
        assert_eq!(clean_link_url("example.com"), "https://example.com");
        assert_eq!(clean_link_url("mailto:user@test.com"), "mailto:user@test.com");
        assert_eq!(clean_link_url("../Люди/Михаил%20Валиев.md"), "../Люди/Михаил%20Валиев.md");
        assert_eq!(clean_link_url("<../Люди/Михаил Валиев.md>"), "../Люди/Михаил Валиев.md");
        assert_eq!(clean_link_url("Михаил%20Валиев.md"), "Михаил%20Валиев.md");
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("../Люди/Михаил%20Валиев.md"), "../Люди/Михаил Валиев.md");
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("simple_text.md"), "simple_text.md");
        assert_eq!(percent_decode("%D0%9C%D0%B8%D1%88%D0%B0"), "Миша");
    }

    #[test]
    fn test_obsidian_document_link_and_wikilinks() {
        let sentence = "Сегодня решил начать вести дневник в Обсидиан. [Миша](../Люди/Михаил%20Валиев.md) на меня так посмотрел, словно рублём одарил. И сказал: \"О, курва!\".";
        let spans = parse_inline_syntax_spans(sentence);
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Link { text, url } if text == "Миша" && url == "../Люди/Михаил%20Валиев.md")));

        let stripped = strip_markdown_tags(sentence);
        assert_eq!(stripped, "Сегодня решил начать вести дневник в Обсидиан. Миша на меня так посмотрел, словно рублём одарил. И сказал: \"О, курва!\".");

        // Wikilink without alias
        let wiki1 = "Привет [[Михаил Валиев]]!";
        let spans1 = parse_inline_syntax_spans(wiki1);
        assert!(spans1.iter().any(|s| matches!(s, SyntaxSpan::Link { text, url } if text == "Михаил Валиев" && url == "Михаил Валиев")));
        assert_eq!(strip_markdown_tags(wiki1), "Привет Михаил Валиев!");

        // Wikilink with alias
        let wiki2 = "Привет [[Михаил Валиев|Миша]]!";
        let spans2 = parse_inline_syntax_spans(wiki2);
        assert!(spans2.iter().any(|s| matches!(s, SyntaxSpan::Link { text, url } if text == "Миша" && url == "Михаил Валиев")));
        assert_eq!(strip_markdown_tags(wiki2), "Привет Миша!");
    }

    #[test]
    fn test_visual_col_roundtrip_with_links_and_tags() {
        let line = "A [link](https://example.com) B";
        let spans = parse_inline_syntax_spans(line);
        // On an inactive line "A link B" is displayed (length: 1 + 1 + 4 + 1 + 1 = 8 characters)
        // Source line: 31 characters
        let raw_char = 2; // Character 'l' in "link" (in the source line after "A [")
        let vis_col = row_char_to_visual_col(&spans, raw_char, false, 0);
        assert_eq!(vis_col, 2); // 'l' in the visual line "A link B"

        let recovered_raw = visual_col_to_row_char(&spans, vis_col, false, 0);
        assert_eq!(recovered_raw, raw_char);
    }

    #[test]
    fn test_okabe_ito_colors() {
        assert_eq!(HEADING_COLOR_PRESETS.len(), 8);

        // Verify all Okabe-Ito colors and exact RGB values requested by user
        let orange = find_heading_preset("orange");
        assert_eq!(orange.label, "Оранжевый");
        assert_eq!(orange.rgb, (230, 159, 0));

        let sky_blue = find_heading_preset("sky_blue");
        assert_eq!(sky_blue.label, "Голубой");
        assert_eq!(sky_blue.rgb, (86, 180, 233));

        let bluish_green = find_heading_preset("bluish_green");
        assert_eq!(bluish_green.label, "Сине-зелёный");
        assert_eq!(bluish_green.rgb, (0, 158, 115));

        let yellow = find_heading_preset("yellow");
        assert_eq!(yellow.label, "Жёлтый");
        assert_eq!(yellow.rgb, (240, 228, 66));

        let blue = find_heading_preset("blue");
        assert_eq!(blue.label, "Синий");
        assert_eq!(blue.rgb, (0, 114, 178));

        let vermilion = find_heading_preset("vermilion");
        assert_eq!(vermilion.label, "Алый");
        assert_eq!(vermilion.rgb, (213, 94, 0));

        let reddish_purple = find_heading_preset("reddish_purple");
        assert_eq!(reddish_purple.label, "Розово-фиолетовый");
        assert_eq!(reddish_purple.rgb, (204, 121, 167));

        let red = find_heading_preset("red");
        assert_eq!(red.label, "Красный");
        assert_eq!(red.hex, "#F44336");
        assert_eq!(red.rgb, (244, 67, 54));

        // Backward compatibility mappings
        assert_eq!(find_heading_preset("indigo").id, "blue");
        assert_eq!(find_heading_preset("violet").id, "reddish_purple");
        assert_eq!(find_heading_preset("teal").id, "bluish_green");
        assert_eq!(find_heading_preset("crimson").id, "red");
        assert_eq!(find_heading_preset("#F44336").id, "red");
    }

    #[test]
    fn test_visual_row_height_index_basic() {
        let index = VisualRowHeightIndex::new(vec![24.0, 38.0, 24.0]);
        assert_eq!(index.len(), 3);
        assert_eq!(index.total(), 86.0);
        assert_eq!(index.top_of(0), 0.0);
        assert_eq!(index.top_of(1), 24.0);
        assert_eq!(index.top_of(2), 62.0);
        assert_eq!(index.top_of(3), 86.0);
        assert_eq!(index.height_of(0), 24.0);
        assert_eq!(index.height_of(1), 38.0);
        assert_eq!(index.height_of(2), 24.0);
    }

    #[test]
    fn test_visual_row_height_index_v_idx_at_y() {
        let index = VisualRowHeightIndex::new(vec![24.0, 38.0, 24.0]);
        assert_eq!(index.v_idx_at_y(0.0), 0);
        assert_eq!(index.v_idx_at_y(10.0), 0);
        assert_eq!(index.v_idx_at_y(23.9), 0);
        assert_eq!(index.v_idx_at_y(24.0), 1);
        assert_eq!(index.v_idx_at_y(50.0), 1);
        assert_eq!(index.v_idx_at_y(61.9), 1);
        assert_eq!(index.v_idx_at_y(62.0), 2);
        assert_eq!(index.v_idx_at_y(85.9), 2);
        assert_eq!(index.v_idx_at_y(100.0), 2); // clamped to last row
    }

    #[test]
    fn test_visual_row_height_index_visible_range() {
        let heights = vec![24.0; 100];
        let index = VisualRowHeightIndex::new(heights);
        assert_eq!(index.total(), 2400.0);

        // scroll_top = 240.0 (row 10), viewport_h = 240.0 (10 rows), overscan = 0
        let range = index.visible_range(240.0, 240.0, 0.0);
        assert_eq!(range, 10..21);

        // with overscan of 48.0 (2 rows above and below)
        let range_overscan = index.visible_range(240.0, 240.0, 48.0);
        assert_eq!(range_overscan, 8..23);
    }

    /// Past 2^27 logical px the f32 grid is 16 px wide — coarser than a 24 px
    /// row — so a prefix sum in f32 starts spacing rows 32 px apart there while
    /// the rows are painted 24 px tall. That mismatch is exactly the gaps and
    /// overlaps seen in a 600 MB note, so the index has to stay exact at any
    /// size. 1000 px rows cross the threshold in 134k rows instead of 5.6M,
    /// which keeps the test cheap while exercising the same arithmetic.
    #[test]
    fn test_height_index_stays_exact_past_f32_grid() {
        let rows = 200_000usize;
        let row_h = 1000.0f32;
        let index = VisualRowHeightIndex::new(vec![row_h; rows]);

        assert!(index.total() > 134_217_728.0, "the test must cross 2^27");
        assert_eq!(index.total(), row_h as f64 * rows as f64, "total");

        for k in [rows - 4, rows - 3, rows - 2, rows - 1] {
            assert_eq!(index.top_of(k), row_h as f64 * k as f64, "top of row {k}");
            assert_eq!(
                index.top_of(k + 1) - index.top_of(k),
                row_h as f64,
                "pitch of row {k}"
            );
            assert_eq!(index.v_idx_at_y(index.top_of(k)), k, "lookup of row {k}");
        }

        // The last viewport must still resolve to real rows, not to the tail.
        let last = index.visible_range(index.total(), 800.0, 300.0);
        assert_eq!(last.end, rows);
        assert!(last.start < rows);
    }

    #[test]
    fn test_find_visual_row_for_cursor_large_doc() {
        let visual_rows: Vec<VisualRow> = (0..20_000)
            .map(|i| VisualRow {
                line_idx: i,
                sub_idx: 0,
                is_last_sub: true,
                char_start: 0,
                char_end: 20,
            })
            .collect();

        let (v_idx, col) = find_visual_row_for_cursor(&visual_rows, 15_432, 5).unwrap();
        assert_eq!(v_idx, 15_432);
        assert_eq!(col, 5);
    }

    // --- row_painted_text -------------------------------------------------------
    // These lock down the geometry model: the painted text must be exactly what
    // `render_live_styled_row` draws, and `line_chars` must map every painted column
    // back to the line char it came from.

    fn painted(
        line: &str,
        classified: &LineKind<'_>,
        char_start: usize,
        char_end: usize,
        sub_idx: usize,
        is_active_line: bool,
    ) -> PaintedRow {
        let v = VisualRow {
            line_idx: 0,
            sub_idx,
            is_last_sub: true,
            char_start,
            char_end,
        };
        let reveal = if is_active_line {
            InlineReveal::All
        } else {
            InlineReveal::None
        };
        row_painted_text(
            line,
            classified,
            &v,
            is_active_line,
            &reveal,
            &crate::settings::HeadingsSettings::default(),
        )
    }

    fn text_of(row: &PaintedRow) -> String {
        row.segments.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn painted_paragraph_is_the_whole_row() {
        let line = "hello";
        let row = painted(line, &classify_line(line, None), 0, 5, 0, false);
        assert_eq!(text_of(&row), "hello");
        assert_eq!(row.line_chars, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(row.columns(), 5);
    }

    #[test]
    fn painted_paragraph_drops_hidden_markers() {
        let line = "a**b**c";
        let row = painted(line, &classify_line(line, None), 0, 7, 0, false);
        assert_eq!(text_of(&row), "abc");
        // The two hidden `**` runs still consume line chars, so the columns skip them.
        assert_eq!(row.line_chars, vec![0, 3, 6, 7]);
        assert_eq!(row.columns(), 3);
    }

    #[test]
    fn painted_inline_code_keeps_its_delimiters_when_idle() {
        let line = "a `x` b";
        let kind = classify_line(line, None);

        // The delimiters keep their cells off the active line too, so the code chip has a
        // constant width and the selection rectangle does not jump when the line becomes
        // active. Every other hidden marker still collapses (see the `**` test above).
        let idle = painted(line, &kind, 0, 7, 0, false);
        assert_eq!(text_of(&idle), "a `x` b");
        assert_eq!(idle.line_chars, vec![0, 1, 2, 3, 4, 5, 6, 7]);

        let active = painted(line, &kind, 0, 7, 0, true);
        assert_eq!(text_of(&active), "a `x` b");
        assert_eq!(active.line_chars, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn inline_code_run_length_matches_the_closing_delimiter() {
        // A double-backtick span holds literal single backticks; a single one inside it
        // must not close it.
        let spans = parse_inline_syntax_spans("a ``b`c`` d");
        let codes: Vec<&str> = spans
            .iter()
            .filter_map(|s| match s {
                SyntaxSpan::InlineCode(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(codes, vec!["b`c"]);
        let markers: Vec<&str> = spans
            .iter()
            .filter_map(|s| match s {
                SyntaxSpan::Marker(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(markers, vec!["``", "``"]);
    }

    #[test]
    fn an_unmatched_backtick_run_stays_literal() {
        // No run of the same length: the backticks are text, and the lone pair after it
        // still forms a span. The old single-backtick reader turned `` into an empty span.
        let spans = parse_inline_syntax_spans("a `` b `c`");
        let codes: Vec<&str> = spans
            .iter()
            .filter_map(|s| match s {
                SyntaxSpan::InlineCode(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(codes, vec!["c"]);
        let text: String = spans
            .iter()
            .map(|s| match s {
                SyntaxSpan::Text(t) | SyntaxSpan::InlineCode(t) | SyntaxSpan::Marker(t) => t.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(text, "a `` b `c`");
    }

    #[test]
    fn every_backtick_run_is_a_code_delimiter() {
        assert!(is_inline_code_delim("`"));
        assert!(is_inline_code_delim("``"));
        assert!(is_inline_code_delim("```"));
        assert!(!is_inline_code_delim(""));
        assert!(!is_inline_code_delim("~"));
        assert!(!is_inline_code_delim("\\`"));
        assert!(!is_inline_code_delim("``a"));
    }

    #[test]
    fn painted_double_backtick_code_is_one_chip_over_both_delimiters() {
        // The exact line from `markdown-test.md:83`: three spans, the middle one opened
        // and closed by two backticks and holding literal backticks inside.
        let line = "Обычный текст `Встроенный код`, ``код с `обратными` кавычками внутри``, ` пробелы по краям `. Обычный текст";
        let kind = classify_line(line, None);
        let len = line.chars().count();
        let row = painted(line, &kind, 0, len, 0, false);

        // Nothing is dropped, and one painted char owns one column.
        assert_eq!(text_of(&row), line);

        let chars: Vec<char> = text_of(&row).chars().collect();
        let chips: Vec<String> = row
            .code_ranges()
            .iter()
            .map(|&(s, e)| chars[s..e].iter().collect())
            .collect();
        assert_eq!(
            chips,
            vec![
                "`Встроенный код`".to_string(),
                "``код с `обратными` кавычками внутри``".to_string(),
                "` пробелы по краям `".to_string(),
            ]
        );
    }

    #[test]
    fn a_backslash_escape_keeps_the_next_char_literal() {
        // `\`` must not open a code span and `\*` must not open emphasis: the escaped char
        // is plain text, the backslash is a marker.
        let spans = parse_inline_syntax_spans("a \\`не код\\` b");
        assert_eq!(
            spans,
            vec![
                SyntaxSpan::Text("a ".to_string()),
                SyntaxSpan::Marker("\\".to_string()),
                SyntaxSpan::Text("`".to_string()),
                SyntaxSpan::Text("не код".to_string()),
                SyntaxSpan::Marker("\\".to_string()),
                SyntaxSpan::Text("`".to_string()),
                SyntaxSpan::Text(" b".to_string()),
            ]
        );
    }

    #[test]
    fn a_non_punctuation_after_a_backslash_stays_escaped_free() {
        // `\a` is not an escape: the backslash is plain text, not a marker.
        let spans = parse_inline_syntax_spans("a \\b");
        assert_eq!(spans, vec![SyntaxSpan::Text("a \\b".to_string())]);
    }

    #[test]
    fn a_backslash_escaped_backtick_draws_no_chip() {
        let line = "\\`не код\\`";
        let kind = classify_line(line, None);
        let len = line.chars().count();

        // Neither state may produce a chip: the backticks are escaped, so they are text.
        assert!(painted(line, &kind, 0, len, 0, false).code_ranges().is_empty());
        assert!(painted(line, &kind, 0, len, 0, true).code_ranges().is_empty());

        // Idle: the backslashes are hidden, the escaped backticks stay visible as text.
        assert_eq!(text_of(&painted(line, &kind, 0, len, 0, false)), "`не код`");
        // Active: the backslashes are painted muted, so the raw syntax stays visible.
        assert_eq!(text_of(&painted(line, &kind, 0, len, 0, true)), "\\`не код\\`");
    }

    #[test]
    fn the_escaping_demo_line_opens_no_spans() {
        // `markdown-test.md:91-92` (the two lines are byte-identical).
        let line = "Экранирование: \\*не курсив\\*, \\_не курсив\\_, \\# не заголовок, \\[не ссылка\\], \\`не код\\`, \\\\ обратный слэш, \\> не цитата, \\- не список, \\| не таблица.";
        let spans = parse_inline_syntax_spans(line);
        assert!(
            spans
                .iter()
                .all(|s| matches!(s, SyntaxSpan::Text(_) | SyntaxSpan::Marker(_))),
            "no span may open on escaped punctuation: {spans:?}"
        );

        let kind = classify_line(line, None);
        let len = line.chars().count();
        let idle = painted(line, &kind, 0, len, 0, false);
        assert!(idle.code_ranges().is_empty());
        // Every `\X` collapses to `X` — including `\\` to a single backslash.
        assert_eq!(
            text_of(&idle),
            "Экранирование: *не курсив*, _не курсив_, # не заголовок, [не ссылка], `не код`, \\ обратный слэш, > не цитата, - не список, | не таблица."
        );
    }

    #[test]
    fn painted_paragraph_keeps_markers_on_the_active_line() {
        let line = "a**b**c";
        let row = painted(line, &classify_line(line, None), 0, 7, 0, true);
        assert_eq!(text_of(&row), "a**b**c");
        assert_eq!(row.line_chars, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    /// The editor paints the code chip from these column ranges instead of from a
    /// text-run background, so they must cover the delimiters as well as the code —
    /// the delimiters are what reads as the chip's padding.
    #[test]
    fn code_ranges_cover_the_chip_including_its_delimiters() {
        let line = "a `x` b";
        let kind = classify_line(line, None);

        // `a ` + the chip (`` `x` ``) + ` b`.
        assert_eq!(painted(line, &kind, 0, 7, 0, false).code_ranges(), vec![(2, 5)]);
        assert_eq!(painted(line, &kind, 0, 7, 0, true).code_ranges(), vec![(2, 5)]);

        // Two chips stay two ranges, and hidden markers before them shift the columns.
        let two = "a**b** `c` `d`";
        let two_row = painted(two, &classify_line(two, None), 0, two.chars().count(), 0, false);
        assert_eq!(text_of(&two_row), "ab `c` `d`");
        assert_eq!(two_row.code_ranges(), vec![(3, 6), (7, 10)]);

        // Adjacent spans inside one chip merge into a single range.
        let bold_code = "`a`**b**`c`";
        let bold_row = painted(bold_code, &classify_line(bold_code, None), 0, bold_code.chars().count(), 0, false);
        assert_eq!(text_of(&bold_row), "`a`b`c`");
        assert_eq!(bold_row.code_ranges(), vec![(0, 3), (4, 7)]);

        // A heading paints its marker as separate columns, so the chip moves with it.
        let heading = "## Заголовок с `кодом`";
        let heading_kind = classify_line(heading, None);
        let heading_active = painted(heading, &heading_kind, 0, heading.chars().count(), 0, true);
        let prefix = 3; // "## "
        let code_start = "## Заголовок с ".chars().count();
        assert_eq!(heading_active.code_ranges(), vec![(code_start, code_start + 7)]);
        assert!(prefix < code_start);

        // Plain text has no chip at all.
        assert!(painted("hello", &classify_line("hello", None), 0, 5, 0, false).code_ranges().is_empty());
    }

    #[test]
    fn painted_heading_hides_the_marker_until_active() {
        let line = "# Title";
        let kind = classify_line(line, None);

        let idle = painted(line, &kind, 0, 7, 0, false);
        assert_eq!(text_of(&idle), "Title");
        assert_eq!(idle.line_chars, vec![2, 3, 4, 5, 6, 7]);

        let active = painted(line, &kind, 0, 7, 0, true);
        assert_eq!(text_of(&active), "# Title");
        assert_eq!(active.line_chars, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn painted_heading_continuation_has_no_marker() {
        // "# Title": line chars are '#',' ','T','i','t','l','e'.
        let line = "# Title";
        let row = painted(line, &classify_line(line, None), 4, 7, 1, true);
        assert_eq!(text_of(&row), "tle");
        assert_eq!(row.line_chars, vec![4, 5, 6, 7]);
    }

    #[test]
    fn painted_bullet_renders_a_dot_and_reveals_its_marker_when_active() {
        let line = "- item";
        let kind = classify_line(line, None);

        // Idle rows paint the rendered dot, the way the previews do.
        let first = painted(line, &kind, 0, 6, 0, false);
        assert_eq!(text_of(&first), "• item");
        assert_eq!(first.line_chars, vec![0, 1, 2, 3, 4, 5, 6]);

        // The line under the caret keeps its raw marker, so `- ` stays editable.
        let active = painted(line, &kind, 0, 6, 0, true);
        assert_eq!(text_of(&active), "- item");
        assert_eq!(
            active.line_chars, first.line_chars,
            "the dot and the marker must map to the same source chars"
        );

        // A wrapped continuation paints an indent of the same width; its padding
        // resolves to the first char of the wrapped content.
        let wrapped = painted(line, &kind, 4, 6, 1, false);
        assert_eq!(text_of(&wrapped), "  em");
        assert_eq!(wrapped.line_chars, vec![4, 4, 4, 5, 6]);
    }

    #[test]
    fn nested_bullets_paint_a_dot_at_their_own_indent() {
        // The user-facing case: 4-space nesting, mixed markers.
        let cases = [
            ("- Пункт 1", "• Пункт 1"),
            ("    - Вложенный пункт", "    • Вложенный пункт"),
            ("    * Ещё вложенный", "    • Ещё вложенный"),
        ];
        for (line, idle_text) in cases {
            let kind = classify_line(line, None);
            assert!(matches!(kind, LineKind::BulletList { .. }), "{line:?} must classify as a bullet");
            let len = line.chars().count();
            let idle = painted(line, &kind, 0, len, 0, false);
            assert_eq!(text_of(&idle), idle_text);
            let active = painted(line, &kind, 0, len, 0, true);
            assert_eq!(text_of(&active), line, "{line:?} keeps its raw marker when active");
            assert_eq!(
                idle.line_chars, active.line_chars,
                "{line:?}: the dot must not shift the caret grid"
            );
        }
    }

    #[test]
    fn nested_bullet_continuation_lines_align_with_the_item_text() {
        let line = "    - Вложенный пункт списка, который надо перенести подальше";
        let kind = classify_line(line, None);
        let rows = painted_wrap_rows(line, &kind, false, &InlineReveal::None, 12);
        assert!(rows.len() > 1, "the sample must wrap at 12 columns");
        assert!(rows[0].0.starts_with("    • "), "first row: {:?}", rows[0].0);
        for (text, columns) in rows.iter().skip(1) {
            assert!(text.starts_with("      "), "continuation row is offset under the item text: {text:?}");
            assert!(*columns <= 12, "row {text:?} paints {columns} columns");
        }
    }

    #[test]
    fn nested_bullets_get_an_indent_guide_per_level() {
        assert!(bullet_guide_columns(0, 4).is_empty(), "a top-level item has no guide");
        assert!(bullet_guide_columns(4, 0).is_empty(), "no step detected, no guides");
        assert!(bullet_guide_columns(2, 4).is_empty(), "an indent shorter than the step nests nothing");

        // Each guide sits in the centre of the ancestor dot's cell, so the line
        // hangs straight under the dot it belongs to.
        assert_eq!(bullet_guide_columns(4, 4), vec![0.5]);
        assert_eq!(bullet_guide_columns(8, 4), vec![0.5, 4.5]);
        assert_eq!(bullet_guide_columns(12, 4), vec![0.5, 4.5, 8.5]);
        // The same document written with a two-space step gets a guide per level.
        assert_eq!(bullet_guide_columns(2, 2), vec![0.5]);
        assert_eq!(bullet_guide_columns(4, 2), vec![0.5, 2.5]);
        assert_eq!(bullet_guide_columns(8, 2), vec![0.5, 2.5, 4.5, 6.5]);
        // An indent that is not a whole number of steps keeps the completed ones.
        assert_eq!(bullet_guide_columns(7, 4), vec![0.5]);

        for step in 2..=6usize {
            for level in 0..8usize {
                let indent = level * step;
                let cols = bullet_guide_columns(indent, step);
                assert_eq!(cols.len(), level, "indent {indent}, step {step}: {cols:?}");
                assert!(cols.iter().all(|&c| c < indent as f32), "indent {indent}: {cols:?}");
                assert!(cols.windows(2).all(|w| w[0] < w[1]), "indent {indent}: {cols:?}");
            }
        }
    }

    #[test]
    fn list_indent_step_follows_the_smallest_nesting_of_the_window() {
        assert_eq!(list_indent_step([]), 0);
        assert_eq!(list_indent_step(["- a", "# heading", "text"]), 0);
        assert_eq!(list_indent_step(["- a", "    - b", "        - c"]), 4);
        assert_eq!(list_indent_step(["- a", "  - b", "    - c"]), 2);
        assert_eq!(list_indent_step(["- a", "    - b", "  - sloppy"]), 2);
        assert_eq!(list_indent_step(["* a", "+ b", "   - c"]), 3);
        // Numbered rows nest the same way and set the step too.
        assert_eq!(list_indent_step(["1. a", "    2. b"]), 4);
        assert_eq!(list_indent_step(["- a", "  1. b", "    2. c"]), 2);
        // A one-column indent cannot nest a list item, so it cannot set the step.
        assert_eq!(list_indent_step(["- a", " - b"]), 0);
    }

    #[test]
    fn numbered_rows_guide_the_same_way_as_bullets() {
        // A numbered row hangs its guide under the parent marker, exactly like a
        // bullet row — it must not paint a line through its own number.
        let numbered = [
            Some("1. one"),
            Some("    1. nested"),
            Some("    2. sibling"),
            Some("        1. deep"),
        ];
        let bullets = [
            Some("- one"),
            Some("    - nested"),
            Some("    - sibling"),
            Some("        - deep"),
        ];
        assert_eq!(
            list_block_guide_columns(numbered),
            list_block_guide_columns(bullets),
        );
        let guides = list_block_guide_columns(numbered);
        assert!(guides[0].is_empty(), "a top-level number has no guide");
        assert_eq!(guides[1], vec![0.5], "the guide sits under the parent number");
        assert_eq!(guides[2], vec![0.5]);
        assert_eq!(guides[3], vec![0.5, 4.5]);
    }

    #[test]
    fn list_blocks_measure_their_own_step() {
        let lines = [
            Some("1. one"),
            Some("    1. nested"),
            Some("        2. deep"),
            Some(""),
            None,
            Some("1. one"),
            Some("   1. three-space"),
            Some("   2. three-space"),
        ];
        let guides = list_block_guide_columns(lines);
        assert!(guides[0].is_empty(), "a top-level number has no guide");
        assert_eq!(guides[1], vec![0.5], "the four-space block nests by four");
        assert_eq!(guides[2], vec![0.5, 4.5]);
        assert!(guides[3].is_empty());
        assert!(guides[4].is_empty());
        assert!(guides[5].is_empty());
        assert_eq!(guides[6], vec![0.5], "the three-space block keeps its own step");
        assert_eq!(guides[7], vec![0.5]);

        let bullets = [Some("- one"), Some("    - two"), Some("        - three")];
        let guides = list_block_guide_columns(bullets);
        assert_eq!(guides[1], vec![0.5], "a bullet guide hangs under the parent dot");
        assert_eq!(guides[2], vec![0.5, 4.5]);
    }

    #[test]
    fn task_rows_take_part_in_list_blocks() {
        let guides = list_block_guide_columns([
            Some("- [ ] one"),
            Some("  - [x] nested"),
            Some("    1. [ ] deep"),
        ]);
        assert!(guides[0].is_empty(), "a top-level task has no guide");
        assert_eq!(guides[1], vec![0.5], "a nested task keeps the parent guide");
        assert_eq!(guides[2], vec![0.5, 2.5], "a numbered task nests like a number");
    }

    #[test]
    fn painted_quote_hides_its_marker_until_the_line_is_edited() {
        let line = "> hi";
        let kind = classify_line(line, None);

        // Idle rows hide the marker but keep its cells, so the text sits where
        // the renderer hangs the quote bar.
        let first = painted(line, &kind, 0, 4, 0, false);
        assert_eq!(text_of(&first), "  hi");
        assert_eq!(first.line_chars, vec![0, 1, 2, 3, 4]);

        // The caret line shows the marker again, in exactly the same cells.
        let active = painted(line, &kind, 0, 4, 0, true);
        assert_eq!(text_of(&active), "> hi");
        assert_eq!(active.line_chars, first.line_chars);

        let wrapped = painted(line, &kind, 3, 4, 1, false);
        assert_eq!(text_of(&wrapped), "  i");
        assert_eq!(wrapped.line_chars, vec![3, 3, 3, 4]);
    }

    #[test]
    fn quote_prefix_counts_every_nesting_level() {
        assert_eq!(quote_prefix("> a").map(|q| (q.depth, q.chars)), Some((1, 2)));
        assert_eq!(quote_prefix("> > a").map(|q| (q.depth, q.chars)), Some((2, 4)));
        assert_eq!(quote_prefix("> > > a").map(|q| (q.depth, q.chars)), Some((3, 6)));
        // Bare `>` closes a paragraph inside the quote; it is still a level.
        assert_eq!(quote_prefix(">").map(|q| (q.depth, q.chars)), Some((1, 1)));
        assert_eq!(quote_prefix("> >").map(|q| (q.depth, q.chars)), Some((2, 3)));
        // `>` opens a level with or without the space markdown allows after it.
        assert_eq!(quote_prefix(">a").map(|q| (q.depth, q.chars)), Some((1, 1)));
        assert_eq!(quote_prefix(""), None);
        assert_eq!(quote_prefix("plain"), None);
        // An indented marker is not a quote at line start.
        assert_eq!(quote_prefix("  > a"), None);
    }

    #[test]
    fn nested_quote_paints_one_marker_pair_per_level() {
        let line = "> > > deep";
        let kind = classify_line(line, None);
        assert_eq!(kind, LineKind::Quote("deep"));

        let idle = painted(line, &kind, 0, 10, 0, false);
        assert_eq!(text_of(&idle), "      deep");
        assert_eq!(idle.line_chars, (0..=10).collect::<Vec<_>>());

        let active = painted(line, &kind, 0, 10, 0, true);
        assert_eq!(text_of(&active), "> > > deep");
        assert_eq!(active.line_chars, idle.line_chars);
    }

    #[test]
    fn a_bare_quote_marker_keeps_its_own_row() {
        assert_eq!(classify_line(">", None), LineKind::Quote(""));
        assert_eq!(classify_line("> >", None), LineKind::Quote(""));

        let line = ">";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, 1, 0, false);
        assert_eq!(text_of(&row), " ");
        assert_eq!(row.line_chars, vec![0, 1]);
    }

    #[test]
    fn quote_bars_sit_on_the_level_markers() {
        assert_eq!(quote_bar_columns("> "), vec![0.0]);
        assert_eq!(quote_bar_columns("> > "), vec![0.0, 2.0]);
        assert_eq!(quote_bar_columns("> > > "), vec![0.0, 2.0, 4.0]);
        // Markers written without their spaces stay off the text.
        assert_eq!(quote_bar_columns(">>> "), vec![0.0, 1.0, 2.0]);
        assert_eq!(quote_bar_columns(""), Vec::<f32>::new());
    }

    #[test]
    fn quote_prefix_costs_the_same_whether_hidden_or_revealed() {
        for line in ["> hi", "> > hi", "> > > hi", ">", "> >", "> > >"] {
            let kind = classify_line(line, None);
            let len = line.chars().count();
            let idle = painted(line, &kind, 0, len, 0, false);
            let active = painted(line, &kind, 0, len, 0, true);
            assert_eq!(idle.columns(), active.columns(), "{line:?}");
            assert_eq!(idle.line_chars, active.line_chars, "{line:?}");
            assert_eq!(idle.columns(), len, "{line:?}");
        }
    }

    #[test]
    fn quote_continuation_rows_keep_the_nesting_indent() {
        let line = "> > hi";
        let kind = classify_line(line, None);
        let wrapped = painted(line, &kind, 5, 6, 1, false);
        assert_eq!(text_of(&wrapped), "    i");
        assert_eq!(wrapped.line_chars, vec![5, 5, 5, 5, 5, 6]);
    }

    #[test]
    fn the_quotes_section_nests_level_by_level() {
        // One bar per level at the marker it replaces, and the text one marker
        // past the last bar — the shape the sample in the task asks for.
        let cases: &[(&str, &str, &[f32])] = &[
            ("> Обычная цитата.", "  Обычная цитата.", &[0.0]),
            (">", " ", &[0.0]),
            ("> > Вложенная цитата.", "    Вложенная цитата.", &[0.0, 2.0]),
            ("> >", "   ", &[0.0, 2.0]),
            (
                "> > > Ещё более вложенная цитата.",
                "      Ещё более вложенная цитата.",
                &[0.0, 2.0, 4.0],
            ),
        ];
        for &(line, painted_text, bars) in cases {
            let kind = classify_line(line, None);
            let len = line.chars().count();
            let row = painted(line, &kind, 0, len, 0, false);
            assert_eq!(text_of(&row), painted_text, "{line:?}");
            let prefix = quote_prefix(line).unwrap();
            assert_eq!(quote_bar_columns(&line[..prefix.bytes]), bars.to_vec(), "{line:?}");
        }
    }

    #[test]
    fn a_callout_marker_inside_a_nested_quote_stays_quote_text() {
        // Several `>` make a nested quote first of all: what follows the markers
        // is quoted text, even when it looks like a callout header, and it is
        // painted exactly as the source spells it.
        let line = "> >[!tip] Познавательная заметка";
        let kind = classify_line(line, None);
        assert_eq!(kind, LineKind::Quote("[!tip] Познавательная заметка"));
        assert_eq!(callout_prefix(line), None);
        let quote = quote_prefix(line).expect("a nested quote");
        assert_eq!(quote.depth, 2);
        assert_eq!(quote.chars, 3);
        assert_eq!(quote_bar_columns(&line[..quote.bytes]), vec![0.0, 2.0]);

        let len = line.chars().count();
        let idle = painted(line, &kind, 0, len, 0, false);
        assert_eq!(text_of(&idle), "   [!tip] Познавательная заметка");
        assert_eq!(idle.columns(), len);
        assert_eq!(continuation_prefix_cols(line, &kind), quote.chars);

        // A body line is an ordinary nested quote as well.
        let body = "> > Тело выноски";
        let body_kind = classify_line(body, None);
        assert_eq!(callout_prefix(body), None);
        let body_idle = painted(body, &body_kind, 0, body.chars().count(), 0, false);
        assert_eq!(text_of(&body_idle), "    Тело выноски");
        assert_eq!(quote_prefix(body).map(|q| q.chars), Some(4));
    }

    #[test]
    fn callout_prefix_reads_the_type_and_the_title() {
        let untitled = callout_prefix("> [!note]").expect("a note callout");
        assert_eq!(untitled.kind, CalloutKind::Note);
        assert_eq!(untitled.label, "note");
        assert_eq!(untitled.chars, 9);
        assert_eq!(untitled.title, "");

        let titled = callout_prefix("> [!info] Заголовок выноски").expect("an info callout");
        assert_eq!(titled.kind, CalloutKind::Info);
        assert_eq!(titled.title, "Заголовок выноски");
        // The marker region takes the one space markdown allows after it, so the
        // title starts in the column the source gives it.
        assert_eq!(&"> [!info] Заголовок выноски"[..titled.bytes], "> [!info] ");
        assert_eq!(titled.chars, 10);

        // Obsidian's aliases keep their family.
        assert_eq!(callout_prefix("> [!warning]").unwrap().kind, CalloutKind::Warning);
        assert_eq!(callout_prefix("> [!NOTE]").unwrap().kind, CalloutKind::Note);
        assert_eq!(callout_prefix("> [!danger]").unwrap().kind, CalloutKind::Danger);
        assert_eq!(callout_prefix("> [!tip]").unwrap().kind, CalloutKind::Tip);
        assert_eq!(callout_prefix("> [!quote]").unwrap().kind, CalloutKind::Quote);

        // Not callouts: a plain quote, an unknown type, an empty one, a marker that
        // is not in a quote.
        assert_eq!(callout_prefix("> plain"), None);
        assert_eq!(callout_prefix("> [!nope] title"), None);
        assert_eq!(callout_prefix("> []"), None);
        assert_eq!(callout_prefix("[!note]"), None);
    }

    #[test]
    fn painted_callout_header_shows_its_type_where_the_marker_was() {
        // An untitled callout takes its label as its title: the marker cells become
        // the type's icon, `Note` and padding, exactly as wide as the source marker.
        let line = "> [!note]";
        let kind = classify_line(line, None);
        assert_eq!(kind, LineKind::Quote("[!note]"));

        let idle = painted(line, &kind, 0, 9, 0, false);
        assert_eq!(text_of(&idle), "     Note");
        assert_eq!(idle.line_chars, vec![0, 1, 2, 2, 2, 5, 6, 7, 8, 9]);
        assert_eq!(idle.columns(), line.chars().count());

        let active = painted(line, &kind, 0, 9, 0, true);
        assert_eq!(text_of(&active), "> [!note]");
        // The icon spends two cells on the source's `[!`, so the cell-to-char map
        // differs; only the column count has to survive the reveal.
        assert_eq!(active.columns(), idle.columns());
    }

    #[test]
    fn a_titled_callout_hides_the_type_and_starts_the_title_after_the_icon() {
        let line = "> [!info] Заголовок выноски";
        let kind = classify_line(line, None);
        let len = line.chars().count();

        // The marker word is gone: only the quote indent and the icon column
        // stand between the bar and the title.
        let idle = painted(line, &kind, 0, len, 0, false);
        let title_chars = callout_prefix(line).expect("an info callout").title.chars().count();
        assert_eq!(text_of(&idle), "     Заголовок выноски");
        assert_eq!(idle.columns(), 2 + CALLOUT_ICON_CELLS + title_chars);

        // Editing the line reveals the source marker, as it does around any other
        // hidden inline marker.
        let active = painted(line, &kind, 0, len, 0, true);
        assert_eq!(text_of(&active), line);
        assert_eq!(active.columns(), len);

        // A wrapped header continues under the title, past the icon column.
        assert_eq!(continuation_prefix_cols(line, &kind), 2 + CALLOUT_ICON_CELLS);
        let wrapped = painted(line, &kind, 10, len, 1, false);
        assert_eq!(text_of(&wrapped), "     Заголовок выноски");
        assert_eq!(wrapped.columns(), 2 + CALLOUT_ICON_CELLS + len - 10);
    }

    #[test]
    fn an_untitled_callout_shows_its_localized_type() {
        let line = "> [!tip]";
        let kind = classify_line(line, None);
        let callout = callout_prefix(line).expect("a tip callout");

        // The painted placeholder is language-independent; the renderer's label
        // comes from the locale.
        let len = line.chars().count();
        assert_eq!(text_of(&painted(line, &kind, 0, len, 0, false)), "     Tip");

        assert_eq!(callout_label(&callout, "ru"), "Совет");
        assert_eq!(callout_label(&callout, "en"), "Tip");
        // Obsidian's aliases are translated under the type they stand for.
        let abstract_callout = callout_prefix("> [!summary]").expect("an abstract alias");
        assert_eq!(callout_label(&abstract_callout, "ru"), "Аннотация");
        // An unrecognised code falls back to the app's default locale.
        assert_eq!(callout_label(&callout, "xx"), "Совет");
    }

    #[test]
    fn a_callout_header_hands_the_title_to_the_inline_spans() {
        // The label is block syntax; the title is content, so inline formatting in
        // it parses as usual.
        let line = "> [!info] **Заголовок**";
        let (spans, offset) = line_inline_spans(line, &classify_line(line, None));
        assert_eq!(offset, 10);
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Bold(_))));

        let (_, offset) = line_inline_spans("> hi", &classify_line("> hi", None));
        assert_eq!(offset, 2);
    }

    #[test]
    fn callout_label_is_the_type_in_title_case() {
        let leading = 2 + CALLOUT_ICON_CELLS;
        let untitled = callout_prefix("> [!note]").unwrap();
        // What the marker has left after the quote and the icon column: `[!note]`
        // leaves four cells, and `Note` takes exactly those.
        assert_eq!(callout_label_padded(&untitled, leading), "Note");

        let titled = callout_prefix("> [!info] Заголовок выноски").unwrap();
        // The region includes the space after the marker, so five cells remain.
        assert_eq!(callout_label_padded(&titled, leading), "Info ");
    }

    #[test]
    fn callout_colours_separate_the_families_in_both_themes() {
        let kinds = [
            CalloutKind::Note,
            CalloutKind::Tip,
            CalloutKind::Warning,
            CalloutKind::Danger,
        ];
        for &is_dark in &[true, false] {
            for (i, a) in kinds.iter().enumerate() {
                for b in &kinds[i + 1..] {
                    assert_ne!(
                        callout_color(*a, is_dark),
                        callout_color(*b, is_dark),
                        "{a:?} and {b:?} share a colour (dark: {is_dark})"
                    );
                }
            }
        }
        assert_ne!(
            callout_color(CalloutKind::Note, true),
            callout_color(CalloutKind::Note, false)
        );
    }

    #[test]
    fn a_callout_block_covers_its_quote_and_marks_both_ends() {
        let lines: Vec<String> = [
            "> [!note] Title",
            "> first body line",
            ">",
            "> last body line",
            "",
            "after",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let states = vec![None; lines.len()];
        let blocks = callout_blocks(&lines, &states);

        let first = blocks[0].expect("the header opens a block");
        assert_eq!(first.kind, CalloutKind::Note);
        assert!(first.first);
        assert!(!first.last);
        assert_eq!(blocks[1], Some(CalloutBlock { kind: CalloutKind::Note, first: false, last: false }));
        assert_eq!(blocks[2], Some(CalloutBlock { kind: CalloutKind::Note, first: false, last: false }));
        let last = blocks[3].expect("the last quoted row closes the block");
        assert_eq!(last.kind, CalloutKind::Note);
        assert!(!last.first);
        assert!(last.last);
        assert_eq!(blocks[4], None, "a blank line ends the quote");
        assert_eq!(blocks[5], None);
    }

    #[test]
    fn a_single_row_callout_is_both_ends() {
        let lines = vec!["> [!tip]".to_string()];
        let blocks = callout_blocks(&lines, &[None]);
        assert_eq!(blocks[0], Some(CalloutBlock { kind: CalloutKind::Tip, first: true, last: true }));
    }

    #[test]
    fn a_second_header_opens_its_own_block() {
        let lines: Vec<String> = [
            "> [!note] one",
            "> body",
            "> [!warning] two",
            "> body",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let blocks = callout_blocks(&lines, &vec![None; lines.len()]);
        assert_eq!(blocks[0].unwrap().kind, CalloutKind::Note);
        assert!(blocks[1].unwrap().last, "the next header ends the first block");
        assert_eq!(blocks[2].unwrap().kind, CalloutKind::Warning);
        assert!(blocks[2].unwrap().first);
        assert!(blocks[3].unwrap().last);
    }

    #[test]
    fn a_nested_quote_opens_no_callout_block() {
        let lines: Vec<String> = [
            "> >[!tip] Познавательная заметка",
            "> > тело цитаты",
            "> [!note] верхняя выноска",
            "> тело выноски",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let blocks = callout_blocks(&lines, &vec![None; lines.len()]);

        assert_eq!(blocks[0], None, "several `>` are a nested quote, not a callout");
        assert_eq!(blocks[1], None);
        assert_eq!(blocks[2].unwrap().kind, CalloutKind::Note);
        assert!(blocks[2].unwrap().first);
        assert!(blocks[3].unwrap().last);

        // Deeper rows stay in an enclosing top-level callout's block.
        let nested_inside: Vec<String> = [
            "> [!tip]",
            "> > вложенная цитата внутри выноски",
            "> обычное тело",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let blocks = callout_blocks(&nested_inside, &vec![None; nested_inside.len()]);
        assert!(blocks.iter().all(Option::is_some));
    }

    #[test]
    fn a_fenced_callout_marker_opens_no_block() {
        let lines: Vec<String> = [
            "```",
            "> [!note]",
            "```",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let states = analyze_code_blocks(&lines.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        assert!(states.iter().all(|s| s.is_some()));
        let blocks = callout_blocks(&lines, &states);
        assert!(blocks.iter().all(Option::is_none));
    }

    #[test]
    fn painted_code_line_keeps_raw_markers() {
        // Code rows paint the raw slice, so `**` stays visible even on an idle line.
        let line = "**x**";
        let row = painted(line, &LineKind::CodeLine(line), 0, 5, 0, false);
        assert_eq!(text_of(&row), "**x**");
        assert_eq!(row.line_chars, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn painted_empty_line_is_one_blank_cell() {
        let row = painted("", &classify_line("", None), 0, 0, 0, false);
        assert_eq!(text_of(&row), " ");
        assert_eq!(row.line_chars, vec![0, 0]);
        assert_eq!(row.columns(), 1);
    }

    #[test]
    fn painted_rule_follows_the_raw_line() {
        // A collapsed rule paints no text, but the geometry must still follow the line
        // so that a click resolves to a character.
        let line = "---";
        let row = painted(line, &LineKind::HorizontalRule, 0, 3, 0, false);
        assert_eq!(text_of(&row), "---");
        assert_eq!(row.line_chars, vec![0, 1, 2, 3]);
    }

    #[test]
    fn task_checkbox_reveals_only_while_the_caret_touches_it() {
        let line = "- [ ] buy milk";
        let kind = classify_line(line, None);
        // The prefix is six cells: "- [ ] ".
        assert!(!inline_reveal(line, &kind, Some(8), None, false).shows(0, 6));
        assert!(!inline_reveal(line, &kind, Some(6), None, false).shows(0, 6));
        assert!(inline_reveal(line, &kind, Some(3), None, false).shows(0, 6));
        assert!(inline_reveal(line, &kind, Some(0), None, false).shows(0, 6));
        assert!(inline_reveal(line, &kind, None, Some((0, 4)), false).shows(0, 6));
        assert!(!inline_reveal(line, &kind, None, Some((7, 10)), false).shows(0, 6));
        assert!(inline_reveal(line, &kind, None, None, true).shows(0, 6));
    }

    #[test]
    fn painted_task_hides_the_box_but_keeps_its_indent_and_number() {
        // An idle row draws the checkbox itself: the bracket marker is blank,
        // while the numbered marker stays. The line being edited paints its raw
        // source.
        let line = "   12. [X] done";
        let kind = classify_line(line, None);
        let chars = line.chars().count();
        let row = painted(line, &kind, 0, chars, 0, false);
        assert_eq!(text_of(&row), "   12.     done");
        assert_eq!(row.line_chars, (0..=chars).collect::<Vec<_>>());

        let row = painted(line, &kind, 0, chars, 0, true);
        assert_eq!(text_of(&row), line);

        // A continuation row pads the whole prefix, indentation included.
        let row = painted(line, &kind, 11, chars, 1, false);
        assert_eq!(text_of(&row), format!("{}done", " ".repeat(11)));
    }

    #[test]
    fn painted_row_columns_always_match_line_chars() {
        let cases: &[(&str, bool)] = &[
            ("plain text", false),
            ("plain text", true),
            ("**bold** and _italic_", false),
            ("**bold** and _italic_", true),
            ("# Heading with emoji 😀 tail", false),
            ("# Heading with emoji 😀 tail", true),
            ("- bullet with 🚀 emoji", false),
            ("> quote with ⚠️ emoji", false),
            ("- [ ] task with 🏳️‍🌈 flag", false),
            ("  - [x] nested task", false),
            ("1. [ ] numbered task", false),
            ("12. [X] numbered task", true),
            ("1. numbered", false),
        ];
        for &(line, active) in cases {
            let kind = classify_line(line, None);
            let len = line.chars().count();
            let row = painted(line, &kind, 0, len, 0, active);
            assert_eq!(
                row.line_chars.len(),
                row.columns() + 1,
                "line_chars/columns mismatch for {line:?} (active={active})"
            );
            assert_eq!(
                text_of(&row).chars().count(),
                row.columns(),
                "painted text length mismatch for {line:?} (active={active})"
            );
            // Monotone non-decreasing, so the reverse lookup can binary search.
            assert!(
                row.line_chars.windows(2).all(|w| w[0] <= w[1]),
                "line_chars is not monotone for {line:?} (active={active})"
            );
        }
    }

    #[test]
    fn test_heading_row_height_multiples_of_four_and_integer_pixels() {
        let scales = [1.0f32, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0];
        // Standard heading sizes from default settings: h1=22, h2=19, h3=17, h4=15, h5=14, h6=13
        for h_size in [22.0f32, 19.0, 17.0, 15.0, 14.0, 13.0] {
            let row_h = heading_row_height(h_size, 24.0);
            assert_eq!(
                row_h % 4.0,
                0.0,
                "heading row_h {row_h} for h_size {h_size} must be a multiple of 4"
            );
            assert!(
                row_h >= 24.0,
                "heading row_h {row_h} must be at least base_line_h"
            );
            for scale in scales {
                let physical = row_h * scale;
                assert_eq!(
                    physical,
                    physical.round(),
                    "physical pixels {physical} must be an exact integer at scale {scale}"
                );
            }
        }
    }

    #[test]
    fn test_strip_markdown_tags() {
        assert_eq!(strip_markdown_tags("# Header 1"), "Header 1");
        assert_eq!(strip_markdown_tags("### Subheader"), "Subheader");
        assert_eq!(strip_markdown_tags("Text with **bold** and *italic* and `code`"), "Text with bold and italic and code");
        assert_eq!(strip_markdown_tags("Link: [Click](https://example.com) and ==highlight=="), "Link: Click and highlight");
        assert_eq!(strip_markdown_tags("<u>Underlined</u> text"), "Underlined text");
        assert_eq!(strip_markdown_tags("> Quote line"), "Quote line");
        assert_eq!(strip_markdown_tags("> > Nested quote line"), "Nested quote line");
        assert_eq!(strip_markdown_tags("> > > Deep quote line"), "Deep quote line");
        // A callout header previews as its title, not as its marker.
        assert_eq!(strip_markdown_tags("> [!info] **Title**"), "Title");

        let code = "```rust\nlet x = 10;\nprintln!(\"{x}\");\n```";
        assert_eq!(strip_markdown_tags(code), "let x = 10;\nprintln!(\"{x}\");");

        let table = "| Col 1 | Col 2 |\n|---|---|\n| **Val 1** | *Val 2* |";
        assert_eq!(strip_markdown_tags(table), "Col 1\tCol 2\nVal 1\tVal 2");

        // Partial selection cleanup
        assert_eq!(strip_markdown_tags("**Partial bold"), "Partial bold");
        assert_eq!(strip_markdown_tags("Partial bold**"), "Partial bold");
        assert_eq!(strip_markdown_tags("`partial code"), "partial code");
    }

    #[test]
    fn test_parse_tags() {
        let text = "Сегодня я встретил нескольких старых знакомых: #гончаров, #сергей, #валиев и #миша.";
        let spans = parse_inline_syntax_spans(text);
        let tags: Vec<String> = spans
            .into_iter()
            .filter_map(|s| match s {
                SyntaxSpan::Tag(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(tags, vec!["#гончаров", "#сергей", "#валиев", "#миша"]);

        // Not tags: # followed by space (headings), or mid-word foo#bar
        let non_tags = parse_inline_syntax_spans("foo#bar # and #123");
        let non_tags_found: Vec<String> = non_tags
            .into_iter()
            .filter_map(|s| match s {
                SyntaxSpan::Tag(t) => Some(t),
                _ => None,
            })
            .collect();
        assert!(non_tags_found.is_empty(), "Expected no tags, found: {:?}", non_tags_found);
    }

    #[test]
    fn test_tag_colors() {
        // Determinism: leading '#' and case do not alter color
        let (bg1, fg1) = tag_color_pair("#Rust", false);
        let (bg2, fg2) = tag_color_pair("rust", false);
        assert_eq!(bg1, bg2);
        assert_eq!(fg1, fg2);

        // Different tags map to the palette deterministically
        let (bg_dark, fg_dark) = tag_color_pair("#project/work", true);
        assert!(TAG_PALETTE_DARK_HEX.iter().any(|&(b, f)| gpui::rgba(b) == bg_dark && gpui::rgba(f) == fg_dark));

        let words = ["миша", "гончаров", "сергей", "валиев", "todo", "obsidian", "dev", "notes"];
        let mut distinct = std::collections::HashSet::new();
        for w in words {
            let c = tag_color_pair(w, false).0;
            distinct.insert((c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()));
        }
        assert!(distinct.len() >= 5, "Expected good distribution across catalog, got {}", distinct.len());
    }

    #[test]
    fn test_painted_row_tag_ranges() {
        let line = "Привет #миша и #гончаров!";
        let char_count = line.chars().count();
        let painted_row = painted(line, &classify_line(line, None), 0, char_count, 0, false);
        let ranges = painted_row.tag_ranges();
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].2, "#миша");
        assert_eq!(ranges[1].2, "#гончаров");

        let full_text: String = painted_row.segments.iter().map(|s| s.text.as_str()).collect();
        let chars: Vec<char> = full_text.chars().collect();
        let s0 = ranges[0].0;
        let e0 = ranges[0].1;
        let s1 = ranges[1].0;
        let e1 = ranges[1].1;
        let tag0_str: String = chars[s0..e0].iter().collect();
        let tag1_str: String = chars[s1..e1].iter().collect();
        assert_eq!(tag0_str, "#миша");
        assert_eq!(tag1_str, "#гончаров");
    }

    #[test]
    fn inline_element_ranges_cover_markers_and_content() {
        let spans = parse_inline_syntax_spans("[a](u) **b** `c`");
        assert_eq!(inline_element_ranges(&spans), vec![(0, 6), (7, 12), (13, 16)]);
    }

    #[test]
    fn reveal_only_the_element_under_the_caret() {
        let line = "x [a](u) y **b**";
        let kind = classify_line(line, None);

        let caret_in_link = inline_reveal(line, &kind, Some(5), None, false);
        assert!(caret_in_link.shows(2, 3), "link opener must be visible");
        assert!(!caret_in_link.shows(11, 13), "bold markers must stay hidden");

        assert_eq!(
            inline_reveal(line, &kind, Some(1), None, false),
            InlineReveal::None,
            "a caret in plain text reveals nothing"
        );
        assert_eq!(
            inline_reveal(line, &kind, Some(5), None, true),
            InlineReveal::All,
            "Alt+click reveals the whole line"
        );
    }

    #[test]
    fn caret_at_the_visible_end_of_an_element_reveals_it() {
        // The closing "](url)" is hidden, so a click right after "abc" maps to
        // the raw char after it: the element's end boundary. It has to count as
        // inside, or the element collapses under the caret exactly where the
        // text ends.
        let line = "x [abc](https://example.com) y";
        let kind = classify_line(line, None);
        let element_end = "x [abc](https://example.com)".chars().count();

        assert_eq!(
            inline_reveal(line, &kind, Some(element_end), None, false),
            InlineReveal::Ranges(vec![(2, element_end)])
        );

        // One char further is plain text and hides the markup again.
        assert_eq!(
            inline_reveal(line, &kind, Some(element_end + 1), None, false),
            InlineReveal::None
        );
    }

    #[test]
    fn selection_reveals_every_touched_element() {
        let line = "x [a](u) y **b**";
        let kind = classify_line(line, None);
        let reveal = inline_reveal(line, &kind, None, Some((7, 12)), false);
        assert!(reveal.shows(2, 3), "the selection starts inside the link");
        assert!(reveal.shows(11, 13), "the selection reaches the bold markers");
    }

    #[test]
    fn reveal_offsets_include_the_block_prefix() {
        let line = "# x [a](u)";
        let kind = classify_line(line, None);
        let reveal = inline_reveal(line, &kind, Some(5), None, false);
        // The link's `[` sits at line char 4 (after the "# " block prefix), not 2.
        assert!(reveal.shows(4, 5));

        let headings = crate::settings::HeadingsSettings::default();
        let v = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: line.chars().count(),
        };
        let row = row_painted_text(line, &kind, &v, true, &reveal, &headings);
        assert_eq!(text_of(&row), "# x [a](u)");
    }

    #[test]
    fn reveal_controls_painted_markers() {
        let line = "x [a](u) y **b**";
        let kind = classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let v = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: line.chars().count(),
        };

        let hidden = row_painted_text(line, &kind, &v, false, &InlineReveal::None, &headings);
        assert_eq!(text_of(&hidden), "x a y b");

        let link = row_painted_text(
            line,
            &kind,
            &v,
            false,
            &inline_reveal(line, &kind, Some(5), None, false),
            &headings,
        );
        assert_eq!(text_of(&link), "x [a](u) y b");

        let all = row_painted_text(line, &kind, &v, false, &InlineReveal::All, &headings);
        assert_eq!(text_of(&all), "x [a](u) y **b**");
    }

    /// Wrap `line` by painted width and return what each row actually paints.
    /// Going through `row_painted_text` is the point: the test would fail if the
    /// wrap and the renderer's geometry ever disagreed.
    fn painted_wrap_rows(
        line: &str,
        kind: &LineKind<'_>,
        active: bool,
        reveal: &InlineReveal,
        cols: usize,
    ) -> Vec<(String, usize)> {
        let headings = crate::settings::HeadingsSettings::default();
        let metrics = painted_line_metrics(line, kind, active, reveal, &headings);
        let ranges = wrap_line_ranges_painted(line, &metrics, cols);
        let count = ranges.len();
        ranges
            .iter()
            .enumerate()
            .map(|(sub_idx, &(char_start, char_end))| {
                let v = VisualRow {
                    line_idx: 0,
                    sub_idx,
                    is_last_sub: sub_idx + 1 == count,
                    char_start,
                    char_end,
                };
                let row = row_painted_text(line, kind, &v, active, reveal, &headings);
                let text: String = row.segments.iter().map(|s| s.text.as_str()).collect();
                (text, row.columns())
            })
            .collect()
    }

    #[test]
    fn hidden_url_takes_no_columns_when_wrapping() {
        let line = "слово [ссылка](https://example.com/очень/длинный/путь) конец";
        let kind = classify_line(line, None);

        let canonical = painted_wrap_rows(line, &kind, false, &InlineReveal::None, 30);
        assert_eq!(canonical.len(), 1, "the hidden URL must not force a second row");
        assert_eq!(canonical[0].0, "слово ссылка конец");

        // The same line with the link revealed pays for the URL and wraps.
        let revealed = painted_wrap_rows(line, &kind, false, &InlineReveal::All, 30);
        assert!(revealed.len() > 1);
    }

    #[test]
    fn revealed_link_wraps_between_label_and_url() {
        let line = "1. Добавить передачу формы курсора из rfbClient.ts во вьюпорт [RfbViewport.vue](file:///d:/Naben/Github/rfb-cloud/web/apps/app/src/components/RfbViewport.vue).";
        let kind = classify_line(line, None);

        // When revealed, with wrap columns set to 90, the label fits on the first line,
        // and the url wraps to the second line.
        let rows = painted_wrap_rows(line, &kind, true, &InlineReveal::All, 90);
        assert!(rows.len() >= 2);
        assert!(rows[0].0.ends_with("[RfbViewport.vue]"));
        assert!(rows[1].0.contains("(file:///d:/Naben/Github/rfb-cloud"));

        // When hidden, the entire line fits in 90 columns because the URL is collapsed.
        let hidden = painted_wrap_rows(line, &kind, false, &InlineReveal::None, 90);
        assert_eq!(hidden.len(), 1);
    }

    #[test]
    fn wrap_line_ranges_breaks_between_link_label_and_url() {
        let line = "prefix [label](https://example.com/very/long/url)";
        let ranges = wrap_line_ranges(line, 40);
        assert_eq!(ranges.len(), 2);
        let chars: Vec<char> = line.chars().collect();
        let row0: String = chars[ranges[0].0..ranges[0].1].iter().collect();
        let row1: String = chars[ranges[1].0..ranges[1].1].iter().collect();
        assert_eq!(row0, "prefix [label]");
        assert_eq!(row1, "(https://example.com/very/long/url)");
    }

    #[test]
    fn painted_columns_skip_hidden_markers() {
        let line = "x [abc](https://example.com) y";
        let kind = classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let char_count = line.chars().count();
        let v = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: char_count,
        };
        let painted = row_painted_text(line, &kind, &v, false, &InlineReveal::None, &headings);

        // "x abc y": the "[" and the whole "(url)" take no cell.
        assert_eq!(painted.columns(), 7);

        // Raw chars: 0 'x', 1 ' ', 2 '[', 3 'a'.
        assert_eq!(painted_column_of_line_char(&painted, 3), 2);
        // A hidden marker shares the painted column of the text after it, so
        // vertical motion does not drift on the caret line.
        assert_eq!(painted_column_of_line_char(&painted, 2), 2);
        // Round trip to the raw char the caret must land on.
        assert_eq!(line_char_at_painted_column(&painted, 2), 3);
        assert_eq!(
            line_char_at_painted_column(&painted, painted.columns()),
            char_count
        );
    }

    #[test]
    fn click_at_the_end_of_a_word_lands_inside_the_element() {
        let line = "x [abc](https://example.com) y";
        let kind = classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let v = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: line.chars().count(),
        };
        let painted = row_painted_text(line, &kind, &v, false, &InlineReveal::None, &headings);
        // Painted cells: "x abc y".
        assert_eq!(painted.columns(), 7);

        // The boundary after "abc" must not jump over the hidden "](url)":
        // the caret has to stay inside the link source, or the element
        // collapses exactly where its visible text ends.
        let after_word = line_char_for_painted_boundary(&painted.line_chars, 5);
        assert_eq!(after_word, 6, "the caret must stay inside the link source");
        assert!(matches!(
            inline_reveal(line, &kind, Some(after_word), None, false),
            InlineReveal::Ranges(_)
        ));

        // The start of the word is inside the element as well.
        let before_word = line_char_for_painted_boundary(&painted.line_chars, 2);
        assert_eq!(before_word, 2);
        assert!(matches!(
            inline_reveal(line, &kind, Some(before_word), None, false),
            InlineReveal::Ranges(_)
        ));

        // The row's end stays the row's end, so typing there appends after the
        // whole line instead of inside the markup.
        assert_eq!(
            line_char_for_painted_boundary(&painted.line_chars, 7),
            line.chars().count()
        );
    }

    #[test]
    fn heading_marker_is_paid_for_only_when_active() {
        let line = "# abcdefghij";
        let kind = classify_line(line, None);

        let canonical = painted_wrap_rows(line, &kind, false, &InlineReveal::None, 10);
        assert_eq!(canonical.len(), 1, "the hidden \"# \" costs no column");
        assert_eq!(canonical[0].0, "abcdefghij");

        let active = painted_wrap_rows(line, &kind, true, &InlineReveal::None, 10);
        assert_eq!(active.len(), 2, "the visible \"# \" takes two columns");
        assert_eq!(active[0].0, "# abcdefgh");
    }

    #[test]
    fn inline_code_keeps_its_cells_and_emphasis_does_not() {
        let line = "**a** `b` c";
        let kind = classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let metrics = painted_line_metrics(line, &kind, false, &InlineReveal::None, &headings);

        let widths = &metrics.char_widths;
        // `**a** `b` c`: the bold markers collapse, the backticks keep a cell.
        assert_eq!(&widths[0..2], &[0, 0], "bold opener is collapsed");
        assert_eq!(widths[2], 1, "'a' is painted");
        assert_eq!(&widths[3..5], &[0, 0], "bold closer is collapsed");
        assert_eq!(widths[6], 1, "the inline-code opener keeps its cell");
        assert_eq!(widths[7], 1, "'b' is painted");
        assert_eq!(widths[8], 1, "the inline-code closer keeps its cell");
        assert_eq!(
            widths.iter().sum::<u8>(),
            7,
            "only the four emphasis markers cost nothing"
        );
    }

    #[test]
    fn continuation_rows_pay_for_their_indent() {
        let line = "- aaaa bbbb cccc dddd eeee";
        let kind = classify_line(line, None);
        let rows = painted_wrap_rows(line, &kind, false, &InlineReveal::None, 10);
        assert!(rows.len() > 1);
        for (i, (text, columns)) in rows.iter().enumerate() {
            assert!(
                *columns <= 10,
                "row {i} paints {columns} columns ({text:?}), over the 10-column budget"
            );
        }
        assert!(rows[0].0.starts_with("• "), "the first row carries the bullet");
        assert!(rows[1].0.starts_with("  "), "continuation rows are indented");
    }

    #[test]
    fn painted_ranges_cover_the_line_contiguously() {
        let lines = [
            "обычный абзац без разметки, который надо перенести",
            "[ссылка](https://example.com/путь) и **жирный** текст",
            "# Заголовок с [ссылкой](u)",
            "- пункт списка с `кодом` и *курсивом*",
            "> цитата с очень длинной строкой, которая точно не поместится",
            "```rust",
            "    отступ и ещё немного текста для переноса",
        ];
        let headings = crate::settings::HeadingsSettings::default();
        for line in lines {
            let kind = classify_line(line, None);
            let char_count = line.chars().count();
            for cols in [1usize, 5, 10, 40] {
                for (active, reveal) in [
                    (false, InlineReveal::None),
                    (true, InlineReveal::All),
                ] {
                    let metrics =
                        painted_line_metrics(line, &kind, active, &reveal, &headings);
                    let ranges = wrap_line_ranges_painted(line, &metrics, cols);
                    assert_eq!(ranges[0].0, 0, "{line:?} cols={cols}");
                    assert_eq!(
                        ranges.last().unwrap().1,
                        char_count,
                        "{line:?} cols={cols}"
                    );
                    for pair in ranges.windows(2) {
                        assert_eq!(
                            pair[0].1, pair[1].0,
                            "ranges must be contiguous for {line:?} cols={cols}"
                        );
                    }
                    for &(s, e) in &ranges {
                        assert!(e > s, "empty range in {line:?} cols={cols}");
                    }
                }
            }
        }
    }

    #[test]
    fn paragraph_rows_never_exceed_the_budget() {
        let line = "раз два три четыре пять шесть семь восемь девять десять";
        let kind = classify_line(line, None);
        for cols in [5usize, 9, 14, 27] {
            for (text, columns) in painted_wrap_rows(line, &kind, false, &InlineReveal::None, cols) {
                assert!(
                    columns <= cols,
                    "row {text:?} paints {columns} columns over budget {cols}"
                );
            }
        }
    }

    // --- school-grammar markup --------------------------------------------------

    #[test]
    fn grammar_member_uses_its_school_line_and_paints_its_text() {
        let cases = [
            ("s", GrammarLine::Solid),
            ("p", GrammarLine::Double),
            ("d", GrammarLine::Wavy),
            ("o", GrammarLine::Dotted),
            ("a", GrammarLine::DashDot),
            ("v", GrammarLine::None),
            ("i", GrammarLine::None),
        ];
        for (tag, line_style) in cases {
            let line = format!("::{tag}[слово]");
            let kind = classify_line(&line, None);
            let row = painted(&line, &kind, 0, line.chars().count(), 0, false);
            assert_eq!(text_of(&row), "слово", "tag {tag}");
            assert_eq!(
                row.grammar_ranges(),
                vec![(
                    0,
                    5,
                    GrammarSpanStyle { line: line_style, line_color: None, text_color: None }
                )],
                "tag {tag}"
            );
        }

        // While the line is active its markup is painted, and the member starts
        // after the opening marker.
        let line = "::s[слово]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, true);
        assert_eq!(text_of(&row), "::s[слово]");
        assert_eq!(row.grammar_ranges().len(), 1);
        assert_eq!(row.grammar_ranges()[0].0, 4);
    }

    #[test]
    fn grammar_tag_colours_set_line_then_text() {
        // One colour is the line colour; the text keeps the row's colour.
        let line = "::s{#2a7}[слово]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(text_of(&row), "слово");
        assert_eq!(
            row.grammar_ranges()[0].2,
            GrammarSpanStyle {
                line: GrammarLine::Solid,
                line_color: Some((0x22, 0xaa, 0x77)),
                text_color: None,
            }
        );

        // Two colours: the first is the line, the second the text.
        let line = "::s{red/blue}[слово]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(
            row.grammar_ranges()[0].2,
            GrammarSpanStyle {
                line: GrammarLine::Solid,
                line_color: Some((255, 0, 0)),
                text_color: Some((0, 0, 255)),
            }
        );

        // A style with no line has nowhere to paint a line colour, so one colour
        // still colours the text.
        let line = "::v{green}[слово]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(
            row.grammar_ranges()[0].2,
            GrammarSpanStyle {
                line: GrammarLine::None,
                line_color: None,
                text_color: Some((0, 128, 0)),
            }
        );

        // `rgb()` is accepted, and a missing half stays unset.
        let line = "::p{rgb(1, 2, 3)/#ffffff}[слово]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(
            row.grammar_ranges()[0].2,
            GrammarSpanStyle {
                line: GrammarLine::Double,
                line_color: Some((1, 2, 3)),
                text_color: Some((255, 255, 255)),
            }
        );
        assert_eq!(parse_grammar_color("green"), Some((0, 128, 0)));
        assert_eq!(parse_grammar_color("not-a-colour"), None);
    }

    #[test]
    fn unknown_grammar_tag_stays_plain_text() {
        let line = "::x[слово] и ::s[кошка]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(text_of(&row), "::x[слово] и кошка");
        // Only the known tag produced a member.
        let ranges = row.grammar_ranges();
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].0, 13);
    }

    #[test]
    fn escaped_bracket_inside_grammar_member_is_painted_literal() {
        let line = "::s[закрывающая \\] скобка]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(text_of(&row), "закрывающая ] скобка");
        let ranges = row.grammar_ranges();
        assert_eq!(ranges.len(), 1);
        assert_eq!((ranges[0].0, ranges[0].1), (0, 20));
        // The painted `]` maps back to the source bracket, not to the backslash.
        assert_eq!(row.line_chars[12], 17);
    }

    #[test]
    fn grammar_member_paints_emphasis_inside_itself() {
        let line = "::d{red}[**наш** и *рыжий*]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(text_of(&row), "наш и рыжий");
        // Hidden nested markers take no cell, so the member's line stays unbroken.
        assert_eq!(
            row.grammar_ranges(),
            vec![(
                0,
                11,
                GrammarSpanStyle {
                    line: GrammarLine::Wavy,
                    line_color: Some((255, 0, 0)),
                    text_color: None,
                }
            )]
        );
        assert!(row.segments.iter().any(|s| s.text == "наш" && s.bold && s.grammar.is_some()));
        assert!(row.segments.iter().any(|s| s.text == "рыжий" && s.italic && s.grammar.is_some()));
        // Copying the line yields the visible text, not the nested markup.
        assert_eq!(strip_inline_markdown(line), "наш и рыжий");

        // On the active line the nested markers are painted too, still inside the member.
        let row = painted(line, &kind, 0, line.chars().count(), 0, true);
        assert_eq!(text_of(&row), line);
        let ranges = row.grammar_ranges();
        assert_eq!(ranges.len(), 1);
        assert_eq!((ranges[0].0, ranges[0].1), (9, line.chars().count() - 1));
    }

    #[test]
    fn grammar_member_paints_strikethrough_and_inline_code() {
        let line = "::o[~~старый~~ и `код`]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(text_of(&row), "старый и `код`");
        assert!(row.segments.iter().any(|s| s.text == "старый" && s.grammar.is_some()));
        assert!(row.segments.iter().any(|s| s.text == "код" && s.code && s.grammar.is_some()));
        // The chip keeps its delimiters, and the member's dotted line covers it.
        assert_eq!(row.code_ranges(), vec![(9, 14)]);
        assert_eq!(row.grammar_ranges().len(), 1);
        assert_eq!((row.grammar_ranges()[0].0, row.grammar_ranges()[0].1), (0, 14));
    }

    #[test]
    fn grammar_member_inline_markup_keeps_source_columns() {
        let line = "::s[**наш**]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(text_of(&row), "наш");
        // Hidden `**` take no cell, so each painted char maps to its source char.
        assert_eq!(row.line_chars, vec![6, 7, 8, 12]);

        // A wrapped row slices the member's content and keeps its line.
        let sliced = painted(line, &kind, 7, 9, 0, false);
        assert_eq!(text_of(&sliced), "аш");
        assert_eq!(sliced.line_chars, vec![7, 8, 9]);
        assert_eq!((sliced.grammar_ranges()[0].0, sliced.grammar_ranges()[0].1), (0, 2));
    }

    #[test]
    fn grammar_member_keeps_other_markup_literal() {
        let line = "::d[а ==выделено== б]";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(text_of(&row), "а ==выделено== б");
        assert_eq!(row.grammar_ranges().len(), 1);
    }

    #[test]
    fn grammar_tags_and_the_text_between_them_are_painted_in_order() {
        let line = "::d{green}[Маленькая] ::s{red/blue}[кошка] ::a[тихо] ::p{#2a7}[спала] ::a{orange}[на подоконнике].";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(text_of(&row), "Маленькая кошка тихо спала на подоконнике.");
        let ranges: Vec<(usize, usize)> =
            row.grammar_ranges().iter().map(|(s, e, _)| (*s, *e)).collect();
        assert_eq!(ranges, vec![(0, 9), (10, 15), (16, 20), (21, 26), (27, 41)]);
        let styles: Vec<GrammarSpanStyle> =
            row.grammar_ranges().iter().map(|(_, _, style)| *style).collect();
        assert_eq!(
            styles[0],
            GrammarSpanStyle {
                line: GrammarLine::Wavy,
                line_color: Some((0, 128, 0)),
                text_color: None,
            }
        );
        assert_eq!(
            styles[1],
            GrammarSpanStyle {
                line: GrammarLine::Solid,
                line_color: Some((255, 0, 0)),
                text_color: Some((0, 0, 255)),
            }
        );
        assert_eq!(
            styles[2],
            GrammarSpanStyle { line: GrammarLine::DashDot, line_color: None, text_color: None }
        );
        assert_eq!(
            styles[3],
            GrammarSpanStyle {
                line: GrammarLine::Double,
                line_color: Some((0x22, 0xaa, 0x77)),
                text_color: None,
            }
        );
        assert_eq!(
            styles[4],
            GrammarSpanStyle {
                line: GrammarLine::DashDot,
                line_color: Some((255, 165, 0)),
                text_color: None,
            }
        );
    }

    #[test]
    fn grammar_markup_does_not_break_markdown() {
        let text = "**жирный** *курсив* ~~зачёркивание~~ `код` [ссылка](https://example.com)";
        let spans = parse_inline_syntax_spans(text);
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Bold(t) if t == "жирный")));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Italic(t) if t == "курсив")));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Strikethrough(_))));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::InlineCode(t) if t == "код")));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Link { .. })));

        let line = "::s[кошка] и [ссылка](https://example.com)";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, line.chars().count(), 0, false);
        assert_eq!(text_of(&row), "кошка и ссылка");
    }
