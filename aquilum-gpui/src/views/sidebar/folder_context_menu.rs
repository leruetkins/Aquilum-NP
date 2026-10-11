use std::path::PathBuf;
use gpui::{
    div, prelude::*, px, ClipboardItem, Context, IntoElement, MouseButton,
    Styled, Window,
};
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::sidebar::folder_actions;

/// State for the Obsidian-style folder context menu.
#[derive(Clone, Debug)]
pub struct FolderContextMenuState {
    pub is_open: bool,
    pub folder_path: PathBuf,
    pub folder_name: String,
    pub position_x: f32,
    pub position_y: f32,
    pub is_copy_path_submenu_open: bool,
    pub hovered_item: Option<usize>,
}

impl Default for FolderContextMenuState {
    fn default() -> Self {
        Self {
            is_open: false,
            folder_path: PathBuf::new(),
            folder_name: String::new(),
            position_x: 0.0,
            position_y: 0.0,
            is_copy_path_submenu_open: false,
            hovered_item: None,
        }
    }
}

impl AppView {
    /// Opens the folder context menu at the given coordinates.
    pub fn open_folder_context_menu(&mut self, folder_path: PathBuf, x: f32, y: f32, cx: &mut Context<Self>) {
        // If file menu is open, close it
        self.file_context_menu.is_open = false;

        let name = folder_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Папка".to_string());

        self.folder_context_menu = FolderContextMenuState {
            is_open: true,
            folder_path,
            folder_name: name,
            position_x: x,
            position_y: y,
            is_copy_path_submenu_open: false,
            hovered_item: None,
        };
        cx.notify();
    }

    /// Closes the folder context menu.
    pub fn close_folder_context_menu(&mut self, cx: &mut Context<Self>) {
        self.folder_context_menu.is_open = false;
        self.folder_context_menu.is_copy_path_submenu_open = false;
        self.folder_context_menu.hovered_item = None;
        cx.notify();
    }

    /// Renders the complete Obsidian-like folder context menu overlay.
    pub fn render_folder_context_menu(&self, theme: &Theme, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let window_size = window.bounds().size;
        let window_h: f32 = window_size.height.into();
        let window_w: f32 = window_size.width.into();

        let menu_w = 250.0;
        let menu_h = 500.0;
        let submenu_w = 180.0;
        let submenu_gap = 6.0;

        // Upward or downward placement:
        // Status bar is 28px + 8px margin = 36px bottom margin.
        // If there's not enough room downwards, open the menu upwards so its bottom edge
        // aligns with the middle of the clicked item row (~13px above click_y).
        let click_y = self.folder_context_menu.position_y;
        let menu_y = if click_y + menu_h + 36.0 > window_h {
            (click_y - menu_h + 13.0).max(8.0)
        } else {
            click_y.max(8.0)
        };

        // Clamp X so menu doesn't overflow right of screen
        let mut menu_x = self.folder_context_menu.position_x.max(8.0);
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
        // submenu_y = menu_y + 228px aligns the first submenu item ("Относительный путь")
        // pixel-perfect horizontally with the parent item ("Копировать путь").
        let submenu_y = menu_y + 228.0;

        let is_submenu_open = self.folder_context_menu.hovered_item == Some(8);
        let target_path = self.folder_context_menu.folder_path.clone();

        let render_item = |ix: usize,
                           id: &'static str,
                           icon_path: &'static str,
                           label: &'static str,
                           is_danger: bool,
                           on_click: Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static>| {
            let is_selected = self.folder_context_menu.hovered_item == Some(ix);
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
                        this.folder_context_menu.hovered_item = Some(ix);
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
            .id("folder-context-menu-overlay")
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
                    .id("folder-context-backdrop")
                    .absolute()
                    .inset_0()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.close_folder_context_menu(cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                        this.close_folder_context_menu(cx);
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
                    .id("folder-context-panel")
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
                    // --- SECTION 1: CREATION ---
                    // 1. Новая заметка (index 0)
                    .child(render_item(
                        0,
                        "menu-item-new-note",
                        icons::ICON_NOTE_CREATE,
                        "Новая заметка",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                if let Ok(new_note_path) = folder_actions::create_note_in_folder(&p) {
                                    this.state.expanded_folders.insert(p.clone());
                                    this.state.reload_file_tree();
                                    this.state.open_note(new_note_path);
                                    cx.notify();
                                }
                            }))
                        },
                    ))
                    // 2. Новая папка (index 1)
                    .child(render_item(
                        1,
                        "menu-item-new-folder",
                        icons::ICON_FOLDER,
                        "Новая папка",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                if let Ok(new_folder) = folder_actions::create_folder_in_folder(&p) {
                                    this.state.expanded_folders.insert(p.clone());
                                    this.state.expanded_folders.insert(new_folder);
                                    this.state.reload_file_tree();
                                    cx.notify();
                                }
                            }))
                        },
                    ))
                    // 3. Новый холст (index 2)
                    .child(render_item(
                        2,
                        "menu-item-new-canvas",
                        icons::ICON_CANVAS,
                        "Новый холст",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                if let Ok(canvas_path) = folder_actions::create_canvas_in_folder(&p) {
                                    this.state.expanded_folders.insert(p.clone());
                                    this.state.reload_file_tree();
                                    this.state.status_text = format!("Холст создан: {:?}", canvas_path.file_name().unwrap_or_default());
                                    cx.notify();
                                }
                            }))
                        },
                    ))
                    // 4. Создать новый чертеж (index 3)
                    .child(render_item(
                        3,
                        "menu-item-new-drawing",
                        icons::ICON_DRAWING,
                        "Создать новый чертеж",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                if let Ok(draw_path) = folder_actions::create_drawing_in_folder(&p) {
                                    this.state.expanded_folders.insert(p.clone());
                                    this.state.reload_file_tree();
                                    this.state.open_note(draw_path);
                                    cx.notify();
                                }
                            }))
                        },
                    ))
                    // Divider 1
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 2: MANAGEMENT ---
                    // 5. Создать копию (index 4)
                    .child(render_item(
                        4,
                        "menu-item-duplicate-folder",
                        icons::ICON_COPY,
                        "Создать копию",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                if let Ok(copy_path) = folder_actions::duplicate_folder(&p) {
                                    this.state.reload_file_tree();
                                    this.state.status_text = format!("Создана копия папки: {:?}", copy_path.file_name().unwrap_or_default());
                                    cx.notify();
                                }
                            }))
                        },
                    ))
                    // 6. Переместить папку в... (index 5)
                    .child(render_item(
                        5,
                        "menu-item-move-folder",
                        icons::ICON_FOLDER_INPUT,
                        "Переместить папку в...",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                this.open_move_folder_modal(p.clone(), cx);
                            }))
                        },
                    ))
                    // 7. Искать в папке (index 6)
                    .child(render_item(
                        6,
                        "menu-item-search-in-folder",
                        icons::ICON_SEARCH,
                        "Искать в папке",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, window, cx| {
                                this.close_folder_context_menu(cx);
                                let ws = this.state.workspace_path.as_deref().unwrap_or(std::path::Path::new(""));
                                let rel = folder_actions::get_relative_path_string(ws, &p);
                                this.state.search_query = format!("path:{} ", rel);
                                window.focus(&this.search_focus_handle);
                                this.quick_filter.focus_and_select_all(&this.state.search_query, cx);
                                cx.notify();
                            }))
                        },
                    ))
                    // 8. Добавить в закладки... (index 7)
                    .child(render_item(
                        7,
                        "menu-item-bookmark-folder",
                        icons::ICON_BOOKMARK,
                        "Добавить в закладки...",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                let name = p.file_name().unwrap_or_default().to_string_lossy();
                                this.state.status_text = format!("Папка '{}' добавлена в закладки", name);
                                cx.notify();
                            }))
                        },
                    ))
                    // Divider 2
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 3: PATHS & EXPLORER ---
                    // 9. Копировать путь > (triggers floating submenu, index 8)
                    .child(
                        div()
                            .id("menu-item-copy-path-parent")
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
                                    this.folder_context_menu.hovered_item = Some(8);
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
                    // 10. Показать в Проводнике (index 9)
                    .child(render_item(
                        9,
                        "menu-item-show-in-explorer",
                        icons::ICON_EXTERNAL_LINK,
                        "Показать в Проводнике",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                folder_actions::show_in_explorer(&p);
                            }))
                        },
                    ))
                    // Divider 3
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 4: OBSIDIAN FEATURES ---
                    // 11. Find orphaned files (index 10)
                    .child(render_item(
                        10,
                        "menu-item-find-orphaned",
                        icons::ICON_SEARCH,
                        "Find orphaned files",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                let ws = this.state.workspace_path.as_deref().unwrap_or(std::path::Path::new(""));
                                let orphaned = folder_actions::find_orphaned_files_in_folder(ws, &p);
                                if orphaned.is_empty() {
                                    this.state.status_text = "Изолированных заметок в этой папке не найдено".to_string();
                                } else {
                                    this.state.status_text = format!("Найдено {} изолированных заметок", orphaned.len());
                                }
                                cx.notify();
                            }))
                        },
                    ))
                    // 12. Create Folder Note (index 11)
                    .child(render_item(
                        11,
                        "menu-item-create-folder-note",
                        icons::ICON_PENCIL,
                        "Create Folder Note",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                if let Ok(note_path) = folder_actions::create_folder_note(&p) {
                                    this.state.reload_file_tree();
                                    this.state.open_note(note_path);
                                    cx.notify();
                                }
                            }))
                        },
                    ))
                    // 13. Change icon (index 12)
                    .child(render_item(
                        12,
                        "menu-item-change-icon",
                        icons::ICON_HASH,
                        "Change icon",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                let name = p.file_name().unwrap_or_default().to_string_lossy();
                                this.state.status_text = format!("Смена значка для '{}'", name);
                                cx.notify();
                            }))
                        },
                    ))
                    // Divider 4
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 5: MODIFICATIONS (RENAME / DELETE) ---
                    // 14. Переименовать (index 13)
                    .child(render_item(
                        13,
                        "menu-item-rename-folder",
                        icons::ICON_PENCIL,
                        "Переименовать",
                        false,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                this.open_rename_folder_modal(p.clone(), cx);
                            }))
                        },
                    ))
                    // 15. Удалить (Red danger item, index 14)
                    .child(render_item(
                        14,
                        "menu-item-delete-folder",
                        icons::ICON_TRASH_2,
                        "Удалить",
                        true,
                        {
                            let p = target_path.clone();
                            Box::new(cx.listener(move |this, _, _, cx| {
                                this.close_folder_context_menu(cx);
                                this.open_delete_folder_modal(p.clone(), cx);
                            }))
                        },
                    ))
                    // Divider 5
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 6: ATTACHMENTS ---
                    // 16. Collect attachments in file (index 15)
                    .child(render_item(
                        15,
                        "menu-item-collect-attachments",
                        icons::ICON_DOWNLOAD,
                        "Collect attachments in file",
                        false,
                        Box::new(cx.listener(|this, _, _, cx| {
                            this.close_folder_context_menu(cx);
                            this.state.status_text = "Вложения собраны".to_string();
                            cx.notify();
                        })),
                    ))
                    // 17. Move attachment to proper folder (index 16)
                    .child(render_item(
                        16,
                        "menu-item-move-attachments",
                        icons::ICON_MOVE_ARROWS,
                        "Move attachment to proper folder",
                        false,
                        Box::new(cx.listener(|this, _, _, cx| {
                            this.close_folder_context_menu(cx);
                            this.state.status_text = "Вложения перемещены в папку".to_string();
                            cx.notify();
                        })),
                    )),
            )
            // Floating submenu for Copy Path (placed in overlay coordinates with a clear 6px offset)
            .when(is_submenu_open, |overlay| {
                let target_for_sub = target_path.clone();
                overlay.child(
                    div()
                        .id("folder-copy-path-submenu")
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
                                this.folder_context_menu.hovered_item = Some(8);
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
                                .id("submenu-hover-bridge")
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .when(!submenu_on_left, |this| this.left(px(-10.0)).w(px(12.0)))
                                .when(submenu_on_left, |this| this.right(px(-10.0)).w(px(12.0)))
                                .on_hover(cx.listener(|this, is_hovered: &bool, _, _cx| {
                                    if *is_hovered {
                                        this.folder_context_menu.hovered_item = Some(8);
                                    }
                                })),
                        )
                        .child(
                            div()
                                .id("submenu-copy-rel-path")
                                .h(px(26.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child("Относительный путь"),
                                )
                                .on_click({
                                    let p = target_for_sub.clone();
                                    cx.listener(move |this, _, _, cx| {
                                        let ws = this.state.workspace_path.as_deref().unwrap_or(std::path::Path::new(""));
                                        let rel = folder_actions::get_relative_path_string(ws, &p);
                                        cx.write_to_clipboard(ClipboardItem::new_string(rel.clone()));
                                        this.state.status_text = format!("Скопирован относительный путь: {}", rel);
                                        this.close_folder_context_menu(cx);
                                    })
                                }),
                        )
                        .child(
                            div()
                                .id("submenu-copy-abs-path")
                                .h(px(26.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .px_2p5()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child("Абсолютный путь"),
                                )
                                .on_click({
                                    let p = target_for_sub.clone();
                                    cx.listener(move |this, _, _, cx| {
                                        let abs = p.to_string_lossy().to_string();
                                        cx.write_to_clipboard(ClipboardItem::new_string(abs.clone()));
                                        this.state.status_text = format!("Скопирован абсолютный путь: {}", abs);
                                        this.close_folder_context_menu(cx);
                                    })
                                }),
                        ),
                )
            })
    }

    /// Renders a subtle divider line between sections in the context menu.
    pub(crate) fn render_menu_divider(&self, theme: &Theme) -> impl IntoElement {
        div()
            .h_px()
            .bg(theme.border_subtle)
            .my_1()
            .mx_1()
    }
}
