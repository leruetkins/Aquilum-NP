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

/// Reads `styles.css` from the plugin folder. The file is optional: its absence is not an error.
/// An unreadable file likewise must not prevent the plugin code from loading.
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

        // styles.css next to main.js: optional, but if present it is the only source
        // of the plugin's real styling rules.
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
mod tests;

