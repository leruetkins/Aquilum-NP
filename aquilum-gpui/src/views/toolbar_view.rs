//! Toolbar UI rendering: horizontal bar, recursive dropdown menus,
//! split buttons, overflow menus, and keyboard navigation.
//!
//! The strip layout lives in `strip`, the dropdown and tooltip popups in `popup`,
//! the right-click menu in `context_menu`, and icon reordering in `drag`.

mod context_menu;
mod drag;
mod helpers;
mod popup;
mod strip;

#[cfg(test)]
mod tests;

use crate::toolbar::ToolbarItem;

/// Dropdown state for toolbar popups.
#[derive(Clone, Debug)]
pub struct OpenToolbarDropdown {
    pub toolbar_id: String,
    pub item_id: String,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub items: Vec<ToolbarItem>,
    /// active_path stores the index path for opened submenus:
    /// e.g. [2] means item index 2 in items is open as a submenu.
    pub active_path: Vec<usize>,
    /// Hovered item index at the deepest active menu level
    pub hovered_index: Option<usize>,
}

/// State of the editor toolbar view.
#[derive(Clone, Debug, Default)]
pub struct ToolbarViewState {
    pub open_dropdown: Option<OpenToolbarDropdown>,
    pub hovered_tooltip: Option<ToolbarTooltipInfo>,
}

/// Context menu opened by right-clicking the empty part of a toolbar strip.
#[derive(Clone, Debug, Default)]
pub struct ToolbarContextMenuState {
    pub is_open: bool,
    pub position_x: f32,
    pub position_y: f32,
    /// Strip rectangle in window coordinates, used to reopen the menu on a repeated
    /// right-click inside the same toolbar.
    pub area_x: f32,
    pub area_y: f32,
    pub area_w: f32,
    pub area_h: f32,
}

/// Active drag of a toolbar icon.
#[derive(Clone, Debug)]
pub struct ToolbarDragState {
    pub toolbar_id: String,
    pub source_id: String,
    /// Pointer X at the mouse-down that started the gesture, in window coordinates.
    pub start_x: f32,
    pub pointer_x: f32,
    pub pointer_y: f32,
    pub is_dragging: bool,
    /// Icon under the pointer, highlighted as the drop slot.
    pub target_id: Option<String>,
    /// Separators and the overflow button can be clicked but not dragged.
    pub can_drag: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolbarTooltipInfo {
    pub title: String,
    pub hotkey: Option<String>,
    pub x: f32,
    pub y: f32,
    pub item_w: f32,
}
