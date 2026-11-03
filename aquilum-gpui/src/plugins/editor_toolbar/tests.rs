    use super::*;
    use crate::toolbar::{ToolbarItemKind, MAX_TOOLBAR_DROPDOWN_DEPTH};

    #[test]
    fn test_default_toolbar_items_contain_all_required_actions() {
        let items = default_toolbar_items();
        assert!(!items.is_empty());

        let mut commands = Vec::new();
        fn collect_commands(items: &[ToolbarItem], out: &mut Vec<String>) {
            for it in items {
                match &it.kind {
                    ToolbarItemKind::Button { command_id, .. } => out.push(command_id.clone()),
                    ToolbarItemKind::Dropdown { command, items, .. } => {
                        if let Some(cmd) = command {
                            out.push(cmd.clone());
                        }
                        collect_commands(items, out);
                    }
                    ToolbarItemKind::Separator => {}
                }
            }
        }
        collect_commands(&items, &mut commands);

        // Required commands from specification
        let expected = [
            "editor:undo", "editor:redo", "editor:clear-formatting",
            "editor:heading-1", "editor:heading-2", "editor:heading-3",
            "editor:heading-4", "editor:heading-5", "editor:heading-6",
            "editor:bold", "editor:italic", "editor:strikethrough", "editor:underline", "editor:highlight",
            "editor:attach-file", "editor:insert-table", "editor:toggle-task",
            "editor:code-block", "editor:inline-code",
            "editor:insert-callout", "editor:indent", "editor:outdent",
            "editor:list-bullet", "editor:list-numbered", "editor:list-task",
            "editor:align-left", "editor:align-center", "editor:align-right",
            "editor:toggle-fullscreen",
        ];

        for cmd in expected {
            assert!(
                commands.contains(&cmd.to_string()),
                "Default toolbar items missing required command: {}",
                cmd
            );
        }

        // Verify separators are present between groups
        let separator_count = items.iter().filter(|i| i.is_separator()).count();
        assert!(separator_count >= 5, "Expected at least 5 separators between button groups");
    }

    #[test]
    fn test_toolbar_config_file_json_roundtrip() {
        let items = vec![
            ToolbarItem::button("b1", "editor:undo", "undo", Some("Undo")),
            ToolbarItem::separator(),
            ToolbarItem::dropdown(
                "d1",
                "heading-n",
                Some("Headings"),
                None,
                vec![ToolbarItem::button("h1", "editor:heading-1", "h1", Some("Heading 1"))],
            ),
        ];

        let cfg = ToolbarConfigFile {
            version: 1,
            items: items.clone(),
            alignment: crate::toolbar::ToolbarAlignment::Center,
        };

        let json = serde_json::to_string_pretty(&cfg).expect("Serialization failed");
        assert!(json.contains("\"type\": \"button\""));
        assert!(json.contains("\"command\": \"editor:undo\""));
        assert!(json.contains("\"type\": \"separator\""));
        assert!(json.contains("\"type\": \"dropdown\""));

        let deserialized: ToolbarConfigFile = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.items.len(), 3);
        assert_eq!(deserialized.items[0].command_id(), Some("editor:undo"));
    }

    #[test]
    fn test_corrupted_json_recovery_fallback() {
        let temp_dir = std::env::temp_dir().join(format!("vibe_test_tb_corrupt_{}", uuid::Uuid::new_v4().simple()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let corrupt_path = get_config_path(&temp_dir);
        if let Some(p) = corrupt_path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        std::fs::write(&corrupt_path, "{ broken json: true, ").expect("Write corrupted file");

        // Loading corrupted config must NOT crash; it should fall back to default items
        let loaded = load_toolbar_config(Some(&temp_dir));
        let defaults = default_toolbar_items();
        assert_eq!(loaded.len(), defaults.len());
        assert_eq!(loaded[0].command_id(), defaults[0].command_id());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_and_load_roundtrip_on_disk() {
        let temp_dir = std::env::temp_dir().join(format!("vibe_test_tb_disk_{}", uuid::Uuid::new_v4().simple()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let test_items = vec![
            ToolbarItem::button("tb-custom-1", "editor:bold", "bold", Some("Bold text")),
            ToolbarItem::button("tb-custom-2", "editor:italic", "italic", Some("Italic text")),
        ];

        save_toolbar_config(&temp_dir, &test_items).expect("Save config");
        let loaded = load_toolbar_config(Some(&temp_dir));

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "tb-custom-1");
        assert_eq!(loaded[0].command_id(), Some("editor:bold"));
        assert_eq!(loaded[1].id, "tb-custom-2");
        assert_eq!(loaded[1].command_id(), Some("editor:italic"));

        // Reset to defaults
        let reset_items = reset_toolbar_config(&temp_dir).expect("Reset config");
        let defaults = default_toolbar_items();
        assert_eq!(reset_items.len(), defaults.len());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_depth_pruning_protects_against_deeply_nested_json() {
        // Build a chain with depth 7
        let level6 = vec![ToolbarItem::button("leaf", "editor:bold", "bold", None)];
        let level5 = vec![ToolbarItem::dropdown("d5", "icon", None, None, level6)];
        let level4 = vec![ToolbarItem::dropdown("d4", "icon", None, None, level5)];
        let level3 = vec![ToolbarItem::dropdown("d3", "icon", None, None, level4)];
        let level2 = vec![ToolbarItem::dropdown("d2", "icon", None, None, level3)];
        let level1 = vec![ToolbarItem::dropdown("d1", "icon", None, None, level2)];
        let root = vec![ToolbarItem::dropdown("d0", "icon", None, None, level1)];

        let pruned = validate_and_prune_items(root, 1);
        assert_eq!(pruned.len(), 1);

        // Check depth limit
        let mut depth = 0;
        let mut curr = &pruned[0];
        while let ToolbarItemKind::Dropdown { items, .. } = &curr.kind {
            depth += 1;
            if let Some(first) = items.first() {
                curr = first;
            } else {
                break;
            }
        }
        assert!(depth <= MAX_TOOLBAR_DROPDOWN_DEPTH, "Depth {} exceeded max {}", depth, MAX_TOOLBAR_DROPDOWN_DEPTH);
    }
