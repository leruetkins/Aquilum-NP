use serde::{Deserialize, Serialize};

pub type RequestId = u64;

/// Requests initiated by a plugin running in its isolated thread, sent to the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApiRequest {
    /// Read file content within vault: app.vault.read(path)
    VaultRead { path: String },
    /// Modify file content within vault: app.vault.modify(path, data)
    VaultModify { path: String, data: String },
    /// Create new file within vault: app.vault.create(path, data)
    VaultCreate { path: String, data: String },
    /// Delete file within vault: app.vault.delete(path)
    VaultDelete { path: String },
    /// Rename or move file within vault: app.vault.rename(old_path, new_path)
    VaultRename { old_path: String, new_path: String },
    /// Display notification notice: new Notice(text, timeout)
    Notice { text: String, timeout_ms: u32 },
    /// Open note in the active editor workspace
    OpenFile { path: String },
    /// Outgoing HTTP request: requestUrl({url, method, ...})
    Http {
        url: String,
        method: String,
        body: Option<String>,
    },
    /// Updates declarative UI component tree for a registered view: plugin.setViewState({tree})
    SetViewTree {
        view_id: u64,
        tree: crate::plugins::ui::UiNode,
    },
    /// Reads plugin persistent settings: plugin.loadData()
    LoadData,
    /// Writes plugin persistent settings: plugin.saveData(data)
    SaveData { data: serde_json::Value },
    /// Registers a custom markdown code block processor: plugin.registerMarkdownCodeBlockProcessor(lang, handler)
    RegisterCodeBlockProcessor { language: String },
    /// Updates declarative UI component tree for a custom markdown code block
    SetCodeBlockView {
        block_id: usize,
        tree: crate::plugins::ui::UiNode,
    },
    /// Register a command in the command palette: plugin.addCommand({ id, name, hotkey })
    AddCommand {
        id: String,
        name: String,
        hotkey: Option<String>,
    },
    /// Register ribbon action: plugin.addRibbonIcon(icon, title, callback)
    AddRibbonIcon {
        id: String,
        icon: String,
        title: String,
    },
    /// Execute a command by id
    ExecuteCommand {
        id: String,
    },
    /// Replaces the currently selected text in the active editor
    EditorReplaceSelection {
        text: String,
    },
    /// Unhandled request kind (logged to console, does not trigger status bar toast)
    Unknown {
        req_kind: String,
    },
}

/// Envelops an API request sent from a plugin worker to the host bridge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiCall {
    pub plugin_id: String,
    pub request_id: RequestId,
    pub request: ApiRequest,
}

/// Events and responses pushed from the host UI thread back to the plugin worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostEvent {
    /// Async response fulfilling a previous ApiCall request.
    Response {
        id: RequestId,
        ok: bool,
        result: serde_json::Value,
    },
    /// Vault file modified event: app.vault.on("modify")
    VaultModify { path: String },
    /// Vault file created event: app.vault.on("create")
    VaultCreate { path: String },
    /// Vault file deleted event: app.vault.on("delete")
    VaultDelete { path: String },
    /// Vault file renamed event: app.vault.on("rename")
    VaultRename { old_path: String, new_path: String },
    /// Active file opened or closed: app.workspace.on("file-open")
    FileOpen { path: Option<String> },
    /// UI interaction event triggered on a declarative component.
    UiEvent {
        view_id: u64,
        node_id: String,
        event: String,
        value: Option<String>,
    },
    /// Requests the plugin to process a markdown code block: (language, source, block_id)
    ProcessCodeBlock {
        language: String,
        source: String,
        block_id: usize,
    },
    /// Requests the plugin to execute a registered command by its id
    ExecuteCommand {
        command_id: String,
    },
    /// Dispatches an editor context menu event to plugins: app.workspace.on('editor-menu', (menu, editor))
    EditorMenu {
        menu_session_id: u64,
        selected_text: String,
    },
    /// Invokes a menu item callback clicked by the user
    ExecuteMenuAction {
        action_id: String,
    },
    /// Instructs the plugin runtime to clean up resources and exit.
    Unload,
}

impl HostEvent {
    pub fn response_ok(id: RequestId, value: serde_json::Value) -> Self {
        Self::Response {
            id,
            ok: true,
            result: value,
        }
    }

    pub fn response_err(id: RequestId, message: &str) -> Self {
        Self::Response {
            id,
            ok: false,
            result: serde_json::json!({ "error": message }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_call_serialization_roundtrip() {
        let call = ApiCall {
            plugin_id: "test-plugin".into(),
            request_id: 101,
            request: ApiRequest::VaultRead {
                path: "Notes/Todo.md".into(),
            },
        };

        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("\"kind\":\"vault_read\""));
        assert!(json.contains("\"request_id\":101"));

        let deserialized: ApiCall = serde_json::from_str(&json).unwrap();
        assert_eq!(call, deserialized);
    }

    #[test]
    fn test_host_event_response_roundtrip() {
        let event = HostEvent::response_ok(
            101,
            serde_json::json!({
                "content": "# Hello World"
            }),
        );

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"kind\":\"response\""));
        assert!(json.contains("\"ok\":true"));

        let deserialized: HostEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_file_open_event_none_path() {
        let event = HostEvent::FileOpen { path: None };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: HostEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }
}
