use std::path::{Path, PathBuf};
use gpui::{
    div, prelude::*, px, ClipboardItem, Context, IntoElement, MouseButton,
    Styled, Window,
};
use crate::blink_manager::BlinkTarget;
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::sidebar::file_actions;
use crate::views::sidebar::folder_actions;

/// Active submenu identifier inside the more options context menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveSubmenu {
    None,
    CopyPath,
    LinkedView,
}

/// State for the Obsidian-style note/pane "More options" context menu (3 vertical dots).
#[derive(Clone, Debug)]
pub struct MoreOptionsMenuState {
    pub is_open: bool,
    pub target_path: Option<PathBuf>,
    pub leaf_id: Option<crate::workspace::LeafId>,
    pub position_x: f32,
    pub position_y: f32,
    pub hovered_item: Option<&'static str>,
    pub active_submenu: ActiveSubmenu,
}

impl Default for MoreOptionsMenuState {
    fn default() -> Self {
        Self {
            is_open: false,
            target_path: None,
            leaf_id: None,
            position_x: 0.0,
            position_y: 0.0,
            hovered_item: None,
            active_submenu: ActiveSubmenu::None,
        }
    }
}

impl AppView {
    /// Opens the more options context menu at coordinates `(x, y)` for the current note.
    pub fn open_more_options_menu(
        &mut self,
        target_path: Option<PathBuf>,
        leaf_id: Option<crate::workspace::LeafId>,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.file_context_menu.is_open = false;
        self.folder_context_menu.is_open = false;
        self.tab_context_menu.is_open = false;
        self.ribbon_context_menu.is_open = false;
        self.close_tabs_popup(cx);

        self.more_options_menu = MoreOptionsMenuState {
            is_open: true,
            target_path,
            leaf_id,
            position_x: x,
            position_y: y,
            hovered_item: None,
            active_submenu: ActiveSubmenu::None,
        };
        cx.notify();
    }

    /// Closes the more options context menu.
    pub fn close_more_options_menu(&mut self, cx: &mut Context<Self>) {
        if self.more_options_menu.is_open {
            self.more_options_menu.is_open = false;
            self.more_options_menu.hovered_item = None;
            self.more_options_menu.active_submenu = ActiveSubmenu::None;
            cx.notify();
        }
    }

    /// Action helper to add YAML frontmatter property to the active note.
    pub fn add_file_property_action(&mut self, cx: &mut Context<Self>) {
        let has_doc = self.state.active_document().is_some();
        if !has_doc {
            return;
        }

        let content = self
            .state
            .active_document()
            .map(|d| d.content().to_string())
            .unwrap_or_default();

        if content.starts_with("---\n") || content.starts_with("---\r\n") {
            let start_search = if content.starts_with("---\r\n") { 5 } else { 4 };
            if let Some(pos) = content[start_search..].find("\n---") {
                let insert_pos = start_search + pos + 1;
                self.state
                    .active_replace_range(insert_pos..insert_pos, "property: \n".to_string(), insert_pos + 10);
            } else {
                self.state
                    .active_replace_range(start_search..start_search, "property: \n".to_string(), start_search + 10);
            }
        } else {
            let frontmatter = "---\nproperty: \n---\n\n";
            self.state
                .active_replace_range(0..0, frontmatter.to_string(), 14);
        }

        self.state.status_text = self.t("app.more_options.property_added");
        cx.notify();
    }

    /// Renders one menu item in the more options menu.
    fn render_more_options_item(
        &self,
        id: &'static str,
        icon_path: &'static str,
        label: String,
        is_danger: bool,
        has_submenu: bool,
        submenu_target: ActiveSubmenu,
        on_click: Box<dyn Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static>,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_selected = self.more_options_menu.hovered_item == Some(id)
            || (has_submenu && self.more_options_menu.active_submenu == submenu_target);
        let text_color = if is_danger {
            gpui::rgb(0xe05252)
        } else {
            theme.text_primary
        };
        let icon_color = if is_danger {
            gpui::rgb(0xe05252)
        } else {
            theme.text_secondary
        };

        div()
            .id(id)
            .h(px(28.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_2p5()
            .rounded_md()
            .cursor_pointer()
            .when(is_selected, |d| d.bg(theme.bg_hover))
            .hover(|h| h.bg(theme.bg_hover))
            .on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                if *is_hovered {
                    this.more_options_menu.hovered_item = Some(id);
                    this.more_options_menu.active_submenu = submenu_target;
                    cx.notify();
                } else if this.more_options_menu.hovered_item == Some(id) {
                    this.more_options_menu.hovered_item = None;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .child(icons::icon(icon_path, 14.0, icon_color))
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_color)
                            .child(label),
                    ),
            )
            .when(has_submenu, |d| {
                d.child(icons::icon(icons::ICON_CHEVRON_RIGHT, 12.0, theme.text_muted))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                on_click(this, window, cx);
            }))
    }

    fn render_more_options_check_item(
        &self,
        id: &'static str,
        icon_path: &'static str,
        label: String,
        checked: bool,
        on_click: Box<dyn Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static>,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_selected = self.more_options_menu.hovered_item == Some(id);
        let text_color = theme.text_primary;
        let icon_color = theme.text_secondary;

        div()
            .id(id)
            .h(px(28.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_2p5()
            .rounded_md()
            .cursor_pointer()
            .when(is_selected, |d| d.bg(theme.bg_hover))
            .hover(|h| h.bg(theme.bg_hover))
            .on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                if *is_hovered {
                    this.more_options_menu.hovered_item = Some(id);
                    this.more_options_menu.active_submenu = ActiveSubmenu::None;
                    cx.notify();
                } else if this.more_options_menu.hovered_item == Some(id) {
                    this.more_options_menu.hovered_item = None;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .child(icons::icon(icon_path, 14.0, icon_color))
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_color)
                            .child(label),
                    ),
            )
            .when(checked, |d| {
                d.child(icons::icon(icons::ICON_CHECK, 12.0, theme.accent))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                on_click(this, window, cx);
            }))
    }

    /// Renders the complete Obsidian-like more options context menu overlay.
    pub fn render_more_options_menu(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let window_size = window.bounds().size;
        let window_h: f32 = window_size.height.into();
        let window_w: f32 = window_size.width.into();

        let menu_w = 270.0;
        let submenu_w = 240.0;
        let submenu_gap = 4.0;

        let click_x = self.more_options_menu.position_x;
        let click_y = self.more_options_menu.position_y;

        // Position menu directly below the 3-dots button, right-aligned with button
        let mut menu_x = (click_x + 14.0 - menu_w).max(8.0);
        if menu_x + menu_w > window_w - 12.0 {
            menu_x = (window_w - menu_w - 12.0).max(8.0);
        }

        let menu_y = (click_y + 14.0).max(8.0);

        // Submenu coordinates: open to the left if near right screen edge
        let submenu_on_left = menu_x + menu_w + submenu_w + submenu_gap + 12.0 > window_w;
        let submenu_x = if submenu_on_left {
            (menu_x - submenu_w - submenu_gap).max(8.0)
        } else {
            menu_x + menu_w + submenu_gap
        };

        let target_path = self.more_options_menu.target_path.clone();

        div()
            .id("more-options-menu-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_move(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            // Backdrop
            .child(
                div()
                    .id("more-options-backdrop")
                    .absolute()
                    .inset_0()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.close_more_options_menu(cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                        this.close_more_options_menu(cx);
                    }))
                    .on_mouse_move(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    }),
            )
            // Context menu popup panel
            .child(
                div()
                    .id("more-options-panel")
                    .absolute()
                    .left(px(menu_x))
                    .top(px(menu_y))
                    .w(px(menu_w))
                    .max_h(px((window_h - menu_y - 12.0).max(180.0)))
                    .overflow_y_scroll()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_lg()
                    .shadow_xl()
                    .p_1()
                    .flex()
                    .flex_col()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    // --- GROUP 1: VIEWS & BACKLINKS ---
                    // 1. Show backlinks
                    .child(self.render_more_options_item(
                        "more-item-show-backlinks",
                        icons::ICON_LINK,
                        self.t("app.more_options.show_backlinks"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        Box::new(|this, _, cx| {
                            this.right_sidebar_collapsed = false;
                            this.state.status_text = this.t("app.more_options.showing_backlinks");
                            this.close_more_options_menu(cx);
                            cx.notify();
                        }),
                        theme,
                        cx,
                    ))
                    // 2. Reading view / Live preview
                    .child(if self.edit_mode {
                        self.render_more_options_item(
                            "more-item-reading-view",
                            icons::ICON_BOOK_OPEN,
                            self.t("app.more_options.reading_view"),
                            false,
                            false,
                            ActiveSubmenu::None,
                            Box::new(|this, _, cx| {
                                this.edit_mode = false;
                                if let Some(view) = this.state.active_view_mut() {
                                    view.visual_cache_dirty = true;
                                }
                                this.row_shape_cache.borrow_mut().clear();
                                let current_scroll = this.current_scroll_top();
                                this.state.snapshot_active_note_settings(current_scroll, this.edit_mode, this.word_wrap, this.source_mode);
                                this.close_more_options_menu(cx);
                                cx.notify();
                            }),
                            theme,
                            cx,
                        )
                    } else {
                        self.render_more_options_item(
                            "more-item-live-preview",
                            icons::ICON_EYE,
                            self.t("app.more_options.live_preview"),
                            false,
                            false,
                            ActiveSubmenu::None,
                            Box::new(|this, _, cx| {
                                this.edit_mode = true;
                                this.source_mode = false;
                                if let Some(view) = this.state.active_view_mut() {
                                    view.source_mode = false;
                                    view.visual_cache_dirty = true;
                                }
                                this.row_shape_cache.borrow_mut().clear();
                                this.table_layouts.borrow_mut().clear();
                                let current_scroll = this.current_scroll_top();
                                this.state.snapshot_active_note_settings(current_scroll, this.edit_mode, this.word_wrap, this.source_mode);
                                this.reset_editor_cursor_blink(cx);
                                this.close_more_options_menu(cx);
                                cx.notify();
                            }),
                            theme,
                            cx,
                        )
                    })
                    // 3. Source view / Live preview toggle
                    .child(if self.edit_mode && self.source_mode {
                        self.render_more_options_item(
                            "more-item-live-preview",
                            icons::ICON_EYE,
                            self.t("app.more_options.live_preview"),
                            false,
                            false,
                            ActiveSubmenu::None,
                            Box::new(|this, _, cx| {
                                this.edit_mode = true;
                                this.source_mode = false;
                                if let Some(view) = this.state.active_view_mut() {
                                    view.source_mode = false;
                                    view.visual_cache_dirty = true;
                                }
                                this.row_shape_cache.borrow_mut().clear();
                                this.table_layouts.borrow_mut().clear();
                                let current_scroll = this.current_scroll_top();
                                this.state.snapshot_active_note_settings(current_scroll, this.edit_mode, this.word_wrap, this.source_mode);
                                this.reset_editor_cursor_blink(cx);
                                this.close_more_options_menu(cx);
                                cx.notify();
                            }),
                            theme,
                            cx,
                        )
                    } else {
                        self.render_more_options_item(
                            "more-item-source-view",
                            icons::ICON_CODE,
                            self.t("app.more_options.source_view"),
                            false,
                            false,
                            ActiveSubmenu::None,
                            Box::new(|this, _, cx| {
                                this.edit_mode = true;
                                this.source_mode = true;
                                if let Some(view) = this.state.active_view_mut() {
                                    view.source_mode = true;
                                    view.visual_cache_dirty = true;
                                }
                                this.row_shape_cache.borrow_mut().clear();
                                this.table_layouts.borrow_mut().clear();
                                let current_scroll = this.current_scroll_top();
                                this.state.snapshot_active_note_settings(current_scroll, this.edit_mode, this.word_wrap, this.source_mode);
                                this.reset_editor_cursor_blink(cx);
                                this.close_more_options_menu(cx);
                                cx.notify();
                            }),
                            theme,
                            cx,
                        )
                    })
                    // 4. Show invisibles
                    .child(self.render_more_options_check_item(
                        "more-item-toggle-invisibles",
                        icons::ICON_TYPE,
                        self.t("app.more_options.show_invisibles"),
                        self.show_invisibles,
                        Box::new(|this, _, cx| {
                            this.toggle_show_invisibles(cx);
                            this.close_more_options_menu(cx);
                        }),
                        theme,
                        cx,
                    ))
                    // Divider 1
                    .child(self.render_menu_divider(theme))
                    // --- GROUP 2: SPLITS & WINDOWS ---
                    // 4. Split right
                    .child(self.render_more_options_item(
                        "more-item-split-right",
                        icons::ICON_SPLIT,
                        self.t("app.more_options.split_right"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, window, cx| {
                                if let Some(ref path) = p {
                                    this.split_tab_right(path, window, cx);
                                }
                                this.close_more_options_menu(cx);
                            })
                        },
                        theme,
                        cx,
                    ))
                    // 5. Split down
                    .child(self.render_more_options_item(
                        "more-item-split-down",
                        icons::ICON_SPLIT_DOWN,
                        self.t("app.more_options.split_down"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, window, cx| {
                                if let Some(ref path) = p {
                                    this.split_tab_down(path, window, cx);
                                }
                                this.close_more_options_menu(cx);
                            })
                        },
                        theme,
                        cx,
                    ))
                    // 6. Open in new window
                    .child(self.render_more_options_item(
                        "more-item-new-window",
                        icons::ICON_APP_WINDOW,
                        self.t("app.more_options.open_new_window"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        Box::new(|this, _, cx| {
                            this.state.status_text = this.t("tabs.new_window_status");
                            this.close_more_options_menu(cx);
                            cx.notify();
                        }),
                        theme,
                        cx,
                    ))
                    // Divider 2
                    .child(self.render_menu_divider(theme))
                    // --- GROUP 3: FILE MANAGEMENT ---
                    // 7. Rename
                    .child(self.render_more_options_item(
                        "more-item-rename",
                        icons::ICON_PENCIL,
                        self.t("app.more_options.rename"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, window, cx| {
                                this.close_more_options_menu(cx);
                                if let Some(ref path) = p {
                                    this.start_sidebar_tree_rename(path.clone(), false, window, cx);
                                }
                            })
                        },
                        theme,
                        cx,
                    ))
                    // 8. Move file to...
                    .child(self.render_more_options_item(
                        "more-item-move",
                        icons::ICON_FOLDER_INPUT,
                        self.t("app.more_options.move_file_to"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, _, cx| {
                                this.close_more_options_menu(cx);
                                if let Some(ref path) = p {
                                    this.open_move_folder_modal(path.clone(), cx);
                                }
                            })
                        },
                        theme,
                        cx,
                    ))
                    // 9. Add / remove from bookmarks...
                    .child({
                        let is_bm = target_path
                            .as_ref()
                            .map(|p| self.is_file_bookmarked(p))
                            .unwrap_or(false);
                        let label_str = if is_bm {
                            self.t("app.more_options.remove_from_bookmarks")
                        } else {
                            self.t("app.more_options.add_to_bookmarks")
                        };
                        let p = target_path.clone();
                        self.render_more_options_item(
                            "more-item-bookmark",
                            if is_bm { icons::ICON_BOOKMARK_FILLED } else { icons::ICON_BOOKMARK },
                            label_str,
                            false,
                            false,
                            ActiveSubmenu::None,
                            Box::new(move |this, _, cx| {
                                if let Some(ref path) = p {
                                    this.toggle_file_bookmark(path, cx);
                                }
                                this.close_more_options_menu(cx);
                            }),
                            theme,
                            cx,
                        )
                    })
                    // 10. Merge entire file with...
                    .child(self.render_more_options_item(
                        "more-item-merge",
                        icons::ICON_MERGE,
                        self.t("app.more_options.merge_entire_file"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, _, cx| {
                                if let Some(ref path) = p {
                                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                                    this.state.status_text = this.t_fmt("sidebar.merge_with", &[&name]);
                                }
                                this.close_more_options_menu(cx);
                                cx.notify();
                            })
                        },
                        theme,
                        cx,
                    ))
                    // 11. Add file property
                    .child(self.render_more_options_item(
                        "more-item-add-property",
                        icons::ICON_CIRCLE_PLUS,
                        self.t("app.more_options.add_file_property"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        Box::new(|this, _, cx| {
                            this.add_file_property_action(cx);
                            this.close_more_options_menu(cx);
                        }),
                        theme,
                        cx,
                    ))
                    // 12. Export to PDF
                    .child(self.render_more_options_item(
                        "more-item-export-pdf",
                        icons::ICON_DOWNLOAD,
                        self.t("app.more_options.export_pdf"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, _, cx| {
                                this.close_more_options_menu(cx);
                                this.export_note_to_pdf(p.clone(), cx);
                            })
                        },
                        theme,
                        cx,
                    ))
                    // 12b. Export MD to zip
                    .child(self.render_more_options_item(
                        "more-item-export-md",
                        icons::ICON_DOWNLOAD,
                        self.t("app.more_options.export_md_zip"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, _, cx| {
                                this.close_more_options_menu(cx);
                                this.open_export_dialog(p.clone(), cx);
                            })
                        },
                        theme,
                        cx,
                    ))
                    // Divider 3
                    .child(self.render_menu_divider(theme))
                    // --- GROUP 4: SEARCH & REPLACE ---
                    // 13. Find...
                    .child(self.render_more_options_item(
                        "more-item-find",
                        icons::ICON_SEARCH,
                        self.t("app.more_options.find"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        Box::new(|this, window, cx| {
                            this.close_more_options_menu(cx);
                            this.find_bar.replace_open = false;
                            this.open_find_bar(window, cx);
                        }),
                        theme,
                        cx,
                    ))
                    // 14. Replace...
                    .child(self.render_more_options_item(
                        "more-item-replace",
                        icons::ICON_REFRESH,
                        self.t("app.more_options.replace"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        Box::new(|this, window, cx| {
                            this.close_more_options_menu(cx);
                            this.open_find_bar(window, cx);
                            this.find_bar.replace_open = true;
                            this.find_bar.replace.focus(window);
                            this.find_bar.replace.blink.pause_and_show(BlinkTarget::FindReplace, cx);
                            cx.notify();
                        }),
                        theme,
                        cx,
                    ))
                    // Divider 4
                    .child(self.render_menu_divider(theme))
                    // --- GROUP 5: PATHS & LINKED VIEWS (SUBMENUS) ---
                    // 15. Copy path >
                    .child(self.render_more_options_item(
                        "more-item-copy-path",
                        icons::ICON_COPY,
                        self.t("app.more_options.copy_path"),
                        false,
                        true,
                        ActiveSubmenu::CopyPath,
                        Box::new(|this, _, cx| {
                            this.more_options_menu.active_submenu = ActiveSubmenu::CopyPath;
                            cx.notify();
                        }),
                        theme,
                        cx,
                    ))
                    // 16. Open linked view >
                    .child(self.render_more_options_item(
                        "more-item-linked-view",
                        icons::ICON_LINK,
                        self.t("app.more_options.open_linked_view"),
                        false,
                        true,
                        ActiveSubmenu::LinkedView,
                        Box::new(|this, _, cx| {
                            this.more_options_menu.active_submenu = ActiveSubmenu::LinkedView;
                            cx.notify();
                        }),
                        theme,
                        cx,
                    ))
                    // Divider 5
                    .child(self.render_menu_divider(theme))
                    // --- GROUP 6: EXTERNAL & EXPLORER ---
                    // 17. Open in default app
                    .child(self.render_more_options_item(
                        "more-item-open-default",
                        icons::ICON_EXTERNAL_LINK,
                        self.t("app.more_options.open_default_app"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, _, cx| {
                                if let Some(ref path) = p {
                                    file_actions::open_in_default_app(path);
                                }
                                this.close_more_options_menu(cx);
                            })
                        },
                        theme,
                        cx,
                    ))
                    // 18. Show in Explorer
                    .child(self.render_more_options_item(
                        "more-item-reveal-explorer",
                        icons::ICON_EXTERNAL_LINK,
                        self.t("app.more_options.show_explorer"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, _, cx| {
                                if let Some(ref path) = p {
                                    folder_actions::show_in_explorer(path);
                                }
                                this.close_more_options_menu(cx);
                            })
                        },
                        theme,
                        cx,
                    ))
                    // 19. Show in file manager
                    .child(self.render_more_options_item(
                        "more-item-reveal-file-manager",
                        icons::ICON_FOLDER,
                        self.t("app.more_options.show_file_manager"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, _, cx| {
                                if let Some(ref path) = p {
                                    this.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
                                    this.sidebar_collapsed = false;
                                    this.selected_sidebar_folder = None;
                                    this.state.reveal_path_in_tree(path);
                                    this.need_scroll_sidebar_to_active.set(true);
                                }
                                this.close_more_options_menu(cx);
                                cx.notify();
                            })
                        },
                        theme,
                        cx,
                    ))
                    // Divider 6
                    .child(self.render_menu_divider(theme))
                    // --- GROUP 7: PLUGINS / NOTE TOOLS ---
                    // 20. Change icon
                    .child(self.render_more_options_item(
                        "more-item-change-icon",
                        icons::ICON_HASH,
                        self.t("app.more_options.change_icon"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        Box::new(|this, _, cx| {
                            this.state.status_text = this.t("sidebar.change_note_icon");
                            this.close_more_options_menu(cx);
                            cx.notify();
                        }),
                        theme,
                        cx,
                    ))
                    // 21. Encrypt note
                    .child(self.render_more_options_item(
                        "more-item-encrypt-note",
                        icons::ICON_LOCK,
                        self.t("app.more_options.encrypt_note"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        Box::new(|this, _, cx| {
                            this.state.status_text = this.t("app.more_options.note_encryption_saved");
                            this.close_more_options_menu(cx);
                            cx.notify();
                        }),
                        theme,
                        cx,
                    ))
                    // 22. Check file stats
                    .child(self.render_more_options_item(
                        "more-item-check-stats",
                        icons::ICON_ANALYSIS,
                        self.t("app.more_options.check_file_stats"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, _, cx| {
                                let (lines, words, chars) = if let Some(doc) = this.state.active_document() {
                                    let text = doc.content();
                                    (doc.line_index.lines(), text.split_whitespace().count(), text.chars().count())
                                } else {
                                    (0, 0, 0)
                                };
                                let size = p
                                    .as_ref()
                                    .and_then(|path| std::fs::metadata(path).ok())
                                    .map(|m| m.len())
                                    .unwrap_or(chars as u64);
                                this.state.status_text = this.t_fmt("app.more_options.file_stats", &[&lines.to_string(), &words.to_string(), &chars.to_string(), &size.to_string()]);
                                this.close_more_options_menu(cx);
                                cx.notify();
                            })
                        },
                        theme,
                        cx,
                    ))
                    // Divider 7
                    .child(self.render_menu_divider(theme))
                    // --- GROUP 8: DELETE (RED DANGER) ---
                    // 23. Delete file
                    .child(self.render_more_options_item(
                        "more-item-delete-file",
                        icons::ICON_TRASH_2,
                        self.t("app.more_options.delete_file"),
                        true,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, _, cx| {
                                this.close_more_options_menu(cx);
                                if let Some(ref path) = p {
                                    this.open_delete_folder_modal(path.clone(), cx);
                                }
                            })
                        },
                        theme,
                        cx,
                    ))
                    // Divider 8
                    .child(self.render_menu_divider(theme))
                    // --- GROUP 9: ATTACHMENTS & DRAWINGS ---
                    // 24. Collect attachments in file
                    .child(self.render_more_options_item(
                        "more-item-collect-attachments",
                        icons::ICON_DOWNLOAD,
                        self.t("app.more_options.collect_attachments"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        Box::new(|this, _, cx| {
                            this.state.status_text = this.t("sidebar.attachments_collected_file");
                            this.close_more_options_menu(cx);
                            cx.notify();
                        }),
                        theme,
                        cx,
                    ))
                    // 25. Create new drawing
                    .child(self.render_more_options_item(
                        "more-item-new-drawing",
                        icons::ICON_DRAWING,
                        self.t("app.more_options.new_drawing"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        {
                            let p = target_path.clone();
                            Box::new(move |this, _, cx| {
                                this.close_more_options_menu(cx);
                                let target_dir = p
                                    .as_ref()
                                    .and_then(|path| path.parent().map(|d| d.to_path_buf()))
                                    .or_else(|| this.state.workspace_path.clone());
                                if let Some(dir) = target_dir {
                                    if let Ok(draw_path) = folder_actions::create_drawing_in_folder(&dir) {
                                        this.state.reload_file_tree();
                                        this.state.open_note(draw_path);
                                        cx.notify();
                                    }
                                }
                            })
                        },
                        theme,
                        cx,
                    ))
                    // 26. Unused attachments
                    .child(self.render_more_options_item(
                        "more-item-unused-attachments",
                        icons::ICON_PAPERCLIP,
                        self.t("app.more_options.unused_attachments"),
                        false,
                        false,
                        ActiveSubmenu::None,
                        Box::new(|this, _, cx| {
                            this.close_more_options_menu(cx);
                            this.open_unused_attachments(cx);
                        }),
                        theme,
                        cx,
                    )),
            )
            // SUBMENU: Copy Path
            .when(self.more_options_menu.active_submenu == ActiveSubmenu::CopyPath, |overlay| {
                let sub_y = (menu_y + 360.0).min(window_h - 110.0).max(8.0);
                let target_for_copy = target_path.clone();
                overlay.child(
                    div()
                        .id("more-submenu-copy-path")
                        .absolute()
                        .left(px(submenu_x))
                        .top(px(sub_y))
                        .w(px(submenu_w))
                        .bg(theme.bg_sidebar)
                        .border_1()
                        .border_color(theme.border)
                        .rounded_lg()
                        .shadow_xl()
                        .p_1()
                        .flex()
                        .flex_col()
                        .cursor(gpui::CursorStyle::Arrow)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Right, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        // Hover bridge
                        .child(
                            div()
                                .id("more-sub-copy-bridge")
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .when(!submenu_on_left, |d| d.left(px(-10.0)).w(px(12.0)))
                                .when(submenu_on_left, |d| d.right(px(-10.0)).w(px(12.0)))
                                .on_hover(cx.listener(|this, is_hovered: &bool, _, _cx| {
                                    if *is_hovered {
                                        this.more_options_menu.active_submenu = ActiveSubmenu::CopyPath;
                                    }
                                })),
                        )
                        // Subitem 1: Relative path
                        .child({
                            let p = target_for_copy.clone();
                            div()
                                .id("more-sub-copy-rel")
                                .h(px(28.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2p5()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(icons::ICON_COPY, 13.0, theme.text_secondary))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child(self.t("app.more_options.copy_relative_path")),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(ref path) = p {
                                        let rel_str = if let Some(ref ws) = this.state.workspace_path {
                                            path.strip_prefix(ws).unwrap_or(path).to_string_lossy().to_string()
                                        } else {
                                            path.file_name().unwrap_or_default().to_string_lossy().to_string()
                                        };
                                        cx.write_to_clipboard(ClipboardItem::new_string(rel_str.clone()));
                                        let msg = this.t_fmt("sidebar.copy_relative_path_msg", &[&rel_str]);
                                        this.push_notification(
                                            crate::views::notification::NotificationType::Info,
                                            None,
                                            msg,
                                            Some(3000),
                                            cx,
                                        );
                                    }
                                    this.close_more_options_menu(cx);
                                }))
                        })
                        // Subitem 2: Absolute path
                        .child({
                            let p = target_for_copy.clone();
                            div()
                                .id("more-sub-copy-abs")
                                .h(px(28.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2p5()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(icons::ICON_COPY, 13.0, theme.text_secondary))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child(self.t("app.more_options.copy_absolute_path")),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(ref path) = p {
                                        let abs_str = path.to_string_lossy().to_string();
                                        cx.write_to_clipboard(ClipboardItem::new_string(abs_str.clone()));
                                        let msg = this.t_fmt("sidebar.copy_absolute_path_msg", &[&abs_str]);
                                        this.push_notification(
                                            crate::views::notification::NotificationType::Info,
                                            None,
                                            msg,
                                            Some(3000),
                                            cx,
                                        );
                                    }
                                    this.close_more_options_menu(cx);
                                }))
                        })
                        // Subitem 3: as Obsidian URL
                        .child({
                            let p = target_for_copy.clone();
                            div()
                                .id("more-sub-copy-obsidian-url")
                                .h(px(28.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2p5()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(icons::ICON_LINK, 13.0, theme.text_secondary))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child(self.t("app.more_options.as_obsidian_url")),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(ref path) = p {
                                        let ws = this.state.workspace_path.as_deref().unwrap_or(Path::new(""));
                                        let uri = file_actions::get_obsidian_uri(ws, path);
                                        cx.write_to_clipboard(ClipboardItem::new_string(uri.clone()));
                                        let msg = this.t_fmt("sidebar.copy_obsidian_url", &[&uri]);
                                        this.push_notification(
                                            crate::views::notification::NotificationType::Info,
                                            None,
                                            msg,
                                            Some(3000),
                                            cx,
                                        );
                                    }
                                    this.close_more_options_menu(cx);
                                }))
                        }),
                )
            })
            // SUBMENU: Open Linked View
            .when(self.more_options_menu.active_submenu == ActiveSubmenu::LinkedView, |overlay| {
                let sub_y = (menu_y + 388.0).min(window_h - 110.0).max(8.0);
                let target_for_linked = target_path.clone();
                overlay.child(
                    div()
                        .id("more-submenu-linked-view")
                        .absolute()
                        .left(px(submenu_x))
                        .top(px(sub_y))
                        .w(px(submenu_w))
                        .bg(theme.bg_sidebar)
                        .border_1()
                        .border_color(theme.border)
                        .rounded_lg()
                        .shadow_xl()
                        .p_1()
                        .flex()
                        .flex_col()
                        .cursor(gpui::CursorStyle::Arrow)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Right, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        // Hover bridge
                        .child(
                            div()
                                .id("more-sub-linked-bridge")
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .when(!submenu_on_left, |d| d.left(px(-10.0)).w(px(12.0)))
                                .when(submenu_on_left, |d| d.right(px(-10.0)).w(px(12.0)))
                                .on_hover(cx.listener(|this, is_hovered: &bool, _, _cx| {
                                    if *is_hovered {
                                        this.more_options_menu.active_submenu = ActiveSubmenu::LinkedView;
                                    }
                                })),
                        )
                        // Subitem 1: Linked preview area
                        .child({
                            let p = target_for_linked.clone();
                            div()
                                .id("more-sub-linked-preview")
                                .h(px(28.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2p5()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(icons::ICON_BOOK_OPEN, 13.0, theme.text_secondary))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child(self.t("app.more_options.linked_reading_view")),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if let Some(ref path) = p {
                                        this.split_tab_right(path, window, cx);
                                        this.edit_mode = false;
                                        let current_scroll = this.current_scroll_top();
                                        this.state.snapshot_active_note_settings(current_scroll, this.edit_mode, this.word_wrap, this.source_mode);
                                    }
                                    this.close_more_options_menu(cx);
                                    cx.notify();
                                }))
                        })
                        // Subitem 2: Backlinks
                        .child(
                            div()
                                .id("more-sub-linked-backlinks")
                                .h(px(28.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2p5()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(icons::ICON_LINK, 13.0, theme.text_secondary))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child(self.t("app.more_options.backlinks")),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.right_sidebar_collapsed = false;
                                    this.state.status_text = this.t("app.more_options.backlinks");
                                    this.close_more_options_menu(cx);
                                    cx.notify();
                                })),
                        )
                        // Subitem 3: Outgoing links
                        .child(
                            div()
                                .id("more-sub-linked-outgoing")
                                .h(px(28.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2p5()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(icons::ICON_EXTERNAL_LINK, 13.0, theme.text_secondary))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child(self.t("app.more_options.outgoing_links")),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.right_sidebar_collapsed = false;
                                    this.state.status_text = this.t("app.more_options.outgoing_links");
                                    this.close_more_options_menu(cx);
                                    cx.notify();
                                })),
                        ),
                )
            })
    }
}
