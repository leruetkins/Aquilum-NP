    use super::*;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("aquilum_plugin_test_{}_{}", name, uuid::Uuid::new_v4()));
            let _ = fs::create_dir_all(&path);
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_discover_plugins_success() {
        let temp = TempDir::new("success");
        let plugin_dir = temp.path.join("my-cool-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest_json = r#"{
            "id": "my-cool-plugin",
            "name": "My Cool Plugin",
            "version": "1.0.0",
            "description": "Test plugin description",
            "author": "Alice"
        }"#;
        fs::write(plugin_dir.join("manifest.json"), manifest_json).unwrap();
        fs::write(plugin_dir.join("main.js"), "console.log('Plugin init');").unwrap();

        let discovered = discover_plugins(&temp.path);
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].manifest.id, "my-cool-plugin");
        assert_eq!(discovered[0].manifest.name, "My Cool Plugin");
    }

    #[test]
    fn test_read_styles_css_absent_is_none() {
        let temp = TempDir::new("css_absent");
        assert_eq!(read_styles_css(&temp.path), None);
    }

    #[test]
    fn test_read_styles_css_reads_file() {
        let temp = TempDir::new("css_present");
        fs::write(temp.path.join("styles.css"), ".panel { color: #fff; }").unwrap();
        assert_eq!(
            read_styles_css(&temp.path).as_deref(),
            Some(".panel { color: #fff; }")
        );
    }

    #[test]
    fn test_discover_plugins_missing_main_js_ignored() {
        let temp = TempDir::new("missing_main");
        let plugin_dir = temp.path.join("broken-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest_json = r#"{
            "id": "broken-plugin",
            "name": "Broken",
            "version": "1.0.0"
        }"#;
        fs::write(plugin_dir.join("manifest.json"), manifest_json).unwrap();
        // main.js is intentionally missing

        let discovered = discover_plugins(&temp.path);
        assert_eq!(discovered.len(), 0);
    }

    #[test]
    fn test_load_and_run_installed_plugin_full_lifecycle() {
        let temp = TempDir::new("lifecycle");
        let plugin_dir = temp.path.join("counter-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest_json = r#"{
            "id": "counter-plugin",
            "name": "Counter Plugin",
            "version": "1.2.0",
            "description": "Lifecycle test plugin",
            "author": "Antigravity",
            "permissions": {
                "vault": "read"
            }
        }"#;
        fs::write(plugin_dir.join("manifest.json"), manifest_json).unwrap();

        let script_js = r#"
            const plugin = new Plugin(app, { id: "counter-plugin" });
            new Notice("Counter started");
            plugin.registerMarkdownCodeBlockProcessor("counter", (source) => {
                return {
                    type: "text",
                    value: "Count: " + source.trim().length
                };
            });
        "#;
        fs::write(plugin_dir.join("main.js"), script_js).unwrap();

        let host_state = Arc::new(PluginHostState::new());
        let (api_tx, mut api_rx) = futures::channel::mpsc::unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(crate::state::Snapshot::default()));

        let enabled_list = vec!["counter-plugin".to_string()];
        let handles = load_installed_plugins(
            &temp.path,
            Some(&enabled_list),
            api_tx,
            snapshot,
            &host_state,
        );

        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].manifest.id, "counter-plugin");
        assert!(host_state.is_plugin_registered("counter-plugin"));

        std::thread::sleep(std::time::Duration::from_millis(60));

        // 1. Notice request received by host
        let notice_req = api_rx.try_recv().unwrap();
        assert_eq!(notice_req.plugin_id, "counter-plugin");
        assert!(matches!(notice_req.request, crate::plugins::api::ApiRequest::Notice { .. }));

        // 2. RegisterCodeBlockProcessor received by host
        let reg_req = api_rx.try_recv().unwrap();
        assert_eq!(reg_req.plugin_id, "counter-plugin");
        if let crate::plugins::api::ApiRequest::RegisterCodeBlockProcessor { language } = reg_req.request {
            assert_eq!(language, "counter");
        } else {
            panic!("Expected RegisterCodeBlockProcessor");
        }

        // 3. Send ProcessCodeBlock event to plugin
        let _ = handles[0].host_events_tx.unbounded_send(crate::plugins::api::HostEvent::ProcessCodeBlock {
            language: "counter".to_string(),
            source: "hello world".to_string(),
            block_id: 101,
        });

        std::thread::sleep(std::time::Duration::from_millis(60));

        // 4. Plugin responds with SetCodeBlockView
        let view_req = api_rx.try_recv().unwrap();
        assert_eq!(view_req.plugin_id, "counter-plugin");
        if let crate::plugins::api::ApiRequest::SetCodeBlockView { block_id, tree } = view_req.request {
            assert_eq!(block_id, 101);
            if let crate::plugins::ui::UiNode::Text { value, .. } = tree {
                assert_eq!(value, "Count: 11");
            } else {
                panic!("Expected UiNode::Text");
            }
        } else {
            panic!("Expected SetCodeBlockView");
        }

        // 5. Unload plugin
        let _ = handles[0].host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
        host_state.unregister_plugin("counter-plugin");
        assert!(!host_state.is_plugin_registered("counter-plugin"));
    }

    /// End-to-end styles check: styles.css → loader → runtime → shim → NodeStyle.
    /// Without it the chain is only tested piecemeal, and a swapped argument cannot be caught.
    #[test]
    fn test_styles_css_reaches_the_rendered_tree() {
        let temp = TempDir::new("css_e2e");
        let plugin_dir = temp.path.join("styled-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();

        fs::write(
            plugin_dir.join("manifest.json"),
            r#"{
                "id": "styled-plugin",
                "name": "Styled",
                "version": "1.0.0"
            }"#,
        )
        .unwrap();
        fs::write(
            plugin_dir.join("styles.css"),
            ".counter-value { color: #ff0000; padding: 4px 6px; }",
        )
        .unwrap();
        fs::write(
            plugin_dir.join("main.js"),
            r#"
                const plugin = new Plugin(app, { id: "styled-plugin" });
                plugin.registerMarkdownCodeBlockProcessor("counter", (source) => {
                    const el = createDiv({ cls: "counter-value" });
                    el.setText("Count: " + source.trim().length);
                    return __velement_to_uinode(el);
                });
            "#,
        )
        .unwrap();

        let host_state = Arc::new(PluginHostState::new());
        let (api_tx, mut api_rx) = futures::channel::mpsc::unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(crate::state::Snapshot::default()));

        let handles = load_installed_plugins(&temp.path, None, api_tx, snapshot, &host_state);
        assert_eq!(handles.len(), 1);

        std::thread::sleep(std::time::Duration::from_millis(80));
        let _ = handles[0]
            .host_events_tx
            .unbounded_send(crate::plugins::api::HostEvent::ProcessCodeBlock {
                language: "counter".to_string(),
                source: "hello world".to_string(),
                block_id: 7,
            });
        std::thread::sleep(std::time::Duration::from_millis(80));

        let mut tree = None;
        while let Ok(req) = api_rx.try_recv() {
            if let crate::plugins::api::ApiRequest::SetCodeBlockView { block_id, tree: t } = req.request {
                if block_id == 7 {
                    tree = Some(t);
                    break;
                }
            }
        }
        let tree = tree.expect("плагин не вернул дерево для блока");

        match tree {
            crate::plugins::ui::UiNode::Styled { style, .. } => {
                assert_eq!(style.color, Some(0xff0000ff), "цвет из styles.css не дошёл");
                assert_eq!(
                    style.padding,
                    Some(crate::plugins::ui::Sides {
                        top: 4.0,
                        right: 6.0,
                        bottom: 4.0,
                        left: 6.0
                    })
                );
            }
            other => panic!("ожидался styled-узел, получено {:?}", other),
        }
    }
