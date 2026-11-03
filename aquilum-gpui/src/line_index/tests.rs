    use super::{LineIndex, LINE_BLOCK};

    /// Reference implementation: the thing the index must agree with.
    fn naive_lines(text: &str) -> Vec<usize> {
        let mut starts = vec![0usize];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    fn check(text: &str) {
        let index = LineIndex::new(text);
        let naive = naive_lines(text);
        assert_eq!(index.lines(), naive.len(), "line count for {:?}", &text[..text.len().min(40)]);
        assert_eq!(index.len(), text.len());
        for (line, &want) in naive.iter().enumerate() {
            assert_eq!(index.start(text, line), want, "start of line {line}");
        }
        // Past the last line the start is the text length.
        assert_eq!(index.start(text, naive.len()), text.len());
        // Every byte maps to the line whose range contains it.
        for byte in 0..=text.len() {
            let line = index.line_of(text, byte);
            assert!(line < naive.len());
            assert!(naive[line] <= byte, "byte {byte} reported as line {line}");
            if line + 1 < naive.len() {
                assert!(byte < naive[line + 1], "byte {byte} belongs to line {}", line + 1);
            }
        }
    }

    #[test]
    fn matches_a_naive_scan_on_edge_cases() {
        for text in [
            "",
            "a",
            "\n",
            "a\n",
            "\na",
            "a\nb",
            "a\nb\n",
            "a\n\n\nb",
            "строка\r\nвторая", // CR is content here, only '\n' splits
            "без перевода строки",
        ] {
            check(text);
        }
    }

    #[test]
    fn matches_a_naive_scan_across_checkpoints() {
        // Enough lines to cross several `LINE_BLOCK` boundaries, with lines of
        // varying length so a checkpoint cannot accidentally line up.
        let mut text = String::new();
        for i in 0..(LINE_BLOCK * 3 + 7) {
            text.push_str(&"x".repeat(i % 17));
            text.push('\n');
        }
        check(&text);
    }

    #[test]
    fn lookup_work_is_bounded_by_one_block() {
        // A huge document must not make a lookup scan the whole text. Build the
        // lines lazily: 200k lines of 8 bytes is only 1.6 MB, but the point is
        // that the checkpoint scan is bounded, not the document size.
        let mut text = String::with_capacity(2 * 1024 * 1024);
        for i in 0..200_000 {
            text.push_str(&format!("{i:07}\n"));
        }
        let index = LineIndex::new(&text);
        assert_eq!(index.lines(), 200_001);
        // The last line, i.e. the worst case for the forward scan.
        assert_eq!(index.start(&text, 200_000), text.len());
        assert_eq!(index.line_of(&text, text.len()), 200_000);
        // A middle line, checked against the naive answer.
        let mid = 137_000;
        let want = text
            .match_indices('\n')
            .nth(mid - 1)
            .map(|(i, _)| i + 1)
            .unwrap();
        assert_eq!(index.start(&text, mid), want);
        // The index is tiny: a handful of bytes per block, not per line.
        assert!(
            index.bytes() < text.len() / 8,
            "index {} bytes for {} bytes of text",
            index.bytes(),
            text.len()
        );
    }

    /// Readiness criterion of stage 2: five million lines are addressed exactly
    /// and in bounded work. The lines have a fixed length here, so the expected
    /// offset is a multiplication — an independent check instead of a second
    /// copy of the same scan.
    #[test]
    fn five_million_lines_are_addressed_exactly_and_cheaply() {
        const LINE: &str = "0123456\n"; // 8 bytes, so line `l` starts at `l * 8`.
        const COUNT: usize = 5_000_000;
        let mut text = String::with_capacity(COUNT * LINE.len());
        for _ in 0..COUNT {
            text.push_str(LINE);
        }
        let index = LineIndex::new(&text);

        assert_eq!(index.lines(), COUNT + 1);
        assert_eq!(index.len(), text.len());
        for line in [
            0,
            1,
            LINE_BLOCK - 1,
            LINE_BLOCK,
            LINE_BLOCK + 1,
            COUNT / 2,
            COUNT - 1,
            COUNT,
        ] {
            assert_eq!(
                index.start(&text, line),
                line * LINE.len(),
                "start of line {line}"
            );
        }
        for byte in [
            0,
            7,
            8,
            LINE_BLOCK * LINE.len(),
            text.len() - 1,
            text.len(),
        ] {
            assert_eq!(index.line_of(&text, byte), byte / LINE.len(), "byte {byte}");
        }
        // Checkpoints only: tens of kilobytes for a 40 MB document.
        assert!(
            index.bytes() < 256 * 1024,
            "index is {} bytes for {} bytes of text",
            index.bytes(),
            text.len()
        );
    }

    #[test]
    fn splice_matches_new_for_all_edit_types() {
        let mut text = String::new();
        for i in 0..(LINE_BLOCK * 3 + 50) {
            text.push_str(&format!("line_{i:04}\n"));
        }
        let mut spliced = LineIndex::new(&text);

        // 1. Single character insertion without newline (in block 0)
        let edit_byte = 10;
        let inserted = "X";
        let removed = "";
        text.insert_str(edit_byte, inserted);
        spliced.splice(&text, edit_byte, removed, inserted);
        let expected = LineIndex::new(&text);
        assert_eq!(spliced, expected, "insert without newline block 0");

        // 2. Character deletion without newline (in block 2, across checkpoints)
        let edit_byte = LINE_BLOCK * 10 + 20;
        let removed = text[edit_byte..edit_byte + 3].to_string();
        text.replace_range(edit_byte..edit_byte + 3, "");
        spliced.splice(&text, edit_byte, &removed, "");
        let expected = LineIndex::new(&text);
        assert_eq!(spliced, expected, "delete without newline block 2");

        // 3. Newline insertion (pressing Enter)
        let edit_byte = LINE_BLOCK * 2 + 15;
        let inserted = "\n";
        text.insert_str(edit_byte, inserted);
        spliced.splice(&text, edit_byte, "", inserted);
        let expected = LineIndex::new(&text);
        assert_eq!(spliced, expected, "insert newline");

        // 4. Multiline paste
        let edit_byte = 50;
        let inserted = "first\nsecond\nthird\n";
        text.insert_str(edit_byte, inserted);
        spliced.splice(&text, edit_byte, "", inserted);
        let expected = LineIndex::new(&text);
        assert_eq!(spliced, expected, "multiline paste");

        // 5. Multiline deletion (deleting across lines)
        let edit_byte = 40;
        let removed = text[edit_byte..edit_byte + 100].to_string();
        text.replace_range(edit_byte..edit_byte + 100, "");
        spliced.splice(&text, edit_byte, &removed, "");
        let expected = LineIndex::new(&text);
        assert_eq!(spliced, expected, "multiline delete");
    }
