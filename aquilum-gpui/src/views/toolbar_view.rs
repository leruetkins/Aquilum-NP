//! Toolbar UI rendering: horizontal bar, recursive dropdown menus,
//! split buttons, overflow menus, and keyboard navigation.

use gpui::{
    div, prelude::*, px, Context, IntoElement, MouseButton, SharedString, Styled, TextRun, Window,
};
use crate::formatting::ActiveFormatting;
use crate::icons;
use crate::theme::Theme;
use crate::toolbar::{ToolbarItem, ToolbarItemKind, ToolbarPlacement, ToolbarSpec, MAX_TOOLBAR_DROPDOWN_DEPTH};
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

        let mut strip = div()
            .id(SharedString::from(format!("toolbar-strip-{}", toolbar_id)))
            .flex()
            .flex_row()
            .items_center()
            .h(px(40.0))
            .px(px(8.0))
            .gap(px(3.0))
            .overflow_hidden();

        let item_start_y = origin_y;

        // Render visible items with accurate accumulated X positions in window coordinates
        let mut current_item_x = origin_x + 8.0;
        for (i, item) in visible_items.iter().enumerate() {
            let item_w = visible_widths[i];
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
            let overflow_dropdown_item = ToolbarItem {
                id: format!("{}-overflow", toolbar_id),
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

        strip
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
                    .rounded_md();

                if is_disabled {
                    btn = btn
                        .opacity(0.35)
                        .cursor_not_allowed();
                } else {
                    btn = btn
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover))
                        .active(|a| a.bg(theme.bg_hover));

                    let cmd_to_exec = cmd_id.clone();
                    btn = btn.on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        this.close_toolbar_dropdown(cx);
                        this.execute_command(&cmd_to_exec, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }));

                    if let Some(title) = tooltip_text {
                        let hk = hotkey;
                        btn = btn.on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
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
                        .when(is_open, |d| d.bg(theme.bg_hover))
                        .hover(|h| h.bg(theme.bg_hover));

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
                        .hover(|h| h.bg(theme.bg_hover))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                            this.close_toolbar_dropdown(cx);
                            this.execute_command(&cmd_to_exec, window, cx);
                            window.focus(&this.focus_handle);
                            cx.notify();
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
                        .hover(|h| h.bg(theme.bg_hover))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_toolbar_dropdown(
                                &tb_id_arrow,
                                &it_id_arrow,
                                item_x,
                                item_y + 42.0,
                                dd_items_arrow.clone(),
                                cx,
                            );
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
                        .when(is_open, |d| d.bg(theme.bg_hover))
                        .hover(|h| h.bg(theme.bg_hover))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_toolbar_dropdown(
                                &tb_id_click,
                                &it_id_click,
                                item_x,
                                item_y + 42.0,
                                dd_items_click.clone(),
                                cx,
                            );
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
