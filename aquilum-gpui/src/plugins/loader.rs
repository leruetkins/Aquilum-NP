use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use arc_swap::ArcSwap;
use futures::channel::mpsc::UnboundedSender;

use crate::plugins::api::ApiCall;
use crate::plugins::bridge::PluginHostState;
use crate::plugins::manifest::PluginManifest;
use crate::plugins::runtime::{spawn_plugin_runtime_with_styles, PluginRuntimeHandle, PluginSources};
use crate::state::Snapshot;

/// Information about an installed plugin discovered on disk.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub manifest: PluginManifest,
    pub dir_path: PathBuf,
    pub main_js_path: PathBuf,
}

/// Scans the given plugins directory (typically `<vault>/.vault/plugins/`) for valid plugin folders.
/// A valid plugin folder must contain `manifest.json` and `main.js`.
pub fn discover_plugins(plugins_dir: &Path) -> Vec<DiscoveredPlugin> {
    let mut discovered = Vec::new();

    let entries = match fs::read_dir(plugins_dir) {
        Ok(e) => e,
        Err(_) => return discovered,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("manifest.json");
        let main_js_path = path.join("main.js");

        if !manifest_path.is_file() || !main_js_path.is_file() {
            continue;
        }

        let manifest_content = match fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[plugins] Failed to read manifest at {:?}: {}", manifest_path, e);
                continue;
            }
        };

        let manifest: PluginManifest = match serde_json::from_str(&manifest_content) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[plugins] Failed to parse manifest at {:?}: {}", manifest_path, e);
                continue;
            }
        };

        if let Err(e) = manifest.validate() {
            eprintln!("[plugins] Invalid manifest for '{}': {}", manifest.id, e);
            continue;
        }

        discovered.push(DiscoveredPlugin {
            manifest,
            dir_path: path,
            main_js_path,
        });
    }

    discovered
}

/// Читает `styles.css` из папки плагина. Файл необязателен: его отсутствие — не ошибка.
/// Нечитаемый файл тоже не должен мешать загрузке кода плагина.
fn read_styles_css(dir: &Path) -> Option<String> {
    let path = dir.join("styles.css");
    if !path.is_file() {
        return None;
    }
    match fs::read_to_string(&path) {
        Ok(css) => Some(css),
        Err(e) => {
            eprintln!("[plugins] Failed to read styles.css at {:?}: {}", path, e);
            None
        }
    }
}

/// Loads discovered plugins, spawns their QuickJS runtime threads, and registers them in PluginHostState.
/// If `enabled_plugins` is Some, only plugins with IDs present in the slice will be loaded.
pub fn load_installed_plugins(
    plugins_dir: &Path,
    enabled_plugins: Option<&[String]>,
    api_tx: UnboundedSender<ApiCall>,
    snapshot: Arc<ArcSwap<Snapshot>>,
    host: &PluginHostState,
) -> Vec<PluginRuntimeHandle> {
    let discovered = discover_plugins(plugins_dir);
    let mut handles = Vec::new();

    for plugin in discovered {
        if let Some(enabled) = enabled_plugins {
            if !enabled.iter().any(|id| id == &plugin.manifest.id) {
                continue;
            }
        }

        let script = match fs::read_to_string(&plugin.main_js_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[plugins] Failed to read main.js for '{}': {}", plugin.manifest.id, e);
                continue;
            }
        };

        // styles.css рядом с main.js: необязателен, но если есть — это единственный источник
        // настоящих правил оформления плагина.
        let sources = PluginSources {
            script,
            styles: read_styles_css(&plugin.dir_path),
        };

        match spawn_plugin_runtime_with_styles(
            plugin.manifest.clone(),
            sources,
            api_tx.clone(),
            snapshot.clone(),
        ) {
            Ok(handle) => {
                host.register_plugin(plugin.manifest, handle.host_events_tx.clone());
                handles.push(handle);
            }
            Err(e) => {
                eprintln!("[plugins] Failed to spawn runtime for '{}': {}", plugin.manifest.id, e);
            }
        }
    }

    handles
}

#[cfg(test)]
mod tests {
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

    /// Сквозная проверка стилей: styles.css → загрузчик → runtime → шим → NodeStyle.
    /// Без неё связка проверяется только по кускам, и перепутанный аргумент не поймать.
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
}

