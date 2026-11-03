//! Bundled plugin: `editor-toolbar`.
//!
//! Provides the default Markdown formatting toolbar above the editor,
//! loads and persists configuration to `.vault/plugins/editor-toolbar/data.json`,
//! handles corrupted config fallback, and hot-reload.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::toolbar::{
    validate_and_prune_items, ToolbarItem, ToolbarPlacement, ToolbarSpec,
};
use crate::vault::VaultCore;

pub const TOOLBAR_PLUGIN_ID: &str = "editor-toolbar";

/// Serialized format for `data.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolbarConfigFile {
    #[serde(default = "default_config_version")]
    pub version: u32,
    pub items: Vec<ToolbarItem>,
}

fn default_config_version() -> u32 {
    1
}

/// Generates the default set of toolbar items matching the user specification:
/// undo, redo, eraser | H1, H2, H3, Hn▾ | B, I, S, U, highlight | attachment, table, task, code block, inline code | callout▾, text formatting▾, lists▾, alignment▾ | fullscreen.
pub fn default_toolbar_items() -> Vec<ToolbarItem> {
    vec![
        // 1. History & Cleansers
        ToolbarItem::button("tb-undo", "editor:undo", "undo", Some("Отменить")),
        ToolbarItem::button("tb-redo", "editor:redo", "redo", Some("Повторить")),
        ToolbarItem::button("tb-clear", "editor:clear-formatting", "eraser", Some("Очистить форматирование")),

        ToolbarItem::separator_with_id("tb-sep-1"),

        // 2. Headings
        ToolbarItem::button("tb-h1", "editor:heading-1", "h1", Some("Заголовок 1")),
        ToolbarItem::button("tb-h2", "editor:heading-2", "h2", Some("Заголовок 2")),
        ToolbarItem::button("tb-h3", "editor:heading-3", "h3", Some("Заголовок 3")),
        ToolbarItem::dropdown(
            "tb-hn-dropdown",
            "heading-n",
            Some("Другие заголовки"),
            None,
            vec![
                ToolbarItem::button("tb-h4", "editor:heading-4", "h4", Some("Заголовок 4")),
                ToolbarItem::button("tb-h5", "editor:heading-5", "h5", Some("Заголовок 5")),
                ToolbarItem::button("tb-h6", "editor:heading-6", "h6", Some("Заголовок 6")),
            ],
        ),

        ToolbarItem::separator_with_id("tb-sep-2"),

        // 3. Inline formatting
        ToolbarItem::button("tb-bold", "editor:bold", "bold", Some("Полужирный")),
        ToolbarItem::button("tb-italic", "editor:italic", "italic", Some("Курсив")),
        ToolbarItem::button("tb-strike", "editor:strikethrough", "strikethrough", Some("Зачёркнутый")),
        ToolbarItem::button("tb-underline", "editor:underline", "underline", Some("Подчёркнутый")),
        ToolbarItem::button("tb-highlight", "editor:highlight", "highlighter", Some("Выделение")),

        ToolbarItem::separator_with_id("tb-sep-3"),

        // 4. Content blocks
        ToolbarItem::button("tb-attach", "editor:attach-file", "paperclip", Some("Вложить файл")),
        ToolbarItem::button("tb-table", "editor:insert-table", "table", Some("Вставить таблицу")),
        ToolbarItem::button("tb-task", "editor:toggle-task", "check", Some("Задача")),
        ToolbarItem::button("tb-code-block", "editor:code-block", "code-block", Some("Блок кода")),
        ToolbarItem::button("tb-inline-code", "editor:inline-code", "inline-code", Some("Встроенный код")),

        ToolbarItem::separator_with_id("tb-sep-4"),

        // 5. Advanced formatting dropdowns
        ToolbarItem::dropdown(
            "tb-callout-dropdown",
            "message-square",
            Some("Callout блоки"),
            None,
            vec![
                ToolbarItem::button("tb-callout-note", "editor:insert-callout", "message-square", Some("Вставить Callout")),
            ],
        ),
        ToolbarItem::dropdown(
            "tb-indent-dropdown",
            "type",
            Some("Форматирование текста"),
            None,
            vec![
                ToolbarItem::button("tb-indent", "editor:indent", "indent", Some("Увеличить отступ")),
                ToolbarItem::button("tb-outdent", "editor:outdent", "outdent", Some("Уменьшить отступ")),
            ],
        ),
        ToolbarItem::dropdown(
            "tb-lists-dropdown",
            "list-bullet",
            Some("Списки"),
            None,
            vec![
                ToolbarItem::button("tb-list-bullet", "editor:list-bullet", "list-bullet", Some("Маркированный список")),
                ToolbarItem::button("tb-list-numbered", "editor:list-numbered", "list-numbered", Some("Нумерованный список")),
                ToolbarItem::button("tb-list-task", "editor:list-task", "list-task", Some("Список задач")),
            ],
        ),
        ToolbarItem::dropdown(
            "tb-align-dropdown",
            "align-left",
            Some("Выравнивание"),
            None,
            vec![
                ToolbarItem::button("tb-align-left", "editor:align-left", "align-left", Some("По левому краю")),
                ToolbarItem::button("tb-align-center", "editor:align-center", "align-center", Some("По центру")),
                ToolbarItem::button("tb-align-right", "editor:align-right", "align-right", Some("По правому краю")),
            ],
        ),

        ToolbarItem::separator_with_id("tb-sep-5"),

        // 6. View toggles
        ToolbarItem::button("tb-fullscreen", "editor:toggle-fullscreen", "fullscreen", Some("Полноэкранный режим")),
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

/// Reads the toolbar config from vault disk, or returns default items.
/// Catches and recovers from corrupted JSON without crashing.
pub fn load_toolbar_config(vault_root: Option<&Path>) -> Vec<ToolbarItem> {
    let Some(root) = vault_root else {
        return default_toolbar_items();
    };

    let path = get_config_path(root);
    if !path.exists() {
        return default_toolbar_items();
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<ToolbarConfigFile>(&content) {
            Ok(config) => {
                let validated = validate_and_prune_items(config.items, 1);
                if validated.is_empty() {
                    eprintln!("[editor-toolbar] Config in {:?} contained no valid items, falling back to defaults", path);
                    default_toolbar_items()
                } else {
                    validated
                }
            }
            Err(e) => {
                eprintln!(
                    "[editor-toolbar] Failed to parse config file {:?}: {}. Falling back to default configuration.",
                    path, e
                );
                default_toolbar_items()
            }
        },
        Err(e) => {
            eprintln!(
                "[editor-toolbar] Failed to read config file {:?}: {}. Using defaults.",
                path, e
            );
            default_toolbar_items()
        }
    }
}

/// Persists the toolbar items to `.vault/plugins/editor-toolbar/data.json`.
pub fn save_toolbar_config(vault_root: &Path, items: &[ToolbarItem]) -> Result<(), String> {
    let path = get_config_path(vault_root);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let config = ToolbarConfigFile {
        version: 1,
        items: items.to_vec(),
    };

    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
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
    let spec = ToolbarSpec {
        id: TOOLBAR_PLUGIN_ID.to_string(),
        placement: ToolbarPlacement::Top,
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
