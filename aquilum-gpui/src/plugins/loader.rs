use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use arc_swap::ArcSwap;
use futures::channel::mpsc::UnboundedSender;

use crate::plugins::api::ApiCall;
use crate::plugins::bridge::PluginHostState;
use crate::plugins::manifest::PluginManifest;
use crate::plugins::runtime::{spawn_plugin_runtime, PluginRuntimeHandle};
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

        match spawn_plugin_runtime(
            plugin.manifest.clone(),
            script,
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
}
