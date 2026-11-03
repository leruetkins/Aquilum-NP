    use super::*;

    #[test]
    fn test_bold_produces_two_markers_and_style() {
        let src = "a **b** c";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].range, 2..4); // "**"
        assert_eq!(markers[1].range, 5..7); // "**"

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Bold))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 4..5); // "b"
        assert_eq!(styles[0].node, markers[0].node);
        assert_eq!(styles[0].node, markers[1].node);
    }

    #[test]
    fn test_italic_produces_markers_and_style() {
        let src = "hello *world* test";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].range, 6..7); // "*"
        assert_eq!(markers[1].range, 12..13); // "*"

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Italic))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 7..12); // "world"
    }

    #[test]
    fn test_inline_code_produces_markers_and_style() {
        let src = "test `let x = 1;` end";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].range, 5..6); // "`"
        assert_eq!(markers[1].range, 16..17); // "`"

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Code))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 6..16); // "let x = 1;"
    }

    #[test]
    fn test_link_produces_markers_and_style() {
        let src = "click [here](https://example.com) now";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].range, 6..7); // "["
        assert_eq!(markers[1].range, 11..33); // "](https://example.com)"

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Link))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 7..11); // "here"
    }

    #[test]
    fn test_heading_produces_marker_and_style() {
        let src = "## Header Two";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, BlockKind::Heading(2));

        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].range, 0..3); // "## "

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Heading(2)))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 3..13); // "Header Two"
    }

    #[test]
    fn test_cursor_state_active_nodes_boundaries() {
        let doc = MarkdownDocument::new("a **b** c".to_string());
        // pos = 0 (on "a") -> no active nodes
        let c0 = doc.cursor_state(0);
        assert!(c0.active_nodes.is_empty());

        // pos = 2 (right before "**") -> node is active (start boundary)
        let c2 = doc.cursor_state(2);
        assert_eq!(c2.active_nodes.len(), 1);

        // pos = 4 (inside "b") -> node is active
        let c4 = doc.cursor_state(4);
        assert_eq!(c4.active_nodes.len(), 1);

        // pos = 7 (right after the closing "**") -> node is active (end boundary)
        let c7 = doc.cursor_state(7);
        assert_eq!(c7.active_nodes.len(), 1);

        // pos = 8 (on the space after "**") -> no active nodes
        let c8 = doc.cursor_state(8);
        assert!(c8.active_nodes.is_empty());
    }

    #[test]
    fn test_strikethrough_produces_markers_and_style() {
        let src = "text ~~deleted~~ more";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].range, 5..7); // "~~"
        assert_eq!(markers[1].range, 14..16); // "~~"

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Strike))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 7..14); // "deleted"
    }

    #[test]
    fn test_nested_bold_and_italic() {
        let src = "**a *b* c**";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;

        // Strong markers
        let strong_markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker && s.range.len() == 2).collect();
        assert_eq!(strong_markers.len(), 2);
        assert_eq!(strong_markers[0].range, 0..2); // "**"
        assert_eq!(strong_markers[1].range, 9..11); // "**"

        // Emph markers
        let emph_markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker && s.range.len() == 1).collect();
        assert_eq!(emph_markers.len(), 2);
        assert_eq!(emph_markers[0].range, 4..5); // "*"
        assert_eq!(emph_markers[1].range, 6..7); // "*"
    }

    #[test]
    fn test_multi_block_document_parsing() {
        let doc_text = "# Header\n\nParagraph with **bold**.\n\n```rust\nfn main() {}\n```\n";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(doc_text, 0, &mut ids);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(blocks[1].kind, BlockKind::Paragraph);
        assert_eq!(blocks[2].kind, BlockKind::CodeFence);
    }

    #[test]
    fn test_height_index_block_at() {
        let h = HeightIndex::new(vec![10.0, 20.0, 30.0]);
        assert_eq!(h.total(), 60.0);
        assert_eq!(h.block_at(0.0), 0);
        assert_eq!(h.block_at(5.0), 0);
        assert_eq!(h.block_at(10.0), 1);
        assert_eq!(h.block_at(29.9), 1);
        assert_eq!(h.block_at(30.0), 2);
        assert_eq!(h.block_at(59.9), 2);
        assert_eq!(h.block_at(100.0), 2);
    }

    #[test]
    fn test_height_index_visible_range() {
        let h = HeightIndex::new(vec![10.0, 20.0, 30.0, 40.0, 50.0]);
        // scroll_top = 15.0, viewport_h = 30.0, overscan = 0.0
        // top = 15.0 (block 1, top = 10..30), bottom = 45.0 (block 2, top = 30..60)
        let r = h.visible_range(15.0, 30.0, 0.0);
        assert_eq!(r, 1..3);
    }

    #[test]
    fn test_height_index_splice_and_set_height() {
        let mut h = HeightIndex::new(vec![10.0, 20.0, 30.0]);
        h.set_height(1, 25.0);
        assert_eq!(h.height_of(1), 25.0);
        assert_eq!(h.top_of(2), 35.0);
        assert_eq!(h.total(), 65.0);

        // splice: replace block 1 with two blocks [15.0, 15.0]
        h.splice(1..2, vec![15.0, 15.0]);
        assert_eq!(h.len(), 4);
        assert_eq!(h.height_of(0), 10.0);
        assert_eq!(h.height_of(1), 15.0);
        assert_eq!(h.height_of(2), 15.0);
        assert_eq!(h.height_of(3), 30.0);
        assert_eq!(h.top_of(3), 40.0);
        assert_eq!(h.total(), 70.0);
    }

    #[test]
    fn test_height_index_empty() {
        let h = HeightIndex::new(vec![]);
        assert_eq!(h.total(), 0.0);
        assert_eq!(h.block_at(10.0), 0);
        assert_eq!(h.visible_range(0.0, 100.0, 0.0), 0..0);
    }

    #[test]
    fn test_build_runs_markers_hidden_when_cursor_outside() {
        let doc = MarkdownDocument::new("a **b** c".to_string());
        let cursor = doc.cursor_state(0); // pos = 0
        let runs = build_runs(&doc.blocks[0], &cursor);
        // Markers are hidden! Expect 3 runs: "a ", "b" (bold), " c"
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].source, 0..2);
        assert!(!runs[0].style.marker && !runs[0].style.bold);

        assert_eq!(runs[1].source, 4..5); // "b"
        assert!(!runs[1].style.marker && runs[1].style.bold);

        assert_eq!(runs[2].source, 7..9);
        assert!(!runs[2].style.marker && !runs[2].style.bold);
    }

    #[test]
    fn test_build_runs_markers_shown_when_cursor_on_boundary() {
        let doc = MarkdownDocument::new("a **b** c".to_string());
        let cursor = doc.cursor_state(2); // pos = 2 (before "**")
        let runs = build_runs(&doc.blocks[0], &cursor);
        // Markers are shown! Expect 5 runs: "a ", "**" (marker), "b" (bold), "**" (marker), " c"
        assert_eq!(runs.len(), 5);
        assert_eq!(runs[0].source, 0..2);

        assert_eq!(runs[1].source, 2..4); // "**"
        assert!(runs[1].style.marker);

        assert_eq!(runs[2].source, 4..5); // "b"
        assert!(runs[2].style.bold);

        assert_eq!(runs[3].source, 5..7); // "**"
        assert!(runs[3].style.marker);

        assert_eq!(runs[4].source, 7..9);
    }

    #[test]
    fn test_layout_cache_lru() {
        let mut cache: LayoutCache<String> = LayoutCache::new(2);
        let key1 = LayoutKey { block_id: 1, text_hash: 10, active_hash: 0, width_px: 800, scale_factor_x100: 100, style_rev: 1 };
        let key2 = LayoutKey { block_id: 2, text_hash: 20, active_hash: 0, width_px: 800, scale_factor_x100: 100, style_rev: 1 };
        let key3 = LayoutKey { block_id: 3, text_hash: 30, active_hash: 0, width_px: 800, scale_factor_x100: 100, style_rev: 1 };

        let val1 = cache.get_or_insert_with(key1.clone(), || "block1".to_string());
        assert_eq!(*val1, "block1");
        assert_eq!(cache.len(), 1);

        let val1_again = cache.get(&key1);
        assert!(val1_again.is_some());

        // Insert key2
        cache.insert(key2.clone(), Arc::new("block2".to_string()));
        assert_eq!(cache.len(), 2);

        // Insert key3 -> key1 must be evicted from LRU (capacity 2)
        cache.insert(key3.clone(), Arc::new("block3".to_string()));
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&key1).is_none());
        assert!(cache.get(&key2).is_some());
        assert!(cache.get(&key3).is_some());
    }

    #[test]
    fn test_stage4_cursor_movement_step_by_step() {
        let text = "Hello **bold** world";
        let doc = MarkdownDocument::new(text.to_string());
        let mut cursor = doc.cursor_state(0);

        // Reset counters
        crate::perf::PERF.snapshot_and_reset();

        // 1. Movement inside "Hello" (positions 0..=5): markers hidden, invalidation = 0
        for pos in 1..=5 {
            let outcome = doc.update_cursor(&mut cursor, pos);
            assert_eq!(outcome, CursorUpdateOutcome::Unchanged { old_pos: pos - 1, new_pos: pos });
            let runs = build_runs(&doc.blocks[0], &cursor);
            assert!(runs.iter().all(|r| !r.style.marker));
        }

        // 2. Entering the "**" boundary (position 6): node activates, exactly 1 block is invalidated
        let outcome = doc.update_cursor(&mut cursor, 6);
        match outcome {
            CursorUpdateOutcome::ActiveNodesChanged { invalidated_blocks, .. } => {
                assert_eq!(invalidated_blocks.len(), 1);
            }
            _ => panic!("Expected ActiveNodesChanged on boundary at pos 6"),
        }
        let runs = build_runs(&doc.blocks[0], &cursor);
        assert!(runs.iter().any(|r| r.style.marker));

        // 3. Movement inside "**bold**" (positions 7..=14): markers expanded, invalidation = 0!
        for pos in 7..=14 {
            let outcome = doc.update_cursor(&mut cursor, pos);
            assert_eq!(outcome, CursorUpdateOutcome::Unchanged { old_pos: pos - 1, new_pos: pos });
            let runs = build_runs(&doc.blocks[0], &cursor);
            assert!(runs.iter().any(|r| r.style.marker));
        }

        // 4. Leaving "**" (position 15, the space before "world"): node deactivates, exactly 1 block is invalidated
        let outcome = doc.update_cursor(&mut cursor, 15);
        match outcome {
            CursorUpdateOutcome::ActiveNodesChanged { invalidated_blocks, .. } => {
                assert_eq!(invalidated_blocks.len(), 1);
            }
            _ => panic!("Expected ActiveNodesChanged when leaving node at pos 15"),
        }
        let runs = build_runs(&doc.blocks[0], &cursor);
        assert!(runs.iter().all(|r| !r.style.marker));

        // 5. Movement in "world" (16..=20): markers hidden, invalidation = 0
        for pos in 16..=text.len() {
            let outcome = doc.update_cursor(&mut cursor, pos);
            assert_eq!(outcome, CursorUpdateOutcome::Unchanged { old_pos: pos - 1, new_pos: pos });
        }

        // The parser was never called across all steps!
        let counters = crate::perf::PERF.snapshot_and_reset();
        assert_eq!(counters[0], 0, "parse_calls must be 0");
        assert_eq!(counters[1], 0, "span_rebuilds must be 0");
    }

    #[test]
    fn test_stage4_multi_block_cursor_crossing() {
        let text = "# Heading\n\nSecond paragraph text.";
        let doc = MarkdownDocument::new(text.to_string());
        let mut cursor = doc.cursor_state(0); // block 0

        crate::perf::PERF.snapshot_and_reset();

        // Move from block 0 to block 1 (position 12)
        let outcome = doc.update_cursor(&mut cursor, 12);
        match outcome {
            CursorUpdateOutcome::ActiveNodesChanged { invalidated_blocks, .. } => {
                assert_eq!(invalidated_blocks.len(), 2, "crossing block boundary invalidates old and new block");
                assert_eq!(invalidated_blocks[0], doc.blocks[0].id);
                assert_eq!(invalidated_blocks[1], doc.blocks[1].id);
            }
            _ => panic!("Expected ActiveNodesChanged when crossing block boundary"),
        }

        let counters = crate::perf::PERF.snapshot_and_reset();
        assert_eq!(counters[0], 0, "parse_calls must be 0");
        assert_eq!(counters[1], 0, "span_rebuilds must be 0");
    }

    #[test]
    fn test_stage5_visual_map_roundtrip() {
        let m = VisualMap::new(0, vec![2..4, 5..7]);
        assert_eq!(m.to_visual(0), 0);
        assert_eq!(m.to_visual(1), 1);
        assert_eq!(m.to_visual(2), 2); // start of the first hidden range
        assert_eq!(m.to_visual(4), 2); // end of the first hidden range
        assert_eq!(m.to_visual(5), 3); // "b" (length 1), start of the second hidden range
        assert_eq!(m.to_visual(7), 3); // end of the second hidden range
        assert_eq!(m.to_visual(8), 4); // " c"

        assert_eq!(m.to_source(0), 0);
        assert_eq!(m.to_source(1), 1);
        assert_eq!(m.to_source(2), 4); // after the first hidden range
        assert_eq!(m.to_source(3), 7); // after the second hidden range

        // Round-trip check: to_visual(to_source(vis)) == vis
        for vis in 0..=4 {
            let src = m.to_source(vis);
            assert_eq!(m.to_visual(src), vis);
        }
    }

    #[test]
    fn test_stage5_visual_map_from_block() {
        let doc = MarkdownDocument::new("a **b** c".to_string());
        let block = &doc.blocks[0];

        // 1. Cursor outside the node -> markers hidden -> VisualMap is compressed
        let cursor_outside = doc.cursor_state(0);
        let map_outside = VisualMap::from_block(block, &cursor_outside);
        assert_eq!(map_outside.hidden.len(), 2);
        assert_eq!(map_outside.to_visual(4), 2); // 'b' is at visual position 2

        // 2. Cursor on the boundary -> markers expanded -> VisualMap has no hidden ranges
        let cursor_active = doc.cursor_state(2);
        let map_active = VisualMap::from_block(block, &cursor_active);
        assert!(map_active.hidden.is_empty());
        assert_eq!(map_active.to_visual(4), 4);
        assert_eq!(map_active.to_source(4), 4);
    }

    #[test]
    fn test_stage5_clamp_hidden_and_normalize_atomic() {
        let m = VisualMap::new(0, vec![2..4]);
        // Inside 2..4: moving_right -> 4, moving_left -> 2
        assert_eq!(m.clamp_hidden_pos(3, true), 4);
        assert_eq!(m.clamp_hidden_pos(3, false), 2);
        // Outside: remains unchanged
        assert_eq!(m.clamp_hidden_pos(5, true), 5);

        // Atomic span: cursor jumps over
        let block = MarkdownBlock {
            id: 1,
            source_range: 0..30,
            kind: BlockKind::Paragraph,
            spans: vec![Span {
                range: 10..20,
                kind: SpanKind::Atomic,
                node: 1,
            }],
            layout: None,
        };
        assert_eq!(normalize_pos(&block, 15, true), 20);
        assert_eq!(normalize_pos(&block, 15, false), 10);
        assert_eq!(normalize_pos(&block, 5, true), 5);
    }

    #[test]
    fn test_stage6_typography_stable_regardless_of_cursor() {
        let doc = MarkdownDocument::new("# Heading 1\n\nParagraph text".to_string());
        let h1_block = &doc.blocks[0];

        let base_font = 16.0;
        let base_lh = 24.0;

        // Cursor outside the heading (in the paragraph at position 15)
        let cursor_outside = doc.cursor_state(15);
        assert!(!cursor_outside.is_node_active(h1_block.spans[0].node));

        let typo_outside = BlockTypography::for_block(h1_block.kind, base_font, base_lh);

        // Cursor inside the heading (at position 3)
        let cursor_inside = doc.cursor_state(3);
        assert!(cursor_inside.is_node_active(h1_block.spans[0].node));

        let typo_inside = BlockTypography::for_block(h1_block.kind, base_font, base_lh);

        // Font sizes and line height are STRICTLY identical
        assert_eq!(typo_outside.font_size, typo_inside.font_size);
        assert_eq!(typo_outside.line_height, typo_inside.line_height);
        assert_eq!(typo_inside.font_size, 16.0 * 1.8);
        assert_eq!(typo_inside.line_height, 24.0 * 1.8);
    }

    #[test]
    fn test_stage6_heading_hierarchy_scaling() {
        let base = 10.0;
        assert_eq!(font_size_for(BlockKind::Heading(1), base), 18.0);
        assert_eq!(font_size_for(BlockKind::Heading(2), base), 15.0);
        assert_eq!(font_size_for(BlockKind::Heading(3), base), 12.5);
        assert_eq!(font_size_for(BlockKind::Heading(4), base), 11.0);
        assert_eq!(font_size_for(BlockKind::Paragraph, base), 10.0);
        assert_eq!(font_size_for(BlockKind::CodeFence, base), 10.0);
    }

    #[test]
    fn test_stage7_incremental_edit_matches_full_reparse() {
        let initial_text = "# Title\n\nFirst paragraph text.\n\nSecond paragraph with **bold** text.\n\nThird paragraph here.\n";
        let mut doc = MarkdownDocument::new(initial_text.to_string());
        assert_eq!(doc.blocks.len(), 4);

        // Insert the word "MODIFIED " into the second paragraph (before "Second")
        let edit_pos = doc.blocks[2].source_range.start;
        doc.apply_edit(edit_pos..edit_pos, "MODIFIED ");

        // Compare with a full reparse
        let mut ids = IdGen::new();
        let full_blocks = parse_blocks(&doc.source, 0, &mut ids);

        assert_eq!(doc.blocks.len(), full_blocks.len(), "Block counts must match");
        for (i, (inc_b, full_b)) in doc.blocks.iter().zip(&full_blocks).enumerate() {
            assert_eq!(inc_b.kind, full_b.kind, "Block {i} kind mismatch");
            assert_eq!(inc_b.source_range, full_b.source_range, "Block {i} source_range mismatch");
            assert_eq!(inc_b.spans.len(), full_b.spans.len(), "Block {i} spans len mismatch");
            for (s_inc, s_full) in inc_b.spans.iter().zip(&full_b.spans) {
                assert_eq!(s_inc.kind, s_full.kind);
                assert_eq!(s_inc.range, s_full.range);
            }
        }
    }

    #[test]
    fn test_stage7_wide_reparse_trigger() {
        assert!(needs_wide_reparse("```rust\n", ""));
        assert!(needs_wide_reparse("", "```"));
        assert!(needs_wide_reparse("---", ""));
        assert!(needs_wide_reparse("<!--", ""));
        assert!(!needs_wide_reparse("обычный текст", ""));
    }

    #[test]
    fn test_stage7_deletion_incremental_edit() {
        let initial_text = "# Header\n\nAbc def ghi.\n\nEnd.\n";
        let mut doc = MarkdownDocument::new(initial_text.to_string());
        assert_eq!(doc.blocks.len(), 3);

        // Remove "def " from the second block
        let b1_start = doc.blocks[1].source_range.start;
        doc.apply_edit(b1_start + 4..b1_start + 8, "");

        let mut ids = IdGen::new();
        let full_blocks = parse_blocks(&doc.source, 0, &mut ids);

        assert_eq!(doc.blocks.len(), full_blocks.len());
        for (inc_b, full_b) in doc.blocks.iter().zip(&full_blocks) {
            assert_eq!(inc_b.kind, full_b.kind);
            assert_eq!(inc_b.source_range, full_b.source_range);
        }
    }
