use gpui::{
    div, prelude::*, px, Context, IntoElement, MouseButton, SharedString, Stateful, Styled, Window,
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
        let is_ru = self.config.ui.language.starts_with("ru");
        match id {
            "ribbon-quick-switcher" => if is_ru { "Меню быстрого перехода" } else { "Quick switcher" },
            "ribbon-graph" => if is_ru { "Граф" } else { "Graph view" },
            "ribbon-canvas" => if is_ru { "Создать новый холст" } else { "New canvas" },
            "ribbon-new-note" => if is_ru { "Новая заметка" } else { "New note" },
            "ribbon-calendar" => if is_ru { "Сегодняшняя заметка" } else { "Daily note" },
            "ribbon-template" => if is_ru { "Вставить шаблон" } else { "Insert template" },
            "ribbon-commands" => if is_ru { "Открыть палитру команд" } else { "Open command palette" },
            "ribbon-drawing" => if is_ru { "Создать новый чертеж" } else { "New drawing" },
            "ribbon-refresh" => if is_ru { "Синхронизировать / Обновить" } else { "Sync / Reload" },
            "ribbon-trash" => if is_ru { "Корзина" } else { "Trash" },
            _ => id,
        }.to_string()
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
                let is_ru = self.config.ui.language.starts_with("ru");
                self.state.status_text = if is_ru {
                    "Граф заметок".to_string()
                } else {
                    "Graph view".to_string()
                };
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

        div()
            .id("ribbon-rail")
            .flex()
            .flex_col()
            .items_center()
            .w(px(44.0))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.bg_sidebar)
            .pt_2()
            .pb_2()
            .gap_2()
            // Right-click on ribbon rail background opens the ribbon context menu
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                this.open_ribbon_context_menu(event.position.x.into(), event.position.y.into(), cx);
            }))
            // Global mouse-up resets dragging state safely
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.ribbon_is_dragging {
                    this.state.session.save(&crate::state::app_data_dir());
                }
                this.ribbon_dragged_id = None;
                this.ribbon_drag_start_y = None;
                this.ribbon_is_dragging = false;
                cx.notify();
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
                    .w(px(32.0))
                    .h(px(32.0))
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_this_dragged, |d| {
                        d.border_1()
                            .border_color(theme.accent)
                            .bg(theme.bg_selected)
                    })
                    .when(!is_this_dragged, |d| {
                        d.hover(|h| h.bg(theme.bg_hover))
                    })
                    .child(icons::icon(item.icon, 17.0, theme.text_secondary))
                    // Start drag on left mouse down
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        this.ribbon_dragged_id = Some(id_down.clone());
                        this.ribbon_drag_start_y = Some(event.position.y.into());
                        this.ribbon_is_dragging = false;
                        cx.notify();
                    }))
                    // Open context menu on right mouse down
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        this.open_ribbon_context_menu(event.position.x.into(), event.position.y.into(), cx);
                    }))
                    // Reorder items when dragging over another ribbon item
                    .on_mouse_move(cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                        if let Some(from_id) = this.ribbon_dragged_id.clone() {
                            let cur_y: f32 = event.position.y.into();
                            let start_y = this.ribbon_drag_start_y.unwrap_or(cur_y);
                            if (cur_y - start_y).abs() > 4.0 {
                                this.ribbon_is_dragging = true;
                            }
                            if this.ribbon_is_dragging && from_id != id_move {
                                this.reorder_ribbon_item(&from_id, &id_move, cx);
                            }
                        }
                    }))
                    // On mouse up: execute action if not dragged, or finish drag
                    .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        let was_dragging = this.ribbon_is_dragging;
                        this.ribbon_dragged_id = None;
                        this.ribbon_drag_start_y = None;
                        this.ribbon_is_dragging = false;

                        if !was_dragging {
                            this.execute_ribbon_action(&id_up, window, cx);
                        } else {
                            this.state.session.save(&crate::state::app_data_dir());
                        }
                        cx.notify();
                    }))
            }))
            // Spacer to push settings to bottom
            .child(div().flex_1())
            // Settings button at bottom of ribbon
            .child(
                div()
                    .id("rail-btn-settings")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(32.0))
                    .h(px(32.0))
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

        let is_ru = self.config.ui.language.starts_with("ru");
        let hide_panel_label = if is_ru {
            "Скрыть вертикальную панель"
        } else {
            "Hide vertical panel"
        };

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
                    // Bottom item: "Скрыть вертикальную панель"
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
