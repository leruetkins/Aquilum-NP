    use super::*;

    #[test]
    fn test_toggle_inline_bold_empty_selection_inserts_and_removes() {
        let text = "Hello world";
        // Cursor at 5 (after "Hello")
        let res = apply_toggle_inline(text, 5, 5, "**", "**");
        assert_eq!(res.byte_range, 5..5);
        assert_eq!(res.new_text, "****");
        assert_eq!(res.new_cursor, 7); // between ** and **

        // Second press when cursor is between ** and **
        let text_with_markers = "Hello**** world";
        let res2 = apply_toggle_inline(text_with_markers, 7, 7, "**", "**");
        assert_eq!(res2.byte_range, 5..9);
        assert_eq!(res2.new_text, "");
        assert_eq!(res2.new_cursor, 5);
    }

    #[test]
    fn test_toggle_inline_bold_with_selection() {
        let text = "Hello world";
        // Select "world" (chars 6..11)
        let res = apply_toggle_inline(text, 6, 11, "**", "**");
        assert_eq!(res.byte_range, 6..11);
        assert_eq!(res.new_text, "**world**");

        // Toggle again on "**world**"
        let bold_text = "Hello **world**";
        // Select "**world**" (chars 6..15)
        let res2 = apply_toggle_inline(bold_text, 6, 15, "**", "**");
        assert_eq!(res2.byte_range, 6..15);
        assert_eq!(res2.new_text, "world");

        // Select only "world" inside "**world**" (chars 8..13)
        let res3 = apply_toggle_inline(bold_text, 8, 13, "**", "**");
        assert_eq!(res3.byte_range, 6..15);
        assert_eq!(res3.new_text, "world");
    }

    #[test]
    fn test_toggle_inline_unicode_russian() {
        let text = "Привет мир";
        // Select the second word (chars 7..10; the first word plus its space is 7 chars)
        let res = apply_toggle_inline(text, 7, 10, "**", "**");
        let byte_start = "Привет ".len();
        let byte_end = "Привет мир".len();
        assert_eq!(res.byte_range, byte_start..byte_end);
        assert_eq!(res.new_text, "**мир**");
    }

    #[test]
    fn test_toggle_heading() {
        let text = "Introduction";
        let res_h1 = apply_toggle_heading(text, 0, 0, 1);
        assert_eq!(res_h1.new_text, "# Introduction");

        // Toggle to H2
        let res_h2 = apply_toggle_heading("# Introduction", 2, 2, 2);
        assert_eq!(res_h2.new_text, "## Introduction");

        // Toggle H2 again -> removes heading
        let res_remove = apply_toggle_heading("## Introduction", 3, 3, 2);
        assert_eq!(res_remove.new_text, "Introduction");
    }

    #[test]
    fn test_clear_formatting() {
        let text = "**Bold** and *Italic* and `code` with [link](https://test.com)";
        let res = apply_clear_formatting(text, 0, text.chars().count());
        assert_eq!(res.new_text, "Bold and Italic and code with link");
    }

    #[test]
    fn test_lists_and_tasks() {
        let text = "First item\nSecond item";
        let res_bullet = apply_list_bullet(text, 0, text.chars().count());
        assert_eq!(res_bullet.new_text, "- First item\n- Second item");

        let res_task = apply_toggle_task("- First item", 0, 0);
        assert_eq!(res_task.new_text, "- [ ] First item");

        let res_checked = apply_toggle_task("- [ ] First item", 0, 0);
        assert_eq!(res_checked.new_text, "- [x] First item");
    }

    #[test]
    fn test_indent_outdent() {
        let text = "Line 1\nLine 2";
        let indented = apply_indent(text, 0, text.chars().count());
        assert_eq!(indented.new_text, "    Line 1\n    Line 2");

        let outdented = apply_outdent(&indented.new_text, 0, indented.new_text.chars().count());
        assert_eq!(outdented.new_text, "Line 1\nLine 2");
    }

    #[test]
    fn test_russian_unicode_formatting_operations() {
        let text = "Заголовок документа\nТекст с русскими словами для проверки.";

        // 1. Highlight the target word (chars 28..36 in text)
        let ch_start = text.chars().position(|c| c == 'р').unwrap();
        let target_word = "русскими";
        let ch_end = ch_start + target_word.chars().count();
        let res_hl = apply_toggle_inline(text, ch_start, ch_end, "==", "==");
        assert_eq!(res_hl.new_text, "==русскими==");
        assert!(text.is_char_boundary(res_hl.byte_range.start));
        assert!(text.is_char_boundary(res_hl.byte_range.end));
        let mut full_after_hl = text.to_string();
        full_after_hl.replace_range(res_hl.byte_range.clone(), &res_hl.new_text);
        assert!(full_after_hl.is_char_boundary(res_hl.new_cursor));
        assert!(full_after_hl.is_char_boundary(res_hl.new_anchor.unwrap()));

        // 2. Heading-2 on the document title
        let res_h2 = apply_toggle_heading(&full_after_hl, 5, 5, 2);
        assert!(res_h2.new_text.starts_with("## Заголовок документа"));
        assert!(full_after_hl.is_char_boundary(res_h2.byte_range.start));
        assert!(full_after_hl.is_char_boundary(res_h2.byte_range.end));

        // 3. Insert table in Russian document
        let res_tbl = apply_insert_table(&full_after_hl, 10, 10);
        assert!(full_after_hl.is_char_boundary(res_tbl.byte_range.start));
        assert!(full_after_hl.is_char_boundary(res_tbl.byte_range.end));

        // 4. Clear formatting on Russian text
        let formatted_ru = "## **Жирный** и *курсив* текст";
        let res_clean = apply_clear_formatting(formatted_ru, 0, formatted_ru.chars().count());
        assert_eq!(res_clean.new_text, "Жирный и курсив текст");
    }

    #[test]
    fn test_inspect_active_formatting() {
        let text = "Here is **bold** text and # Heading";
        let active_bold = inspect_active_formatting(text, 10, 14); // inside "bold"
        assert!(active_bold.is_bold);
        assert!(!active_bold.is_italic);

        let active_h1 = inspect_active_formatting("# Heading", 2, 2);
        assert_eq!(active_h1.heading_level, Some(1));
    }

    /// Applies the code-block wrap to `selection` inside `doc`, the way
    /// `active_apply_formatting` does: only a short window around the selection is
    /// handed to the helper.
    fn code_block_doc(doc: &str, selection: &str) -> String {
        let start = doc.find(selection).expect("selection must be in the document");
        let end = start + selection.len();
        let before_tail: String = doc[..start]
            .chars()
            .rev()
            .take(8)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let after_head: String = doc[end..].chars().take(8).collect();
        let (replacement, _) = wrap_in_code_block(&before_tail, selection, &after_head);
        format!("{}{}{}", &doc[..start], replacement, &doc[end..])
    }

    #[test]
    fn test_code_block_is_separated_by_empty_lines() {
        assert_eq!(
            code_block_doc("para\ncode\nnext", "code"),
            "para\n\n```\ncode\n```\n\nnext"
        );
    }

    #[test]
    fn test_code_block_does_not_double_existing_empty_lines() {
        assert_eq!(
            code_block_doc("para\n\ncode\n\nnext", "code"),
            "para\n\n```\ncode\n```\n\nnext"
        );
    }

    #[test]
    fn test_code_block_at_document_edges() {
        assert_eq!(code_block_doc("code", "code"), "```\ncode\n```\n");
    }

    #[test]
    fn test_code_block_wrapped_mid_line() {
        assert_eq!(
            code_block_doc("text code tail", "code"),
            "text \n\n```\ncode\n```\n\n tail"
        );
    }

    #[test]
    fn test_code_block_empty_selection_keeps_a_body_line() {
        let (replacement, cursor) = wrap_in_code_block("", "", "");
        assert_eq!(replacement, "```\n\n```\n");
        assert_eq!(cursor, 4);

        let (replacement, cursor) = wrap_in_code_block("para\n", "", "\nnext");
        assert_eq!(replacement, "\n```\n\n```\n");
        assert_eq!(cursor, 5);
    }
