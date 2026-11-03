use gpui::prelude::*;
use gpui::{div, px};
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn style_parses_supported_css_subset() {
        let raw = BTreeMap::from([
            ("width".to_string(), "320px".to_string()),
            ("height".to_string(), "100%".to_string()),
            ("padding".to_string(), "4px 8px".to_string()),
            ("paddingLeft".to_string(), "2px".to_string()),
            ("backgroundColor".to_string(), "#ff0000".to_string()),
            ("opacity".to_string(), "0.5".to_string()),
            ("display".to_string(), "none".to_string()),
            ("position".to_string(), "absolute".to_string()),
            ("top".to_string(), "12px".to_string()),
            ("zIndex".to_string(), "10".to_string()),
        ]);
        let style = NodeStyle::from_map(&raw);

        assert_eq!(style.width, Some(Len::Px(320.0)));
        assert_eq!(style.height, Some(Len::Pct(100.0)));
        assert_eq!(style.padding, Some(Sides { top: 4.0, right: 8.0, bottom: 4.0, left: 2.0 }));
        assert_eq!(style.background, Some(0xff00_00ff));
        assert_eq!(style.opacity, Some(0.5));
        assert!(style.hidden);
        assert_eq!(style.position, Some(StylePosition::Absolute));
        assert_eq!(style.inset.top, Some(Len::Px(12.0)));
    }

    #[test]
    fn style_parses_colors() {
        assert_eq!(parse_color("#f00"), Some(0xff00_00ff));
        assert_eq!(parse_color("#ff0000"), Some(0xff00_00ff));
        assert_eq!(parse_color("#ff000080"), Some(0xff00_0080));
        assert_eq!(parse_color("rgba(0, 128, 255, 0.5)"), Some(0x0080_ff7f));
        assert_eq!(parse_color("transparent"), Some(0));
        assert_eq!(parse_color("не цвет"), None);
    }

    #[test]
    fn class_matching_respects_word_boundaries() {
        assert!(has_word("setting-item-name", "name"));
        assert!(!has_word("username", "name"));
        assert!(!has_word("rename", "name"));
        assert!(!has_word("subtitle", "title"));
        assert!(has_word("is-active", "active"));
        assert!(!has_word("inactive", "active"));
    }

    #[test]
    fn text_only_style_does_not_claim_layout() {
        let mut style = NodeStyle::default();
        style.color = Some(0xff0000ff);
        style.font_size = Some(14.0);
        style.font_weight = Some(600);
        // For a wrapper container it is enough to pass color and font size down: a column
        // inside a row of several spans would break the layout.
        assert!(!style.affects_layout());

        style.padding = Some(Sides { top: 4.0, right: 4.0, bottom: 4.0, left: 4.0 });
        assert!(style.affects_layout());

        let mut inset = NodeStyle::default();
        inset.inset.left = Some(Len::Px(8.0));
        assert!(inset.affects_layout());

        let mut hidden = NodeStyle::default();
        hidden.hidden = true;
        // display:none does not need a box — the node is not rendered anyway.
        assert!(!hidden.affects_layout());
    }

    #[test]
    fn styled_node_keeps_id_and_children() {
        let node = UiNode::Styled {
            style: NodeStyle::default(),
            child: Box::new(UiNode::Input {
                id: "query".into(),
                placeholder: None,
                value: None,
            }),
        };
        // The wrapper is transparent: the id stays with the child, and the validator sees it once.
        assert_eq!(node.id(), None);
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.children()[0].id(), Some("query"));
        assert!(validate_tree(&node).is_ok());
    }

    #[test]
    fn styled_wrapper_does_not_duplicate_child_id() {
        // Exactly what failed in the app: a CSS rule covered the input, the wrapper
        // inherited its id, and the tree was rejected as "Duplicate UI node id".
        let tree = UiNode::Column {
            id: None,
            class: None,
            gap: Gap::None,
            children: vec![UiNode::Styled {
                style: NodeStyle::default(),
                child: Box::new(UiNode::Input {
                    id: "search".into(),
                    placeholder: None,
                    value: None,
                }),
            }],
        };
        assert!(validate_tree(&tree).is_ok(), "обёртка не должна давать дубликат id");
        assert_eq!(tree.get_input_value("search"), None);
        let mut tree = tree;
        assert!(tree.update_input_value("search", "typed"));
        assert_eq!(tree.get_input_value("search"), Some("typed".into()));
    }

    #[test]
    fn styled_box_nodes_take_the_sheet_without_a_wrapper() {
        // A wrapper around the column made its parent "auto-height", and `flex_1` (basis 0)
        // collapsed to zero: the file list in Empty Files Cleaner vanished entirely.
        let column = UiNode::Column {
            id: None,
            class: Some("ef-list".into()),
            gap: Gap::None,
            children: vec![],
        };
        let row = UiNode::Row {
            id: None,
            class: Some("ef-item".into()),
            gap: Gap::None,
            children: vec![],
        };
        let text = UiNode::Text {
            value: "x".into(),
            class: Some("ef-item-name".into()),
            style: TextStyle::Normal,
        };
        assert!(takes_style_directly(&column));
        assert!(takes_style_directly(&row));
        assert!(!takes_style_directly(&text), "тексту обёртка не мешает — у него нет своей геометрии");
    }

    #[test]
    fn sheet_padding_wins_over_class_heuristic_on_the_same_box() {
        // `.ef-header { padding: 28px 28px 20px }` versus the `is_header` heuristic (24/24/14).
        // One box, cascade: the style is applied last and overrides all four sides.
        let style = NodeStyle::from_map(&BTreeMap::from([("padding".to_string(), "28px 28px 20px".to_string())]));
        let mut el = apply_sheet(div().px(px(24.0)).pt(px(24.0)).pb(px(14.0)), Some(&style));
        let padding = el.style().padding.clone();
        assert_eq!(padding.left, Some(px(28.0).into()));
        assert_eq!(padding.right, Some(px(28.0).into()));
        assert_eq!(padding.top, Some(px(28.0).into()));
        assert_eq!(padding.bottom, Some(px(20.0).into()));
    }

    #[test]
    fn sheet_without_box_properties_keeps_the_heuristic() {
        // A color in the stylesheet does not cancel the heuristic padding: otherwise a plugin
        // with a single `.item { color: red }` rule would lose its whole box.
        let style = NodeStyle::from_map(&BTreeMap::from([("color".to_string(), "#ff0000".to_string())]));
        let mut el = apply_sheet(div().px(px(24.0)), Some(&style));
        assert_eq!(el.style().padding.left, Some(px(24.0).into()));
    }

    #[test]
    fn styled_node_round_trips_through_json() {
        let node = UiNode::Styled {
            style: NodeStyle::from_map(&BTreeMap::from([
                ("width".to_string(), "320px".to_string()),
                ("backgroundColor".to_string(), "#ff0000".to_string()),
                ("padding".to_string(), "4px 8px".to_string()),
                ("position".to_string(), "absolute".to_string()),
                ("top".to_string(), "12px".to_string()),
            ])),
            child: Box::new(UiNode::Text {
                value: "x".into(),
                class: None,
                style: TextStyle::Normal,
            }),
        };
        let json = serde_json::to_string(&node).unwrap();
        let back: UiNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node, back, "json: {json}");
    }

    #[test]
    fn test_ui_node_serialization_and_count() {
        let tree = UiNode::Column {
            id: None,
            class: None,
            gap: Gap::Medium,
            children: vec![
                UiNode::Text {
                    value: "Header".into(),
                    class: None,
                    style: TextStyle::Heading,
                },
                UiNode::Button {
                    id: "submit-btn".into(),
                    label: "Submit".into(),
                    class: None,
                    variant: ButtonVariant::Primary,
                    disabled: false,
                },
                UiNode::Separator,
            ],
        };

        assert_eq!(tree.count_nodes(), 4);

        let json = serde_json::to_string(&tree).unwrap();
        assert!(json.contains("\"type\":\"column\""));
        assert!(json.contains("\"type\":\"button\""));
        assert!(json.contains("\"variant\":\"primary\""));

        let deserialized: UiNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, tree);
    }

    #[test]
    fn test_validate_tree_duplicate_ids() {
        let tree = UiNode::Column {
            id: None,
            class: None,
            gap: Gap::None,
            children: vec![
                UiNode::Button {
                    id: "btn-1".into(),
                    label: "One".into(),
                    class: None,
                    variant: ButtonVariant::Default,
                    disabled: false,
                },
                UiNode::Button {
                    id: "btn-1".into(),
                    label: "Two".into(),
                    class: None,
                    variant: ButtonVariant::Default,
                    disabled: false,
                },
            ],
        };

        let result = validate_tree(&tree);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate UI node id"));
    }

    #[test]
    fn test_validate_tree_ok() {
        let tree = UiNode::Column {
            id: None,
            class: None,
            gap: Gap::Small,
            children: vec![
                UiNode::Text {
                    value: "Description".into(),
                    class: None,
                    style: TextStyle::Normal,
                },
                UiNode::Checkbox {
                    id: "agree".into(),
                    label: "I agree".into(),
                    checked: true,
                },
            ],
        };

        assert!(validate_tree(&tree).is_ok());
    }

    #[test]
    fn test_setting_item_and_controls_roundtrip() {
        let tree = UiNode::Column {
            id: None,
            class: None,
            gap: Gap::Medium,
            children: vec![
                UiNode::SettingItem {
                    name: "Enable Live Preview".into(),
                    desc: Some("Toggle real-time Markdown rendering".into()),
                    control: Box::new(UiNode::Toggle {
                        id: "toggle-live-preview".into(),
                        value: true,
                    }),
                },
                UiNode::SettingItem {
                    name: "Editor Font Size".into(),
                    desc: Some("Adjust base font size in pixels".into()),
                    control: Box::new(UiNode::Slider {
                        id: "slider-font-size".into(),
                        min: 10.0,
                        max: 32.0,
                        step: 1.0,
                        value: 16.0,
                    }),
                },
                UiNode::SettingItem {
                    name: "Default View".into(),
                    desc: None,
                    control: Box::new(UiNode::Select {
                        id: "select-default-view".into(),
                        options: vec![
                            SelectOption { value: "source".into(), label: "Source Mode".into() },
                            SelectOption { value: "preview".into(), label: "Live Preview".into() },
                        ],
                        value: "preview".into(),
                    }),
                },
            ],
        };

        assert!(validate_tree(&tree).is_ok());
        // 1 Column + 3 SettingItems + 3 controls (Toggle, Slider, Select) = 7 nodes
        assert_eq!(tree.count_nodes(), 7);

        let json = serde_json::to_string(&tree).unwrap();
        assert!(json.contains("\"type\":\"setting_item\""));
        assert!(json.contains("\"type\":\"toggle\""));
        assert!(json.contains("\"type\":\"slider\""));
        assert!(json.contains("\"type\":\"select\""));

        let deserialized: UiNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, tree);
    }

    #[test]
    fn test_ui_node_input_value_update() {
        let mut tree = UiNode::Column {
            id: None,
            class: None,
            gap: Gap::Medium,
            children: vec![
                UiNode::SettingItem {
                    name: "Custom Format".into(),
                    desc: Some("Format template".into()),
                    control: Box::new(UiNode::Input {
                        id: "input-format".into(),
                        placeholder: Some("{words}".into()),
                        value: Some("initial".into()),
                    }),
                },
            ],
        };

        assert_eq!(tree.get_input_value("input-format"), Some("initial".to_string()));
        assert!(tree.update_input_value("input-format", "{words} руб."));
        assert_eq!(tree.get_input_value("input-format"), Some("{words} руб.".to_string()));
        assert!(!tree.update_input_value("non-existent", "test"));
    }

    #[test]
    fn test_ui_node_textarea() {
        let mut tree = UiNode::Column {
            id: None,
            class: None,
            gap: Gap::Medium,
            children: vec![
                UiNode::Textarea {
                    id: "words-json".into(),
                    placeholder: Some("Enter JSON".into()),
                    value: Some("{\"RUB\": []}".into()),
                    rows: Some(10),
                },
            ],
        };

        assert_eq!(tree.count_nodes(), 2);
        assert_eq!(tree.get_input_value("words-json"), Some("{\"RUB\": []}".to_string()));
        assert!(tree.update_input_value("words-json", "{\"RUB\": [\"руб\"]}"));
        assert_eq!(tree.get_input_value("words-json"), Some("{\"RUB\": [\"руб\"]}".to_string()));

        let json = serde_json::to_string(&tree).unwrap();
        assert!(json.contains("\"type\":\"textarea\""));
        assert!(json.contains("\"rows\":10"));
        let deserialized: UiNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, tree);
    }

    // ──────────────────────────────────────────────────────── golden: real trees

    /// Golden bundle: live UI trees from a probe of the top 500 Obsidian plugins
    /// (probe-one collects set_view_tree on load + activation; the bundle gets
    /// up to three trees per plugin, one file per plugin). Regenerate with:
    /// `node make-fixtures.cjs <probe-out> <fixtures-dir>` in tools/plugin-probe.
    const GOLDEN_BUNDLE: &str = include_str!("../tests/ui_golden_bundle.json");

    #[derive(serde::Deserialize)]
    struct GoldenEntry {
        #[allow(dead_code)]
        id: String,
        #[serde(default)]
        #[allow(dead_code)]
        view_id: u64,
        tree: UiNode,
    }

    #[derive(serde::Deserialize)]
    struct GoldenBundle {
        #[allow(dead_code)]
        count: usize,
        trees: Vec<GoldenEntry>,
    }

    fn golden_bundle() -> GoldenBundle {
        serde_json::from_str(GOLDEN_BUNDLE).expect("golden bundle must deserialize")
    }

    #[test]
    fn golden_corpus_parses_and_validates() {
        let bundle = golden_bundle();
        assert!(bundle.trees.len() >= 500, "golden bundle too small: {}", bundle.trees.len());

        for entry in &bundle.trees {
            // The direct serde load already happened above; the round-trip checks that
            // Serialize and Deserialize of the UiNode shape have not drifted apart (the channel
            // invariant worker → host: the tree is passed through serde_json on both sides).
            let json = serde_json::to_string(&entry.tree).unwrap();
            let back: UiNode = serde_json::from_str(&json).unwrap();
            assert_eq!(back, entry.tree, "{}: serde round-trip drifted", entry.id);

            validate_tree(&entry.tree)
                .unwrap_or_else(|e| panic!("{}: validate_tree rejected a real tree: {e}", entry.id));
        }
    }

    #[test]
    fn golden_corpus_translation_coverage() {
        let bundle = golden_bundle();

        let mut total = TreeStats::default();
        for entry in &bundle.trees {
            total.add(&tree_translation_stats(&entry.tree));
        }

        let nodes = total.nodes as f32;
        let pct = |v: usize| format!("{:.1}%", v as f32 / nodes * 100.0);
        // Coverage baseline on the 2026-10 corpus. The point of the thresholds is not "pretty" but "stable":
        // when the renderer or bootstrap changes, the numbers must not creep down. Growth is a signal
        // that a new node/property is being translated, and the baseline should be raised.
        // display:none is a valid state of a live tree (plugins hide nodes
        // programmatically, ~28 styles per corpus): rendered is counted separately from nodes.
        assert!(total.rendered <= total.nodes);
        assert!(total.rendered > 9500, "rendered nodes collapsed: {}", total.rendered);
        assert!(total.styled > 50, "corpus has no styled nodes");
        assert!(total.by_kind["setting_item"] > 3000, "setting rows vanished from the corpus");
        assert!(total.by_kind["input"] > 1000);
        assert!(total.by_kind["button"] > 400);
        assert!(total.by_kind["toggle"] > 800);
        assert!(total.by_kind["text"] > 1400);
        assert!(total.by_kind["column"] > 1100);
        assert!(total.by_kind["select"] > 400);
        assert!(total.style_props.contains_key("width"), "inline width styles vanished");

        println!("golden coverage: {} trees, {} nodes", bundle.trees.len(), total.nodes);
        println!("  rendered {} / styled {} / layout-styles {}",
            pct(total.rendered), pct(total.styled), pct(total.layout_styles));
        println!("  text bytes: {}", total.text_bytes);
        for (kind, n) in &total.by_kind {
            println!("  {kind}: {n} ({})", pct(*n));
        }
        println!("  style props: {:?}", total.style_props);
    }
