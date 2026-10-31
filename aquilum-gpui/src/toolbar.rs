//! Toolbar specification, item models, registry, and state management.
//!
//! Supports recursive dropdowns (up to MAX_TOOLBAR_DROPDOWN_DEPTH = 4),
//! split buttons, separators, overflow menus, and keyboard navigation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const MAX_TOOLBAR_DROPDOWN_DEPTH: usize = 4;

/// Placement for the toolbar in the editor view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarPlacement {
    Top,
    Bottom,
}

impl Default for ToolbarPlacement {
    fn default() -> Self {
        ToolbarPlacement::Top
    }
}

/// Specification for a complete toolbar registered by core or a plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolbarSpec {
    pub id: String,
    #[serde(default)]
    pub placement: ToolbarPlacement,
    pub items: Vec<ToolbarItem>,
    #[serde(skip)]
    pub owner: Option<String>,
}

/// An individual item in the toolbar or dropdown menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolbarItem {
    #[serde(default = "generate_item_id")]
    pub id: String,
    #[serde(flatten)]
    pub kind: ToolbarItemKind,
}

fn generate_item_id() -> String {
    format!("item-{}", uuid::Uuid::new_v4().simple())
}

/// The variant of a toolbar item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolbarItemKind {
    Button {
        #[serde(rename = "command", alias = "command_id")]
        command_id: String,
        icon: String,
        #[serde(default)]
        tooltip: Option<String>,
    },
    Dropdown {
        icon: String,
        #[serde(default)]
        tooltip: Option<String>,
        #[serde(default, alias = "command", alias = "command_id")]
        command: Option<String>,
        #[serde(default)]
        items: Vec<ToolbarItem>,
    },
    Separator,
}

impl ToolbarItem {
    pub fn button(id: impl Into<String>, command_id: impl Into<String>, icon: impl Into<String>, tooltip: Option<&str>) -> Self {
        Self {
            id: id.into(),
            kind: ToolbarItemKind::Button {
                command_id: command_id.into(),
                icon: icon.into(),
                tooltip: tooltip.map(|s| s.to_string()),
            },
        }
    }

    pub fn dropdown(
        id: impl Into<String>,
        icon: impl Into<String>,
        tooltip: Option<&str>,
        command: Option<&str>,
        items: Vec<ToolbarItem>,
    ) -> Self {
        Self {
            id: id.into(),
            kind: ToolbarItemKind::Dropdown {
                icon: icon.into(),
                tooltip: tooltip.map(|s| s.to_string()),
                command: command.map(|s| s.to_string()),
                items,
            },
        }
    }

    pub fn separator() -> Self {
        Self {
            id: generate_item_id(),
            kind: ToolbarItemKind::Separator,
        }
    }

    pub fn separator_with_id(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: ToolbarItemKind::Separator,
        }
    }

    pub fn is_separator(&self) -> bool {
        matches!(self.kind, ToolbarItemKind::Separator)
    }

    pub fn command_id(&self) -> Option<&str> {
        match &self.kind {
            ToolbarItemKind::Button { command_id, .. } => Some(command_id),
            ToolbarItemKind::Dropdown { command, .. } => command.as_deref(),
            ToolbarItemKind::Separator => None,
        }
    }

    pub fn icon(&self) -> Option<&str> {
        match &self.kind {
            ToolbarItemKind::Button { icon, .. } => Some(icon),
            ToolbarItemKind::Dropdown { icon, .. } => Some(icon),
            ToolbarItemKind::Separator => None,
        }
    }

    pub fn tooltip(&self) -> Option<&str> {
        match &self.kind {
            ToolbarItemKind::Button { tooltip, .. } => tooltip.as_deref(),
            ToolbarItemKind::Dropdown { tooltip, .. } => tooltip.as_deref(),
            ToolbarItemKind::Separator => None,
        }
    }
}

/// Validates toolbar items recursively, warning and pruning any nodes beyond `MAX_TOOLBAR_DROPDOWN_DEPTH`.
pub fn validate_and_prune_items(items: Vec<ToolbarItem>, current_depth: usize) -> Vec<ToolbarItem> {
    if current_depth > MAX_TOOLBAR_DROPDOWN_DEPTH {
        eprintln!(
            "[toolbar] Exceeded max dropdown recursion depth ({}); pruning excess items.",
            MAX_TOOLBAR_DROPDOWN_DEPTH
        );
        return Vec::new();
    }

    let mut result = Vec::with_capacity(items.len());
    for item in items {
        match item.kind {
            ToolbarItemKind::Button { .. } | ToolbarItemKind::Separator => {
                result.push(item);
            }
            ToolbarItemKind::Dropdown { icon, tooltip, command, items: sub_items } => {
                if current_depth == MAX_TOOLBAR_DROPDOWN_DEPTH {
                    // At max depth, dropdown cannot have sub-dropdowns
                    eprintln!(
                        "[toolbar] Dropdown '{}' exceeds max depth {}; converting or dropping sub-items",
                        item.id, MAX_TOOLBAR_DROPDOWN_DEPTH
                    );
                    result.push(ToolbarItem {
                        id: item.id,
                        kind: ToolbarItemKind::Dropdown {
                            icon,
                            tooltip,
                            command,
                            items: Vec::new(),
                        },
                    });
                } else {
                    let pruned_subs = validate_and_prune_items(sub_items, current_depth + 1);
                    result.push(ToolbarItem {
                        id: item.id,
                        kind: ToolbarItemKind::Dropdown {
                            icon,
                            tooltip,
                            command,
                            items: pruned_subs,
                        },
                    });
                }
            }
        }
    }
    result
}

/// Central registry holding active toolbars from core and third-party plugins.
#[derive(Default)]
pub struct ToolbarRegistry {
    toolbars: Mutex<HashMap<String, ToolbarSpec>>,
    order: Mutex<Vec<String>>,
}

impl ToolbarRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or updates a toolbar specification.
    pub fn add_toolbar(&self, mut spec: ToolbarSpec, owner: Option<String>) {
        spec.owner = owner;
        spec.items = validate_and_prune_items(spec.items, 1);
        let id = spec.id.clone();

        let mut map = self.toolbars.lock().unwrap();
        let mut order = self.order.lock().unwrap();
        if !order.contains(&id) {
            order.push(id.clone());
        }
        map.insert(id, spec);
    }

    /// Updates items for an existing toolbar.
    pub fn update_toolbar(&self, id: &str, items: Vec<ToolbarItem>) -> bool {
        let mut map = self.toolbars.lock().unwrap();
        if let Some(spec) = map.get_mut(id) {
            spec.items = validate_and_prune_items(items, 1);
            true
        } else {
            false
        }
    }

    /// Removes a toolbar by its id.
    pub fn remove_toolbar(&self, id: &str) -> bool {
        let mut map = self.toolbars.lock().unwrap();
        let mut order = self.order.lock().unwrap();
        order.retain(|item_id| item_id != id);
        map.remove(id).is_some()
    }

    /// Removes all toolbars registered by a specific plugin owner.
    pub fn remove_all_of(&self, plugin_id: &str) {
        let mut map = self.toolbars.lock().unwrap();
        let mut order = self.order.lock().unwrap();

        let to_remove: Vec<String> = map
            .iter()
            .filter(|(_, spec)| spec.owner.as_deref() == Some(plugin_id))
            .map(|(id, _)| id.clone())
            .collect();

        for id in &to_remove {
            map.remove(id);
        }
        order.retain(|id| !to_remove.contains(id));
    }

    /// Retrieves all toolbars matching the given placement.
    pub fn get_toolbars(&self, placement: ToolbarPlacement) -> Vec<ToolbarSpec> {
        let map = self.toolbars.lock().unwrap();
        let order = self.order.lock().unwrap();

        let mut list = Vec::new();
        for id in order.iter() {
            if let Some(spec) = map.get(id) {
                if spec.placement == placement {
                    list.push(spec.clone());
                }
            }
        }
        list
    }

    /// Retrieves a specific toolbar by id.
    pub fn get_toolbar(&self, id: &str) -> Option<ToolbarSpec> {
        let map = self.toolbars.lock().unwrap();
        map.get(id).cloned()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Dynamic Icon Registry
// ─────────────────────────────────────────────────────────────────────────────

static CUSTOM_ICONS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn custom_icons_map() -> &'static Mutex<HashMap<String, String>> {
    CUSTOM_ICONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers a custom SVG string under the given name.
pub fn register_custom_icon(name: &str, svg: &str) {
    let mut map = custom_icons_map().lock().unwrap();
    map.insert(name.to_lowercase(), svg.to_string());
}

/// Look up a custom SVG string by name.
pub fn get_custom_icon(name: &str) -> Option<String> {
    let map = custom_icons_map().lock().unwrap();
    map.get(&name.to_lowercase()).cloned()
}

/// Checks if a custom icon is registered.
pub fn has_custom_icon(name: &str) -> bool {
    let map = custom_icons_map().lock().unwrap();
    map.contains_key(&name.to_lowercase())
}

// ─────────────────────────────────────────────────────────────────────────────
// UI Dropdown Navigation State
// ─────────────────────────────────────────────────────────────────────────────

/// Represents the hierarchy path of currently open dropdown menus.
/// e.g. ["heading-dropdown", "sub-heading-dropdown"]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DropdownMenuPath {
    pub open_ids: Vec<String>,
    pub selected_index: Option<usize>,
}

impl DropdownMenuPath {
    pub fn is_open(&self, id: &str) -> bool {
        self.open_ids.iter().any(|item| item == id)
    }

    pub fn open_level(&mut self, id: String, depth: usize) {
        if depth < self.open_ids.len() {
            self.open_ids.truncate(depth);
        }
        if depth == self.open_ids.len() && depth < MAX_TOOLBAR_DROPDOWN_DEPTH {
            self.open_ids.push(id);
            self.selected_index = Some(0);
        }
    }

    pub fn close_level(&mut self) {
        self.open_ids.pop();
        self.selected_index = None;
    }

    pub fn close_all(&mut self) {
        self.open_ids.clear();
        self.selected_index = None;
    }

    pub fn depth(&self) -> usize {
        self.open_ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolbar_spec_serialization_roundtrip() {
        let json = r#"{
          "version": 1,
          "items": [
            { "type": "button", "command": "editor:undo", "icon": "undo", "tooltip": "Undo" },
            { "type": "separator" },
            {
              "type": "dropdown",
              "icon": "heading-n",
              "tooltip": "Headings",
              "command": "editor:heading-1",
              "items": [
                { "type": "button", "command": "editor:heading-4", "icon": "h4" },
                {
                  "type": "dropdown",
                  "icon": "list",
                  "tooltip": "More",
                  "items": [
                    { "type": "button", "command": "editor:heading-5", "icon": "h5" }
                  ]
                }
              ]
            }
          ]
        }"#;

        #[derive(Deserialize)]
        struct ConfigWrapper {
            items: Vec<ToolbarItem>,
        }

        let parsed: ConfigWrapper = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.items.len(), 3);
        assert!(!parsed.items[0].id.is_empty());
        assert_eq!(parsed.items[0].command_id(), Some("editor:undo"));
        assert!(parsed.items[1].is_separator());

        if let ToolbarItemKind::Dropdown { ref items, ref command, .. } = parsed.items[2].kind {
            assert_eq!(command.as_deref(), Some("editor:heading-1"));
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].command_id(), Some("editor:heading-4"));
        } else {
            panic!("Expected dropdown");
        }
    }

    #[test]
    fn test_validate_and_prune_exceeding_depth() {
        // Build 5 levels deep
        let level5 = vec![ToolbarItem::button("b5", "cmd5", "icon5", None)];
        let level4 = vec![ToolbarItem::dropdown("d4", "icon4", None, None, level5)];
        let level3 = vec![ToolbarItem::dropdown("d3", "icon3", None, None, level4)];
        let level2 = vec![ToolbarItem::dropdown("d2", "icon2", None, None, level3)];
        let root = vec![ToolbarItem::dropdown("d1", "icon1", None, None, level2)];

        let pruned = validate_and_prune_items(root, 1);
        assert_eq!(pruned.len(), 1);

        // Traverse down: at level 4, items should be pruned
        if let ToolbarItemKind::Dropdown { items: l2, .. } = &pruned[0].kind {
            if let ToolbarItemKind::Dropdown { items: l3, .. } = &l2[0].kind {
                if let ToolbarItemKind::Dropdown { items: l4, .. } = &l3[0].kind {
                    if let ToolbarItemKind::Dropdown { items: l5, .. } = &l4[0].kind {
                        assert!(l5.is_empty(), "Level 5 items must be pruned");
                    }
                }
            }
        }
    }

    #[test]
    fn test_toolbar_registry_lifecycle() {
        let registry = ToolbarRegistry::new();
        let spec = ToolbarSpec {
            id: "test-tb".into(),
            placement: ToolbarPlacement::Top,
            items: vec![ToolbarItem::button("btn1", "editor:bold", "bold", None)],
            owner: Some("my-plugin".into()),
        };

        registry.add_toolbar(spec, Some("my-plugin".into()));
        assert_eq!(registry.get_toolbars(ToolbarPlacement::Top).len(), 1);

        // Update items
        assert!(registry.update_toolbar("test-tb", vec![ToolbarItem::separator()]));
        let updated = registry.get_toolbar("test-tb").unwrap();
        assert!(updated.items[0].is_separator());

        // Remove by owner
        registry.remove_all_of("my-plugin");
        assert!(registry.get_toolbar("test-tb").is_none());
    }

    #[test]
    fn test_custom_icon_registry() {
        register_custom_icon("my-star", "<svg>star</svg>");
        assert_eq!(get_custom_icon("my-star").as_deref(), Some("<svg>star</svg>"));
        assert_eq!(get_custom_icon("MY-STAR").as_deref(), Some("<svg>star</svg>"));
        assert!(get_custom_icon("nonexistent").is_none());
    }
}
