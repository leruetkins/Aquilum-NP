    use super::*;

    #[test]
    fn test_single_leaf_lifecycle() {
        let l1 = LeafId::new();
        let tree = WorkspaceTree::new_single_leaf(l1);
        assert_eq!(tree.leaves(), vec![l1]);
        assert_eq!(tree.top_leaves(), vec![l1]);
        assert!(tree.is_leaf_in_titlebar(l1));
    }

    #[test]
    fn test_split_and_top_leaves_rules() {
        // 2x2 grid: Row[Column[L1, L3], Column[L2, L4]]
        let l1 = LeafId::new();
        let l2 = LeafId::new();
        let l3 = LeafId::new();
        let l4 = LeafId::new();

        let mut tree = WorkspaceTree::new_single_leaf(l1);
        assert!(tree.split_leaf(l1, SplitDirection::Row, l2));
        assert!(tree.split_leaf(l1, SplitDirection::Column, l3));
        assert!(tree.split_leaf(l2, SplitDirection::Column, l4));

        let leaves = tree.leaves();
        assert_eq!(leaves.len(), 4);
        assert!(leaves.contains(&l1));
        assert!(leaves.contains(&l2));
        assert!(leaves.contains(&l3));
        assert!(leaves.contains(&l4));

        // Top leaves: across all column ancestors, must be first child.
        // L1 is top in col1, L2 is top in col2. L3 and L4 are lower children.
        let top = tree.top_leaves();
        assert_eq!(top.len(), 2);
        assert!(top.contains(&l1));
        assert!(top.contains(&l2));
        assert!(!top.contains(&l3));
        assert!(!top.contains(&l4));
    }

    #[test]
    fn test_close_leaf_and_collapse() {
        let l1 = LeafId::new();
        let l2 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);
        tree.split_leaf(l1, SplitDirection::Row, l2);
        assert_eq!(tree.leaves().len(), 2);

        // Close l2, tree collapses back to single leaf l1
        let active = tree.close_leaf(l2);
        assert_eq!(active, Some(l1));
        assert_eq!(tree.leaves(), vec![l1]);
        assert_eq!(tree.root, WorkspaceNode::Leaf { id: l1 });
    }

    #[test]
    fn test_open_file_modes_and_table_6_2() {
        let l1 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);
        let mut leaves = HashMap::new();
        let doc_a = DocumentId::new();
        let doc_b = DocumentId::new();

        // 1. Open new file in empty leaf
        let res1 = tree.open_file(&mut leaves, doc_a, l1, OpenMode::Reuse);
        match res1 {
            OpenOutcome::OpenedNewTab { leaf, tab_index, .. } => {
                assert_eq!(leaf, l1);
                assert_eq!(tab_index, 0);
            }
            _ => panic!("Expected OpenedNewTab"),
        }

        // 2. Open doc_a again with Reuse -> switches to existing tab
        let res2 = tree.open_file(&mut leaves, doc_a, l1, OpenMode::Reuse);
        assert_eq!(res2, OpenOutcome::FocusedExisting { leaf: l1, tab_index: 0 });

        // 3. Open doc_a with ForceNewTab -> creates duplicate tab in same leaf
        let res3 = tree.open_file(&mut leaves, doc_a, l1, OpenMode::ForceNewTab);
        match res3 {
            OpenOutcome::OpenedNewTab { leaf, tab_index, .. } => {
                assert_eq!(leaf, l1);
                assert_eq!(tab_index, 1);
            }
            _ => panic!("Expected OpenedNewTab duplicate"),
        }
        assert_eq!(leaves.get(&l1).unwrap().tabs.len(), 2);

        // 4. Open doc_b with OpenInDirection(Row) -> creates split
        let res4 = tree.open_file(&mut leaves, doc_b, l1, OpenMode::OpenInDirection(SplitDirection::Row));
        match res4 {
            OpenOutcome::SplitCreated { new_leaf, .. } => {
                assert_ne!(new_leaf, l1);
                assert_eq!(tree.leaves().len(), 2);
                assert_eq!(tree.active_leaf, new_leaf);
                assert_eq!(leaves.get(&new_leaf).unwrap().tabs.len(), 1);
            }
            _ => panic!("Expected SplitCreated"),
        }
    }

    #[test]
    fn test_top_columns_and_dividers() {
        let l1 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);

        // 1. Single leaf: 1 column, ratio 1.0, no divider
        let cols = tree.top_columns();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].leaf_id, l1);
        assert!((cols[0].ratio - 1.0).abs() < 0.001);
        assert!(cols[0].divider_after.is_none());

        // 2. Row split: 2 leaves side by side
        let l2 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Row, l2);
        let cols = tree.top_columns();
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].leaf_id, l1);
        assert_eq!(cols[1].leaf_id, l2);
        assert!((cols[0].ratio - 0.5).abs() < 0.001);
        assert!((cols[1].ratio - 0.5).abs() < 0.001);
        assert!(cols[0].divider_after.is_some());
        let div = cols[0].divider_after.as_ref().unwrap();
        assert_eq!(div.divider_index, 0);
        assert!(cols[1].divider_after.is_none());

        // 3. 2x2 grid: split l1 down to l3, and split l2 down to l4
        let l3 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Column, l3);
        let l4 = LeafId::new();
        tree.split_leaf(l2, SplitDirection::Column, l4);

        // In 2x2 grid, top row is l1 and l2. l3 and l4 are bottom panels.
        let cols = tree.top_columns();
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].leaf_id, l1);
        assert_eq!(cols[1].leaf_id, l2);
        assert!((cols[0].ratio - 0.5).abs() < 0.001);
        assert!((cols[1].ratio - 0.5).abs() < 0.001);
        assert!(cols[0].divider_after.is_some());
        assert!(cols[1].divider_after.is_none());

        // Leaves in titlebar:
        assert!(tree.is_leaf_in_titlebar(l1));
        assert!(tree.is_leaf_in_titlebar(l2));
        assert!(!tree.is_leaf_in_titlebar(l3));
        assert!(!tree.is_leaf_in_titlebar(l4));
    }

    #[test]
    fn test_horizontal_span_and_width_ratios() {
        let l1 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);

        // Single leaf: span (0.0, 1.0)
        let (start, width) = tree.horizontal_span_of_leaf(l1);
        assert!((start - 0.0).abs() < 0.001);
        assert!((width - 1.0).abs() < 0.001);
        assert!((tree.horizontal_ratio_of_leaf(l1) - 0.0).abs() < 0.001);
        assert!((tree.horizontal_width_ratio_of_leaf(l1) - 1.0).abs() < 0.001);

        // Row split: l1 (left 50%) and l2 (right 50%)
        let l2 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Row, l2);
        let (s1, w1) = tree.horizontal_span_of_leaf(l1);
        let (s2, w2) = tree.horizontal_span_of_leaf(l2);
        assert!((s1 - 0.0).abs() < 0.001);
        assert!((w1 - 0.5).abs() < 0.001);
        assert!((s2 - 0.5).abs() < 0.001);
        assert!((w2 - 0.5).abs() < 0.001);
        assert!((tree.horizontal_width_ratio_of_leaf(l1) - 0.5).abs() < 0.001);
        assert!((tree.horizontal_width_ratio_of_leaf(l2) - 0.5).abs() < 0.001);

        // Column split on l1 (l1 top, l3 bottom): both keep 50% width and 0.0 start
        let l3 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Column, l3);
        let (s1, w1) = tree.horizontal_span_of_leaf(l1);
        let (s3, w3) = tree.horizontal_span_of_leaf(l3);
        assert!((s1 - 0.0).abs() < 0.001);
        assert!((w1 - 0.5).abs() < 0.001);
        assert!((s3 - 0.0).abs() < 0.001);
        assert!((w3 - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_vertical_span_and_height_ratios() {
        let l1 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);

        // Single leaf: span (0.0, 1.0)
        let (start, height) = tree.vertical_span_of_leaf(l1);
        assert!((start - 0.0).abs() < 0.001);
        assert!((height - 1.0).abs() < 0.001);
        assert!((tree.vertical_ratio_of_leaf(l1) - 0.0).abs() < 0.001);
        assert!((tree.vertical_height_ratio_of_leaf(l1) - 1.0).abs() < 0.001);

        // Column split: l1 (top 50%) and l2 (bottom 50%)
        let l2 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Column, l2);
        let (s1, h1) = tree.vertical_span_of_leaf(l1);
        let (s2, h2) = tree.vertical_span_of_leaf(l2);
        assert!((s1 - 0.0).abs() < 0.001);
        assert!((h1 - 0.5).abs() < 0.001);
        assert!((s2 - 0.5).abs() < 0.001);
        assert!((h2 - 0.5).abs() < 0.001);
        assert!((tree.vertical_height_ratio_of_leaf(l1) - 0.5).abs() < 0.001);
        assert!((tree.vertical_height_ratio_of_leaf(l2) - 0.5).abs() < 0.001);

        // Row split on l1 (l1 left, l3 right): both keep 50% height and 0.0 start
        let l3 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Row, l3);
        let (s1, h1) = tree.vertical_span_of_leaf(l1);
        let (s3, h3) = tree.vertical_span_of_leaf(l3);
        assert!((s1 - 0.0).abs() < 0.001);
        assert!((h1 - 0.5).abs() < 0.001);
        assert!((s3 - 0.0).abs() < 0.001);
        assert!((h3 - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_flattened_multi_row_splits() {
        let l1 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);

        let l2 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Row, l2);

        let l3 = LeafId::new();
        tree.split_leaf(l2, SplitDirection::Row, l3);

        let cols = tree.top_columns();
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0].leaf_id, l1);
        assert_eq!(cols[1].leaf_id, l2);
        assert_eq!(cols[2].leaf_id, l3);
        assert!((cols[0].ratio - 0.5).abs() < 0.001);
        assert!((cols[1].ratio - 0.25).abs() < 0.001);
        assert!((cols[2].ratio - 0.25).abs() < 0.001);
        assert_eq!(cols[0].divider_after.as_ref().unwrap().divider_index, 0);
        assert_eq!(cols[1].divider_after.as_ref().unwrap().divider_index, 1);
        assert!(cols[2].divider_after.is_none());

        // Split l1 as well: creates 4 windows, l1 -> [l1, l4]
        let l4 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Row, l4);
        let cols4 = tree.top_columns();
        assert_eq!(cols4.len(), 4);
        assert_eq!(cols4[0].leaf_id, l1);
        assert_eq!(cols4[1].leaf_id, l4);
        assert_eq!(cols4[2].leaf_id, l2);
        assert_eq!(cols4[3].leaf_id, l3);
        assert!((cols4[0].ratio - 0.25).abs() < 0.001);
        assert!((cols4[1].ratio - 0.25).abs() < 0.001);
        assert!((cols4[2].ratio - 0.25).abs() < 0.001);
        assert!((cols4[3].ratio - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_flatten_arbitrary_nested_splits() {
        let l1 = LeafId::new();
        let l2 = LeafId::new();
        let l3 = LeafId::new();
        let l4 = LeafId::new();

        // Construct manually nested tree: Row(l1, Row(l2, Row(l3, l4)))
        let mut root = WorkspaceNode::Split {
            direction: SplitDirection::Row,
            children: vec![
                WorkspaceNode::Leaf { id: l1 },
                WorkspaceNode::Split {
                    direction: SplitDirection::Row,
                    children: vec![
                        WorkspaceNode::Leaf { id: l2 },
                        WorkspaceNode::Split {
                            direction: SplitDirection::Row,
                            children: vec![
                                WorkspaceNode::Leaf { id: l3 },
                                WorkspaceNode::Leaf { id: l4 },
                            ],
                            ratios: vec![0.5, 0.5],
                        },
                    ],
                    ratios: vec![0.5, 0.5],
                },
            ],
            ratios: vec![0.5, 0.5],
        };

        root.flatten_same_direction_splits();
        root.normalize_ratios();

        match &root {
            WorkspaceNode::Split { direction, children, ratios } => {
                assert_eq!(*direction, SplitDirection::Row);
                assert_eq!(children.len(), 4);
                assert_eq!(ratios.len(), 4);
                assert!((ratios[0] - 0.5).abs() < 0.001);
                assert!((ratios[1] - 0.25).abs() < 0.001);
                assert!((ratios[2] - 0.125).abs() < 0.001);
                assert!((ratios[3] - 0.125).abs() < 0.001);
            }
            _ => panic!("Expected Split"),
        }
    }
