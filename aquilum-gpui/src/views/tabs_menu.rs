use gpui::{
    div, prelude::*, px, Context, IntoElement, MouseButton,
    SharedString, Styled,
};
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;

impl AppView {
    /// Toggles the open tabs dropdown menu.
    pub fn toggle_tabs_menu(&mut self, cx: &mut Context<Self>) {
        self.is_tabs_menu_open = !self.is_tabs_menu_open;
        cx.notify();
    }

    /// Closes all open tabs and resets active note state.
    pub fn close_all_tabs(&mut self, cx: &mut Context<Self>) {
        if let Some(ref note) = self.state.active_note {
            if note.is_dirty {
                self.state.save_active_note();
            }
        }
        self.commit_title_rename_internal();
        self.state.open_tabs.clear();
        self.state.active_note = None;
        self.state.session.open_tabs.clear();
        self.state.session.active_note = None;
        self.state.save_session_state();
        self.title_editing_path = None;
        self.title_edit_text.clear();
        self.title_cursor_pos = 0;
        self.title_selection_anchor = None;
        self.title_is_mouse_selecting = false;
        cx.notify();
    }

    /// Renders the chevron-down button for opening the tabs dropdown menu.
    pub fn render_tabs_dropdown_button(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let is_open = self.is_tabs_menu_open;
        div()
            .id("btn-tabs-dropdown")
            .flex()
            .items_center()
            .justify_center()
            .w(px(32.0))
            .h(px(32.0))
            .rounded_md()
            .cursor_pointer()
            .when(is_open, |d| d.bg(theme.bg_hover))
            .hover(|h| h.bg(theme.bg_hover))
            .child(icons::icon(
                icons::ICON_CHEVRON_DOWN,
                14.0,
                if is_open { theme.text_primary } else { theme.text_secondary },
            ))
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_tabs_menu(cx);
            }))
    }

    /// Renders the right sidebar toggle button.
    pub fn render_right_sidebar_toggle_button(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let is_open = !self.right_sidebar_collapsed;
        div()
            .id("btn-toggle-right-sidebar")
            .flex()
            .items_center()
            .justify_center()
            .w(px(32.0))
            .h(px(32.0))
            .rounded_md()
            .cursor_pointer()
            .hover(|h| h.bg(theme.bg_hover))
            .child(icons::icon(
                icons::ICON_PANEL_RIGHT,
                15.0,
                if is_open { theme.accent } else { theme.text_secondary },
            ))
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_right_sidebar(cx);
            }))
    }

    /// Renders the open tabs popup dropdown menu (Obsidian-style).
    pub fn render_tabs_dropdown_menu(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let count = self.state.open_tabs.len();
        let bookmark_label = format_bookmark_label(count);

        div()
            .id("tabs-dropdown-overlay")
            .absolute()
            .inset_0()
            // Invisible backdrop to close menu on outside click
            .child(
                div()
                    .id("tabs-menu-backdrop")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.is_tabs_menu_open = false;
                        cx.notify();
                    })),
            )
            // Popup menu container
            .child(
                div()
                    .id("tabs-dropdown-panel")
                    .absolute()
                    .top(px(38.0))
                    .right(px(140.0))
                    .w(px(260.0))
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .shadow_lg()
                    .py_1p5()
                    .flex()
                    .flex_col()
                    // 1. "Сложить вкладки"
                    .child(
                        div()
                            .id("menu-item-stack-tabs")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1p5()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_LAYERS, 14.0, theme.text_secondary))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_primary)
                                    .child("Сложить вкладки"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.is_tabs_menu_open = false;
                                this.state.status_text = "Режим сложенных вкладок".to_string();
                                cx.notify();
                            })),
                    )
                    // 2. "Добавить в закладки N вкладок..."
                    .child(
                        div()
                            .id("menu-item-bookmark-tabs")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1p5()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_BOOKMARK, 14.0, theme.text_secondary))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_primary)
                                    .child(bookmark_label),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.is_tabs_menu_open = false;
                                this.state.status_text = "Вкладки добавлены в закладки".to_string();
                                cx.notify();
                            })),
                    )
                    // Divider 1
                    .child(
                        div()
                            .h_px()
                            .bg(theme.border_subtle)
                            .my_1()
                            .mx_2(),
                    )
                    // 3. "Закрыть все вкладки"
                    .child(
                        div()
                            .id("menu-item-close-all-tabs")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1p5()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_CLOSE, 13.0, theme.text_secondary))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_primary)
                                    .child("Закрыть все вкладки"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_all_tabs(cx);
                                this.is_tabs_menu_open = false;
                            })),
                    )
                    // Divider 2
                    .child(
                        div()
                            .h_px()
                            .bg(theme.border_subtle)
                            .my_1()
                            .mx_2(),
                    )
                    // 4. Open tabs list
                    .child({
                        let mut list = div()
                            .id("tabs-menu-list-scroll")
                            .flex()
                            .flex_col()
                            .max_h(px(280.0))
                            .overflow_scroll();

                        if self.state.open_tabs.is_empty() {
                            list = list.child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .text_xs()
                                    .text_color(theme.text_muted)
                                    .child("Нет открытых вкладок"),
                            );
                        } else {
                            for (idx, tab_path) in self.state.open_tabs.iter().enumerate() {
                                let is_active = self.state.active_note.as_ref().map(|n| &n.path == tab_path).unwrap_or(false);
                                let tab_path_clone = tab_path.clone();
                                let display_name = tab_path
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Без названия".to_string());

                                list = list.child(
                                    div()
                                        .id(SharedString::from(format!("tab-item-{}", idx)))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .px_3()
                                        .py_1p5()
                                        .cursor_pointer()
                                        .when(is_active, |d| d.bg(theme.bg_hover))
                                        .hover(|h| h.bg(theme.bg_hover))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_2()
                                                .min_w_0()
                                                .flex_1()
                                                .child(icons::icon(
                                                    icons::ICON_FILE_TEXT,
                                                    14.0,
                                                    if is_active { theme.accent } else { theme.text_secondary },
                                                ))
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .truncate()
                                                        .text_color(theme.text_primary)
                                                        .child(display_name),
                                                ),
                                        )
                                        .when(is_active, |d| {
                                            d.child(icons::icon(icons::ICON_CHECK, 14.0, theme.text_secondary))
                                        })
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.select_note(tab_path_clone.clone(), true, window, cx);
                                            this.is_tabs_menu_open = false;
                                            cx.notify();
                                        })),
                                );
                            }
                        }
                        list
                    }),
            )
    }
}

/// Formats the bookmark label with proper Russian pluralization.
pub fn format_bookmark_label(count: usize) -> String {
    let suffix = match count % 10 {
        1 if count % 100 != 11 => "вкладку...",
        2..=4 if !(12..=14).contains(&(count % 100)) => "вкладки...",
        _ => "вкладок...",
    };
    if count > 0 {
        format!("Добавить в закладки {} {}", count, suffix)
    } else {
        "Добавить в закладки вкладку...".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bookmark_label() {
        assert_eq!(format_bookmark_label(0), "Добавить в закладки вкладку...");
        assert_eq!(format_bookmark_label(1), "Добавить в закладки 1 вкладку...");
        assert_eq!(format_bookmark_label(2), "Добавить в закладки 2 вкладки...");
        assert_eq!(format_bookmark_label(4), "Добавить в закладки 4 вкладки...");
        assert_eq!(format_bookmark_label(5), "Добавить в закладки 5 вкладок...");
        assert_eq!(format_bookmark_label(11), "Добавить в закладки 11 вкладок...");
        assert_eq!(format_bookmark_label(12), "Добавить в закладки 12 вкладок...");
        assert_eq!(format_bookmark_label(21), "Добавить в закладки 21 вкладку...");
        assert_eq!(format_bookmark_label(22), "Добавить в закладки 22 вкладки...");
    }
}
