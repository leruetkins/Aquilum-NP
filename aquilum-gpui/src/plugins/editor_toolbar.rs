//! Bundled plugin: `editor-toolbar`.
//!
//! Provides the default Markdown formatting toolbar above the editor,
//! loads and persists configuration to `.vault/plugins/editor-toolbar/data.json`,
//! handles corrupted config fallback, and hot-reload.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::toolbar::{
    validate_and_prune_items, ToolbarAlignment, ToolbarItem, ToolbarItemKind, ToolbarPlacement,
    ToolbarSpec,
};
use crate::vault::VaultCore;

pub const TOOLBAR_PLUGIN_ID: &str = "editor-toolbar";

/// Serialized format for `data.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolbarConfigFile {
    #[serde(default = "default_config_version")]
    pub version: u32,
    pub items: Vec<ToolbarItem>,
    #[serde(default)]
    pub alignment: ToolbarAlignment,
}

fn default_config_version() -> u32 {
    2
}

/// Drops every stored tooltip. Version 1 of `data.json` kept labels that were
/// generated in one language; from version 2 they are resolved at render time.
fn drop_all_tooltips(items: &mut [ToolbarItem]) {
    for item in items {
        match &mut item.kind {
            ToolbarItemKind::Button { tooltip, .. } => *tooltip = None,
            ToolbarItemKind::Dropdown { tooltip, items, .. } => {
                *tooltip = None;
                drop_all_tooltips(items);
            }
            ToolbarItemKind::Separator => {}
        }
    }
}

/// Generates the default set of toolbar items matching the user specification:
/// undo, redo, eraser | H1, H2, H3, Hn▾ | B, I, S, U, highlight | attachment, table, task, code block, inline code | callout▾, text formatting▾, lists▾, alignment▾ | fullscreen.
///
/// Tooltips are not stored: `toolbar_i18n` resolves the label from the command
/// or item id at render time, so the toolbar follows the interface language.
pub fn default_toolbar_items() -> Vec<ToolbarItem> {
    vec![
        // 1. History & Cleansers
        ToolbarItem::button("tb-undo", "editor:undo", "undo", None),
        ToolbarItem::button("tb-redo", "editor:redo", "redo", None),
        ToolbarItem::button("tb-clear", "editor:clear-formatting", "eraser", None),

        ToolbarItem::separator_with_id("tb-sep-1"),

        // 2. Headings
        ToolbarItem::button("tb-h1", "editor:heading-1", "h1", None),
        ToolbarItem::button("tb-h2", "editor:heading-2", "h2", None),
        ToolbarItem::button("tb-h3", "editor:heading-3", "h3", None),
        ToolbarItem::dropdown(
            "tb-hn-dropdown",
            "heading-n",
            None,
            None,
            vec![
                ToolbarItem::button("tb-h4", "editor:heading-4", "h4", None),
                ToolbarItem::button("tb-h5", "editor:heading-5", "h5", None),
                ToolbarItem::button("tb-h6", "editor:heading-6", "h6", None),
            ],
        ),

        ToolbarItem::separator_with_id("tb-sep-2"),

        // 3. Inline formatting
        ToolbarItem::button("tb-bold", "editor:bold", "bold", None),
        ToolbarItem::button("tb-italic", "editor:italic", "italic", None),
        ToolbarItem::button("tb-strike", "editor:strikethrough", "strikethrough", None),
        ToolbarItem::button("tb-underline", "editor:underline", "underline", None),
        ToolbarItem::button("tb-highlight", "editor:highlight", "highlighter", None),

        ToolbarItem::separator_with_id("tb-sep-3"),

        // 4. Content blocks
        ToolbarItem::button("tb-attach", "editor:attach-file", "paperclip", None),
        ToolbarItem::button("tb-table", "editor:insert-table", "table", None),
        ToolbarItem::button("tb-task", "editor:toggle-task", "check", None),
        ToolbarItem::button("tb-code-block", "editor:code-block", "code-block", None),
        ToolbarItem::button("tb-inline-code", "editor:inline-code", "inline-code", None),

        ToolbarItem::separator_with_id("tb-sep-4"),

        // 5. Advanced formatting dropdowns
        ToolbarItem::dropdown(
            "tb-callout-dropdown",
            "message-square",
            None,
            None,
            vec![
                ToolbarItem::button("tb-callout-note", "editor:insert-callout", "message-square", None),
            ],
        ),
        ToolbarItem::dropdown(
            "tb-indent-dropdown",
            "type",
            None,
            None,
            vec![
                ToolbarItem::button("tb-indent", "editor:indent", "indent", None),
                ToolbarItem::button("tb-outdent", "editor:outdent", "outdent", None),
            ],
        ),
        ToolbarItem::dropdown(
            "tb-lists-dropdown",
            "list-bullet",
            None,
            None,
            vec![
                ToolbarItem::button("tb-list-bullet", "editor:list-bullet", "list-bullet", None),
                ToolbarItem::button("tb-list-numbered", "editor:list-numbered", "list-numbered", None),
                ToolbarItem::button("tb-list-task", "editor:list-task", "list-task", None),
            ],
        ),
        ToolbarItem::dropdown(
            "tb-align-dropdown",
            "align-left",
            None,
            None,
            vec![
                ToolbarItem::button("tb-align-left", "editor:align-left", "align-left", None),
                ToolbarItem::button("tb-align-center", "editor:align-center", "align-center", None),
                ToolbarItem::button("tb-align-right", "editor:align-right", "align-right", None),
            ],
        ),

        ToolbarItem::separator_with_id("tb-sep-5"),

        // 6. View toggles
        ToolbarItem::button("tb-fullscreen", "editor:toggle-fullscreen", "fullscreen", None),
    ]
}

/// Returns the path to `.vault/plugins/editor-toolbar/data.json` for a given vault.
pub fn get_config_path(vault_root: &Path) -> PathBuf {
    vault_root
        .join(".vault")
        .join("plugins")
        .join("editor-toolbar")
        .join("data.json")
}

/// Reads the toolbar config from vault disk, falling back to the defaults on any problem.
pub fn load_toolbar_config_file(vault_root: Option<&Path>) -> ToolbarConfigFile {
    let fallback = || ToolbarConfigFile {
        version: default_config_version(),
        items: default_toolbar_items(),
        alignment: ToolbarAlignment::Left,
    };
    let Some(root) = vault_root else {
        return fallback();
    };

    let path = get_config_path(root);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return fallback();
    };

    match serde_json::from_str::<ToolbarConfigFile>(&content) {
        Ok(mut config) => {
            if config.version < default_config_version() {
                drop_all_tooltips(&mut config.items);
                config.version = default_config_version();
            }
            config.items = validate_and_prune_items(config.items, 1);
            if config.items.is_empty() {
                eprintln!(
                    "[editor-toolbar] Config in {:?} contained no valid items, falling back to defaults",
                    path
                );
                config.items = default_toolbar_items();
            }
            config
        }
        Err(e) => {
            eprintln!(
                "[editor-toolbar] Failed to parse config file {:?}: {}. Falling back to default configuration.",
                path, e
            );
            fallback()
        }
    }
}

/// Reads the toolbar config from vault disk, or returns default items.
pub fn load_toolbar_config(vault_root: Option<&Path>) -> Vec<ToolbarItem> {
    load_toolbar_config_file(vault_root).items
}

/// Reads the persisted toolbar alignment (defaults to left).
pub fn load_toolbar_alignment(vault_root: Option<&Path>) -> ToolbarAlignment {
    load_toolbar_config_file(vault_root).alignment
}

fn write_toolbar_config(
    root: &Path,
    items: &[ToolbarItem],
    alignment: ToolbarAlignment,
) -> Result<(), String> {
    let path = get_config_path(root);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let config = ToolbarConfigFile {
        version: default_config_version(),
        items: items.to_vec(),
        alignment,
    };

    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Persists the toolbar items to `.vault/plugins/editor-toolbar/data.json`,
/// keeping the current alignment.
pub fn save_toolbar_config(vault_root: &Path, items: &[ToolbarItem]) -> Result<(), String> {
    let alignment = load_toolbar_config_file(Some(vault_root)).alignment;
    write_toolbar_config(vault_root, items, alignment)
}

/// Persists the toolbar alignment, keeping the current items.
pub fn save_toolbar_alignment(
    vault_root: &Path,
    alignment: ToolbarAlignment,
) -> Result<(), String> {
    let items = load_toolbar_config(Some(vault_root));
    write_toolbar_config(vault_root, &items, alignment)
}

/// Resets the toolbar configuration back to defaults and saves it to disk.
pub fn reset_toolbar_config(vault_root: &Path) -> Result<Vec<ToolbarItem>, String> {
    let defaults = default_toolbar_items();
    save_toolbar_config(vault_root, &defaults)?;
    Ok(defaults)
}

/// Registers or live-updates the bundled `editor-toolbar` in the registry.
pub fn register_or_update_bundled_toolbar(
    state: &crate::state::AppState,
    items: Vec<ToolbarItem>,
) {
    let root = state
        .vault
        .as_ref()
        .map(|vault| vault.root.as_path())
        .or(state.workspace_path.as_deref());
    let spec = ToolbarSpec {
        id: TOOLBAR_PLUGIN_ID.to_string(),
        placement: ToolbarPlacement::Top,
        alignment: load_toolbar_alignment(root),
        items,
        owner: Some(TOOLBAR_PLUGIN_ID.to_string()),
    };
    let owner = Some(TOOLBAR_PLUGIN_ID.to_string());
    state.toolbars.add_toolbar(spec, owner);
}

/// Removes the bundled `editor-toolbar` from the registry (e.g. when disabled).
pub fn unregister_bundled_toolbar(state: &crate::state::AppState) {
    state.toolbars.remove_toolbar(TOOLBAR_PLUGIN_ID);
}

/// Synchronizes the bundled toolbar registration with the vault's core plugins config.
pub fn sync_editor_toolbar_with_vault(
    state: &crate::state::AppState,
    vault: Option<&VaultCore>,
) {
    let is_enabled = vault
        .map(|v| v.is_core_plugin_enabled(TOOLBAR_PLUGIN_ID))
        .unwrap_or(true);

    if is_enabled {
        let items = load_toolbar_config(vault.map(|v| v.root.as_path()));
        register_or_update_bundled_toolbar(state, items);
    } else {
        unregister_bundled_toolbar(state);
    }
}

#[cfg(test)]
mod tests;
