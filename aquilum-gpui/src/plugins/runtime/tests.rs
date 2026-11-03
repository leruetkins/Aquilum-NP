    use super::*;
    use crate::plugins::manifest::PluginPermissions;

    #[test]
    fn test_quickjs_runtime_eval_notice() {
        let manifest = PluginManifest {
            id: "notice-plugin".into(),
            name: "Notice Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            new Notice("Hello from QuickJS!");
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        // Give the background worker thread a moment to initialize and execute
        std::thread::sleep(Duration::from_millis(50));

        // The script should immediately emit an ApiRequest::Notice to api_rx
        let received = api_rx.try_recv().unwrap();
        assert_eq!(received.plugin_id, "notice-plugin");
        if let ApiRequest::Notice { text, .. } = received.request {
            assert_eq!(text, "Hello from QuickJS!");
        } else {
            panic!("Expected ApiRequest::Notice");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_quickjs_async_vault_read_promise_resolution() {
        let manifest = PluginManifest {
            id: "read-plugin".into(),
            name: "Read Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        // The plugin calls async app.vault.read, and upon promise resolution, calls Notice with the result
        let script = r#"
            (async () => {
                const content = await app.vault.read("Notes/Daily.md");
                new Notice("Received: " + content);
            })();
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // 1. Host receives VaultRead request
        let read_req = api_rx.try_recv().unwrap();
        assert_eq!(read_req.plugin_id, "read-plugin");
        if let ApiRequest::VaultRead { path } = read_req.request {
            assert_eq!(path, "Notes/Daily.md");
        } else {
            panic!("Expected ApiRequest::VaultRead");
        }

        // 2. Host sends Response back to plugin worker
        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
            read_req.request_id,
            serde_json::json!("Plan for today"),
        ));

        std::thread::sleep(Duration::from_millis(50));

        // 3. Plugin promise resolves and sends Notice("Received: Plan for today")
        let notice_req = api_rx.try_recv().unwrap();
        if let ApiRequest::Notice { text, .. } = notice_req.request {
            assert_eq!(text, "Received: Plan for today");
        } else {
            panic!("Expected ApiRequest::Notice with resolved content");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_quickjs_load_and_save_data() {
        let manifest = PluginManifest {
            id: "data-plugin".into(),
            name: "Data Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            (async () => {
                const plugin = new Plugin(app, { id: "data-plugin" });
                await plugin.saveData({ theme: "dark", count: 10 });
                const loaded = await plugin.loadData();
                new Notice("Theme: " + loaded.theme);
            })();
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // 1. Host receives SaveData request
        let save_req = api_rx.try_recv().unwrap();
        assert_eq!(save_req.plugin_id, "data-plugin");
        if let ApiRequest::SaveData { data } = save_req.request {
            assert_eq!(data["theme"], "dark");
            assert_eq!(data["count"], 10);
        } else {
            panic!("Expected ApiRequest::SaveData");
        }

        // Fulfill SaveData
        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
            save_req.request_id,
            serde_json::Value::Null,
        ));

        std::thread::sleep(Duration::from_millis(50));

        // 2. Host receives LoadData request
        let load_req = api_rx.try_recv().unwrap();
        assert_eq!(load_req.plugin_id, "data-plugin");
        assert!(matches!(load_req.request, ApiRequest::LoadData));

        // Fulfill LoadData with JSON response
        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
            load_req.request_id,
            serde_json::json!({ "theme": "dark", "count": 10 }),
        ));

        std::thread::sleep(Duration::from_millis(50));

        // 3. Notice is emitted
        let notice_req = api_rx.try_recv().unwrap();
        if let ApiRequest::Notice { text, .. } = notice_req.request {
            assert_eq!(text, "Theme: dark");
        } else {
            panic!("Expected ApiRequest::Notice with loaded theme");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_quickjs_register_markdown_code_block_processor() {
        let manifest = PluginManifest {
            id: "dataview-plugin".to_string(),
            name: "Dataview".to_string(),
            version: "0.5.0".to_string(),
            min_app_version: None,
            author: "Tester".to_string(),
            description: "Dataview query processor".to_string(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const plugin = new Plugin(app, { id: "dataview-plugin" });
            plugin.registerMarkdownCodeBlockProcessor("dataview", (source) => {
                return {
                    type: "column",
                    children: [
                        { type: "text", value: "Query result for: " + source.trim() }
                    ]
                };
            });
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // 1. Host receives RegisterCodeBlockProcessor
        let reg_req = api_rx.try_recv().unwrap();
        assert_eq!(reg_req.plugin_id, "dataview-plugin");
        if let ApiRequest::RegisterCodeBlockProcessor { language } = reg_req.request {
            assert_eq!(language, "dataview");
        } else {
            panic!("Expected ApiRequest::RegisterCodeBlockProcessor");
        }

        // 2. Host sends ProcessCodeBlock
        let _ = handle.host_events_tx.unbounded_send(HostEvent::ProcessCodeBlock {
            language: "dataview".to_string(),
            source: "TABLE file.name FROM #project".to_string(),
            block_id: 42,
        });

        std::thread::sleep(Duration::from_millis(50));

        // 3. Host receives SetCodeBlockView with processed UI tree
        let view_req = api_rx.try_recv().unwrap();
        assert_eq!(view_req.plugin_id, "dataview-plugin");
        if let ApiRequest::SetCodeBlockView { block_id, tree } = view_req.request {
            assert_eq!(block_id, 42);
            if let crate::plugins::ui::UiNode::Column { children, .. } = tree {
                assert_eq!(children.len(), 1);
                if let crate::plugins::ui::UiNode::Text { value, .. } = &children[0] {
                    assert_eq!(value, "Query result for: TABLE file.name FROM #project");
                } else {
                    panic!("Expected UiNode::Text child");
                }
            } else {
                panic!("Expected UiNode::Column");
            }
        } else {
            panic!("Expected ApiRequest::SetCodeBlockView");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_quickjs_class_based_obsidian_plugin_lifecycle() {
        let manifest = PluginManifest {
            id: "new-note-np".to_string(),
            name: "New Note NP".to_string(),
            version: "1.0.0".to_string(),
            min_app_version: Some("1.5.0".to_string()),
            author: "leruetkins".to_string(),
            description: "Adds a Create new note button back to the left ribbon.".to_string(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        // The exact code structure from user's plugin D:\Naben\Vibecodium\.vault\plugins\obsidian-new-note-np\main.js
        let script = r#"
            const { Plugin, Notice } = require("obsidian");

            module.exports = class NewNoteRibbonPlugin extends Plugin {
                async onload() {
                    this.addRibbonIcon("square-pen", "Новая заметка", async () => {
                        try {
                            await this.app.commands.executeCommandById("file-explorer:new-file");
                        } catch (error) {
                            new Notice("Не удалось создать заметку");
                        }
                    });
                }
            };
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // 1. Host receives AddRibbonIcon from plugin's onload()
        let ribbon_req = api_rx.try_recv().unwrap();
        assert_eq!(ribbon_req.plugin_id, "new-note-np");
        let (icon_id, icon_name, icon_title) = match ribbon_req.request {
            ApiRequest::AddRibbonIcon { id, icon, title } => (id, icon, title),
            other => panic!("Expected AddRibbonIcon, got {:?}", other),
        };
        assert_eq!(icon_name, "square-pen");
        assert_eq!(icon_title, "Новая заметка");

        // Fulfill AddRibbonIcon response
        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
            ribbon_req.request_id,
            serde_json::Value::Null,
        ));

        // 2. Simulate ribbon icon click: host triggers ExecuteCommand with registered icon_id
        let _ = handle.host_events_tx.unbounded_send(HostEvent::ExecuteCommand {
            command_id: icon_id,
        });

        std::thread::sleep(Duration::from_millis(50));

        // 3. Plugin callback triggers executeCommandById("file-explorer:new-file")
        let exec_req = api_rx.try_recv().unwrap();
        assert_eq!(exec_req.plugin_id, "new-note-np");
        match exec_req.request {
            ApiRequest::ExecuteCommand { id } => {
                assert_eq!(id, "file-explorer:new-file");
            }
            other => panic!("Expected ExecuteCommand, got {:?}", other),
        }

        // Fulfill command execution
        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
            exec_req.request_id,
            serde_json::Value::Null,
        ));

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_snapshot_permission_enforcement() {
        use crate::plugins::manifest::VaultAccess;

        let manifest = PluginManifest {
            id: "untrusted-plugin".into(),
            name: "Untrusted".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions {
                vault: VaultAccess::None, // NO vault access
                network: vec![],
            },
        };

        let (api_tx, mut api_rx) = unbounded();
        let mut snapshot_data = Snapshot::default();
        snapshot_data.files = Arc::new([crate::vault::FileInfo {
            path: "Secret/Password.md".into(),
            name: "Password.md".into(),
            basename: "Password".into(),
            extension: "md".into(),
        }]);
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snapshot_data));

        let script = r#"
            const files = app.vault.getMarkdownFiles();
            new Notice("Files count: " + files.length);
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let notice_req = api_rx.try_recv().unwrap();
        if let ApiRequest::Notice { text, .. } = notice_req.request {
            // Must be 0 because vault:none cannot view snapshot files
            assert_eq!(text, "Files count: 0");
        } else {
            panic!("Expected Notice");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_setting_tab_and_velement_flow() {
        let manifest = PluginManifest {
            id: "setting-plugin".into(),
            name: "Setting Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, PluginSettingTab, Setting } = require("obsidian");

            class SampleSettingTab extends PluginSettingTab {
                display() {
                    const { containerEl } = this;
                    containerEl.empty();
                    new Setting(containerEl)
                        .setName("Enable Autocomplete")
                        .setDesc("Provides inline suggestions")
                        .addToggle((toggle) => toggle.setValue(true));
                }
            }

            module.exports = class TestPlugin extends Plugin {
                async onload() {
                    this.addSettingTab(new SampleSettingTab(this.app, this));
                }
            };
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(60));

        let view_tree_call = api_rx.try_recv().expect("Expected SetViewTree call");
        if let ApiRequest::SetViewTree { tree, .. } = view_tree_call.request {
            let json = serde_json::to_string(&tree).unwrap();
            assert!(json.contains("Enable Autocomplete"));
            assert!(json.contains("Provides inline suggestions"));
            assert!(json.contains("\"type\":\"setting_item\""));
            assert!(json.contains("\"type\":\"toggle\""));
        } else {
            panic!("Expected SetViewTree request, got {:?}", view_tree_call.request);
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_lazy_fail_and_audited_proxy() {
        let manifest = PluginManifest {
            id: "proxy-test-plugin".into(),
            name: "Proxy Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            // 1. External module require returns lazyFail proxy instead of throwing
            const cm = require("@codemirror/view");
            const hasCm = typeof cm === "function";

            // 2. Feature detection on app: missing property returns undefined
            const missing = typeof app.nonExistentFeature === "undefined";

            // 3. Known-safe member resolves; a truly missing member returns undefined
            // (the audited proxy never throws on reads).
            const noopResolved = typeof app.workspace.requestSaveLayout === "function";
            const unknownMissing = app.workspace.__no_such_member__ === undefined;

            new Notice(`CM:${hasCm}, Missing:${missing}, Noop:${noopResolved}, Unk:${unknownMissing}`);
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let notice_call = api_rx.try_recv().expect("Expected Notice call");
        if let ApiRequest::Notice { text, .. } = notice_call.request {
            assert_eq!(text, "CM:true, Missing:true, Noop:true, Unk:true");
        } else {
            panic!("Expected Notice request");
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_class_extension_and_fallback_stubs() {
        let manifest = PluginManifest {
            id: "class-extension-test".into(),
            name: "Class Extension Test".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const obsidian = require("obsidian");

            // Standard Obsidian classes
            class MyView extends obsidian.View {}
            class MyComponent extends obsidian.Component {}
            class MyModal extends obsidian.Modal {}
            class MySuggestModal extends obsidian.SuggestModal {}

            // Arbitrary unmodeled Obsidian classes (like Homepage or other plugins)
            class CustomObsidianExtension extends obsidian.WorkspaceRibbon {}
            class AnotherUnknownClass extends obsidian.NonExistentClass123 {}

            const v = new MyView();
            const c = new MyComponent();
            const ext = new CustomObsidianExtension();
            const unk = new AnotherUnknownClass();

            new Notice(`SUCCESS:view=${typeof v},comp=${typeof c},ext=${typeof ext},unk=${typeof unk}`);
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let notice_call = api_rx.try_recv().expect("Expected Notice call");
        if let ApiRequest::Notice { text, .. } = notice_call.request {
            assert_eq!(text, "SUCCESS:view=object,comp=object,ext=object,unk=object");
        } else {
            panic!("Expected Notice request, got {:?}", notice_call.request);
        }

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_menu_and_editor_context_menu_dispatch() {
        let manifest = PluginManifest {
            id: "menu-test-plugin".into(),
            name: "Menu Test Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin } = require("obsidian");

            class MyPlugin extends Plugin {
                async onload() {
                    this.registerEvent(
                        this.app.workspace.on("editor-menu", (menu, editor) => {
                            const sel = editor.getSelection();
                            menu.addItem((item) => {
                                item.setTitle("Convert " + sel).setIcon("text");
                                const sub = item.setSubmenu();
                                sub.addItem((subItem) => {
                                    subItem.setTitle("Sub Action").onClick(() => {
                                        editor.replaceSelection("REPLACED:" + sel);
                                    });
                                });
                            });
                        })
                    );
                }
            }
            module.exports = MyPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // Dispatch EditorMenu event
        let session_id = 999;
        let _ = handle.host_events_tx.unbounded_send(HostEvent::EditorMenu {
            menu_session_id: session_id,
            selected_text: "123.45".to_string(),
        });

        std::thread::sleep(Duration::from_millis(50));

        let mut menu_items_res = None;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::Unknown { req_kind } = call.request {
                if let Some(json_str) = req_kind.strip_prefix("editor_menu_result:") {
                    menu_items_res = Some(json_str.to_string());
                    break;
                }
            }
        }

        let json = menu_items_res.expect("Expected editor_menu_result from plugin");
        assert!(json.contains("Convert 123.45"));
        assert!(json.contains("Sub Action"));

        // Extract sub action item id and click it
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let sub_id = parsed[0]["submenu"][0]["id"].as_str().unwrap().to_string();

        let _ = handle.host_events_tx.unbounded_send(HostEvent::ExecuteMenuAction {
            action_id: sub_id,
        });

        std::thread::sleep(Duration::from_millis(50));

        let mut replaced_text = None;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::EditorReplaceSelection { text } = call.request {
                replaced_text = Some(text);
                break;
            }
        }

        assert_eq!(replaced_text, Some("REPLACED:123.45".to_string()));

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_get_language() {
        let manifest = PluginManifest {
            id: "lang-test-plugin".into(),
            name: "Lang Test Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let mut snap = Snapshot::default();
        snap.language = "ru".to_string();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snap));

        let script = r#"
            const { Plugin, Notice, getLanguage, moment } = require("obsidian");

            class LangPlugin extends Plugin {
                async onload() {
                    const langFromObsidian = getLanguage();
                    const langFromGlobal = globalThis.getLanguage ? globalThis.getLanguage() : "none";
                    const momentLocale = moment.locale ? moment.locale() : "none";
                    const winMomentLocale = window.moment ? window.moment.locale() : "none";
                    new Notice(`LANG:${langFromObsidian}|${langFromGlobal}|${momentLocale}|${winMomentLocale}`);
                }
            }
            module.exports = LangPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let mut notice_text = None;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::Notice { text, .. } = call.request {
                notice_text = Some(text);
                break;
            }
        }

        assert_eq!(
            notice_text,
            Some("LANG:ru|ru|ru|ru".to_string())
        );

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_interactive_settings_tab_toggle_and_modal_lifecycle() {
        let manifest = PluginManifest {
            id: "interactive-settings-test".into(),
            name: "Interactive Settings".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, PluginSettingTab, Setting, Modal } = require("obsidian");

            class EditModal extends Modal {
                constructor(app) {
                    super(app);
                }
                onOpen() {
                    this.titleEl.setText("Currency Editor");
                }
            }

            class MySettingTab extends PluginSettingTab {
                constructor(app, plugin) {
                    super(app, plugin);
                }
                display() {
                    const { containerEl } = this;
                    containerEl.empty();
                    new Setting(containerEl)
                        .setName("Auto Detect")
                        .addToggle(t => {
                            t.setValue(this.plugin.settings.autoDetect);
                            t.onChange(val => {
                                this.plugin.settings.autoDetect = val;
                            });
                        });
                    new Setting(containerEl)
                        .setName("Edit Currency")
                        .addButton(b => {
                            b.setButtonText("Edit");
                            b.onClick(() => {
                                new EditModal(this.app).open();
                            });
                        });
                }
            }

            class InteractivePlugin extends Plugin {
                async onload() {
                    this.settings = { autoDetect: false };
                    this.addSettingTab(new MySettingTab(this.app, this));
                }
            }
            module.exports = InteractivePlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // 1. Initial SetViewTree for view_id: 0
        let mut toggle_id = None;
        let mut button_id = None;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetViewTree { view_id, tree } = call.request {
                if view_id == 0 {
                    let json = serde_json::to_string(&tree).unwrap();
                    assert!(json.contains("Auto Detect"));
                    assert!(json.contains("\"value\":false"));
                    // Find toggle node id
                    if let crate::plugins::ui::UiNode::Column { children, .. } = tree {
                        for c in &children {
                            if let crate::plugins::ui::UiNode::SettingItem { name, control, .. } = c {
                                if name == "Auto Detect" {
                                    if let crate::plugins::ui::UiNode::Toggle { id, value } = &**control {
                                        assert_eq!(*value, false);
                                        toggle_id = Some(id.clone());
                                    }
                                } else if name == "Edit Currency" {
                                    if let crate::plugins::ui::UiNode::Button { id, .. } = &**control {
                                        button_id = Some(id.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let tog_id = toggle_id.expect("Toggle ID should be present");
        let btn_id = button_id.expect("Button ID should be present");

        // 2. Dispatch toggle change event from host
        let _ = handle.host_events_tx.unbounded_send(HostEvent::UiEvent {
            view_id: 0,
            node_id: tog_id,
            event: "change".to_string(),
            value: Some("true".to_string()),
        });

        std::thread::sleep(Duration::from_millis(50));

        // 3. Plugin should have re-synced SetViewTree with value: true
        let mut toggle_updated = false;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetViewTree { view_id, tree } = call.request {
                if view_id == 0 {
                    let json = serde_json::to_string(&tree).unwrap();
                    if json.contains("\"value\":true") {
                        toggle_updated = true;
                    }
                }
            }
        }
        assert!(toggle_updated, "Expected SetViewTree to be re-synced with toggle value: true");

        // 4. Dispatch button click event from host to open modal
        let _ = handle.host_events_tx.unbounded_send(HostEvent::UiEvent {
            view_id: 0,
            node_id: btn_id,
            event: "click".to_string(),
            value: None,
        });

        std::thread::sleep(Duration::from_millis(50));

        // 5. Plugin should have emitted SetViewTree for view_id: 1 with modal content
        let mut modal_opened = false;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetViewTree { view_id, tree } = call.request {
                if view_id == 1 {
                    let json = serde_json::to_string(&tree).unwrap();
                    if json.contains("Currency Editor") {
                        modal_opened = true;
                    }
                }
            }
        }
        assert!(modal_opened, "Expected SetViewTree for view_id: 1 with Currency Editor modal");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_setting_add_text_area() {
        let manifest = PluginManifest {
            id: "test-area-plugin".into(),
            name: "Test Area Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, PluginSettingTab, Setting, TextAreaComponent } = require('obsidian');

            class TestAreaPlugin extends Plugin {
                onload() {
                    this.addSettingTab(new AreaTab(this.app, this));
                }
            }

            class AreaTab extends PluginSettingTab {
                display() {
                    const { containerEl } = this;
                    containerEl.empty();
                    new Setting(containerEl)
                        .setName("Multiline Notes")
                        .setDesc("Enter long notes here")
                        .addTextArea(text => {
                            text.setPlaceholder("Notes...")
                                .setValue("Line 1\nLine 2")
                                .onChange(val => {});
                        });
                }
            }

            module.exports = TestAreaPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        std::thread::sleep(Duration::from_millis(50));

        let mut found_textarea = false;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetViewTree { view_id, tree } = call.request {
                if view_id == 0 {
                    let json = serde_json::to_string(&tree).unwrap();
                    if json.contains("\"type\":\"textarea\"") && json.contains("Multiline Notes") {
                        found_textarea = true;
                    }
                }
            }
        }
        assert!(found_textarea, "Expected SetViewTree containing setting with textarea input");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_setting_document_fragment_and_links() {
        let manifest = PluginManifest {
            id: "test-fragment-plugin".into(),
            name: "Test Fragment Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, PluginSettingTab, Setting } = require('obsidian');

            class TestFragmentPlugin extends Plugin {
                async onload() {
                    this.addSettingTab(new FragmentTab(this.app, this));
                }
            }

            class FragmentTab extends PluginSettingTab {
                display() {
                    const { containerEl } = this;
                    containerEl.empty();

                    const frag = activeDocument.createDocumentFragment();
                    const a = activeDocument.createElement("a");
                    a.href = "https://example.com/regex-guide";
                    a.text = "MDN Regex Guide";
                    frag.append("Prefix text. See ");
                    frag.append(a);
                    frag.append(" for details.");

                    new Setting(containerEl)
                        .setName("Fragment Setting")
                        .setDesc(frag)
                        .addText(t => t.setValue("val"));
                }
            }

            module.exports = TestFragmentPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        std::thread::sleep(Duration::from_millis(50));

        let mut found_valid_desc = false;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetViewTree { view_id, tree } = call.request {
                if view_id == 0 {
                    let json = serde_json::to_string(&tree).unwrap();
                    assert!(!json.contains("[object Object]"), "Found [object Object] in UI tree: {}", json);
                    if json.contains("Prefix text. See MDN Regex Guide for details.") {
                        found_valid_desc = true;
                    }
                }
            }
        }
        assert!(found_valid_desc, "Expected SetViewTree with concatenated text from DocumentFragment");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_obsidian_item_view_and_workspace_leaf_lifecycle() {
        let manifest = PluginManifest {
            id: "test-itemview-plugin".into(),
            name: "Test ItemView Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, ItemView } = require('obsidian');

            class CustomView extends ItemView {
                constructor(leaf) {
                    super(leaf);
                }
                getViewType() {
                    return "custom-tool";
                }
                getDisplayText() {
                    return "Tool Panel";
                }
                getIcon() {
                    return "hammer";
                }
                async onOpen() {
                    this.addAction("refresh", "Refresh Tool", () => {
                        this.containerEl.createDiv({ text: "Action Triggered" });
                    });
                    const container = this.contentEl;
                    container.createDiv({ text: "ItemView Body Text" });
                }
            }

            class TestItemViewPlugin extends Plugin {
                async onload() {
                    this.registerView("custom-tool", (leaf) => new CustomView(leaf));
                    const leaf = this.app.workspace.getRightLeaf(false);
                    await leaf.setViewState({ type: "custom-tool" });
                }
            }

            module.exports = TestItemViewPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        std::thread::sleep(Duration::from_millis(60));

        let mut found_leaf = false;
        let mut leaf_action_id = String::new();
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::SetLeafView { leaf_id: _, location, view_type, title, icon, tree, actions } = call.request {
                assert_eq!(location, "right");
                assert_eq!(view_type, "custom-tool");
                assert_eq!(title, "Tool Panel");
                assert_eq!(icon.as_deref(), Some("hammer"));
                assert_eq!(actions.len(), 1);
                assert_eq!(actions[0].icon, "refresh");
                assert_eq!(actions[0].title, "Refresh Tool");
                leaf_action_id = actions[0].id.clone();

                let json = serde_json::to_string(&tree).unwrap();
                assert!(json.contains("ItemView Body Text"), "tree should contain ItemView body text");
                found_leaf = true;
            }
        }
        assert!(found_leaf, "Expected SetLeafView from WorkspaceLeaf.setViewState");

        // Test triggering action
        let _ = handle.host_events_tx.unbounded_send(HostEvent::UiEvent {
            view_id: 0,
            node_id: leaf_action_id,
            event: "action_click".to_string(),
            value: None,
        });

        std::thread::sleep(Duration::from_millis(30));

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_load_actual_recent_files_script() {
        let path = std::path::Path::new(r"D:\Naben\Vibecodium\.vault\plugins\recent-files-obsidian\main.js");
        if !path.exists() {
            return;
        }
        let script = std::fs::read_to_string(path).unwrap();
        let manifest = PluginManifest {
            id: "recent-files-obsidian".into(),
            name: "Recent Files".into(),
            version: "1.3.11".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let handle = spawn_plugin_runtime(
            manifest,
            script,
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        let mut got_left_leaf = false;
        let mut got_reveal = false;

        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(600) {
            while let Ok(call) = api_rx.try_recv() {
                match call.request {
                    ApiRequest::LoadData => {
                        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(call.request_id, serde_json::json!({ "recentFiles": [] })));
                    }
                    ApiRequest::SetLeafView { ref location, ref view_type, ref icon, .. } => {
                        if location == "left" && view_type == "recent-files" {
                            got_left_leaf = true;
                            assert_eq!(icon.as_deref(), Some("clock"));
                        }
                    }
                    ApiRequest::RevealLeaf { .. } => {
                        got_reveal = true;
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(got_left_leaf, "Should have received SetLeafView for left leaf recent-files");
        assert!(got_reveal, "Should have received RevealLeaf for recent-files leaf");

        // Simulate opening a file in the app:
        let _ = handle.host_events_tx.unbounded_send(HostEvent::FileOpen {
            path: Some("notes/daily.md".to_string()),
        });
        std::thread::sleep(Duration::from_millis(100));

        let mut got_saved_data = false;
        while let Ok(call) = api_rx.try_recv() {
            match call.request {
                ApiRequest::SaveData { ref data } => {
                    let s = data.to_string();
                    if s.contains("daily.md") {
                        got_saved_data = true;
                    }
                }
                _ => {}
            }
        }
        assert!(got_saved_data, "recent-files should capture opened note and save updated data");

        // Simulate opening a second file in the app:
        let _ = handle.host_events_tx.unbounded_send(HostEvent::FileOpen {
            path: Some("notes/weekly.md".to_string()),
        });
        std::thread::sleep(Duration::from_millis(150));

        let mut got_leaf_view = false;
        let mut delete_btn_id = None;
        while let Ok(call) = api_rx.try_recv() {
            match call.request {
                ApiRequest::SetLeafView { ref tree, .. } => {
                    let s = serde_json::to_string(tree).unwrap();
                    // Verify that the tree does NOT have concatenated text like "weekly×daily" or "daily×"
                    assert!(!s.contains("weekly×"), "Tree should not contain concatenated text with delete icon: {}", s);
                    assert!(!s.contains("daily×"), "Tree should not contain concatenated text with delete icon: {}", s);
                    if s.contains("weekly") && s.contains("daily") {
                        got_leaf_view = true;
                    }
                    fn find_first_delete(n: &crate::plugins::ui::UiNode) -> Option<String> {
                        match n {
                            crate::plugins::ui::UiNode::Button { id, label, .. } if label == "×" => Some(id.clone()),
                            crate::plugins::ui::UiNode::Column { children, .. } | crate::plugins::ui::UiNode::Row { children, .. } => {
                                for c in children {
                                    if let Some(found) = find_first_delete(c) {
                                        return Some(found);
                                    }
                                }
                                None
                            }
                            _ => None,
                        }
                    }
                    if delete_btn_id.is_none() {
                        delete_btn_id = find_first_delete(tree);
                    }
                }
                _ => {}
            }
        }
        assert!(got_leaf_view, "Should have received SetLeafView containing distinct entries for weekly and daily");
        assert!(delete_btn_id.is_some(), "Should have found delete button in tree");

        // Simulate clicking the delete button (which calls c.stopPropagation() and removeFile)
        let _ = handle.host_events_tx.unbounded_send(HostEvent::UiEvent {
            view_id: 0,
            node_id: delete_btn_id.unwrap(),
            event: "click".to_string(),
            value: None,
        });
        let mut got_delete_saved = false;
        let mut got_updated_leaf_after_delete = false;
        let start_del = std::time::Instant::now();
        while start_del.elapsed() < Duration::from_millis(1000) && (!got_delete_saved || !got_updated_leaf_after_delete) {
            while let Ok(call) = api_rx.try_recv() {
                match call.request {
                    ApiRequest::SaveData { ref data } => {
                        let s = data.to_string();
                        if !s.contains("weekly.md") && s.contains("daily.md") {
                            got_delete_saved = true;
                        }
                        let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
                            call.request_id,
                            serde_json::Value::Null,
                        ));
                    }
                    ApiRequest::SetLeafView { ref tree, .. } => {
                        let s = serde_json::to_string(tree).unwrap();
                        if !s.contains("weekly") && s.contains("daily") {
                            got_updated_leaf_after_delete = true;
                        }
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(got_delete_saved, "Clicking delete button must remove the file from recent files saved data");
        assert!(got_updated_leaf_after_delete, "Clicking delete button must trigger redraw and sync updated leaf view without deleted file");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_event_stop_propagation_and_sanitization() {
        let manifest = PluginManifest {
            id: "test-events".into(),
            name: "Test Events".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, _api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            let stopped = false;
            let prevented = false;
            const el = new VElement("div");
            el.addEventListener("click", (evt) => {
                evt.stopPropagation();
                evt.preventDefault();
                stopped = evt.propagationStopped;
                prevented = evt.defaultPrevented;
            });
            // Dispatch with raw object (like older call sites)
            el.dispatchEvent({ type: "click" });
            if (!stopped || !prevented) throw new Error("Event methods failed");

            // Dispatch with new Event
            stopped = false;
            prevented = false;
            const ev2 = new Event("click");
            el.dispatchEvent(ev2);
            if (!stopped || !prevented) throw new Error("Event class methods failed");
        "#;

        let handle = spawn_plugin_runtime(manifest, script.into(), api_tx, snapshot)
            .expect("failed to spawn runtime");
        std::thread::sleep(Duration::from_millis(50));
        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_velement_micro_dom_and_selectors() {
        let manifest = PluginManifest {
            id: "test-micro-dom".into(),
            name: "Test Micro DOM".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let script = r#"
            const { Plugin, Notice } = require('obsidian');

            class MicroDomTestPlugin extends Plugin {
                async onload() {
                    const root = new VElement("div", { cls: "container", id: "root-el" });
                    
                    // 1. innerHTML parsing and sanitization
                    root.innerHTML = '<div class="card" id="card-1"><h3>Title</h3><p>Hello <b>bold</b> and <i>italic</i></p><hr><span class="badge" data-role="status">Active</span><script>evil()</script><iframe src="bad.com"></iframe><button onclick="evil()">Click</button></div>';

                    const card = root.querySelector(".card");
                    if (!card) throw new Error("card not found");
                    if (card.id !== "card-1") throw new Error("card id mismatch");

                    // Verify dangerous tags stripped
                    if (root.querySelector("script")) throw new Error("script should be sanitized");
                    if (root.querySelector("iframe")) throw new Error("iframe should be sanitized");
                    const btn = root.querySelector("button");
                    if (btn && btn.attributes["onclick"]) throw new Error("onclick should be stripped");

                    // 2. Traversal and selectors
                    const bold = root.querySelector("p > b");
                    if (!bold || bold.textContent !== "bold") throw new Error("p > b selector failed");
                    if (!bold.matches("b")) throw new Error("matches tag failed");
                    if (!bold.matches("p b")) throw new Error("matches descendant failed");
                    if (!bold.matches("p > b")) throw new Error("matches child failed");

                    // closest
                    const parentP = bold.closest("p");
                    if (!parentP || parentP.tag !== "p") throw new Error("closest('p') failed");
                    const closestCard = bold.closest(".card");
                    if (!closestCard || closestCard.id !== "card-1") throw new Error("closest('.card') failed");

                    // querySelectorAll
                    const allSpans = root.querySelectorAll("span, b, i");
                    if (allSpans.length < 3) throw new Error("querySelectorAll multi failed: " + allSpans.length);

                    // Attribute matching
                    const badge = root.querySelector('[data-role="status"]');
                    if (!badge || !badge.classes.has("badge")) throw new Error("attribute selector failed");

                    // 3. Tree manipulation and navigation
                    const p = root.querySelector("p");
                    const newCode = new VElement("code");
                    newCode.text = "let x = 1;";
                    p.insertBefore(newCode, bold);
                    if (newCode.nextSibling !== bold) throw new Error("insertBefore nextSibling failed");
                    if (bold.previousSibling !== newCode) throw new Error("previousSibling failed");
                    if (!p.contains(newCode)) throw new Error("contains failed");

                    // className getter/setter
                    newCode.className = "inline-code hljs";
                    if (!newCode.classes.has("inline-code") || !newCode.classes.has("hljs")) throw new Error("className setter failed");
                    if (newCode.className !== "inline-code hljs" && newCode.className !== "hljs inline-code") throw new Error("className getter failed");

                    // cloneNode
                    const clone = newCode.cloneNode(true);
                    if (clone.text !== "let x = 1;" || !clone.classes.has("inline-code")) throw new Error("cloneNode failed");

                    // replaceChild
                    const replaced = p.replaceChild(clone, newCode);
                    if (replaced !== newCode) throw new Error("replaceChild return mismatch");

                    // removeChild
                    p.removeChild(clone);
                    if (p.contains(clone)) throw new Error("removeChild failed to detach");

                    // 4. document query and getElementById
                    const byId = globalThis.document.getElementById("card-1");
                    if (!byId) throw new Error("document.getElementById failed");

                    // 5. DOMParser
                    const parser = new DOMParser();
                    const xmlDoc = parser.parseFromString('<svg class="icon"><path d="M0,0 L10,10" /></svg>', "text/xml");
                    const svg = xmlDoc.documentElement;
                    if (!svg || svg.tag !== "svg" || !svg.classes.has("icon")) throw new Error("DOMParser svg failed");
                    const path = svg.querySelector("path");
                    if (!path || path.attributes["d"] !== "M0,0 L10,10") throw new Error("DOMParser path failed");

                    // 6. __velement_to_uinode check
                    const hr = root.querySelector("hr");
                    const hrNode = __velement_to_uinode(hr);
                    if (!hrNode || hrNode.type !== "separator") throw new Error("hr to uinode failed: " + JSON.stringify(hrNode));

                    const boldNode = __velement_to_uinode(bold);
                    if (!boldNode || boldNode.type !== "text" || boldNode.style !== "heading") throw new Error("bold to uinode failed: " + JSON.stringify(boldNode));

                    const codeEl = new VElement("code");
                    codeEl.text = "fn main()";
                    const codeNode = __velement_to_uinode(codeEl);
                    if (!codeNode || codeNode.type !== "text" || codeNode.style !== "code") throw new Error("code to uinode failed: " + JSON.stringify(codeNode));

                    new Notice("MICRO_DOM_ALL_PASSED");
                }
            }

            module.exports = MicroDomTestPlugin;
        "#;

        let handle = spawn_plugin_runtime(
            manifest,
            script.to_string(),
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        std::thread::sleep(Duration::from_millis(100));

        let mut passed = false;
        while let Ok(call) = api_rx.try_recv() {
            if let ApiRequest::Notice { ref text, .. } = call.request {
                if text == "MICRO_DOM_ALL_PASSED" {
                    passed = true;
                }
            }
        }
        assert!(passed, "Micro-DOM test assertions should all pass");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }

    #[test]
    fn test_empty_files_cleaner_plugin_actual_script() {
        let manifest = PluginManifest {
            id: "empty-files-cleaner-np".into(),
            name: "Empty Files Cleaner NP".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        let mut snapshot_data = Snapshot::default();
        snapshot_data.files = Arc::new([
            crate::vault::FileInfo {
                path: "notes/empty.md".to_string(),
                name: "empty.md".to_string(),
                basename: "empty".to_string(),
                extension: "md".to_string(),
            },
            crate::vault::FileInfo {
                path: "notes/non_empty.md".to_string(),
                name: "non_empty.md".to_string(),
                basename: "non_empty".to_string(),
                extension: "md".to_string(),
            },
        ]);
        let (api_tx, mut api_rx) = unbounded();
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snapshot_data));

        let script = match std::fs::read_to_string("D:\\Naben\\Vibecodium\\.vault\\plugins\\empty-files-cleaner-np\\main.js") {
            Ok(src) => src,
            Err(_) => return,
        };

        let handle = spawn_plugin_runtime(
            manifest,
            script,
            api_tx,
            snapshot,
        ).expect("failed to spawn runtime");

        std::thread::sleep(Duration::from_millis(50));

        // 1. Trigger the command "find-empty-files"
        let _ = handle.host_events_tx.unbounded_send(HostEvent::ExecuteCommand {
            command_id: "find-empty-files".to_string(),
        });

        // 2. The plugin scans files and issues VaultRead for each file:
        let mut got_empty_read = false;
        let mut got_non_empty_read = false;
        let mut view_tree_with_files = false;
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            while let Ok(call) = api_rx.try_recv() {
                match call.request {
                    ApiRequest::VaultRead { ref path } => {
                        if path == "notes/empty.md" {
                            got_empty_read = true;
                            let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
                                call.request_id,
                                serde_json::json!("   \n  "),
                            ));
                        } else if path == "notes/non_empty.md" {
                            got_non_empty_read = true;
                            let _ = handle.host_events_tx.unbounded_send(HostEvent::response_ok(
                                call.request_id,
                                serde_json::json!("# Title\nSome content"),
                            ));
                        }
                    }
                    ApiRequest::SetViewTree { view_id, ref tree } => {
                        if view_id == 1 {
                            let json = serde_json::to_string(tree).unwrap();
                            if json.contains("empty.md") && json.contains("Delete selected") {
                                view_tree_with_files = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            if view_tree_with_files {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(got_empty_read, "Plugin should read empty.md");
        assert!(got_non_empty_read, "Plugin should read non_empty.md");
        assert!(view_tree_with_files, "Modal view tree should contain found empty file and delete button");

        let _ = handle.host_events_tx.unbounded_send(HostEvent::Unload);
    }
