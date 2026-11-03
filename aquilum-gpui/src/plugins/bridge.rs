use std::collections::HashMap;
use std::sync::Mutex;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::StreamExt;
use gpui::{App, Context, WeakEntity};

use crate::plugins::api::{ApiCall, ApiRequest, HostEvent};
use crate::plugins::manifest::{PluginManifest, VaultAccess};
use crate::vault::VaultError;
use crate::views::app_view::AppView;

mod wire;
pub use wire::*;

/// Represents an active plugin registered in the host.
pub struct PluginHandle {
    pub manifest: PluginManifest,
    pub events_tx: UnboundedSender<HostEvent>,
}

/// Manages active plugins and handles authorization of incoming API calls.
pub struct PluginHostState {
    plugins: Mutex<HashMap<String, PluginHandle>>,
}

impl Default for PluginHostState {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHostState {
    pub fn new() -> Self {
        Self {
            plugins: Mutex::new(HashMap::new()),
        }
    }

    /// Register a running plugin with its communication channel.
    pub fn register_plugin(&self, manifest: PluginManifest, events_tx: UnboundedSender<HostEvent>) {
        let mut map = self.plugins.lock().unwrap();
        map.insert(manifest.id.clone(), PluginHandle { manifest, events_tx });
    }

    /// Check if a plugin is registered.
    pub fn is_plugin_registered(&self, plugin_id: &str) -> bool {
        let map = self.plugins.lock().unwrap();
        map.contains_key(plugin_id)
    }

    /// Unregister a plugin upon unload.
    pub fn unregister_plugin(&self, plugin_id: &str) {
        let mut map = self.plugins.lock().unwrap();
        map.remove(plugin_id);
    }

    /// Returns the human-readable display name of a plugin.
    pub fn get_plugin_name(&self, plugin_id: &str) -> Option<String> {
        let map = self.plugins.lock().unwrap();
        map.get(plugin_id).map(|p| p.manifest.name.clone())
    }

    /// Returns a list of all currently registered plugin IDs.
    pub fn registered_plugin_ids(&self) -> Vec<String> {
        let map = self.plugins.lock().unwrap();
        map.keys().cloned().collect()
    }

    /// Send an event notification to a specific plugin worker thread.
    pub fn send_event(&self, plugin_id: &str, event: HostEvent) -> bool {
        let map = self.plugins.lock().unwrap();
        if let Some(plugin) = map.get(plugin_id) {
            plugin.events_tx.unbounded_send(event).is_ok()
        } else {
            false
        }
    }

    /// Broadcast an event (e.g. vault file changed) to all registered plugins.
    pub fn broadcast_event(&self, event: &HostEvent) {
        let map = self.plugins.lock().unwrap();
        for plugin in map.values() {
            let _ = plugin.events_tx.unbounded_send(event.clone());
        }
    }

    /// Verify whether a plugin has permission to execute the specified request.
    pub fn check_permission(&self, plugin_id: &str, req: &ApiRequest) -> Result<(), String> {
        let map = self.plugins.lock().unwrap();
        let Some(plugin) = map.get(plugin_id) else {
            return Err(format!("Plugin '{}' is not registered", plugin_id));
        };

        match req {
            ApiRequest::VaultRead { .. } | ApiRequest::VaultReadBinary { .. } | ApiRequest::VaultListDir { .. } | ApiRequest::VaultListFolders | ApiRequest::VaultExists { .. } => {
                if plugin.manifest.permissions.vault < VaultAccess::Read {
                    return Err(format!("Plugin '{}' lacks vault:read permission", plugin_id));
                }
            }
            ApiRequest::OpenFile { .. } => {
                if plugin.manifest.permissions.vault < VaultAccess::Read {
                    return Err(format!(
                        "Plugin '{}' lacks vault:read permission required to open files",
                        plugin_id
                    ));
                }
            }
            ApiRequest::VaultModify { .. }
            | ApiRequest::VaultCreate { .. }
            | ApiRequest::VaultCreateFolder { .. }
            | ApiRequest::VaultDelete { .. }
            | ApiRequest::VaultTrash { .. }
            | ApiRequest::VaultRename { .. }
            | ApiRequest::VaultAppend { .. }
            | ApiRequest::VaultCopy { .. }
            | ApiRequest::VaultGetAvailablePath { .. }
            | ApiRequest::VaultModifyBinary { .. }
            | ApiRequest::VaultCreateBinary { .. } => {
                if plugin.manifest.permissions.vault < VaultAccess::Write {
                    return Err(format!("Plugin '{}' lacks vault:write permission", plugin_id));
                }
            }
            ApiRequest::Http { url, .. } => {
                validate_network_url(url, &plugin.manifest.permissions.network)?;
            }
            ApiRequest::OpenUrl { .. }
            | ApiRequest::Notice { .. }
            | ApiRequest::SetViewTree { .. }
            | ApiRequest::SetLeafView { .. }
            | ApiRequest::RevealLeaf { .. }
            | ApiRequest::CloseLeaf { .. }
            | ApiRequest::RegisterCodeBlockProcessor { .. }
            | ApiRequest::SetCodeBlockView { .. }
            | ApiRequest::AddCommand { .. }
            | ApiRequest::RemoveCommand { .. }
            | ApiRequest::AddToolbar { .. }
            | ApiRequest::UpdateToolbar { .. }
            | ApiRequest::RemoveToolbar { .. }
            | ApiRequest::RegisterIcon { .. }
            | ApiRequest::AddStatusBarItem { .. }
            | ApiRequest::UpdateStatusBarItem { .. }
            | ApiRequest::AddRibbonIcon { .. }
            | ApiRequest::ExecuteCommand { .. }
            | ApiRequest::EditorReplaceSelection { .. }
            | ApiRequest::EditorGetBuffer
            | ApiRequest::EditorGetSelection
            | ApiRequest::EditorSetCursor { .. }
            | ApiRequest::EditorSetSelection { .. }
            | ApiRequest::EditorSetBuffer { .. }
            | ApiRequest::EditorReplaceRange { .. }
            | ApiRequest::NoticeUpdate { .. }
            | ApiRequest::NoticeHide { .. }
            | ApiRequest::OpenSettings { .. }
            | ApiRequest::CloseSettings
            | ApiRequest::Unknown { .. }
            | ApiRequest::LoadData
            | ApiRequest::SaveData { .. } => {
                // UI updates, notices, commands, ribbon icons, code block processors, and plugin data.json storage are permitted by default for active plugins.
            }
        }
        Ok(())
    }
}


/// Spawns the background asynchronous bridge loop connecting plugin channels with the GPUI UI thread.
pub fn start_plugin_bridge(
    weak_app: WeakEntity<AppView>,
    mut api_rx: UnboundedReceiver<ApiCall>,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        while let Some(call) = api_rx.next().await {
            let handled = cx.update(|cx| {
                if let Some(app) = weak_app.upgrade() {
                    app.update(cx, |this, cx| {
                        this.handle_plugin_api_call(call, cx);
                    });
                    true
                } else {
                    false
                }
            });

            if matches!(handled, Ok(false) | Err(_)) {
                // Window was closed or AppView destroyed.
                break;
            }
        }
    })
    .detach();
}

impl AppView {
    /// Dispatches an incoming ApiCall on the main GPUI thread.
    pub fn handle_plugin_api_call(&mut self, call: ApiCall, cx: &mut Context<Self>) {
        let plugin_id = call.plugin_id.clone();
        let request_id = call.request_id;

        // 1. Check permissions
        if let Err(err) = self.state.plugin_host.check_permission(&plugin_id, &call.request) {
            let _ = self.state.plugin_host.send_event(
                &plugin_id,
                HostEvent::response_err(request_id, &err),
            );
            return;
        }

        // 2. Dispatch request
        match call.request {
            ApiRequest::Notice { text, timeout_ms, id } => {
                let plugin_name = self.state.plugin_host.get_plugin_name(&plugin_id);
                let duration = if timeout_ms == 0 {
                    Some(5000)
                } else {
                    Some(timeout_ms as u64)
                };
                self.push_plugin_notice(
                    id,
                    plugin_name,
                    text,
                    duration,
                    Some(plugin_id.clone()),
                    cx,
                );
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
                cx.notify();
            }
            ApiRequest::NoticeUpdate { id, text } => {
                self.state.notifications.update(&id, &text);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
                cx.notify();
            }
            ApiRequest::NoticeHide { id } => {
                self.dismiss_notification(&id, cx);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
                cx.notify();
            }
            ApiRequest::OpenUrl { url } => {
                cx.open_url(&url);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
            }
            ApiRequest::OpenFile { path } => {
                let Some(vault) = &self.state.vault else {
                    let _ = self.state.plugin_host.send_event(
                        &plugin_id,
                        HostEvent::response_err(request_id, "Cannot open file: No vault is currently open"),
                    );
                    return;
                };

                let note_path = match vault.resolve(&path) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = self.state.plugin_host.send_event(
                            &plugin_id,
                            HostEvent::response_err(request_id, &e.to_string()),
                        );
                        return;
                    }
                };

                self.state.open_note(note_path);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
                cx.notify();
            }
            ApiRequest::VaultListDir { path } => {
                let result: Result<serde_json::Value, String> = match &self.state.vault {
                    Some(vault) => match vault.resolve(&path) {
                        Err(e) => Err(e.to_string()),
                        Ok(abs) => {
                            let mut files = Vec::new();
                            let mut folders = Vec::new();
                            match std::fs::read_dir(&abs) {
                                Ok(entries) => {
                                    for entry in entries.flatten() {
                                        let name = entry.file_name().to_string_lossy().into_owned();
                                        if name.starts_with('.') {
                                            continue;
                                        }
                                        if entry.path().is_dir() {
                                            folders.push(name);
                                        } else {
                                            files.push(name);
                                        }
                                    }
                                    files.sort();
                                    folders.sort();
                                    Ok(serde_json::json!({ "files": files, "folders": folders }))
                                }
                                Err(e) if abs.exists() => Err(e.to_string()),
                                // A missing folder is an empty listing, not an error:
                                // plugins probe paths before creating content.
                                Err(_) => Ok(serde_json::json!({ "files": [], "folders": [] })),
                            }
                        }
                    },
                    None => Err(VaultError::NotFound("No vault opened".into()).to_string()),
                };

                let event = match result {
                    Ok(val) => HostEvent::response_ok(request_id, val),
                    Err(e) => HostEvent::response_err(request_id, &e),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultReadBinary { path } => {
                let result = match &self.state.vault {
                    Some(vault) => vault.read_file_binary(&path).map(|bytes| {
                        serde_json::json!(crate::plugins::bridge::base64_encode(&bytes))
                    }),
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(val) => HostEvent::response_ok(request_id, val),
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultModifyBinary { path, data_base64 } => {
                let result = match (crate::plugins::bridge::base64_decode(&data_base64), &self.state.vault) {
                    (Some(bytes), Some(vault)) => vault.modify_file_binary(&path, &bytes),
                    (None, _) => Err(VaultError::Io("invalid base64 payload".into())),
                    (_, None) => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(_) => {
                        self.state.events.emit_vault(&crate::events::VaultEvent::Modify {
                            path: path.clone(),
                        });
                        self.state.publish_snapshot();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultCreateBinary { path, data_base64 } => {
                let result = match (crate::plugins::bridge::base64_decode(&data_base64), &self.state.vault) {
                    (Some(bytes), Some(vault)) => vault.create_file_binary(&path, &bytes),
                    (None, _) => Err(VaultError::Io("invalid base64 payload".into())),
                    (_, None) => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(_) => {
                        self.state.events.emit_vault(&crate::events::VaultEvent::Create {
                            path: path.clone(),
                        });
                        self.state.reload_file_tree();
                        self.state.publish_snapshot();
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultRead { path } => {
                println!("[bridge:{}] ApiRequest::VaultRead path: '{}'", plugin_id, path);
                let result = match &self.state.vault {
                    Some(vault) => vault.read_file(&path).map(|c| serde_json::json!(c)),
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(val) => HostEvent::response_ok(request_id, val),
                    Err(e) => {
                        eprintln!("[bridge:{}] VaultRead error for '{}': {}", plugin_id, path, e);
                        HostEvent::response_err(request_id, &e.to_string())
                    }
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultModify { path, data } => {
                let result = match &self.state.vault {
                    Some(vault) => vault.modify_file(&path, &data),
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(_) => {
                        self.state.events.emit_vault(&crate::events::VaultEvent::Modify {
                            path: path.clone(),
                        });
                        self.state.publish_snapshot();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultCreate { path, data } => {
                let result = match &self.state.vault {
                    Some(vault) => vault.create_file(&path, &data),
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(_) => {
                        self.state.events.emit_vault(&crate::events::VaultEvent::Create {
                            path: path.clone(),
                        });
                        self.state.reload_file_tree();
                        self.state.publish_snapshot();
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultCreateFolder { path } => {
                let result: Result<(), String> = match &self.state.vault {
                    Some(vault) => match vault.resolve(&path) {
                        Ok(abs) => std::fs::create_dir_all(&abs).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    None => Err(VaultError::NotFound("No vault opened".into()).to_string()),
                };

                let event = match result {
                    Ok(_) => {
                        self.state.events.emit_vault(&crate::events::VaultEvent::Create {
                            path: path.clone(),
                        });
                        self.state.reload_file_tree();
                        self.state.publish_snapshot();
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultDelete { path } => {
                let abs_path = if let Some(vault) = &self.state.vault {
                    vault.resolve(&path).unwrap_or_else(|_| std::path::PathBuf::from(&path))
                } else {
                    std::path::PathBuf::from(&path)
                };

                let result = match &self.state.vault {
                    Some(vault) => vault.delete_file(&path),
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(_) => {
                        let config = self.state.settings.get_config();
                        if config.files.close_deleted_tabs {
                            self.state.handle_file_deleted(&abs_path, config.files.keep_modified_on_delete);
                        }
                        self.state.events.emit_vault(&crate::events::VaultEvent::Delete {
                            path: path.clone(),
                        });
                        self.state.reload_file_tree();
                        self.state.publish_snapshot();
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultTrash { path, system } => {
                let abs_path = if let Some(vault) = &self.state.vault {
                    vault.resolve(&path).unwrap_or_else(|_| std::path::PathBuf::from(&path))
                } else {
                    std::path::PathBuf::from(&path)
                };

                // Obsidian's trash is a *safe* delete. The OS-recycle-bin variant (`system: true`)
                // is not available natively, so both fall back to the vault-local `.trash` folder
                // rather than unlinking the file.
                let _ = system;
                let result = match &self.state.vault {
                    Some(vault) => vault.trash_file(&path),
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(_) => {
                        let config = self.state.settings.get_config();
                        if config.files.close_deleted_tabs {
                            self.state.handle_file_deleted(&abs_path, config.files.keep_modified_on_delete);
                        }
                        self.state.events.emit_vault(&crate::events::VaultEvent::Delete {
                            path: path.clone(),
                        });
                        self.state.reload_file_tree();
                        self.state.publish_snapshot();
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultRename { old_path, new_path } => {
                let old_abs = if let Some(vault) = &self.state.vault {
                    vault.resolve(&old_path).unwrap_or_else(|_| std::path::PathBuf::from(&old_path))
                } else {
                    std::path::PathBuf::from(&old_path)
                };
                let new_abs = if let Some(vault) = &self.state.vault {
                    vault.resolve(&new_path).unwrap_or_else(|_| std::path::PathBuf::from(&new_path))
                } else {
                    std::path::PathBuf::from(&new_path)
                };

                let result = match &self.state.vault {
                    Some(vault) => vault.rename_file(&old_path, &new_path),
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(_) => {
                        self.state.handle_file_renamed(&old_abs, &new_abs);
                        self.state.events.emit_vault(&crate::events::VaultEvent::Rename {
                            old_path: old_path.clone(),
                            new_path: new_path.clone(),
                        });
                        self.state.reload_file_tree();
                        self.state.publish_snapshot();
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultAppend { path, data } => {
                // Obsidian appends by reading + concatenating + writing; the host keeps the
                // same contract so `modify` events fire exactly once per append.
                let result = match &self.state.vault {
                    Some(vault) => match vault.read_file(&path) {
                        Ok(existing) => {
                            let joined = format!("{existing}{data}");
                            vault.modify_file(&path, &joined).map(|_| existing)
                        }
                        Err(e) if matches!(e, VaultError::NotFound(_)) => {
                            vault.create_file(&path, &data).map(|_| String::new())
                        }
                        Err(e) => Err(e),
                    },
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };
                let event = match result {
                    Ok(_) => {
                        self.state.reload_file_tree();
                        self.state.publish_snapshot();
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultCopy { source, target } => {
                let result = match &self.state.vault {
                    Some(vault) => match vault.read_file(&source) {
                        Ok(content) => vault.create_file(&target, &content).map(|_| ()),
                        Err(e) => Err(e),
                    },
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };
                let event = match result {
                    Ok(_) => {
                        self.state.reload_file_tree();
                        self.state.publish_snapshot();
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultExists { path } => {
                let result = match &self.state.vault {
                    Some(vault) => match vault.resolve(&path) {
                        Ok(abs) => Ok(serde_json::json!(abs.exists())),
                        Err(_) => Ok(serde_json::json!(false)),
                    },
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };
                let event = match result {
                    Ok(val) => HostEvent::response_ok(request_id, val),
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultListFolders => {
                // Walk of the real vault tree, hidden entries excluded — same rule as the
                // file tree view and VaultListDir.
                let result = match &self.state.vault {
                    Some(vault) => {
                        let mut folders = Vec::new();
                        let root = vault.root.clone();
                        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
                            let Ok(entries) = std::fs::read_dir(dir) else { return };
                            for entry in entries.flatten() {
                                let name = entry.file_name().to_string_lossy().into_owned();
                                if name.starts_with('.') {
                                    continue;
                                }
                                let p = entry.path();
                                if p.is_dir() {
                                    if let Ok(rel) = p.strip_prefix(root) {
                                        out.push(rel.to_string_lossy().replace('\\', "/"));
                                    }
                                    walk(&p, root, out);
                                }
                            }
                        }
                        walk(&root, &root, &mut folders);
                        folders.sort();
                        Ok(serde_json::json!(folders))
                    }
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };
                let event = match result {
                    Ok(val) => HostEvent::response_ok(request_id, val),
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::VaultGetAvailablePath { prefix, extension } => {
                // Same numbering rule as Obsidian: first unused "prefix.ext", then
                // "prefix 1.ext", "prefix 2.ext", ...
                let result = match &self.state.vault {
                    Some(vault) => {
                        let base = prefix.trim().trim_end_matches('/').to_string();
                        let ext = extension.as_deref().filter(|e| !e.is_empty());
                        let candidate = |n: u32| match (ext, n) {
                            (Some(e), 0) => format!("{base}.{e}"),
                            (Some(e), _) => format!("{base} {n}.{e}"),
                            (None, 0) => base.clone(),
                            (None, _) => format!("{base} {n}"),
                        };
                        let taken = |p: &String| vault.resolve(p).map(|a| a.exists()).unwrap_or(false);
                        let mut n = 0;
                        let mut path = candidate(n);
                        while taken(&path) {
                            n += 1;
                            path = candidate(n);
                            if n > 1000 {
                                break;
                            }
                        }
                        Ok(serde_json::json!(path))
                    }
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };
                let event = match result {
                    Ok(val) => HostEvent::response_ok(request_id, val),
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::OpenSettings { tab } => {
                // Same mapping the vibecodium:// URL parser uses; unknown ids become a
                // PluginSettings tab (exactly what openTabById(pluginId) means in Obsidian).
                let settings_tab = tab.as_deref().map(|t| match t {
                    "ui" => crate::views::settings_view::SettingsTab::Ui,
                    "editor" => crate::views::settings_view::SettingsTab::Editor,
                    "reader" => crate::views::settings_view::SettingsTab::Reader,
                    "search" => crate::views::settings_view::SettingsTab::Search,
                    "templates" => crate::views::settings_view::SettingsTab::Templates,
                    "files" => crate::views::settings_view::SettingsTab::Files,
                    "analysis" => crate::views::settings_view::SettingsTab::Analysis,
                    "mcp" => crate::views::settings_view::SettingsTab::Mcp,
                    "history" => crate::views::settings_view::SettingsTab::History,
                    "trash" => crate::views::settings_view::SettingsTab::Trash,
                    "system" => crate::views::settings_view::SettingsTab::System,
                    "shortcuts" => crate::views::settings_view::SettingsTab::Shortcuts,
                    "core-plugins" => crate::views::settings_view::SettingsTab::CorePlugins,
                    "community-plugins" => crate::views::settings_view::SettingsTab::CommunityPlugins,
                    other => crate::views::settings_view::SettingsTab::PluginSettings(other.to_string()),
                });
                self.open_settings_with_id(settings_tab, None, cx);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
            }
            ApiRequest::CloseSettings => {
                self.close_settings(cx);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
            }
            ApiRequest::SetViewTree { view_id, tree } => {
                let validation = crate::plugins::ui::validate_tree(&tree);
                let event = match validation {
                    Ok(()) => {
                        let is_empty_modal = view_id != 0 && match &tree {
                            crate::plugins::ui::UiNode::Text { value, .. } => value.is_empty(),
                            _ => false,
                        };
                        if is_empty_modal {
                            println!("[bridge:{}] Removing empty modal for view_id: {}", plugin_id, view_id);
                            self.plugin_views.remove(&(plugin_id.clone(), view_id));
                        } else {
                            println!("[bridge:{}] Stored view tree for view_id: {}", plugin_id, view_id);
                            self.plugin_views.insert((plugin_id.clone(), view_id), tree);
                        }
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(err) => {
                        eprintln!("[bridge:{}] SetViewTree validation error for view_id {}: {}", plugin_id, view_id, err);
                        HostEvent::response_err(request_id, &err)
                    }
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::SetLeafView {
                leaf_id,
                location,
                view_type,
                title,
                icon,
                tree,
                actions,
            } => {
                let validation = crate::plugins::ui::validate_tree(&tree);
                let event = match validation {
                    Ok(()) => {
                        let leaf = crate::plugins::api::PluginLeafView {
                            leaf_id: leaf_id.clone(),
                            plugin_id: plugin_id.clone(),
                            location: location.clone(),
                            view_type,
                            title,
                            icon,
                            tree,
                            actions,
                        };
                        self.plugin_leaf_views.insert(leaf_id.clone(), leaf);
                        if location == "right" {
                            if self.active_right_leaf.is_none() {
                                self.active_right_leaf = Some(leaf_id);
                            }
                        } else if location == "left" {
                            if self.active_left_leaf.is_none() {
                                self.active_left_leaf = Some(leaf_id.clone());
                            }
                        } else if location == "center" {
                            self.active_center_leaf = Some(leaf_id);
                        }
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(err) => HostEvent::response_err(request_id, &err),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::RevealLeaf { leaf_id } => {
                if let Some(leaf) = self.plugin_leaf_views.get(&leaf_id) {
                    if leaf.location == "right" {
                        self.right_sidebar_collapsed = false;
                        self.active_right_leaf = Some(leaf_id.clone());
                    } else if leaf.location == "left" {
                        self.sidebar_collapsed = false;
                        self.active_left_leaf = Some(leaf_id.clone());
                        self.sidebar_mode = crate::views::sidebar::SidebarMode::Plugin(leaf_id.clone());
                    } else if leaf.location == "center" {
                        self.active_center_leaf = Some(leaf_id.clone());
                    }
                }
                cx.notify();
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
            }
            ApiRequest::CloseLeaf { leaf_id } => {
                self.close_plugin_leaf(&leaf_id, cx);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
            }
            ApiRequest::RegisterCodeBlockProcessor { language } => {
                self.state.code_block_processors.register(&language, &plugin_id);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
                cx.notify();
            }
            ApiRequest::SetCodeBlockView { block_id, tree } => {
                let validation = crate::plugins::ui::validate_tree(&tree);
                let event = match validation {
                    Ok(()) => {
                        self.code_block_views.insert(block_id, (plugin_id.clone(), tree));
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(err) => HostEvent::response_err(request_id, &err),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::LoadData => {
                let data = self
                    .state
                    .vault
                    .as_ref()
                    .and_then(|v| v.load_plugin_data(&plugin_id))
                    .unwrap_or(serde_json::Value::Null);

                let event = HostEvent::response_ok(request_id, data);
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::SaveData { data } => {
                let result = match &self.state.vault {
                    Some(vault) => vault.save_plugin_data(&plugin_id, &data),
                    None => Err("No vault opened".to_string()),
                };

                let event = match result {
                    Ok(()) => {
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(e) => HostEvent::response_err(request_id, &e),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
            ApiRequest::RemoveCommand { id } => {
                // Both the bare id and the palette-prefixed "plugin:cmd" id are unregistered.
                self.state.commands.unregister(&id);
                self.state.commands.unregister(&format!("{}:{}", plugin_id, id));
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
            }
            ApiRequest::AddCommand { id, name, hotkey, icon } => {
                let p_id = plugin_id.clone();
                let cmd_id = id.clone();
                let host = self.state.plugin_host.clone();
                let p_name = host.get_plugin_name(&plugin_id).unwrap_or_else(|| plugin_id.clone());
                let command_name = if name.to_lowercase().starts_with(&p_name.to_lowercase()) {
                    name.clone()
                } else {
                    format!("{}: {}", p_name, name)
                };
                let full_cmd_id = format!("{}:{}", plugin_id, id);
                println!("[bridge:{}] Registering command: '{}' (full id: '{}')", plugin_id, command_name, full_cmd_id);
                let cmd = crate::commands::Command {
                    id: full_cmd_id,
                    name: command_name,
                    owner: Some(plugin_id.clone()),
                    hotkey,
                    icon,
                    callback: std::sync::Arc::new(move || {
                        println!("[bridge:{}] Command callback triggered! Sending HostEvent::ExecuteCommand for '{}'", p_id, cmd_id);
                        let _ = host.send_event(
                            &p_id,
                            HostEvent::ExecuteCommand {
                                command_id: cmd_id.clone(),
                            },
                        );
                    }),
                };
                self.state.commands.register(cmd);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
            }
            ApiRequest::AddToolbar { spec } => {
                let id = spec.id.clone();
                self.state.toolbars.add_toolbar(spec, Some(plugin_id.clone()));
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::json!({ "id": id })),
                );
                cx.notify();
            }
            ApiRequest::UpdateToolbar { id, items } => {
                let ok = self.state.toolbars.update_toolbar(&id, items);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Bool(ok)),
                );
                cx.notify();
            }
            ApiRequest::RemoveToolbar { id } => {
                let ok = self.state.toolbars.remove_toolbar(&id);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Bool(ok)),
                );
                cx.notify();
            }
            ApiRequest::RegisterIcon { name, svg } => {
                crate::toolbar::register_custom_icon(&name, &svg);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Bool(true)),
                );
            }
            ApiRequest::AddRibbonIcon { id, icon, title } => {
                let p_id = plugin_id.clone();
                let icon_id = id.clone();
                let host = self.state.plugin_host.clone();
                let icon_static = crate::icons::map_obsidian_icon(&icon);
                let full_action_id = format!("{}:{}", plugin_id, id);
                println!("[bridge:{}] Registering ribbon icon: '{}' (title: '{}', full id: '{}')", plugin_id, icon, title, full_action_id);
                let action = crate::ribbon::RibbonAction {
                    id: full_action_id,
                    icon: icon_static,
                    tooltip: title,
                    owner: Some(plugin_id.clone()),
                    callback: std::sync::Arc::new(move || {
                        println!("[bridge:{}] Ribbon callback triggered! Sending HostEvent::ExecuteCommand for '{}'", p_id, icon_id);
                        let _ = host.send_event(
                            &p_id,
                            HostEvent::ExecuteCommand {
                                command_id: icon_id.clone(),
                            },
                        );
                    }),
                };
                self.state.ribbon.register(action);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
                cx.notify();
            }
            ApiRequest::AddStatusBarItem { id } => {
                let full_id = format!("{}:{}", plugin_id, id);
                self.state.status_bar.set_item(crate::ribbon::StatusBarItem {
                    id: full_id,
                    text: String::new(),
                    owner: Some(plugin_id.clone()),
                    on_click: None,
                });
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
                cx.notify();
            }
            ApiRequest::UpdateStatusBarItem { id, text } => {
                // Resolve either the prefixed or bare id to the plugin-owned item.
                let full_id = format!("{}:{}", plugin_id, id);
                if self.state.status_bar.all().iter().any(|i| i.id == full_id) {
                    self.state.status_bar.set_text(&full_id, &text);
                } else if self.state.status_bar.all().iter().any(|i| i.id == id) {
                    self.state.status_bar.set_text(&id, &text);
                }
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
                cx.notify();
            }
            ApiRequest::ExecuteCommand { id } => {
                let executed = self.state.commands.execute(&id);
                if !executed && id == "file-explorer:new-file" {
                    if let Some(_) = self.state.create_new_note() {
                        self.need_scroll_sidebar_to_active.set(true);
                        cx.notify();
                    }
                }
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
            }
            ApiRequest::EditorReplaceSelection { text } => {
                let replaced = if self.state.active_doc_id().is_some() {
                    self.state.active_replace_selection(&text);
                    true
                } else {
                    false
                };
                if replaced {
                    self.state.save_active_note();
                    cx.notify();
                }
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Bool(replaced)),
                );
            }
            ApiRequest::EditorGetBuffer => {
                let response = match self.state.active_editor_buffer() {
                    Some((text, (line, ch), anchor)) => {
                        let mut cursor = serde_json::json!({ "line": line, "ch": ch });
                        if let Some((a_line, a_ch)) = anchor {
                            cursor["anchor"] = serde_json::json!({ "line": a_line, "ch": a_ch });
                        }
                        serde_json::json!({ "text": text, "cursor": cursor })
                    }
                    None => serde_json::Value::Null,
                };
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, response),
                );
            }
            ApiRequest::EditorGetSelection => {
                let selection = self.state.active_selected_text();
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::json!(selection)),
                );
            }
            ApiRequest::EditorSetBuffer { text } => {
                let changed = self.state.active_set_buffer(&text);
                if changed {
                    self.state.save_active_note();
                    cx.notify();
                }
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Bool(changed)),
                );
            }
            ApiRequest::EditorReplaceRange { from_line, from_ch, to_line, to_ch, text } => {
                let changed = self
                    .state
                    .with_active_doc_and_view(|doc, view| {
                        let from = view.offset_for_line_col(doc, from_line as usize, from_ch as usize);
                        let to = view.offset_for_line_col(doc, to_line as usize, to_ch as usize);
                        from.min(to)..from.max(to)
                    })
                    .map(|range| {
                        let new_cursor = range.start + text.len();
                        self.state.active_replace_range(range, text.clone(), new_cursor);
                        true
                    })
                    .unwrap_or(false);
                if changed {
                    self.state.save_active_note();
                    cx.notify();
                }
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Bool(changed)),
                );
            }
            ApiRequest::EditorSetCursor { line, ch } => {
                let changed = self
                    .state
                    .with_active_doc_and_view(|doc, view| view.offset_for_line_col(doc, line as usize, ch as usize))
                    .map(|offset| self.state.active_set_cursor_offset(offset))
                    .unwrap_or(false);
                if changed {
                    cx.notify();
                }
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Bool(changed)),
                );
            }
            ApiRequest::EditorSetSelection { anchor_line, anchor_ch, head_line, head_ch } => {
                let changed = self
                    .state
                    .with_active_doc_and_view(|doc, view| {
                        let anchor = view.offset_for_line_col(doc, anchor_line as usize, anchor_ch as usize);
                        let head = view.offset_for_line_col(doc, head_line as usize, head_ch as usize);
                        (anchor, head)
                    })
                    .map(|(anchor, head)| self.state.active_set_selection_range(anchor, head))
                    .unwrap_or(false);
                if changed {
                    cx.notify();
                }
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Bool(changed)),
                );
            }
            ApiRequest::Unknown { req_kind } => {
                if let Some(json_str) = req_kind.strip_prefix("editor_menu_result:") {
                    if let Ok(items) = serde_json::from_str::<Vec<crate::views::editor_context_menu::ContextMenuItem>>(json_str) {
                        self.append_plugin_context_menu_items(&plugin_id, items, cx);
                    }
                    return;
                }
                eprintln!("[plugins:{}] Unhandled request kind: {}", plugin_id, req_kind);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_err(request_id, &format!("Unknown request kind: {}", req_kind)),
                );
            }
            ApiRequest::Http { .. } => {
                // Network HTTP requests will be implemented with reqwest/background executor in Phase 3.
                let event = HostEvent::response_err(
                    request_id,
                    "Http requests are not yet implemented in stage 1",
                );
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
            }
        }
    }
}

#[cfg(test)]
mod tests;
