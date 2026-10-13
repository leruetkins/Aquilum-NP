use std::collections::HashMap;
use std::sync::Mutex;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::StreamExt;
use gpui::{App, Context, WeakEntity};

use crate::plugins::api::{ApiCall, ApiRequest, HostEvent};
use crate::plugins::manifest::{PluginManifest, VaultAccess};
use crate::vault::VaultError;
use crate::views::app_view::AppView;

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
            ApiRequest::VaultRead { .. } => {
                if plugin.manifest.permissions.vault < VaultAccess::Read {
                    return Err(format!("Plugin '{}' lacks vault:read permission", plugin_id));
                }
            }
            ApiRequest::VaultModify { .. }
            | ApiRequest::VaultCreate { .. }
            | ApiRequest::VaultDelete { .. }
            | ApiRequest::VaultRename { .. } => {
                if plugin.manifest.permissions.vault < VaultAccess::Write {
                    return Err(format!("Plugin '{}' lacks vault:write permission", plugin_id));
                }
            }
            ApiRequest::Http { url, .. } => {
                let allowed = plugin.manifest.permissions.network.iter().any(|domain| {
                    domain == "*" || url.contains(domain)
                });
                if !allowed {
                    return Err(format!(
                        "Plugin '{}' is not permitted to access network domain for url '{}'",
                        plugin_id, url
                    ));
                }
            }
            ApiRequest::Notice { .. }
            | ApiRequest::OpenFile { .. }
            | ApiRequest::SetViewTree { .. }
            | ApiRequest::RegisterCodeBlockProcessor { .. }
            | ApiRequest::SetCodeBlockView { .. }
            | ApiRequest::AddCommand { .. }
            | ApiRequest::AddRibbonIcon { .. }
            | ApiRequest::ExecuteCommand { .. }
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
            ApiRequest::Notice { text, .. } => {
                self.state.status_text = format!("[{}] {}", plugin_id, text);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
                cx.notify();
            }
            ApiRequest::OpenFile { path } => {
                let note_path = if let Some(vault) = &self.state.vault {
                    match vault.resolve(&path) {
                        Ok(p) => p,
                        Err(e) => {
                            let _ = self.state.plugin_host.send_event(
                                &plugin_id,
                                HostEvent::response_err(request_id, &e.to_string()),
                            );
                            return;
                        }
                    }
                } else {
                    std::path::PathBuf::from(path)
                };

                self.state.open_note(note_path);
                let _ = self.state.plugin_host.send_event(
                    &plugin_id,
                    HostEvent::response_ok(request_id, serde_json::Value::Null),
                );
                cx.notify();
            }
            ApiRequest::VaultRead { path } => {
                let result = match &self.state.vault {
                    Some(vault) => vault.read_file(&path).map(|c| serde_json::json!(c)),
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(val) => HostEvent::response_ok(request_id, val),
                    Err(e) => HostEvent::response_err(request_id, &e.to_string()),
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
            ApiRequest::VaultDelete { path } => {
                let result = match &self.state.vault {
                    Some(vault) => vault.delete_file(&path),
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(_) => {
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
                let result = match &self.state.vault {
                    Some(vault) => vault.rename_file(&old_path, &new_path),
                    None => Err(VaultError::NotFound("No vault opened".into())),
                };

                let event = match result {
                    Ok(_) => {
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
            ApiRequest::SetViewTree { view_id, tree } => {
                let validation = crate::plugins::ui::validate_tree(&tree);
                let event = match validation {
                    Ok(()) => {
                        self.plugin_views.insert((plugin_id.clone(), view_id), tree);
                        cx.notify();
                        HostEvent::response_ok(request_id, serde_json::Value::Null)
                    }
                    Err(err) => HostEvent::response_err(request_id, &err),
                };
                let _ = self.state.plugin_host.send_event(&plugin_id, event);
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
            ApiRequest::AddCommand { id, name, hotkey } => {
                let p_id = plugin_id.clone();
                let cmd_id = id.clone();
                let host = self.state.plugin_host.clone();
                let cmd = crate::commands::Command {
                    id: format!("{}:{}", plugin_id, id),
                    name: name.clone(),
                    owner: Some(plugin_id.clone()),
                    hotkey,
                    callback: std::sync::Arc::new(move || {
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
            ApiRequest::AddRibbonIcon { id, icon, title } => {
                let p_id = plugin_id.clone();
                let icon_id = id.clone();
                let host = self.state.plugin_host.clone();
                let icon_static = crate::icons::map_obsidian_icon(&icon);
                let action = crate::ribbon::RibbonAction {
                    id: format!("{}:{}", plugin_id, id),
                    icon: icon_static,
                    tooltip: title,
                    owner: Some(plugin_id.clone()),
                    callback: std::sync::Arc::new(move || {
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
            ApiRequest::Unknown { req_kind } => {
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
mod tests {
    use super::*;
    use crate::plugins::manifest::PluginPermissions;
    use futures::channel::mpsc::unbounded;

    #[test]
    fn test_plugin_permissions_enforcement() {
        let host = PluginHostState::new();
        let (tx, _rx) = unbounded();

        let manifest = PluginManifest {
            id: "readonly-plugin".into(),
            name: "ReadOnly".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions {
                vault: VaultAccess::Read,
                network: vec!["api.github.com".into()],
            },
        };

        host.register_plugin(manifest, tx);

        // Read is allowed
        assert!(host
            .check_permission(
                "readonly-plugin",
                &ApiRequest::VaultRead {
                    path: "note.md".into()
                }
            )
            .is_ok());

        // Write is rejected
        assert!(host
            .check_permission(
                "readonly-plugin",
                &ApiRequest::VaultModify {
                    path: "note.md".into(),
                    data: "new".into(),
                }
            )
            .is_err());

        // Allowed network domain passes
        assert!(host
            .check_permission(
                "readonly-plugin",
                &ApiRequest::Http {
                    url: "https://api.github.com/repos".into(),
                    method: "GET".into(),
                    body: None,
                }
            )
            .is_ok());

        // Forbidden network domain fails
        assert!(host
            .check_permission(
                "readonly-plugin",
                &ApiRequest::Http {
                    url: "https://evil.com/leak".into(),
                    method: "POST".into(),
                    body: None,
                }
            )
            .is_err());
    }

    #[test]
    fn test_unregistered_plugin_rejected() {
        let host = PluginHostState::new();
        assert!(host
            .check_permission(
                "nonexistent",
                &ApiRequest::VaultRead {
                    path: "note.md".into()
                }
            )
            .is_err());
    }

    #[test]
    fn test_set_view_tree_permission_allowed() {
        let host = PluginHostState::new();
        let (tx, _rx) = unbounded();
        let manifest = PluginManifest {
            id: "ui-plugin".into(),
            name: "UI Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };
        host.register_plugin(manifest, tx);

        let req = ApiRequest::SetViewTree {
            view_id: 1,
            tree: crate::plugins::ui::UiNode::Separator,
        };
        assert!(host.check_permission("ui-plugin", &req).is_ok());
    }
}
