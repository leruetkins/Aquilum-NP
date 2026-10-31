use serde::{Deserialize, Serialize};

pub type RequestId = u64;

/// Requests initiated by a plugin running in its isolated thread, sent to the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApiRequest {
    /// Read file content within vault: app.vault.read(path)
    VaultRead { path: String },
    /// Read file bytes within vault, base64-encoded in the response: app.vault.readBinary(file)
    VaultReadBinary { path: String },
    /// Lists a folder: app.vault.adapter.list(path) -> { files, folders }
    VaultListDir { path: String },
    /// Write binary file content (base64 payload): app.vault.modifyBinary(file, data)
    VaultModifyBinary { path: String, data_base64: String },
    /// Create a new binary file (base64 payload): app.vault.createBinary(path, data)
    VaultCreateBinary { path: String, data_base64: String },
    /// Modify file content within vault: app.vault.modify(path, data)
    VaultModify { path: String, data: String },
    /// Create new file within vault: app.vault.create(path, data)
    VaultCreate { path: String, data: String },
    /// Create a folder within vault: app.vault.createFolder(path)
    VaultCreateFolder { path: String },
    /// Delete file within vault: app.vault.delete(path)
    VaultDelete { path: String },
    /// Move file to the safe trash (Obsidian trash or OS recycle bin): app.vault.trash(path, system)
    VaultTrash { path: String, system: bool },
    /// Rename or move file within vault: app.vault.rename(old_path, new_path)
    VaultRename { old_path: String, new_path: String },
    /// Append text to a file (creates it when missing): app.vault.append(path, data)
    VaultAppend { path: String, data: String },
    /// Copy a file within vault: app.vault.copy(source, target)
    VaultCopy { source: String, target: String },
    /// File existence probe without reading content: app.vault.exists(path)
    VaultExists { path: String },
    /// Vault-wide folder listing: app.vault.getAllFolders() -> folder paths
    VaultListFolders,
    /// A safe target path unused in the vault: app.vault.getAvailablePath(prefix, ext)
    VaultGetAvailablePath { prefix: String, extension: Option<String> },
    /// Opens the settings window: app.setting.open()/openTabById(id)/openTab(tab)
    OpenSettings {
        /// Settings tab key (`community-plugins`, `plugin:obsidian-plugin-id`) or None for the
        /// currently active tab.
        tab: Option<String>,
    },
    /// Closes the settings window: app.setting.close()
    CloseSettings,
    /// Display notification notice: new Notice(text, timeout)
    Notice {
        text: String,
        timeout_ms: u32,
        /// Runtime-assigned notice identity, used by setMessage()/hide().
        #[serde(default)]
        id: Option<String>,
    },
    /// Updates the text of a previously shown notice: Notice.setMessage()
    NoticeUpdate { id: String, text: String },
    /// Dismisses a previously shown notice: Notice.hide()
    NoticeHide { id: String },
    /// Open note in the active editor workspace
    OpenFile { path: String },
    /// Open external URL in system browser
    OpenUrl { url: String },
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
    /// Register a command in the command palette: plugin.addCommand({ id, name, hotkey, icon })
    AddCommand {
        id: String,
        name: String,
        hotkey: Option<String>,
        #[serde(default)]
        icon: Option<String>,
    },
    /// Unregister a previously added command: plugin.removeCommand(id)
    RemoveCommand { id: String },
    /// Register a toolbar: plugin.addToolbar({ id, placement, items })
    AddToolbar {
        spec: crate::toolbar::ToolbarSpec,
    },
    /// Update toolbar items: plugin.updateToolbar(id, items)
    UpdateToolbar {
        id: String,
        items: Vec<crate::toolbar::ToolbarItem>,
    },
    /// Remove a toolbar: plugin.removeToolbar(id)
    RemoveToolbar {
        id: String,
    },
    /// Register a custom SVG icon: plugin.registerIcon(name, svg)
    RegisterIcon {
        name: String,
        svg: String,
    },
    /// Register ribbon action: plugin.addRibbonIcon(icon, title, callback)
    AddRibbonIcon {
        id: String,
        icon: String,
        title: String,
    },
    /// Register a status-bar slot: plugin.addStatusBarItem(); text updates follow.
    AddStatusBarItem { id: String },
    /// Updates the text of a plugin's status-bar item.
    UpdateStatusBarItem { id: String, text: String },
    /// Execute a command by id
    ExecuteCommand {
        id: String,
    },
    /// Replaces the currently selected text in the active editor
    EditorReplaceSelection {
        text: String,
    },
    /// Returns the full text, cursor (line, ch) and selection of the active editor
    EditorGetBuffer,
    /// Replaces the full text of the active editor
    EditorSetBuffer {
        text: String,
    },
    /// Replaces a ch-coordinate range in the active editor (Obsidian EditorPosition uses ch = char index)
    EditorReplaceRange {
        from_line: u32,
        from_ch: u32,
        to_line: u32,
        to_ch: u32,
        text: String,
    },
    /// Reads the selected text of the active editor
    EditorGetSelection,
    /// Moves the cursor to a position (line, ch) in the active editor
    EditorSetCursor {
        line: u32,
        ch: u32,
    },
    /// Selects a range between two positions (line, ch) in the active editor
    EditorSetSelection {
        anchor_line: u32,
        anchor_ch: u32,
        head_line: u32,
        head_ch: u32,
    },
    /// Registers or updates a workspace leaf view (ItemView in sidebar or center)
    SetLeafView {
        leaf_id: String,
        location: String,
        view_type: String,
        title: String,
        icon: Option<String>,
        tree: crate::plugins::ui::UiNode,
        actions: Vec<PluginViewAction>,
    },
    /// Reveals a workspace leaf view (e.g. uncollapses sidebar)
    RevealLeaf {
        leaf_id: String,
    },
    /// Closes a workspace leaf view
    CloseLeaf {
        leaf_id: String,
    },
    /// Unhandled request kind (logged to console, does not trigger status bar toast)
    Unknown {
        req_kind: String,
    },
}

/// Action button registered in the view header: view.addAction(icon, title, callback)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginViewAction {
    pub id: String,
    pub icon: String,
    pub title: String,
}

/// Represents an instantiated workspace leaf view (ItemView in sidebar or center).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginLeafView {
    pub leaf_id: String,
    pub plugin_id: String,
    pub location: String,
    pub view_type: String,
    pub title: String,
    pub icon: Option<String>,
    pub tree: crate::plugins::ui::UiNode,
    pub actions: Vec<PluginViewAction>,
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
