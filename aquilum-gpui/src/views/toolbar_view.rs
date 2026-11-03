//! Toolbar UI rendering: horizontal bar, recursive dropdown menus,
//! split buttons, overflow menus, and keyboard navigation.

use gpui::{
    div, prelude::*, px, AnyElement, Context, IntoElement, MouseButton, MouseMoveEvent,
    SharedString, Styled, TextRun, Window,
};
use crate::formatting::ActiveFormatting;
use crate::icons;
use crate::theme::Theme;
use crate::toolbar::{
    ToolbarAlignment, ToolbarItem, ToolbarItemKind, ToolbarPlacement, ToolbarSpec,
    MAX_TOOLBAR_DROPDOWN_DEPTH,
};
use crate::views::app_view::AppView;

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

impl AppView {
    /// Opens or toggles a toolbar dropdown at `(x, y)`.
    pub fn toggle_toolbar_dropdown(
        &mut self,
        toolbar_id: &str,
        item_id: &str,
        x: f32,
        y: f32,
        items: Vec<ToolbarItem>,
        cx: &mut Context<Self>,
    ) {
        if let Some(ref current) = self.toolbar_view.open_dropdown {
            if current.toolbar_id == toolbar_id && current.item_id == item_id {
                self.toolbar_view.open_dropdown = None;
                self.toolbar_view.hovered_tooltip = None;
                cx.notify();
                return;
            }
        }

        self.toolbar_view.open_dropdown = Some(OpenToolbarDropdown {
            toolbar_id: toolbar_id.to_string(),
            item_id: item_id.to_string(),
            anchor_x: x,
            anchor_y: y,
            items,
            active_path: Vec::new(),
            hovered_index: None,
        });
        self.toolbar_view.hovered_tooltip = None;
        cx.notify();
    }

    /// Closes any open toolbar dropdown.
    pub fn close_toolbar_dropdown(&mut self, cx: &mut Context<Self>) {
        if self.toolbar_view.open_dropdown.is_some() {
            self.toolbar_view.open_dropdown = None;
            cx.notify();
        }
    }

    /// Opens the toolbar context menu at window coordinates.
    pub fn open_toolbar_context_menu(
        &mut self,
        x: f32,
        y: f32,
        area: (f32, f32, f32, f32),
        cx: &mut Context<Self>,
    ) {
        self.close_toolbar_dropdown(cx);
        self.toolbar_context_menu = ToolbarContextMenuState {
            is_open: true,
            position_x: x,
            position_y: y,
            area_x: area.0,
            area_y: area.1,
            area_w: area.2,
            area_h: area.3,
        };
        cx.notify();
    }

    /// Closes the toolbar context menu.
    pub fn close_toolbar_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.toolbar_context_menu.is_open {
            self.toolbar_context_menu.is_open = false;
            cx.notify();
        }
    }

    /// Handles keyboard events when a toolbar dropdown is open.
    /// Returns true if the key was consumed.
    pub fn handle_toolbar_dropdown_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(ref mut dd) = self.toolbar_view.open_dropdown else {
            return false;
        };

        match key {
            "escape" => {
                if !dd.active_path.is_empty() {
                    dd.active_path.pop();
                    dd.hovered_index = None;
                } else {
                    self.toolbar_view.open_dropdown = None;
                }
                cx.notify();
                true
            }
            "arrowdown" => {
                let current_items = get_items_at_path(&dd.items, &dd.active_path);
                let valid_indices: Vec<usize> = current_items
                    .iter()
                    .enumerate()
                    .filter(|(_, it)| !matches!(it.kind, ToolbarItemKind::Separator))
                    .map(|(i, _)| i)
                    .collect();

                if !valid_indices.is_empty() {
                    let next = match dd.hovered_index {
                        Some(curr) => {
                            if let Some(pos) = valid_indices.iter().position(|&i| i == curr) {
                                valid_indices[(pos + 1) % valid_indices.len()]
                            } else {
                                valid_indices[0]
                            }
                        }
                        None => valid_indices[0],
                    };
                    dd.hovered_index = Some(next);
                    cx.notify();
                }
                true
            }
            "arrowup" => {
                let current_items = get_items_at_path(&dd.items, &dd.active_path);
                let valid_indices: Vec<usize> = current_items
                    .iter()
                    .enumerate()
                    .filter(|(_, it)| !matches!(it.kind, ToolbarItemKind::Separator))
                    .map(|(i, _)| i)
                    .collect();

                if !valid_indices.is_empty() {
                    let prev = match dd.hovered_index {
                        Some(curr) => {
                            if let Some(pos) = valid_indices.iter().position(|&i| i == curr) {
                                if pos == 0 {
                                    valid_indices[valid_indices.len() - 1]
                                } else {
                                    valid_indices[pos - 1]
                                }
                            } else {
                                valid_indices[valid_indices.len() - 1]
                            }
                        }
                        None => valid_indices[valid_indices.len() - 1],
                    };
                    dd.hovered_index = Some(prev);
                    cx.notify();
                }
                true
            }
            "arrowright" => {
                let current_items = get_items_at_path(&dd.items, &dd.active_path);
                if let Some(hovered) = dd.hovered_index {
                    if let Some(item) = current_items.get(hovered) {
                        if let ToolbarItemKind::Dropdown { ref items, .. } = item.kind {
                            if !items.is_empty() && dd.active_path.len() < MAX_TOOLBAR_DROPDOWN_DEPTH {
                                dd.active_path.push(hovered);
                                dd.hovered_index = Some(0);
                                cx.notify();
                                return true;
                            }
                        }
                    }
                }
                false
            }
            "arrowleft" => {
                if !dd.active_path.is_empty() {
                    let last = dd.active_path.pop();
                    dd.hovered_index = last;
                    cx.notify();
                    true
                } else {
                    false
                }
            }
            "enter" => {
                let current_items = get_items_at_path(&dd.items, &dd.active_path);
                if let Some(hovered) = dd.hovered_index {
                    if let Some(item) = current_items.get(hovered) {
                        match &item.kind {
                            ToolbarItemKind::Button { command_id, .. } => {
                                let cmd = command_id.clone();
                                self.toolbar_view.open_dropdown = None;
                                self.execute_command(&cmd, window, cx);
                                window.focus(&self.focus_handle);
                                cx.notify();
                                return true;
                            }
                            ToolbarItemKind::Dropdown { command, items, .. } => {
                                if !items.is_empty() && dd.active_path.len() < MAX_TOOLBAR_DROPDOWN_DEPTH {
                                    dd.active_path.push(hovered);
                                    dd.hovered_index = Some(0);
                                    cx.notify();
                                    return true;
                                } else if let Some(cmd) = command {
                                    let cmd = cmd.clone();
                                    self.toolbar_view.open_dropdown = None;
                                    self.execute_command(&cmd, window, cx);
                                    window.focus(&self.focus_handle);
                                    cx.notify();
                                    return true;
                                }
                            }
                            ToolbarItemKind::Separator => {}
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Renders horizontal toolbar strips placed at the given location (e.g. Top).
    pub fn render_toolbar_strips(
        &self,
        placement: ToolbarPlacement,
        origin_x: f32,
        origin_y: f32,
        available_w: f32,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let toolbars = self.state.toolbars.get_toolbars(placement);
        if toolbars.is_empty() {
            return None;
        }

        let fmt_state = self.state.active_formatting_state();

        let mut container = div()
            .id(match placement {
                ToolbarPlacement::Top => "editor-toolbar-top-container",
                ToolbarPlacement::Bottom => "editor-toolbar-bottom-container",
            })
            .flex()
            .flex_col()
            .w_full()
            .bg(theme.bg_card)
            .border_b_1()
            .border_color(theme.border_subtle);

        for spec in toolbars {
            container = container.child(self.render_single_toolbar_strip(
                &spec,
                origin_x,
                origin_y,
                available_w,
                &fmt_state,
                theme,
                cx,
            ));
        }

        Some(container)
    }

    /// Renders the context menu shown by right-clicking the empty toolbar area.
    pub fn render_toolbar_context_menu(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let menu_w = 240.0;
        let menu_h = 44.0;
        let bounds = window.bounds();
        let win_w = f32::from(bounds.size.width);
        let win_h = f32::from(bounds.size.height);
        let menu_x = self
            .toolbar_context_menu
            .position_x
            .clamp(8.0, (win_w - menu_w - 8.0).max(8.0));
        let menu_y = self
            .toolbar_context_menu
            .position_y
            .clamp(8.0, (win_h - menu_h - 8.0).max(8.0));
        let label = self.t("settings.toolbar.open_settings");

        div()
            .id("toolbar-context-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_toolbar_context_menu(cx);
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let x: f32 = event.position.x.into();
                let y: f32 = event.position.y.into();
                let menu = &this.toolbar_context_menu;
                let in_strip = x >= menu.area_x
                    && x <= menu.area_x + menu.area_w
                    && y >= menu.area_y
                    && y <= menu.area_y + menu.area_h;
                let area = (menu.area_x, menu.area_y, menu.area_w, menu.area_h);
                this.close_toolbar_context_menu(cx);
                // A second right-click on the same toolbar reopens the menu there.
                if in_strip {
                    this.open_toolbar_context_menu(x, y, area, cx);
                }
            }))
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("toolbar-context-panel")
                    .absolute()
                    .left(px(menu_x))
                    .top(px(menu_y))
                    .w(px(menu_w))
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_lg()
                    .shadow_xl()
                    .p_1()
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_move(|_, _, cx| cx.stop_propagation())
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id("toolbar-context-settings")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_2p5()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_SETTINGS,
                                14.0,
                                theme.text_secondary,
                            ))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.text_primary)
                                    .child(label),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_toolbar_context_menu(cx);
                                this.open_settings(
                                    Some(crate::views::settings_view::SettingsTab::PluginSettings(
                                        crate::plugins::editor_toolbar::TOOLBAR_PLUGIN_ID
                                            .to_string(),
                                    )),
                                    cx,
                                );
                            })),
                    ),
            )
    }

    /// Renders a single horizontal toolbar strip with overflow support.
    fn render_single_toolbar_strip(
        &self,
        spec: &ToolbarSpec,
        origin_x: f32,
        origin_y: f32,
        available_w: f32,
        fmt: &ActiveFormatting,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let toolbar_id = spec.id.clone();

        // Calculate item widths and partition for overflow (larger 32px buttons)
        let item_widths: Vec<f32> = spec
            .items
            .iter()
            .map(|item| match &item.kind {
                ToolbarItemKind::Button { .. } => 32.0,
                ToolbarItemKind::Dropdown { command, .. } => {
                    if command.is_some() {
                        46.0 // split button: 28px main + 18px arrow
                    } else {
                        44.0 // icon (17px) + chevron (12px) + gap (3px) + padding (6px * 2) = 44px
                    }
                }
                ToolbarItemKind::Separator => 11.0, // 1px line + 10px margin
            })
            .collect();

        let max_w = (available_w - 24.0).max(100.0);
        let mut visible_items = Vec::new();
        let mut visible_widths = Vec::new();
        let mut overflow_items = Vec::new();
        let mut current_w = 0.0;
        let overflow_btn_w = 44.0;

        for (idx, item) in spec.items.iter().enumerate() {
            let w = item_widths[idx] + 3.0; // width + gap
            let remaining_count = spec.items.len() - idx;
            let need_overflow = (current_w + w + if remaining_count > 1 { overflow_btn_w } else { 0.0 }) > max_w;

            if need_overflow && idx > 0 {
                overflow_items.push(item.clone());
            } else {
                current_w += w;
                visible_items.push(item.clone());
                visible_widths.push(item_widths[idx]);
            }
        }

        // Center the row when the toolbar is configured that way. The same offset is used
        // for tooltips, dropdown anchors and drag slots so they stay aligned with the
        // actual flex layout.
        let content_width = visible_widths.iter().sum::<f32>()
            + 3.0 * (visible_widths.len().saturating_sub(1) as f32)
            + if overflow_items.is_empty() {
                0.0
            } else if visible_items.is_empty() {
                overflow_btn_w
            } else {
                3.0 + overflow_btn_w
            };
        let inner_width = (available_w - 16.0).max(0.0);
        let content_offset = if spec.alignment == ToolbarAlignment::Center {
            8.0 + ((inner_width - content_width) / 2.0).max(0.0)
        } else {
            8.0
        };

        // Drag state for this strip: the source and the icon under the pointer. The
        // insertion line is drawn between icons and the order changes only on release.
        let drag = self
            .toolbar_drag
            .as_ref()
            .filter(|drag| drag.is_dragging && drag.toolbar_id == toolbar_id && drag.can_drag);
        let from_index = drag.and_then(|drag| {
            visible_items
                .iter()
                .position(|item| item.id == drag.source_id)
        });
        let to_index = drag.and_then(|drag| {
            drag.target_id
                .as_ref()
                .and_then(|target| visible_items.iter().position(|item| &item.id == target))
        });
        let drop_boundary = match (from_index, to_index) {
            (Some(from), Some(to)) if from != to => {
                Some(if from < to { to + 1 } else { to })
            }
            _ => None,
        };
        let indicator_x = drop_boundary.map(|boundary| {
            content_offset + visible_widths[..boundary].iter().sum::<f32>()
                + 3.0 * boundary as f32
                - 1.5
        });

        let mut strip = div()
            .id(SharedString::from(format!("toolbar-strip-{}", toolbar_id)))
            .relative()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .when(spec.alignment == ToolbarAlignment::Center, |d| {
                d.justify_center()
            })
            .h(px(40.0))
            .px(px(8.0))
            .gap(px(3.0))
            .overflow_hidden();

        let item_start_y = origin_y;
        let mut item_slots: Vec<(f32, f32, String, bool)> = Vec::new();

        // Render visible items with accurate accumulated X positions in window coordinates
        let mut current_item_x = origin_x + content_offset;
        for (i, item) in visible_items.iter().enumerate() {
            let item_w = visible_widths[i];
            item_slots.push((
                current_item_x,
                current_item_x + item_w,
                item.id.clone(),
                !item.is_separator(),
            ));
            strip = strip.child(self.render_toolbar_item(
                &toolbar_id,
                item,
                current_item_x,
                item_start_y,
                item_w,
                fmt,
                theme,
                cx,
            ));
            current_item_x += item_w + 3.0;
        }

        // Render overflow menu if items exceeded available width
        if !overflow_items.is_empty() {
            let overflow_id = format!("{}-overflow", toolbar_id);
            // Clickable (opens the overflow menu) but not part of the reorderable list.
            item_slots.push((
                current_item_x,
                current_item_x + overflow_btn_w,
                overflow_id.clone(),
                false,
            ));
            let overflow_dropdown_item = ToolbarItem {
                id: overflow_id,
                kind: ToolbarItemKind::Dropdown {
                    icon: "more-horizontal".to_string(),
                    tooltip: Some("More actions".to_string()),
                    command: None,
                    items: overflow_items,
                },
            };
            strip = strip.child(self.render_toolbar_item(
                &toolbar_id,
                &overflow_dropdown_item,
                current_item_x,
                item_start_y,
                overflow_btn_w,
                fmt,
                theme,
                cx,
            ));
        }

        // Vertical insertion line where the dragged icon will land.
        if let Some(x) = indicator_x {
            strip = strip.child(
                div()
                    .id(SharedString::from(format!(
                        "toolbar-drop-indicator-{}",
                        toolbar_id
                    )))
                    .absolute()
                    .left(px(x))
                    .top(px(10.0))
                    .w(px(2.0))
                    .h(px(20.0))
                    .rounded_full()
                    .bg(theme.accent),
            );
        }

        // A press on an icon arms the drag; the icon's own mouse-up handler runs the
        // action when the gesture stayed a click.
        strip = strip.on_mouse_down(MouseButton::Left, cx.listener({
            let toolbar_id = toolbar_id.clone();
            let slots = item_slots.clone();
            move |this, event: &gpui::MouseDownEvent, _, cx| {
                let x: f32 = event.position.x.into();
                let y: f32 = event.position.y.into();
                if let Some((_, _, id, can_drag)) =
                    slots.iter().find(|(left, right, _, _)| x >= *left && x < *right)
                {
                    this.toolbar_view.hovered_tooltip = None;
                    this.toolbar_drag = Some(ToolbarDragState {
                        toolbar_id: toolbar_id.clone(),
                        source_id: id.clone(),
                        start_x: x,
                        pointer_x: x,
                        pointer_y: y,
                        is_dragging: false,
                        target_id: None,
                        can_drag: *can_drag,
                    });
                    cx.notify();
                }
            }
        }));
        strip = strip.on_mouse_move(cx.listener({
            let toolbar_id = toolbar_id.clone();
            let slots = item_slots.clone();
            move |this, event: &MouseMoveEvent, _, cx| {
                let Some(drag) = this.toolbar_drag.as_mut() else {
                    return;
                };
                if drag.toolbar_id != toolbar_id {
                    return;
                }
                let x: f32 = event.position.x.into();
                drag.pointer_x = x;
                drag.pointer_y = event.position.y.into();
                if drag.can_drag && (x - drag.start_x).abs() > 4.0 {
                    drag.is_dragging = true;
                }
                if drag.is_dragging {
                    if let Some((_, _, id, can_drag)) =
                        slots.iter().find(|(left, right, _, _)| x >= *left && x < *right)
                    {
                        if *can_drag {
                            drag.target_id = if id.as_str() == drag.source_id {
                                None
                            } else {
                                Some(id.clone())
                            };
                        }
                    }
                    cx.notify();
                }
            }
        }));
        // Released over a gap, separator or empty strip area: commit to the last
        // highlighted icon. Releases over an icon are committed here too, because the
        // icon's handler leaves a drag alone.
        strip = strip.on_mouse_up(MouseButton::Left, cx.listener({
            let toolbar_id = toolbar_id.clone();
            move |this, _, _, cx| {
                let Some(drag) = this.toolbar_drag.take() else {
                    return;
                };
                if drag.toolbar_id == toolbar_id && drag.is_dragging && drag.can_drag {
                    if let Some(target) = drag.target_id {
                        this.reorder_toolbar_item(
                            &toolbar_id,
                            &drag.source_id,
                            &target,
                            cx,
                        );
                    }
                }
                cx.notify();
            }
        }));
        strip = strip.on_mouse_up_out(MouseButton::Left, cx.listener({
            let toolbar_id = toolbar_id.clone();
            move |this, _, _, cx| {
                if this
                    .toolbar_drag
                    .as_ref()
                    .is_some_and(|drag| drag.toolbar_id == toolbar_id)
                {
                    this.toolbar_drag = None;
                    cx.notify();
                }
            }
        }));
        // Leaving the strip clears the indicated slot so the line never points at a
        // place the release cannot reach.
        strip = strip.on_hover(cx.listener({
            let toolbar_id = toolbar_id.clone();
            move |this, hovered: &bool, _, cx| {
                if *hovered {
                    return;
                }
                if let Some(drag) = this.toolbar_drag.as_mut() {
                    if drag.toolbar_id == toolbar_id && drag.target_id.take().is_some() {
                        cx.notify();
                    }
                }
            }
        }));

        // Right-click on the empty part of the strip opens the toolbar menu. Clicks on an
        // icon keep their normal behaviour.
        strip = strip.on_mouse_down(MouseButton::Right, cx.listener({
            let slots = item_slots.clone();
            move |this, event: &gpui::MouseDownEvent, _, cx| {
                let x: f32 = event.position.x.into();
                if slots.iter().any(|(left, right, _, _)| x >= *left && x < *right) {
                    return;
                }
                this.open_toolbar_context_menu(
                    event.position.x.into(),
                    event.position.y.into(),
                    (origin_x, origin_y, available_w, 40.0),
                    cx,
                );
            }
        }));

        strip
    }

    /// Reorders an icon inside a toolbar and persists the bundled toolbar config.
    pub(crate) fn reorder_toolbar_item(
        &mut self,
        toolbar_id: &str,
        from_id: &str,
        to_id: &str,
        cx: &mut Context<Self>,
    ) {
        if from_id == to_id {
            return;
        }
        let Some(spec) = self.state.toolbars.get_toolbar(toolbar_id) else {
            return;
        };
        let mut items = spec.items;
        let Some(from) = items.iter().position(|item| item.id == from_id) else {
            return;
        };
        let Some(to) = items.iter().position(|item| item.id == to_id) else {
            return;
        };

        let item = items.remove(from);
        items.insert(to, item);
        self.state
            .toolbars
            .update_toolbar(toolbar_id, items.clone());

        if toolbar_id == crate::plugins::editor_toolbar::TOOLBAR_PLUGIN_ID {
            let root = self
                .state
                .vault
                .as_ref()
                .map(|vault| vault.root.clone())
                .or_else(|| self.state.workspace_path.clone());
            if let Some(root) = root {
                let _ = crate::plugins::editor_toolbar::save_toolbar_config(&root, &items);
            }
        }
        cx.notify();
    }

    /// Floating preview of the toolbar icon being reordered, drawn at the pointer.
    pub(crate) fn render_toolbar_drag_ghost(&self, theme: &Theme) -> Option<AnyElement> {
        let drag = self.toolbar_drag.as_ref().filter(|drag| drag.is_dragging)?;
        let spec = self.state.toolbars.get_toolbar(&drag.toolbar_id)?;
        let item = spec.items.iter().find(|item| item.id == drag.source_id)?;
        let icon = item.icon()?;
        let title = item
            .tooltip()
            .map(|title| title.to_string())
            .or_else(|| item.command_id().and_then(|cmd| self.get_command_name(cmd)))
            .unwrap_or_default();
        let icon_asset = icons::resolve_toolbar_icon(icon);

        Some(
            div()
                .absolute()
                .left(px(drag.pointer_x + 14.0))
                .top(px(drag.pointer_y + 10.0))
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(theme.bg_card)
                .border_1()
                .border_color(theme.accent)
                .shadow_lg()
                .opacity(0.92)
                .child(
                    gpui::svg()
                        .path(icon_asset)
                        .w(px(15.0))
                        .h(px(15.0))
                        .text_color(theme.accent)
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_primary)
                        .child(title),
                )
                .into_any_element(),
        )
    }

    /// Renders an individual item (button, split-dropdown, dropdown, or separator).
    fn render_toolbar_item(
        &self,
        toolbar_id: &str,
        item: &ToolbarItem,
        item_x: f32,
        item_y: f32,
        item_w: f32,
        fmt: &ActiveFormatting,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_dragged = self.toolbar_drag.as_ref().is_some_and(|drag| {
            drag.is_dragging
                && drag.toolbar_id == toolbar_id
                && drag.source_id == item.id
        });
        // While reordering, the insertion line is the only affordance: no hover/active
        // highlight on the icon under the pointer.
        let drag_active = self
            .toolbar_drag
            .as_ref()
            .is_some_and(|drag| drag.is_dragging);

        match &item.kind {
            ToolbarItemKind::Button { command_id, icon, tooltip } => {
                let cmd_id = command_id.clone();
                let is_disabled = is_command_disabled(&cmd_id, fmt);

                let icon_asset = icons::resolve_toolbar_icon(icon);
                let hotkey = self.get_command_hotkey(&cmd_id);
                let tooltip_text = tooltip.clone().or_else(|| self.get_command_name(&cmd_id));

                let tb_id = toolbar_id.to_string();
                let it_id = item.id.clone();

                let mut btn = div()
                    .id(SharedString::from(format!("tb-btn-{}-{}", tb_id, it_id)))
                    .h(px(32.0))
                    .w(px(32.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .when(is_dragged, |d| {
                        d.border_1()
                            .border_color(theme.accent)
                            .bg(theme.bg_selected)
                    });

                if is_disabled {
                    btn = btn
                        .opacity(0.35)
                        .cursor_not_allowed();
                } else {
                    btn = btn
                        .cursor_pointer()
                        .when(!drag_active, |d| {
                            d.hover(|h| h.bg(theme.bg_hover))
                                .active(|a| a.bg(theme.bg_hover))
                        });

                    let cmd_to_exec = cmd_id.clone();
                    let source_id = it_id.clone();
                    btn = btn.on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        let Some(drag) = this.toolbar_drag.as_ref() else {
                            return;
                        };
                        if drag.is_dragging || drag.source_id != source_id {
                            return;
                        }
                        this.toolbar_drag = None;
                        this.close_toolbar_dropdown(cx);
                        this.execute_command(&cmd_to_exec, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }));

                    if let Some(title) = tooltip_text {
                        let hk = hotkey;
                        btn = btn.on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                            if this
                                .toolbar_drag
                                .as_ref()
                                .is_some_and(|drag| drag.is_dragging)
                            {
                                this.toolbar_view.hovered_tooltip = None;
                                cx.notify();
                                return;
                            }
                            if *is_hovered {
                                this.toolbar_view.hovered_tooltip = Some(ToolbarTooltipInfo {
                                    title: title.clone(),
                                    hotkey: hk.clone(),
                                    x: item_x,
                                    y: item_y + 42.0,
                                    item_w,
                                });
                            } else if let Some(ref current) = this.toolbar_view.hovered_tooltip {
                                if current.title == title {
                                    this.toolbar_view.hovered_tooltip = None;
                                }
                            }
                            cx.notify();
                        }));
                    }
                }

                let icon_color = if is_disabled {
                    theme.text_muted
                } else {
                    theme.text_primary
                };

                btn.child(
                    gpui::svg()
                        .path(icon_asset)
                        .w(px(17.0))
                        .h(px(17.0))
                        .text_color(icon_color)
                        .flex_shrink_0()
                )
            }
            ToolbarItemKind::Dropdown { icon, tooltip, command, items } => {
                let tb_id = toolbar_id.to_string();
                let it_id = item.id.clone();
                let dropdown_items = items.clone();

                let icon_asset = icons::resolve_toolbar_icon(icon);

                let is_open = self
                    .toolbar_view
                    .open_dropdown
                    .as_ref()
                    .map(|d| d.toolbar_id == tb_id && d.item_id == it_id)
                    .unwrap_or(false);

                let icon_color = theme.text_primary;

                let tooltip_text = tooltip.clone();

                if let Some(cmd_id) = command {
                    // Split button: main button executes command, arrow part opens menu
                    let cmd_to_exec = cmd_id.clone();
                    let tb_id_arrow = tb_id.clone();
                    let it_id_arrow = it_id.clone();
                    let dd_items_arrow = dropdown_items.clone();

                    let mut split_container = div()
                        .id(SharedString::from(format!("tb-split-{}-{}", tb_id, it_id)))
                        .flex()
                        .flex_row()
                        .items_center()
                        .h(px(32.0))
                        .w(px(46.0))
                        .flex_shrink_0()
                        .rounded_md()
                        .when(is_dragged, |d| {
                            d.border_1()
                                .border_color(theme.accent)
                                .bg(theme.bg_selected)
                        })
                        .when(is_open, |d| d.bg(theme.bg_hover))
                        .when(!drag_active, |d| d.hover(|h| h.bg(theme.bg_hover)));

                    // Main button
                    let main_btn = div()
                        .h_full()
                        .w(px(28.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .rounded_l_md()
                        .when(!drag_active, |d| d.hover(|h| h.bg(theme.bg_hover)))
                        .on_mouse_up(MouseButton::Left, cx.listener({
                            let source_id = it_id.clone();
                            move |this, _, window, cx| {
                                let Some(drag) = this.toolbar_drag.as_ref() else {
                                    return;
                                };
                                if drag.is_dragging || drag.source_id != source_id {
                                    return;
                                }
                                this.toolbar_drag = None;
                                this.close_toolbar_dropdown(cx);
                                this.execute_command(&cmd_to_exec, window, cx);
                                window.focus(&this.focus_handle);
                                cx.notify();
                            }
                        }))
                        .child(
                            gpui::svg()
                                .path(icon_asset)
                                .w(px(17.0))
                                .h(px(17.0))
                                .text_color(icon_color)
                                .flex_shrink_0()
                        );

                    // Dropdown chevron arrow
                    let arrow_btn = div()
                        .h_full()
                        .w(px(18.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .rounded_r_md()
                        .when(!drag_active, |d| d.hover(|h| h.bg(theme.bg_hover)))
                        .on_mouse_up(MouseButton::Left, cx.listener({
                            let source_id = it_id.clone();
                            move |this, _, _, cx| {
                                let Some(drag) = this.toolbar_drag.as_ref() else {
                                    return;
                                };
                                if drag.is_dragging || drag.source_id != source_id {
                                    return;
                                }
                                this.toolbar_drag = None;
                                this.toggle_toolbar_dropdown(
                                    &tb_id_arrow,
                                    &it_id_arrow,
                                    item_x,
                                    item_y + 42.0,
                                    dd_items_arrow.clone(),
                                    cx,
                                );
                            }
                        }))
                        .child(
                            gpui::svg()
                                .path(icons::ICON_CHEVRON_DOWN)
                                .w(px(12.0))
                                .h(px(12.0))
                                .text_color(theme.text_secondary)
                                .flex_shrink_0()
                        );

                    split_container = split_container.child(main_btn).child(arrow_btn);

                    if let Some(title) = tooltip_text {
                        split_container = split_container.on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                            if this
                                .toolbar_drag
                                .as_ref()
                                .is_some_and(|drag| drag.is_dragging)
                            {
                                this.toolbar_view.hovered_tooltip = None;
                                cx.notify();
                                return;
                            }
                            if *is_hovered {
                                this.toolbar_view.hovered_tooltip = Some(ToolbarTooltipInfo {
                                    title: title.clone(),
                                    hotkey: None,
                                    x: item_x,
                                    y: item_y + 42.0,
                                    item_w,
                                });
                            } else if let Some(ref current) = this.toolbar_view.hovered_tooltip {
                                if current.title == title {
                                    this.toolbar_view.hovered_tooltip = None;
                                }
                            }
                            cx.notify();
                        }));
                    }

                    split_container
                } else {
                    // Regular dropdown button: clicking opens dropdown
                    let tb_id_click = tb_id.clone();
                    let it_id_click = it_id.clone();
                    let dd_items_click = dropdown_items.clone();

                    let mut dropdown_btn = div()
                        .id(SharedString::from(format!("tb-dropdown-{}-{}", tb_id, it_id)))
                        .h(px(32.0))
                        .w(px(44.0))
                        .flex_shrink_0()
                        .px(px(6.0))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .gap(px(3.0))
                        .rounded_md()
                        .cursor_pointer()
                        .when(is_dragged, |d| {
                            d.border_1()
                                .border_color(theme.accent)
                                .bg(theme.bg_selected)
                        })
                        .when(is_open, |d| d.bg(theme.bg_hover))
                        .when(!drag_active, |d| d.hover(|h| h.bg(theme.bg_hover)))
                        .on_mouse_up(MouseButton::Left, cx.listener({
                            let source_id = it_id.clone();
                            move |this, _, _, cx| {
                                let Some(drag) = this.toolbar_drag.as_ref() else {
                                    return;
                                };
                                if drag.is_dragging || drag.source_id != source_id {
                                    return;
                                }
                                this.toolbar_drag = None;
                                this.toggle_toolbar_dropdown(
                                    &tb_id_click,
                                    &it_id_click,
                                    item_x,
                                    item_y + 42.0,
                                    dd_items_click.clone(),
                                    cx,
                                );
                            }
                        }))
                        .child(
                            gpui::svg()
                                .path(icon_asset)
                                .w(px(17.0))
                                .h(px(17.0))
                                .text_color(icon_color)
                                .flex_shrink_0()
                        )
                        .child(
                            gpui::svg()
                                .path(icons::ICON_CHEVRON_DOWN)
                                .w(px(12.0))
                                .h(px(12.0))
                                .text_color(theme.text_secondary)
                                .flex_shrink_0()
                        );

                    if let Some(title) = tooltip_text {
                        dropdown_btn = dropdown_btn.on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                            if this
                                .toolbar_drag
                                .as_ref()
                                .is_some_and(|drag| drag.is_dragging)
                            {
                                this.toolbar_view.hovered_tooltip = None;
                                cx.notify();
                                return;
                            }
                            if *is_hovered {
                                this.toolbar_view.hovered_tooltip = Some(ToolbarTooltipInfo {
                                    title: title.clone(),
                                    hotkey: None,
                                    x: item_x,
                                    y: item_y + 42.0,
                                    item_w,
                                });
                            } else if let Some(ref current) = this.toolbar_view.hovered_tooltip {
                                if current.title == title {
                                    this.toolbar_view.hovered_tooltip = None;
                                }
                            }
                            cx.notify();
                        }));
                    }

                    dropdown_btn
                }
            }
            ToolbarItemKind::Separator => div()
                .id(SharedString::from(format!("tb-sep-{}-{}", toolbar_id, item.id)))
                .h(px(20.0))
                .w(px(1.0))
                .bg(theme.border_subtle)
                .mx(px(5.0))
                .flex_shrink_0(),
        }
    }

    /// Renders floating recursive dropdown menus (overlays).
    pub fn render_toolbar_dropdown_overlay(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let dd = self.toolbar_view.open_dropdown.as_ref()?;
        let fmt = self.state.active_formatting_state();

        let window_bounds = window.bounds();
        let window_w: f32 = window_bounds.size.width.into();
        let window_h: f32 = window_bounds.size.height.into();

        let menu_w = 200.0;
        let item_h = 28.0;

        // Backdrop to dismiss on outside click
        let backdrop = div()
            .id("toolbar-dropdown-backdrop")
            .absolute()
            .inset_0()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_toolbar_dropdown(cx);
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_toolbar_dropdown(cx);
            }));

        let mut overlay = div()
            .id("toolbar-dropdown-overlay-root")
            .absolute()
            .inset_0()
            .child(backdrop);

        // Render each open menu hierarchy level (Level 0 .. Level N)
        let levels_count = dd.active_path.len() + 1;
        let mut curr_x = dd.anchor_x.min(window_w - menu_w - 8.0).max(8.0);
        let mut curr_y = dd.anchor_y.min(window_h - 100.0).max(8.0);

        for level in 0..levels_count {
            let path_slice = &dd.active_path[..level];
            let items = get_items_at_path(&dd.items, path_slice);
            if items.is_empty() {
                break;
            }

            let menu_h = (items.len() as f32 * item_h + 10.0).min(window_h - 40.0);
            let display_y = if curr_y + menu_h > window_h - 8.0 {
                (window_h - menu_h - 8.0).max(8.0)
            } else {
                curr_y
            };

            let level_id = format!("tb-dropdown-level-{}", level);
            let mut menu_box = div()
                .id(SharedString::from(level_id))
                .absolute()
                .left(px(curr_x))
                .top(px(display_y))
                .w(px(menu_w))
                .bg(theme.bg_card)
                .border_1()
                .border_color(theme.border_subtle)
                .rounded_lg()
                .shadow_lg()
                .py(px(4.0))
                .overflow_y_scroll()
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Right, |_, _, cx| {
                    cx.stop_propagation();
                });

            for (idx, it) in items.iter().enumerate() {
                let is_hovered = if level == dd.active_path.len() {
                    dd.hovered_index == Some(idx)
                } else {
                    dd.active_path.get(level).copied() == Some(idx)
                };

                let item_element = self.render_dropdown_menu_item(
                    it,
                    idx,
                    level,
                    is_hovered,
                    &fmt,
                    theme,
                    cx,
                );
                menu_box = menu_box.child(item_element);
            }

            overlay = overlay.child(menu_box);

            // Compute coordinates for next level (submenu)
            if let Some(&selected_sub_idx) = dd.active_path.get(level) {
                let mut next_x = curr_x + menu_w + 2.0;
                // If opening right would go off-screen, open to the left!
                if next_x + menu_w > window_w - 8.0 {
                    next_x = (curr_x - menu_w - 2.0).max(8.0);
                }
                curr_x = next_x;
                curr_y = display_y + (selected_sub_idx as f32 * item_h);
            }
        }

        Some(overlay)
    }

    /// Renders a single row item inside the dropdown popup menu.
    fn render_dropdown_menu_item(
        &self,
        item: &ToolbarItem,
        item_idx: usize,
        level: usize,
        is_hovered: bool,
        fmt: &ActiveFormatting,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        match &item.kind {
            ToolbarItemKind::Separator => div()
                .id(SharedString::from(format!("tb-dd-sep-{}-{}", level, item_idx)))
                .h(px(1.0))
                .w_full()
                .bg(theme.border_subtle)
                .my(px(3.0)),
            ToolbarItemKind::Button { command_id, icon, tooltip } => {
                let cmd_id = command_id.clone();
                let icon_asset = icons::resolve_toolbar_icon(icon);
                let label = tooltip
                    .clone()
                    .or_else(|| self.get_command_name(&cmd_id))
                    .unwrap_or_else(|| cmd_id.clone());
                let hotkey = self.get_command_hotkey(&cmd_id);

                let is_disabled = is_command_disabled(&cmd_id, fmt);

                let mut row = div()
                    .id(SharedString::from(format!("tb-dd-item-{}-{}", level, item_idx)))
                    .h(px(28.0))
                    .px(px(8.0))
                    .mx(px(4.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .rounded_md();

                if is_disabled {
                    row = row.opacity(0.4).cursor_not_allowed();
                } else {
                    row = row
                        .cursor_pointer()
                        .when(is_hovered, |d| d.bg(theme.bg_hover))
                        .hover(|h| h.bg(theme.bg_hover));

                    let cmd_to_exec = cmd_id.clone();
                    row = row.on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        this.close_toolbar_dropdown(cx);
                        this.execute_command(&cmd_to_exec, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }));

                    row = row.on_hover(cx.listener(move |this, is_h: &bool, _, cx| {
                        if *is_h {
                            if let Some(ref mut d) = this.toolbar_view.open_dropdown {
                                d.hovered_index = Some(item_idx);
                                // Truncate deeper submenus if hovering a leaf button
                                d.active_path.truncate(level);
                                cx.notify();
                            }
                        }
                    }));
                }

                let icon_color = if is_disabled {
                    theme.text_muted
                } else {
                    theme.text_secondary
                };

                let left_side = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        gpui::svg()
                            .path(icon_asset)
                            .w(px(16.0))
                            .h(px(16.0))
                            .text_color(icon_color)
                            .flex_shrink_0()
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(if is_disabled { theme.text_muted } else { theme.text_primary })
                            .child(label)
                    );

                let mut right_side = div().flex().items_center();
                if let Some(hk) = hotkey {
                    right_side = right_side.child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(hk)
                    );
                }

                row.child(left_side).child(right_side)
            }
            ToolbarItemKind::Dropdown { icon, tooltip, command, items } => {
                let icon_asset = icons::resolve_toolbar_icon(icon);
                let label = tooltip.clone().unwrap_or_else(|| "Submenu".to_string());
                let has_children = !items.is_empty();
                let split_command = command.clone();

                let mut row = div()
                    .id(SharedString::from(format!("tb-dd-sub-{}-{}", level, item_idx)))
                    .h(px(28.0))
                    .px(px(8.0))
                    .mx(px(4.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_hovered, |d| d.bg(theme.bg_hover))
                    .hover(|h| h.bg(theme.bg_hover));

                // Open submenu on hover or click
                row = row.on_hover(cx.listener(move |this, is_h: &bool, _, cx| {
                    if *is_h {
                        if let Some(ref mut d) = this.toolbar_view.open_dropdown {
                            d.hovered_index = Some(item_idx);
                            if has_children && level < MAX_TOOLBAR_DROPDOWN_DEPTH {
                                d.active_path.truncate(level);
                                d.active_path.push(item_idx);
                                cx.notify();
                            }
                        }
                    }
                }));

                // If split command is present, clicking the row executes it if clicked directly
                if let Some(cmd) = split_command {
                    let cmd_to_exec = cmd.clone();
                    row = row.on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        this.close_toolbar_dropdown(cx);
                        this.execute_command(&cmd_to_exec, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }));
                }

                let left_side = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        gpui::svg()
                            .path(icon_asset)
                            .w(px(16.0))
                            .h(px(16.0))
                            .text_color(theme.text_secondary)
                            .flex_shrink_0()
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_primary)
                            .child(label)
                    );

                let right_side = gpui::svg()
                    .path(icons::ICON_CHEVRON_RIGHT)
                    .w(px(12.0))
                    .h(px(12.0))
                    .text_color(theme.text_secondary)
                    .flex_shrink_0();

                row.child(left_side).child(right_side)
            }
        }
    }

    /// Renders floating hover tooltip for toolbar buttons.
    pub fn render_toolbar_tooltip_overlay(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let tip = self.toolbar_view.hovered_tooltip.as_ref()?;
        let _ = cx;

        let content = if let Some(ref hk) = tip.hotkey {
            if tip.title.contains(hk) {
                tip.title.clone()
            } else {
                format!("{} ({})", tip.title, hk)
            }
        } else {
            tip.title.clone()
        };

        let run = TextRun {
            len: content.len(),
            font: gpui::font("Segoe UI"),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(content.clone()),
            px(12.0),
            &[run],
            None,
        );
        let text_w = f32::from(shaped.width);
        // Padding: px(6.0) on left + px(6.0) on right (12.0) + border_1() on left and right (2.0) = 14.0px
        let total_tooltip_w = text_w + 14.0;

        let window_w: f32 = window.bounds().size.width.into();
        let btn_center_x = tip.x + (tip.item_w / 2.0);
        let centered_x = (btn_center_x - (total_tooltip_w / 2.0)).max(8.0);
        let final_x = centered_x.min((window_w - total_tooltip_w - 8.0).max(8.0));

        Some(
            div()
                .id("toolbar-hover-tooltip")
                .absolute()
                .left(px(final_x))
                .top(px(tip.y))
                .px(px(6.0))
                .py(px(2.0))
                .bg(theme.bg_card)
                .border_1()
                .border_color(theme.border_subtle)
                .rounded_md()
                .shadow_md()
                .font_family("Segoe UI")
                .text_size(px(12.0))
                .text_color(theme.text_primary)
                .child(content),
        )
    }

    /// Helper to get command name by ID.
    fn get_command_name(&self, command_id: &str) -> Option<String> {
        self.state.commands.get(command_id).map(|cmd| cmd.name.clone())
    }

    /// Helper to get command hotkey string by ID.
    fn get_command_hotkey(&self, command_id: &str) -> Option<String> {
        self.state.commands.get(command_id).and_then(|cmd| cmd.hotkey.clone())
    }
}

/// Helper function to retrieve child items at a given dropdown index path.
fn get_items_at_path<'a>(
    root: &'a [ToolbarItem],
    path: &[usize],
) -> &'a [ToolbarItem] {
    let mut current = root;
    for &idx in path {
        if let Some(item) = current.get(idx) {
            if let ToolbarItemKind::Dropdown { ref items, .. } = item.kind {
                current = items.as_slice();
            } else {
                return &[];
            }
        } else {
            return &[];
        }
    }
    current
}

/// Checks if a command ID is currently disabled.
fn is_command_disabled(command_id: &str, fmt: &ActiveFormatting) -> bool {
    match command_id {
        "editor:undo" => !fmt.can_undo,
        "editor:redo" => !fmt.can_redo,
        _ => false,
    }
}
