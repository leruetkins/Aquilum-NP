    use super::*;
    use crate::markdown::{analyze_code_blocks, CodeBlockKind};

    /// Build a document of `pairs` fence pairs, each `body` lines long, with
    /// `filler_between` plain lines between them.
    fn fenced_document(pairs: usize, body: usize, filler_between: usize) -> String {
        let mut text = String::new();
        for i in 0..pairs {
            for _ in 0..filler_between {
                text.push_str("plain prose\n");
            }
            text.push_str("```rust\n");
            for j in 0..body {
                text.push_str(&format!("let x{j} = {i};\n"));
            }
            text.push_str("```\n");
        }
        text
    }

    fn index_of(text: &str) -> LineIndex {
        LineIndex::new(text)
    }

    /// A checkpointed scan must reproduce the one-shot scan exactly, at every
    /// line, for a document much longer than one checkpoint block.
    #[test]
    fn checkpoints_match_the_one_shot_scan() {
        let text = fenced_document(6, 1500, 1200);
        let index = index_of(&text);
        assert!(index.lines() > 3 * CHECKPOINT_LINES);

        let lines: Vec<&str> = text.split('\n').collect();
        let expected = analyze_code_blocks(&lines);
        assert_eq!(expected.len(), index.lines());

        let mut checkpoints = CodeCheckpoints::new();
        checkpoints.ensure(&text, &index, index.lines());
        assert!(checkpoints.is_complete(index.lines()));

        for line in 0..index.lines() {
            let state = checkpoints
                .state_at(&text, &index, line)
                .unwrap_or_else(|| panic!("line {line} not scanned"));
            let actual = CodeScanner::from_state(state).step(lines[line]);
            assert_eq!(actual, expected[line], "state mismatch at line {line}");
        }
    }

    /// A fence that opened far above the requested line must still be known.
    #[test]
    fn a_block_opened_far_above_is_recognised() {
        // One fence pair whose body is ~100k lines long.
        let mut text = String::from("intro\n```\n");
        for i in 0..100_000 {
            text.push_str(&format!("body {i}\n"));
        }
        text.push_str("```\noutro\n");
        let index = index_of(&text);

        let mut checkpoints = CodeCheckpoints::new();
        let target = 90_000;
        checkpoints.ensure(&text, &index, target + 1);
        assert_eq!(checkpoints.scanned_lines(), target + 1);

        let state = checkpoints.state_at(&text, &index, target).expect("scanned");
        assert!(state.in_code_block, "the window must know it is inside a block");
        assert_eq!(state.block_id, 0);

        // The window's own scan then continues correctly from there.
        let mut scanner = CodeScanner::from_state(state);
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(
            scanner.step(lines[target]).map(|s| s.kind),
            Some(CodeBlockKind::InsideCode)
        );

        // Extending again from the frontier keeps the same state, and the
        // closing fence still ends the block.
        let closing = 2 + 100_000;
        checkpoints.ensure(&text, &index, closing + 1);
        let state = checkpoints.state_at(&text, &index, closing).expect("scanned");
        assert!(state.in_code_block);
        let mut scanner = CodeScanner::from_state(state);
        assert_eq!(
            scanner.step(lines[closing]).map(|s| s.kind),
            Some(CodeBlockKind::ClosingFence)
        );
    }

    /// A line beyond the frontier must be reported as unknown, never guessed.
    #[test]
    fn state_at_refuses_a_line_beyond_the_frontier() {
        let text = fenced_document(6, 100, 100);
        let index = index_of(&text);
        assert!(index.lines() > 600, "the document must outlast the frontier");
        let mut checkpoints = CodeCheckpoints::new();
        checkpoints.ensure(&text, &index, 500);
        assert_eq!(checkpoints.scanned_lines(), 500);
        assert!(checkpoints.state_at(&text, &index, 500).is_some());
        assert!(checkpoints.state_at(&text, &index, 501).is_none());
    }

    /// Sliced extension must land in exactly the same place as one big one.
    #[test]
    fn sliced_extension_equals_a_single_scan() {
        let text = fenced_document(4, 700, 900);
        let index = index_of(&text);

        let mut whole = CodeCheckpoints::new();
        whole.ensure(&text, &index, index.lines());

        let mut sliced = CodeCheckpoints::new();
        let mut guard = 0;
        while !sliced.is_complete(index.lines()) {
            sliced.extend_slice(&text, &index, 333);
            guard += 1;
            assert!(guard < 10_000, "the sliced scan never finished");
        }
        assert_eq!(sliced.scanned_lines(), whole.scanned_lines());

        for line in (0..index.lines()).step_by(37) {
            assert_eq!(
                sliced.state_at(&text, &index, line),
                whole.state_at(&text, &index, line),
                "mismatch at line {line}"
            );
        }
    }

    /// An edit truncates the table at the edit's line and the rest is rescanned;
    /// the states before the edit must not change.
    #[test]
    fn an_edit_keeps_the_prefix_and_rescans_the_rest() {
        let text = fenced_document(5, 400, 800);
        let index = index_of(&text);
        let mut checkpoints = CodeCheckpoints::new();
        checkpoints.ensure(&text, &index, index.lines());

        // Edit at line 600: insert a line, which shifts everything below.
        let edit_line = 600;
        let at = index.start(&text, edit_line);
        let mut edited = String::with_capacity(text.len() + 16);
        edited.push_str(&text[..at]);
        edited.push_str("inserted line\n");
        edited.push_str(&text[at..]);

        let prefix: Vec<CodeScanState> = (0..edit_line)
            .step_by(101)
            .map(|l| checkpoints.state_at(&text, &index, l).unwrap())
            .collect();

        checkpoints.truncate_to(edit_line);
        assert!(checkpoints.scanned_lines() <= edit_line);
        let edited_index = index_of(&edited);
        checkpoints.ensure(&edited, &edited_index, edited_index.lines());

        let lines: Vec<&str> = edited.split('\n').collect();
        let expected = analyze_code_blocks(&lines);
        for line in 0..edited_index.lines() {
            let state = checkpoints.state_at(&edited, &edited_index, line).unwrap();
            let actual = CodeScanner::from_state(state).step(lines[line]);
            assert_eq!(
                actual, expected[line],
                "state mismatch at line {line} after the edit"
            );
        }

        // Every checkpoint before the edit line is untouched.
        let after: Vec<CodeScanState> = (0..edit_line)
            .step_by(101)
            .map(|l| checkpoints.state_at(&edited, &edited_index, l).unwrap())
            .collect();
        assert_eq!(after, prefix, "the prefix must survive an edit below it");
    }

    /// `reset` forgets everything, including the very first checkpoint.
    #[test]
    fn reset_forgets_everything() {
        let text = fenced_document(3, 200, 300);
        let index = index_of(&text);
        let mut checkpoints = CodeCheckpoints::new();
        checkpoints.ensure(&text, &index, index.lines());
        assert!(checkpoints.scanned_lines() > 0);
        assert!(checkpoints.bytes() > 0);

        checkpoints.reset();
        assert_eq!(checkpoints.scanned_lines(), 0);
        assert_eq!(
            checkpoints.state_at(&text, &index, 0),
            Some(CodeScanState::default())
        );
        assert!(checkpoints.state_at(&text, &index, 1).is_none());
    }

    /// Empty, newline-only and unterminated documents must all terminate.
    #[test]
    fn empty_and_unterminated_documents_terminate() {
        for text in ["", "\n", "no trailing newline", "```\nunterminated"] {
            let index = index_of(text);
            let mut checkpoints = CodeCheckpoints::new();
            checkpoints.ensure(&text, &index, index.lines() + 10);
            assert!(
                checkpoints.is_complete(index.lines()),
                "scan did not terminate for {text:?}"
            );
            let lines: Vec<&str> = text.split('\n').collect();
            let expected = analyze_code_blocks(&lines);
            for line in 0..index.lines() {
                let state = checkpoints.state_at(&text, &index, line).unwrap();
                let actual = CodeScanner::from_state(state).step(lines[line]);
                assert_eq!(actual, expected[line], "mismatch at line {line} of {text:?}");
            }
        }
    }

    /// The table must stay proportional to the document in blocks, not in lines.
    #[test]
    fn the_table_is_one_state_per_block() {
        let text = fenced_document(4, 500, 1000);
        let index = index_of(&text);
        let mut checkpoints = CodeCheckpoints::new();
        checkpoints.ensure(&text, &index, index.lines());
        let blocks = index.lines() / CHECKPOINT_LINES + 1;
        assert!(
            checkpoints.bytes() <= 4 * blocks * std::mem::size_of::<CodeScanState>() + 64,
            "the checkpoint table grew past one state per block"
        );
    }
