    use super::*;

    #[test]
    fn test_settings_tab_keys_and_titles() {
        let comm_tab = SettingsTab::CommunityPlugins;
        assert_eq!(comm_tab.key(), "settings.tabs.community_plugins");
        assert_eq!(comm_tab.title("ru"), "Сторонние плагины");
        assert_eq!(comm_tab.title("en"), "Community plugins");
        assert_eq!(comm_tab.title("de"), "Community plugins");

        let core_tab = SettingsTab::CorePlugins;
        assert_eq!(core_tab.key(), "settings.tabs.core_plugins");
        assert_eq!(core_tab.title("ru"), "Встроенные плагины");
        assert_eq!(core_tab.title("en"), "Core plugins");

        let toolbar_tab = SettingsTab::PluginSettings("editor-toolbar".to_string());
        assert_eq!(toolbar_tab.title("ru"), "Панель инструментов");
        assert_eq!(toolbar_tab.title("en"), "Editor Toolbar");

        let unnamed_tab = SettingsTab::PluginSettings(String::new());
        assert_eq!(unnamed_tab.title("en"), "Plugin settings");

        let custom_plugin_tab = SettingsTab::PluginSettings("obsidian-new-note-np".to_string());
        assert_eq!(custom_plugin_tab.key(), "settings.tabs.plugin_settings");
        assert_eq!(custom_plugin_tab.title("ru"), "obsidian-new-note-np");
    }

    #[test]
    fn test_plugin_readme_file_discovery_and_fallback() {
        let unique_name = format!("vibecodium_test_plugin_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let temp_dir = std::env::temp_dir().join(unique_name);
        let plugin_dir = temp_dir.join("my-test-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        // 1. Without README
        let find_readme = |dir: &std::path::Path| -> Option<String> {
            let p1 = dir.join("README.md");
            if p1.is_file() {
                return std::fs::read_to_string(p1).ok();
            }
            let p2 = dir.join("readme.md");
            if p2.is_file() {
                return std::fs::read_to_string(p2).ok();
            }
            None
        };

        assert!(find_readme(&plugin_dir).is_none());

        // 2. With lowercase readme.md
        std::fs::write(plugin_dir.join("readme.md"), "# Hello Plugin\n- Feature 1").unwrap();
        let read = find_readme(&plugin_dir);
        assert!(read.is_some());
        assert!(read.unwrap().contains("# Hello Plugin"));

        // 3. With uppercase README.md takes priority
        std::fs::write(plugin_dir.join("README.md"), "# Upper README").unwrap();
        let read_upper = find_readme(&plugin_dir);
        assert!(read_upper.is_some());
        assert_eq!(read_upper.unwrap(), "# Upper README");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_markdown_code_block_and_span_parsing() {
        let md = "# Title\n\n```text\nfolder/file.js\n└── styles.css\n```\n\n- [x] Done task\n- [ ] Todo task\n- Bullet item with `inline code` and [Link](https://example.com)\n\nRegular paragraph.";
        let mut lines = md.lines().peekable();
        let mut code_blocks = Vec::new();
        let mut tasks = Vec::new();

        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                let mut block_lines = Vec::new();
                while let Some(next_line) = lines.peek() {
                    if next_line.trim().starts_with("```") {
                        lines.next();
                        break;
                    }
                    block_lines.push(lines.next().unwrap());
                }
                code_blocks.push(block_lines.join("\n"));
            } else if trimmed.starts_with("- [x] ") {
                tasks.push((true, trimmed.strip_prefix("- [x] ").unwrap()));
            } else if trimmed.starts_with("- [ ] ") {
                tasks.push((false, trimmed.strip_prefix("- [ ] ").unwrap()));
            }
        }

        assert_eq!(code_blocks.len(), 1);
        assert_eq!(code_blocks[0], "folder/file.js\n└── styles.css");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0], (true, "Done task"));
        assert_eq!(tasks[1], (false, "Todo task"));
    }

    #[test]
    fn test_settings_tab_navigation_and_variants() {
        let tabs = vec![
            SettingsTab::Ui,
            SettingsTab::Editor,
            SettingsTab::Shortcuts,
            SettingsTab::CorePlugins,
            SettingsTab::CommunityPlugins,
            SettingsTab::System,
            SettingsTab::PluginSettings("dataview".to_string()),
        ];
        assert_eq!(tabs.len(), 7);
        assert!(matches!(tabs[0], SettingsTab::Ui));
        assert!(matches!(tabs[6], SettingsTab::PluginSettings(ref id) if id == "dataview"));
    }

    #[test]
    fn test_toolbar_scroll_handle_and_drag_math() {
        let style = crate::views::scrollbar::ScrollbarStyle::default();
        let mut drag = crate::views::scrollbar::ScrollbarDrag::default();

        assert!(!drag.active);

        // Simulated bounds: track length 240, max offset 1000, scroll at 0
        let geom = crate::views::scrollbar::ScrollbarGeometry::new(
            0.0,
            240.0,
            1000.0,
            0.0,
            &style,
        ).expect("should compute geometry");

        assert_eq!(geom.track_h, 240.0);
        assert_eq!(geom.max_offset, 1000.0);
        assert_eq!(geom.scroll_y, 0.0);
        assert!(geom.thumb_h >= style.min_thumb_h);
        assert_eq!(geom.thumb_top, 0.0);

        // Click on track at y = 120 (middle of 240px track)
        let clicked_target = geom.offset_for_track_click(120.0);
        assert!(clicked_target > 400.0 && clicked_target < 600.0);

        // Drag begins
        drag.begin(50.0, 0.0);
        assert!(drag.active);
        assert_eq!(drag.start_y, 50.0);
        assert_eq!(drag.start_scroll, 0.0);

        // Pointer moves 20px down
        let new_offset = geom.offset_after_drag(&drag, 70.0);
        assert!(new_offset > 0.0);

        // End drag
        drag.end();
        assert!(!drag.active);
    }

    #[test]
    fn test_toolbar_dropdown_scroll_wheel_offset_clamping() {
        let max_height = 400.0f32;
        let line_h = 36.0f32;

        // Start at top (offset = 0)
        let mut cur_offset = 0.0f32;

        // Scroll down 2 lines (dy = -72.0)
        let dy = -2.0 * line_h;
        cur_offset = (cur_offset + dy).clamp(-max_height, 0.0);
        assert_eq!(cur_offset, -72.0);

        // Scroll down beyond max_height (e.g. 15 lines)
        let dy_large = -15.0 * line_h;
        cur_offset = (cur_offset + dy_large).clamp(-max_height, 0.0);
        assert_eq!(cur_offset, -max_height);

        // Scroll up 1 line (dy = +36.0)
        let dy_up = 1.0 * line_h;
        cur_offset = (cur_offset + dy_up).clamp(-max_height, 0.0);
        assert_eq!(cur_offset, -max_height + 36.0);

        // Scroll up beyond top
        let dy_up_large = 20.0 * line_h;
        cur_offset = (cur_offset + dy_up_large).clamp(-max_height, 0.0);
        assert_eq!(cur_offset, 0.0);
    }

    #[test]
    fn test_is_plugin_command_and_type_icon() {
        // Built-in commands with owner: None
        assert!(!is_plugin_command("editor:save", None));
        assert!(!is_plugin_command("workspace:new-tab", None));
        assert!(!is_plugin_command("view:toggle-left-sidebar", None));
        assert_eq!(command_type_icon(is_plugin_command("editor:save", None)), icons::ICON_APP_WINDOW);

        // Plugin commands with owner: Some(plugin_id)
        assert!(is_plugin_command("editor:custom-cmd", Some("my-plugin")));
        assert!(is_plugin_command("git-commit", Some("obsidian-git")));
        assert_eq!(command_type_icon(is_plugin_command("git-commit", Some("obsidian-git"))), icons::ICON_PLUG);

        // Fallback heuristic for unknown owner
        assert!(is_plugin_command("obsidian-git:pull", None));
        assert!(!is_plugin_command("editor:toggle-source", None));
    }

    /// Returns the string literals on a source line, ignoring comments.
    fn string_literals(line: &str) -> Vec<&str> {
        let bytes = line.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'/' if bytes.get(i + 1) == Some(&b'/') => break,
                b'"' => {
                    let start = i + 1;
                    let mut end = start;
                    while end < bytes.len() && bytes[end] != b'"' {
                        if bytes[end] == b'\\' {
                            end += 1;
                        }
                        end += 1;
                    }
                    out.push(&line[start..end.min(line.len())]);
                    i = end + 1;
                }
                _ => i += 1,
            }
        }
        out
    }

    fn has_cyrillic_word(literal: &str) -> bool {
        literal.chars().count() > 1 && literal.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
    }

    #[test]
    fn settings_module_has_no_hardcoded_ui_text() {
        let sources = [
            ("tab.rs", include_str!("tab.rs")),
            ("tabs.rs", include_str!("tabs.rs")),
            ("ui.rs", include_str!("ui.rs")),
            ("plugin_toolbar.rs", include_str!("plugin_toolbar.rs")),
        ];

        for (name, src) in sources {
            let mut allow_next_line = false;
            for (index, line) in src.lines().enumerate() {
                if allow_next_line {
                    allow_next_line = false;
                    continue;
                }
                if line.contains("i18n-allow") {
                    allow_next_line = true;
                    continue;
                }
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for literal in string_literals(line) {
                    assert!(
                        !has_cyrillic_word(literal),
                        "{name}:{} has hardcoded UI text: {literal:?}",
                        index + 1
                    );
                }
            }
        }
    }

    #[test]
    fn settings_module_has_no_russian_only_switch() {
        let sources = [
            ("tab.rs", include_str!("tab.rs")),
            ("tabs.rs", include_str!("tabs.rs")),
            ("ui.rs", include_str!("ui.rs")),
            ("plugin_toolbar.rs", include_str!("plugin_toolbar.rs")),
        ];

        for (name, src) in sources {
            for (index, line) in src.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                assert!(
                    !line.contains("starts_with(\"ru\")"),
                    "{name}:{} must not branch on the Russian locale",
                    index + 1
                );
            }
        }
    }
