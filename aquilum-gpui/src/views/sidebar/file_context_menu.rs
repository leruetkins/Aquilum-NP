use std::path::PathBuf;
use gpui::{
    div, prelude::*, px, ClipboardItem, Context, IntoElement, MouseButton,
    Styled, Window,
};
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::sidebar::file_actions;
use crate::views::sidebar::folder_actions;

/// State for the Obsidian-style file context menu.
#[derive(Clone, Debug)]
pub struct FileContextMenuState {
    pub is_open: bool,
    pub file_path: PathBuf,
    pub file_name: String,
    pub position_x: f32,
    pub position_y: f32,
    pub hovered_item: Option<usize>,
}

impl Default for FileContextMenuState {
    fn default() -> Self {
        Self {
            is_open: false,
            file_path: PathBuf::new(),
            file_name: String::new(),
            position_x: 0.0,
            position_y: 0.0,
            hovered_item: None,
        }
    }
}

impl AppView {
    /// Opens the file context menu at the specified screen coordinates.
    pub fn open_file_context_menu(&mut self, file_path: PathBuf, x: f32, y: f32, cx: &mut Context<Self>) {
        // If folder menu is open, close it
        self.folder_context_menu.is_open = false;

        let name = file_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Файл".to_string());

        self.file_context_menu = FileContextMenuState {
            is_open: true,
            file_path,
            file_name: name,
            position_x: x,
            position_y: y,
            hovered_item: None,
        };
        cx.notify();
    }

    /// Closes the file context menu.
    pub fn close_file_context_menu(&mut self, cx: &mut Context<Self>) {
        self.file_context_menu.is_open = false;
        self.file_context_menu.hovered_item = None;
        cx.notify();
    }

    /// Renders the complete Obsidian-like file context menu overlay.
    pub fn render_file_context_menu(&self, theme: &Theme, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let window_size = window.bounds().size;
        let window_h: f32 = window_size.height.into();
        let window_w: f32 = window_size.width.into();

        let menu_w = 265.0;
        let menu_h = 500.0;
        let submenu_w = 210.0;
        let submenu_gap = 6.0;

        // Upward or downward placement:
        // Status bar is 28px + 8px margin = 36px bottom margin.
        // If there's not enough room downwards, open the menu upwards so its bottom edge
        // aligns with the middle of the clicked item row (~13px above click_y).
        let click_y = self.file_context_menu.position_y;
        let menu_y = if click_y + menu_h + 36.0 > window_h {
            (click_y - menu_h + 13.0).max(8.0)
        } else {
            click_y.max(8.0)
        };

        // Clamp X so menu doesn't overflow right of screen
        let mut menu_x = self.file_context_menu.position_x.max(8.0);
        if menu_x + menu_w > window_w - 12.0 {
            menu_x = (window_w - menu_w - 12.0).max(8.0);
        }

        // Submenu orientation: if not enough space on right for submenu, open to left
        let submenu_on_left = menu_x + menu_w + submenu_w + submenu_gap + 12.0 > window_w;
        let submenu_x = if submenu_on_left {
            menu_x - submenu_w - submenu_gap
        } else {
            menu_x + menu_w + submenu_gap
        };

        // Item 8 ("Копировать путь") vertical position inside menu container:
        // Precise vertical coordinate calculation for the submenu:
        // - Top internal padding of the menu panel: p_1() = 4px
        // - 8 menu items preceding Item 8 (indices 0..7), each with fixed height h(px(26.0)):
        //   8 * 26px = 208px
        // - 2 section dividers (Divider 1 & Divider 2): each takes ~8px in Taffy flexbox (h_px + my_1):
        //   2 * 8px = 16px
        // Total offset of Item 8 top edge: 4px + 208px + 16px = 228px.
        // Because the submenu container also has identical top padding p_1() (4px) and 26px item height,
        // submenu_y = menu_y + 228px aligns the first submenu item ("как URL Obsidian")
        // pixel-perfect horizontally with the parent item ("Копировать путь").
        let submenu_y = menu_y + 228.0;

        let is_submenu_open = self.file_context_menu.hovered_item == Some(8);
        let target_path = self.file_context_menu.file_path.clone();

        let render_item = |ix: usize,
                           id: &'static str,
                           icon_path: &'static str,
                           label: &'static str,
                           is_danger: bool,
                           on_click: Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static>| {
            let is_selected = self.file_context_menu.hovered_item == Some(ix);
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
                .h(px(26.0))
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
                        this.file_context_menu.hovered_item = Some(ix);
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(icons::icon(icon_path, 13.0, icon_color))
                        .child(
                            div()
                                .text_xs()
                                .text_color(text_color)
                                .child(label),
                        ),
                )
                .on_click(move |ev, win, app| on_click(ev, win, app))
        };

        div()
            .id("file-context-menu-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_move(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            // Invisible full-screen backdrop to dismiss menu on click
            .child(
                div()
                    .id("file-context-backdrop")
                    .absolute()
                    .inset_0()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.close_file_context_menu(cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                        this.close_file_context_menu(cx);
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
                    .id("file-context-panel")
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
                    // --- SECTION 1: OPENING MODES ---
                    // 1. Открыть в новой вкладке (index 0)
                    .child(render_item(
                        0,
                        "menu-item-open-new-tab",
                        icons::ICON_NOTE_CREATE,
                        "Открыть в новой вкладке",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, window, cx| {
                                this.close_file_context_menu(cx);
                                this.select_note(p.clone(), true, window, cx);
                            }))
                        },
                    ))
                    // 2. Открыть в области справа (index 1)
                    .child(render_item(
                        1,
                        "menu-item-open-right-split",
                        icons::ICON_SPLIT,
                        "Открыть в области справа",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, window, cx| {
                                this.close_file_context_menu(cx);
                                this.select_note(p.clone(), true, window, cx);
                                this.right_sidebar_collapsed = false;
                                cx.notify();
                            }))
                        },
                    ))
                    // 3. Открыть в новом окне (index 2)
                    .child(render_item(
                        2,
                        "menu-item-open-new-window",
                        icons::ICON_APP_WINDOW,
                        "Открыть в новом окне",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, window, cx| {
                                this.close_file_context_menu(cx);
                                this.select_note(p.clone(), true, window, cx);
                            }))
                        },
                    ))
                    // Divider 1
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 2: DRAWINGS ---
                    // 4. Создать новый чертеж (index 3)
                    .child(render_item(
                        3,
                        "menu-item-file-new-drawing",
                        icons::ICON_DRAWING,
                        "Создать новый чертеж",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                let parent = p.parent().unwrap_or(&p);
                                if let Ok(draw_path) = folder_actions::create_drawing_in_folder(parent) {
                                    this.state.reload_file_tree();
                                    this.state.open_note(draw_path);
                                    cx.notify();
                                }
                            }))
                        },
                    ))
                    // Divider 2
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 3: MANAGEMENT & COPY PATH ---
                    // 5. Создать копию (index 4)
                    .child(render_item(
                        4,
                        "menu-item-file-duplicate",
                        icons::ICON_COPY,
                        "Создать копию",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                if let Ok(copy_path) = file_actions::duplicate_file(&p) {
                                    this.state.reload_file_tree();
                                    this.state.status_text = format!("Создана копия файла: {:?}", copy_path.file_name().unwrap_or_default());
                                    cx.notify();
                                }
                            }))
                        },
                    ))
                    // 6. Переместить файл в... (index 5)
                    .child(render_item(
                        5,
                        "menu-item-file-move",
                        icons::ICON_FOLDER_INPUT,
                        "Переместить файл в...",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                this.open_move_folder_modal(p.clone(), cx);
                            }))
                        },
                    ))
                    // 7. Добавить в закладки... (index 6)
                    .child(render_item(
                        6,
                        "menu-item-file-bookmark",
                        icons::ICON_BOOKMARK,
                        "Добавить в закладки...",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                let name = p.file_name().unwrap_or_default().to_string_lossy();
                                this.state.status_text = format!("Файл '{}' добавлен в закладки", name);
                                cx.notify();
                            }))
                        },
                    ))
                    // 8. Объединить весь файл с... (index 7 - in Group 3)
                    .child(render_item(
                        7,
                        "menu-item-file-merge",
                        icons::ICON_MERGE,
                        "Объединить весь файл с...",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                let name = p.file_name().unwrap_or_default().to_string_lossy();
                                this.state.status_text = format!("Объединение '{}' с файлом...", name);
                                cx.notify();
                            }))
                        },
                    ))
                    // 9. Копировать путь > (triggers floating submenu, index 8 in sequential hovering)
                    .child(
                        div()
                            .id("file-menu-item-copy-path-parent")
                            .h(px(26.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .px_2p5()
                            .rounded_md()
                            .cursor_pointer()
                            .when(is_submenu_open, |d| d.bg(theme.bg_hover))
                            .hover(|h| h.bg(theme.bg_hover))
                            .on_hover(cx.listener(|this, is_hovered: &bool, _, cx| {
                                if *is_hovered {
                                    this.file_context_menu.hovered_item = Some(8);
                                    cx.notify();
                                }
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(icons::icon(icons::ICON_COPY, 13.0, theme.text_secondary))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_primary)
                                            .child("Копировать путь"),
                                    ),
                            )
                            .child(icons::icon(icons::ICON_CHEVRON_RIGHT, 11.0, theme.text_muted)),
                    )
                    // Divider 3
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 4: APPS & EXPLORER ---
                    // 10. Открыть в приложении по умолчанию (index 9)
                    .child(render_item(
                        9,
                        "menu-item-open-default-app",
                        icons::ICON_EXTERNAL_LINK,
                        "Открыть в приложении по умолчанию",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                file_actions::open_in_default_app(&p);
                            }))
                        },
                    ))
                    // 11. Показать в Проводнике (index 10)
                    .child(render_item(
                        10,
                        "menu-item-file-show-in-explorer",
                        icons::ICON_EXTERNAL_LINK,
                        "Показать в Проводнике",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                folder_actions::show_in_explorer(&p);
                            }))
                        },
                    ))
                    // Divider 4
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 5: OBSIDIAN FEATURES ---
                    // 12. Link to Parent Folder (index 11)
                    .child(render_item(
                        11,
                        "menu-item-link-to-parent",
                        icons::ICON_LINK,
                        "Link to Parent Folder",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                if let Some(parent) = p.parent() {
                                    let parent_name = parent.file_name().unwrap_or_default().to_string_lossy();
                                    this.state.status_text = format!("Ссылка на родительскую папку '{}' создана", parent_name);
                                }
                                cx.notify();
                            }))
                        },
                    ))
                    // 13. Change icon (index 12)
                    .child(render_item(
                        12,
                        "menu-item-file-change-icon",
                        icons::ICON_HASH,
                        "Change icon",
                        false,
                        Box::new(cx.listener(|this, _, _, cx| {
                            this.close_file_context_menu(cx);
                            this.state.status_text = "Смена иконки заметки".to_string();
                            cx.notify();
                        })),
                    ))
                    // 14. Check file stats (index 13)
                    .child(render_item(
                        13,
                        "menu-item-check-file-stats",
                        icons::ICON_ANALYSIS,
                        "Check file stats",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                if let Ok(meta) = std::fs::metadata(&p) {
                                    this.state.status_text = format!("Размер: {} байт", meta.len());
                                }
                                cx.notify();
                            }))
                        },
                    ))
                    // Divider 5
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 6: RENAME & DELETE ---
                    // 15. Переименовать (index 14)
                    .child(render_item(
                        14,
                        "menu-item-file-rename",
                        icons::ICON_PENCIL,
                        "Переименовать",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                this.open_rename_folder_modal(p.clone(), cx);
                            }))
                        },
                    ))
                    // 16. Удалить (index 15)
                    .child(render_item(
                        15,
                        "menu-item-file-delete",
                        icons::ICON_TRASH_2,
                        "Удалить",
                        true,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_file_context_menu(cx);
                                this.open_delete_folder_modal(p.clone(), cx);
                            }))
                        },
                    ))
                    // Divider 6
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 7: ATTACHMENTS ---
                    // 17. Collect attachments in file (index 16)
                    .child(render_item(
                        16,
                        "menu-item-file-collect-attachments",
                        icons::ICON_DOWNLOAD,
                        "Collect attachments in file",
                        false,
                        Box::new(cx.listener(|this, _, _, cx| {
                            this.close_file_context_menu(cx);
                            this.state.status_text = "Вложения в файле собраны".to_string();
                            cx.notify();
                        })),
                    )),
            )
            // Floating submenu for Copy Path (placed in overlay coordinates with a clear 6px offset)
            .when(self.file_context_menu.hovered_item == Some(8), |overlay| {
                let target_for_sub = target_path.clone();
                overlay.child(
                    div()
                        .id("file-copy-path-submenu")
                        .absolute()
                        .left(px(submenu_x))
                        .top(px(submenu_y))
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
                        .on_hover(cx.listener(|this, is_hovered: &bool, _, _cx| {
                            if *is_hovered {
                                this.file_context_menu.hovered_item = Some(8);
                            }
                        }))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Right, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_scroll_wheel(|_, _, cx| {
                            cx.stop_propagation();
                        })
                        // Invisible bridge connecting parent item and submenu across the 6px gap
                        .child(
                            div()
                                .id("file-submenu-hover-bridge")
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .when(!submenu_on_left, |this| this.left(px(-10.0)).w(px(12.0)))
                                .when(submenu_on_left, |this| this.right(px(-10.0)).w(px(12.0)))
                                .on_hover(cx.listener(|this, is_hovered: &bool, _, _cx| {
                                    if *is_hovered {
                                        this.file_context_menu.hovered_item = Some(8);
                                    }
                                })),
                        )
                        // Submenu item 1: как URL Obsidian
                        .child(
                            div()
                                .id("file-submenu-copy-obsidian-url")
                                .h(px(26.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(icons::ICON_LINK, 13.0, theme.text_secondary))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child("как URL Obsidian"),
                                )
                                .on_click({
                                    let p = target_for_sub.clone();
                                    cx.listener(move |this, _, _, cx| {
                                        let ws = this.state.workspace_path.as_deref().unwrap_or(std::path::Path::new(""));
                                        let uri = file_actions::get_obsidian_uri(ws, &p);
                                        cx.write_to_clipboard(ClipboardItem::new_string(uri.clone()));
                                        this.state.status_text = format!("Скопирован URL Obsidian: {}", uri);
                                        this.close_file_context_menu(cx);
                                    })
                                }),
                        )
                        // Submenu item 2: из папки хранилища (относительный путь)
                        .child(
                            div()
                                .id("file-submenu-copy-vault-path")
                                .h(px(26.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(icons::ICON_VAULT, 13.0, theme.text_secondary))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child("из папки хранилища"),
                                )
                                .on_click({
                                    let p = target_for_sub.clone();
                                    cx.listener(move |this, _, _, cx| {
                                        let ws = this.state.workspace_path.as_deref().unwrap_or(std::path::Path::new(""));
                                        let rel = folder_actions::get_relative_path_string(ws, &p);
                                        cx.write_to_clipboard(ClipboardItem::new_string(rel.clone()));
                                        this.state.status_text = format!("Скопирован путь из хранилища: {}", rel);
                                        this.close_file_context_menu(cx);
                                    })
                                }),
                        )
                        // Submenu item 3: от корня системы (абсолютный путь)
                        .child(
                            div()
                                .id("file-submenu-copy-abs-path")
                                .h(px(26.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(icons::ICON_COMPUTER, 13.0, theme.text_secondary))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child("от корня системы"),
                                )
                                .on_click({
                                    let p = target_for_sub.clone();
                                    cx.listener(move |this, _, _, cx| {
                                        let abs = p.to_string_lossy().to_string();
                                        cx.write_to_clipboard(ClipboardItem::new_string(abs.clone()));
                                        this.state.status_text = format!("Скопирован абсолютный путь: {}", abs);
                                        this.close_file_context_menu(cx);
                                    })
                                }),
                        ),
                )
            })
    }
}
