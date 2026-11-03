use gpui::{
    div, prelude::*, px, AnyElement, Context, IntoElement, MouseButton, MouseMoveEvent,
    SharedString, Stateful, Styled, Window,
};

use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::settings_view::SettingsTab;

/// State for the Obsidian-style Ribbon context menu.
#[derive(Clone, Debug, Default)]
pub struct RibbonContextMenuState {
    pub is_open: bool,
    pub position_x: f32,
    pub position_y: f32,
}

/// Ribbon rail geometry, mirrored by the drop insertion indicator.
const RIBBON_ITEM_SIZE: f32 = 32.0;
const RIBBON_ITEM_GAP: f32 = 8.0;
const RIBBON_PAD_TOP: f32 = 8.0;

/// Descriptor of a ribbon item for display in the ribbon rail and context menu.
#[derive(Clone, Debug)]
pub struct RibbonItem {
    pub id: String,
    pub title: String,
    pub icon: &'static str,
    pub is_visible: bool,
    pub is_builtin: bool,
}

impl AppView {
    /// Returns the localized title for built-in ribbon items.
    fn builtin_ribbon_title(&self, id: &str) -> String {
        match id {
            "ribbon-quick-switcher" => self.t("app.ribbon.quick_switcher"),
            "ribbon-graph" => self.t("app.ribbon.graph"),
            "ribbon-canvas" => self.t("app.ribbon.new_canvas"),
            "ribbon-new-note" => self.t("app.ribbon.new_note"),
            "ribbon-calendar" => self.t("app.ribbon.daily_note"),
            "ribbon-template" => self.t("app.ribbon.insert_template"),
            "ribbon-commands" => self.t("app.ribbon.command_palette"),
            "ribbon-drawing" => self.t("app.ribbon.new_drawing"),
            "ribbon-refresh" => self.t("app.ribbon.sync_reload"),
            "ribbon-trash" => self.t("app.ribbon.trash"),
            _ => id.to_string(),
        }
    }

    /// Returns the list of all ribbon items (both built-in and plugin-contributed) with current visibility.
    pub fn get_all_ribbon_items(&self) -> Vec<RibbonItem> {
        let hidden = &self.state.session.ribbon_hidden;
        let mut items = Vec::new();

        // 1. Built-in actions matching Obsidian ribbon
        let builtins: [(&str, &'static str); 10] = [
            ("ribbon-quick-switcher", icons::ICON_SEARCH),
            ("ribbon-graph", icons::ICON_LAYERS),
            ("ribbon-canvas", icons::ICON_CANVAS),
            ("ribbon-new-note", icons::ICON_NOTE_CREATE),
            ("ribbon-calendar", icons::ICON_CALENDAR),
            ("ribbon-template", icons::ICON_COPY),
            ("ribbon-commands", icons::ICON_TERMINAL),
            ("ribbon-drawing", icons::ICON_DRAWING),
            ("ribbon-refresh", icons::ICON_REFRESH),
            ("ribbon-trash", icons::ICON_TRASH_2),
        ];

        for (id, icon) in builtins {
            let is_visible = !hidden.iter().any(|h| h == id);
            items.push(RibbonItem {
                id: id.to_string(),
                title: self.builtin_ribbon_title(id),
                icon,
                is_visible,
                is_builtin: true,
            });
        }

        // 2. Dynamic plugin-contributed actions
        for action in self.state.ribbon.all() {
            let is_visible = !hidden.iter().any(|h| h == &action.id);
            items.push(RibbonItem {
                id: action.id.clone(),
                title: action.tooltip.clone(),
                icon: action.icon,
                is_visible,
                is_builtin: false,
            });
        }

        // 3. Apply custom ordering if saved in session
        let order = &self.state.session.ribbon_order;
        if !order.is_empty() {
            items.sort_by_key(|item| {
                order.iter().position(|id| id == &item.id).unwrap_or(usize::MAX)
            });
        }

        items
    }

    /// Executes the action associated with a ribbon item id.
    pub fn execute_ribbon_action(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            "ribbon-quick-switcher" => {
                self.open_search_dialog(window, cx);
            }
            "ribbon-graph" => {
                self.state.status_text = self.t("app.ribbon.graph_status");
                cx.notify();
            }
            "ribbon-canvas" => {
                if let Some(ref root) = self.state.workspace_path {
                    let untitled = self.t("common.untitled");
                    if let Ok(canvas_path) = crate::views::sidebar::folder_actions::create_canvas_in_folder(
                        root,
                        &untitled,
                    ) {
                        self.state.reload_file_tree();
                        self.select_note(canvas_path, false, window, cx);
                    }
                }
            }
            "ribbon-new-note" => {
                self.create_new_note(window, cx);
            }
            "ribbon-calendar" => {
                self.create_new_note(window, cx);
            }
            "ribbon-template" => {
                self.state.status_text = self.t("settings.tabs.templates");
                cx.notify();
            }
            "ribbon-commands" => {
                self.open_command_palette(window, cx);
            }
            "ribbon-drawing" => {
                if let Some(ref root) = self.state.workspace_path {
                    if let Ok(drawing_path) = crate::views::sidebar::folder_actions::create_drawing_in_folder(
                        root,
                    ) {
                        self.state.reload_file_tree();
                        self.select_note(drawing_path, false, window, cx);
                    }
                }
            }
            "ribbon-refresh" => {
                self.state.reload_file_tree();
                cx.notify();
            }
            "ribbon-trash" => {
                self.open_settings(Some(SettingsTab::Trash), cx);
            }
            plugin_id => {
                println!("[ribbon] execute_ribbon_action for '{}'", plugin_id);
                if let Some(action) = self.state.ribbon.get(plugin_id) {
                    println!("[ribbon] Found ribbon action for '{}', invoking callback...", plugin_id);
                    let cb = action.callback.clone();
                    cb();
                    cx.notify();
                } else {
                    eprintln!("[ribbon] No ribbon action registered for '{}'", plugin_id);
                }
            }
        }
    }

    /// Reorders ribbon item `from_id` to the position of `to_id`.
    pub fn reorder_ribbon_item(&mut self, from_id: &str, to_id: &str, cx: &mut Context<Self>) {
        if from_id == to_id {
            return;
        }

        let all_items = self.get_all_ribbon_items();
        let mut order: Vec<String> = all_items.into_iter().map(|it| it.id).collect();

        let from_pos = order.iter().position(|id| id == from_id);
        let to_pos = order.iter().position(|id| id == to_id);

        if let (Some(f_idx), Some(t_idx)) = (from_pos, to_pos) {
            let item_id = order.remove(f_idx);
            order.insert(t_idx, item_id);
            self.state.session.ribbon_order = order;
            self.state.session.save(&crate::state::app_data_dir());
            cx.notify();
        }
    }

    /// Toggles the visibility of a ribbon item in the ribbon rail and persists in session.
    pub fn toggle_ribbon_item_visibility(&mut self, id: &str, cx: &mut Context<Self>) {
        let hidden = &mut self.state.session.ribbon_hidden;
        if let Some(idx) = hidden.iter().position(|h| h == id) {
            hidden.remove(idx);
        } else {
            hidden.push(id.to_string());
        }
        self.state.session.save(&crate::state::app_data_dir());
        cx.notify();
    }

    /// Toggles the visibility of the entire vertical ribbon rail.
    pub fn toggle_ribbon_rail_visibility(&mut self, cx: &mut Context<Self>) {
        self.state.session.ribbon_visible = !self.state.session.ribbon_visible;
        self.state.session.save(&crate::state::app_data_dir());
        cx.notify();
    }

    /// Opens the ribbon context menu popup at the given coordinates.
    pub fn open_ribbon_context_menu(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.file_context_menu.is_open = false;
        self.folder_context_menu.is_open = false;
        self.tab_context_menu.is_open = false;
        self.close_tabs_popup(cx);

        self.ribbon_context_menu = RibbonContextMenuState {
            is_open: true,
            position_x: x,
            position_y: y,
        };
        cx.notify();
    }

    /// Closes the ribbon context menu popup.
    pub fn close_ribbon_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.ribbon_context_menu.is_open {
            self.ribbon_context_menu.is_open = false;
            cx.notify();
        }
    }

    /// Renders the left 44px vertical Ribbon rail with draggable, customizable buttons.
    pub fn render_ribbon_rail(&self, theme: &Theme, cx: &Context<Self>) -> Stateful<gpui::Div> {
        let is_rail_visible = self.state.session.ribbon_visible;
        if !is_rail_visible {
            return div().id("ribbon-rail-hidden").w(px(0.0)).h_full().overflow_hidden();
        }

        let items = self.get_all_ribbon_items();
        let visible_items: Vec<RibbonItem> = items.into_iter().filter(|i| i.is_visible).collect();
        let dragged_id = self.ribbon_dragged_id.clone();
        let is_dragging = self.ribbon_is_dragging;
        let drop_id = self.ribbon_drop_id.clone();

        let from_index = dragged_id
            .as_ref()
            .and_then(|id| visible_items.iter().position(|item| &item.id == id));
        let to_index = drop_id
            .as_ref()
            .and_then(|id| visible_items.iter().position(|item| &item.id == id));
        // Gap between buttons where the icon will land. The commit rule
        // (`reorder_ribbon_item`) inserts after the hovered button when dragging down and
        // before it when dragging up, so the indicator follows the same rule.
        let drop_boundary = match (is_dragging, from_index, to_index) {
            (true, Some(from), Some(to)) if from != to => {
                Some(if from < to { to + 1 } else { to })
            }
            _ => None,
        };

        let mut rail = div()
            .id("ribbon-rail")
            .flex()
            .flex_col()
            .items_center()
            .relative()
            .w(px(44.0))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.bg_sidebar)
            .pt(px(RIBBON_PAD_TOP))
            .pb(px(RIBBON_PAD_TOP))
            .gap(px(RIBBON_ITEM_GAP))
            // Right-click on ribbon rail background opens the ribbon context menu
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                this.open_ribbon_context_menu(event.position.x.into(), event.position.y.into(), cx);
            }))
            // A drag is committed by the rail's own mouse-up below, so releasing in the
            // gap above/below an icon still drops where the insertion line is. This
            // handler only turns a plain click into the item's action.
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                let Some(from_id) = this.ribbon_dragged_id.take() else {
                    return;
                };
                let was_dragging = this.ribbon_is_dragging;
                let target = this.ribbon_drop_id.take();
                this.ribbon_drag_start_y = None;
                this.ribbon_is_dragging = false;

                if was_dragging {
                    if let Some(to_id) = target {
                        this.reorder_ribbon_item(&from_id, &to_id, cx);
                    }
                }
                cx.notify();
            }))
            // Released outside the rail entirely: cancel instead of leaving the ghost armed.
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.ribbon_dragged_id.take().is_some() {
                    this.ribbon_drop_id = None;
                    this.ribbon_drag_start_y = None;
                    this.ribbon_is_dragging = false;
                    cx.notify();
                }
            }))
            // Keeps the ghost glued to the pointer over gaps and empty rail space.
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.ribbon_dragged_id.is_some() {
                    this.ribbon_drag_pointer_x = event.position.x.into();
                    this.ribbon_drag_pointer_y = event.position.y.into();
                    if this.ribbon_is_dragging {
                        cx.notify();
                    }
                }
            }))
            // Leaving the rail cancels the indicated slot, so the insertion line never
            // points at a place the release cannot reach.
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !hovered && this.ribbon_drop_id.take().is_some() {
                    cx.notify();
                }
            }))
            // Render active draggable ribbon action buttons
            .children(visible_items.into_iter().map(|item| {
                let id = item.id.clone();
                let is_this_dragged = is_dragging && dragged_id.as_deref() == Some(&id);

                let id_down = id.clone();
                let id_move = id.clone();
                let id_up = id.clone();
                let element_id = SharedString::from(format!("ribbon-btn-{}", id));

                div()
                    .id(element_id)
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(RIBBON_ITEM_SIZE))
                    .h(px(RIBBON_ITEM_SIZE))
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_this_dragged, |d| {
                        d.border_1()
                            .border_color(theme.accent)
                            .bg(theme.bg_selected)
                    })
                    // No hover feedback while reordering: the insertion line is the
                    // only thing that should draw attention.
                    .when(!is_this_dragged && !is_dragging, |d| {
                        d.hover(|h| h.bg(theme.bg_hover))
                    })
                    .child(icons::icon(item.icon, 17.0, theme.text_secondary))
                    // Start drag on left mouse down
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        this.ribbon_dragged_id = Some(id_down.clone());
                        this.ribbon_drag_start_y = Some(event.position.y.into());
                        this.ribbon_drag_pointer_x = event.position.x.into();
                        this.ribbon_drag_pointer_y = event.position.y.into();
                        this.ribbon_drop_id = None;
                        this.ribbon_is_dragging = false;
                        cx.notify();
                    }))
                    // Open context menu on right mouse down
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        this.open_ribbon_context_menu(event.position.x.into(), event.position.y.into(), cx);
                    }))
                    // Track the pointer and highlight the slot the icon would move to.
                    // The list itself is left untouched until the button is released.
                    .on_mouse_move(cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                        this.ribbon_drag_pointer_x = event.position.x.into();
                        this.ribbon_drag_pointer_y = event.position.y.into();
                        let Some(from_id) = this.ribbon_dragged_id.clone() else {
                            return;
                        };
                        let cur_y: f32 = event.position.y.into();
                        let start_y = this.ribbon_drag_start_y.unwrap_or(cur_y);
                        if (cur_y - start_y).abs() > 4.0 {
                            this.ribbon_is_dragging = true;
                        }
                        if this.ribbon_is_dragging {
                            this.ribbon_drop_id =
                                if from_id == id_move { None } else { Some(id_move.clone()) };
                            cx.notify();
                        }
                    }))
                    // On mouse up: a plain click runs the action; a drag is committed by
                    // the rail so the drop also works outside the button's hitbox.
                    .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        if this.ribbon_is_dragging {
                            return;
                        }
                        let Some(from_id) = this.ribbon_dragged_id.take() else {
                            return;
                        };
                        this.ribbon_drop_id = None;
                        this.ribbon_drag_start_y = None;
                        if from_id == id_up {
                            this.execute_ribbon_action(&id_up, window, cx);
                        }
                        cx.notify();
                    }))
            }));

        // Insertion line in the gap between icons: the line sits where the icon will
        // land, and the buttons themselves are not restyled.
        if let Some(boundary) = drop_boundary {
            rail = rail.child(
                div()
                    .id("ribbon-drop-indicator")
                    .absolute()
                    .left(px((44.0 - RIBBON_ITEM_SIZE) / 2.0))
                    .top(px(RIBBON_PAD_TOP
                        + boundary as f32 * (RIBBON_ITEM_SIZE + RIBBON_ITEM_GAP)
                        - RIBBON_ITEM_GAP / 2.0))
                    .w(px(RIBBON_ITEM_SIZE))
                    .h(px(2.0))
                    .rounded_full()
                    .bg(theme.accent),
            );
        }

        rail
            // Spacer to push settings to bottom
            .child(div().flex_1())
            // Settings button at bottom of ribbon
            .child(
                div()
                    .id("rail-btn-settings")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(RIBBON_ITEM_SIZE))
                    .h(px(RIBBON_ITEM_SIZE))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .child(icons::icon(
                        icons::ICON_SETTINGS,
                        17.0,
                        theme.text_muted,
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_settings(None, cx);
                    })),
            )
    }

    /// Floating preview of the ribbon icon being reordered, drawn at the pointer.
    pub(crate) fn render_ribbon_drag_ghost(&self, theme: &Theme) -> Option<AnyElement> {
        if !self.ribbon_is_dragging {
            return None;
        }
        let dragged_id = self.ribbon_dragged_id.as_deref()?;
        let item = self
            .get_all_ribbon_items()
            .into_iter()
            .find(|item| item.id.as_str() == dragged_id)?;

        Some(
            div()
                .absolute()
                .left(px(self.ribbon_drag_pointer_x + 14.0))
                .top(px(self.ribbon_drag_pointer_y + 10.0))
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
                .child(icons::icon(item.icon, 15.0, theme.accent))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_primary)
                        .child(item.title),
                )
                .into_any_element(),
        )
    }

    /// Renders the floating context menu for ribbon buttons matching Obsidian style.
    pub fn render_ribbon_context_menu(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let items = self.get_all_ribbon_items();
        let menu_w = 260.0;
        let menu_item_h = 32.0;
        let total_items_h = (items.len() + 1) as f32 * menu_item_h + 24.0;

        let window_bounds = window.bounds();
        let win_h = f32::from(window_bounds.size.height);
        let win_w = f32::from(window_bounds.size.width);

        let menu_x = self.ribbon_context_menu.position_x.max(44.0).min(win_w - menu_w - 10.0);
        let menu_y = if self.ribbon_context_menu.position_y + total_items_h > win_h {
            (win_h - total_items_h - 10.0).max(10.0)
        } else {
            self.ribbon_context_menu.position_y.max(10.0)
        };

        let hide_panel_label = self.t("app.ribbon.hide_panel");

        div()
            .id("ribbon-context-backdrop")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_ribbon_context_menu(cx);
            }))
            .child(
                div()
                    .id("ribbon-context-panel")
                    .absolute()
                    .left(px(menu_x))
                    .top(px(menu_y))
                    .w(px(menu_w))
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_lg()
                    .shadow_xl()
                    .p_1p5()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .children(items.into_iter().map(|item| {
                        let id = item.id.clone();
                        let is_visible = item.is_visible;
                        let element_id = SharedString::from(format!("ribbon-menu-item-{}", id));

                        div()
                            .id(element_id)
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .px_2p5()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2p5()
                                    .child(icons::icon(item.icon, 15.0, theme.text_secondary))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.text_primary)
                                            .child(item.title),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(16.0))
                                    .h(px(16.0))
                                    .when(is_visible, |d| {
                                        d.child(icons::icon(icons::ICON_CHECK, 14.0, theme.text_primary))
                                    }),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_ribbon_item_visibility(&id, cx);
                            }))
                    }))
                    // Divider line
                    .child(
                        div()
                            .h(px(1.0))
                            .bg(theme.border_subtle)
                            .my_1(),
                    )
                    // Bottom item: "Hide vertical panel"
                    .child(
                        div()
                            .id("ribbon-menu-toggle-rail")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2p5()
                            .px_2p5()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_PANEL_LEFT, 15.0, theme.text_secondary))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.text_primary)
                                    .child(hide_panel_label),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_ribbon_rail_visibility(cx);
                                this.close_ribbon_context_menu(cx);
                            })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    use crate::state::SessionState;

    #[test]
    fn test_session_state_ribbon_defaults_and_roundtrip() {
        let default_session = SessionState::default();
        assert!(default_session.ribbon_visible);
        assert!(!default_session.ribbon_hidden.is_empty());
        assert!(default_session.ribbon_hidden.contains(&"ribbon-quick-switcher".to_string()));
        assert!(default_session.ribbon_hidden.contains(&"ribbon-graph".to_string()));
        assert!(!default_session.ribbon_hidden.contains(&"ribbon-new-note".to_string()));

        let json = serde_json::to_string(&default_session).unwrap();
        let restored: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.ribbon_visible, default_session.ribbon_visible);
        assert_eq!(restored.ribbon_hidden, default_session.ribbon_hidden);
    }

    #[test]
    fn test_ribbon_reorder_logic() {
        let mut order = vec![
            "ribbon-new-note".to_string(),
            "ribbon-calendar".to_string(),
            "ribbon-commands".to_string(),
            "custom-plugin".to_string(),
        ];

        // Move "custom-plugin" to top (index 0)
        let from_pos = order.iter().position(|id| id == "custom-plugin").unwrap();
        let item = order.remove(from_pos);
        order.insert(0, item);

        assert_eq!(order, vec![
            "custom-plugin".to_string(),
            "ribbon-new-note".to_string(),
            "ribbon-calendar".to_string(),
            "ribbon-commands".to_string(),
        ]);

        // Move "ribbon-new-note" down to index 2
        let from_pos = order.iter().position(|id| id == "ribbon-new-note").unwrap();
        let item = order.remove(from_pos);
        order.insert(2, item);

        assert_eq!(order, vec![
            "custom-plugin".to_string(),
            "ribbon-calendar".to_string(),
            "ribbon-new-note".to_string(),
            "ribbon-commands".to_string(),
        ]);
    }
}
