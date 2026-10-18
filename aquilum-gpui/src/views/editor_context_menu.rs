use serde::{Deserialize, Serialize};
use gpui::{
    div, prelude::*, px, rgba, Context, Div, IntoElement, MouseButton,
    Styled, Window,
};

use crate::icons;
use crate::plugins::api::HostEvent;
use crate::theme::Theme;
use crate::views::app_view::AppView;

/// Represents an item or section header inside a context menu.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextMenuItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub checked: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub is_label: bool,
    #[serde(default)]
    pub is_separator: bool,
    #[serde(default)]
    pub has_callback: bool,
    #[serde(default)]
    pub owner_plugin: Option<String>,
    #[serde(default)]
    pub submenu: Option<Vec<ContextMenuItem>>,
}

/// State for the native editor context menu with multi-level cascading submenus.
#[derive(Clone, Debug, Default)]
pub struct ContextMenuState {
    pub is_open: bool,
    pub session_id: u64,
    pub position_x: f32,
    pub position_y: f32,
    pub items: Vec<ContextMenuItem>,
    /// Hovered path of item indices indicating active submenus at each depth
    pub active_path: Vec<usize>,
}

fn separator(id: &str) -> ContextMenuItem {
    ContextMenuItem {
        id: id.to_string(),
        title: String::new(),
        icon: String::new(),
        checked: false,
        disabled: false,
        is_label: false,
        is_separator: true,
        has_callback: false,
        owner_plugin: None,
        submenu: None,
    }
}

fn item(id: &str, title: String, icon: &str, disabled: bool) -> ContextMenuItem {
    ContextMenuItem {
        id: id.to_string(),
        title,
        icon: icon.to_string(),
        checked: false,
        disabled,
        is_label: false,
        is_separator: false,
        has_callback: true,
        owner_plugin: None,
        submenu: None,
    }
}

fn submenu(id: &str, title: String, icon: &str, sub: Vec<ContextMenuItem>) -> ContextMenuItem {
    ContextMenuItem {
        id: id.to_string(),
        title,
        icon: icon.to_string(),
        checked: false,
        disabled: false,
        is_label: false,
        is_separator: false,
        has_callback: false,
        owner_plugin: None,
        submenu: Some(sub),
    }
}

impl AppView {
    /// Opens the editor context menu at `(x, y)` collecting native items and querying plugins.
    pub fn open_editor_context_menu(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        // Close other popup overlays
        self.file_context_menu.is_open = false;
        self.folder_context_menu.is_open = false;
        self.tab_context_menu.is_open = false;
        self.ribbon_context_menu.is_open = false;
        self.close_tabs_popup(cx);

        let session_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let has_selection = self
            .state
            .active_view()
            .and_then(|v| self.state.active_document().and_then(|d| v.selection_range(d.len())))
            .map(|r| r.start < r.end)
            .unwrap_or(false);

        let selected_text = self
            .state
            .active_selected_text()
            .unwrap_or_default();

        let is_ru = self.config.ui.language.starts_with("ru");
        let mut items = Vec::new();

        // 1. Link Group
        items.push(item(
            "editor-insert-link",
            if is_ru { "Вставить ссылку".to_string() } else { "Insert link".to_string() },
            "link",
            false,
        ));
        items.push(item(
            "editor-insert-external-link",
            if is_ru { "Вставить внешнюю ссылку".to_string() } else { "Insert external link".to_string() },
            "external-link",
            false,
        ));

        // Separator
        items.push(separator("sep-links"));

        // 2. Search & Formatting Group
        let search_title = if has_selection {
            let snippet = if selected_text.chars().count() > 20 {
                format!("{}...", selected_text.chars().take(20).collect::<String>())
            } else {
                selected_text.clone()
            };
            if is_ru { format!("Искать \"{}\"", snippet) } else { format!("Search for \"{}\"", snippet) }
        } else {
            if is_ru { "Искать...".to_string() } else { "Search...".to_string() }
        };

        items.push(item(
            "editor-search-vault",
            search_title,
            "search",
            !has_selection,
        ));
        items.push(item(
            "editor-extract-selection",
            if is_ru { "Извлечь текущее выделение...".to_string() } else { "Extract current selection...".to_string() },
            "note",
            !has_selection,
        ));

        // Submenu: Formatting
        let formatting_items = vec![
            item("editor-format-bold", if is_ru { "Жирный".to_string() } else { "Bold".to_string() }, "type", false),
            item("editor-format-italic", if is_ru { "Курсив".to_string() } else { "Italic".to_string() }, "type", false),
            item("editor-format-strikethrough", if is_ru { "Зачеркнутый".to_string() } else { "Strikethrough".to_string() }, "type", false),
            item("editor-format-highlight", if is_ru { "Выделенный".to_string() } else { "Highlight".to_string() }, "pencil", false),
            item("editor-format-code", if is_ru { "Встроенный код".to_string() } else { "Inline code".to_string() }, "code", false),
        ];
        items.push(submenu(
            "editor-submenu-formatting",
            if is_ru { "Форматирование".to_string() } else { "Formatting".to_string() },
            "pencil",
            formatting_items,
        ));

        // Submenu: Paragraph
        let paragraph_items = vec![
            item("editor-paragraph-h1", if is_ru { "Заголовок 1".to_string() } else { "Heading 1".to_string() }, "hash", false),
            item("editor-paragraph-h2", if is_ru { "Заголовок 2".to_string() } else { "Heading 2".to_string() }, "hash", false),
            item("editor-paragraph-h3", if is_ru { "Заголовок 3".to_string() } else { "Heading 3".to_string() }, "hash", false),
            item("editor-paragraph-h4", if is_ru { "Заголовок 4".to_string() } else { "Heading 4".to_string() }, "hash", false),
            item("editor-paragraph-h5", if is_ru { "Заголовок 5".to_string() } else { "Heading 5".to_string() }, "hash", false),
            item("editor-paragraph-h6", if is_ru { "Заголовок 6".to_string() } else { "Heading 6".to_string() }, "hash", false),
            separator("sep-para"),
            item("editor-paragraph-bullet", if is_ru { "Маркированный список".to_string() } else { "Bullet list".to_string() }, "list", false),
            item("editor-paragraph-number", if is_ru { "Нумерованный список".to_string() } else { "Numbered list".to_string() }, "list", false),
            item("editor-paragraph-task", if is_ru { "Список задач".to_string() } else { "Task list".to_string() }, "check", false),
            item("editor-paragraph-quote", if is_ru { "Цитата".to_string() } else { "Quote".to_string() }, "type", false),
        ];
        items.push(submenu(
            "editor-submenu-paragraph",
            if is_ru { "Абзац".to_string() } else { "Paragraph".to_string() },
            "type",
            paragraph_items,
        ));

        // Submenu: Insert
        let insert_items = vec![
            item("editor-insert-callout", if is_ru { "Блок примечания (Callout)".to_string() } else { "Callout block".to_string() }, "type", false),
            item("editor-insert-codeblock", if is_ru { "Блок кода".to_string() } else { "Code block".to_string() }, "code", false),
            item("editor-insert-table", if is_ru { "Таблица".to_string() } else { "Table".to_string() }, "type", false),
            item("editor-insert-math", if is_ru { "Математическая формула".to_string() } else { "Math formula".to_string() }, "type", false),
            item("editor-insert-footnote", if is_ru { "Сноска".to_string() } else { "Footnote".to_string() }, "file-text", false),
        ];
        items.push(submenu(
            "editor-submenu-insert",
            if is_ru { "Вставка".to_string() } else { "Insert".to_string() },
            "plus",
            insert_items,
        ));

        // Separator
        items.push(separator("sep-clipboard"));

        // 3. Clipboard Group
        items.push(item(
            "editor-cut",
            if is_ru { "Вырезать".to_string() } else { "Cut".to_string() },
            "scissors",
            !has_selection,
        ));
        items.push(item(
            "editor-copy",
            if is_ru { "Копировать".to_string() } else { "Copy".to_string() },
            "copy",
            !has_selection,
        ));
        items.push(item(
            "editor-paste",
            if is_ru { "Вставить".to_string() } else { "Paste".to_string() },
            "clipboard",
            false,
        ));
        items.push(item(
            "editor-paste-plain",
            if is_ru { "Вставить как обычный текст".to_string() } else { "Paste as plain text".to_string() },
            "clipboard-list",
            false,
        ));
        items.push(item(
            "editor-select-all",
            if is_ru { "Выбрать всё".to_string() } else { "Select all".to_string() },
            "check",
            false,
        ));

        // Separator
        items.push(separator("sep-tools"));

        // 4. Tools Group
        let text_tools_items = vec![
            item("editor-text-upper", if is_ru { "В ВЕРХНИЙ РЕГИСТР".to_string() } else { "UPPERCASE".to_string() }, "type", !has_selection),
            item("editor-text-lower", if is_ru { "в нижний регистр".to_string() } else { "lowercase".to_string() }, "type", !has_selection),
            item("editor-text-title", if is_ru { "Первая Заглавная".to_string() } else { "Title Case".to_string() }, "type", !has_selection),
            separator("sep-text-sort"),
            item("editor-text-sort", if is_ru { "Сортировать строки".to_string() } else { "Sort lines".to_string() }, "list", !has_selection),
        ];
        items.push(submenu(
            "editor-submenu-text-tools",
            if is_ru { "Текстовые инструменты".to_string() } else { "Text tools".to_string() },
            "type",
            text_tools_items,
        ));

        let ai_tools_items = vec![
            item("editor-ai-explain", if is_ru { "Объяснить".to_string() } else { "Explain".to_string() }, "bolt", !has_selection),
            item("editor-ai-improve", if is_ru { "Улучшить текст".to_string() } else { "Improve writing".to_string() }, "bolt", !has_selection),
            item("editor-ai-grammar", if is_ru { "Исправить грамматику".to_string() } else { "Fix grammar".to_string() }, "bolt", !has_selection),
            item("editor-ai-summary", if is_ru { "Сделать резюме".to_string() } else { "Summarize".to_string() }, "bolt", !has_selection),
        ];
        items.push(submenu(
            "editor-submenu-ai-tools",
            if is_ru { "AI-инструменты".to_string() } else { "AI tools".to_string() },
            "sparkles",
            ai_tools_items,
        ));

        // 5. Query active plugins for editor-menu items
        let registered_plugins = self.state.plugin_host.registered_plugin_ids();

        for p_id in registered_plugins {
            if let Some(rt) = self.plugin_runtimes.iter().find(|r| r.manifest.id == p_id) {
                let _ = rt.host_events_tx.unbounded_send(HostEvent::EditorMenu {
                    menu_session_id: session_id,
                    selected_text: selected_text.clone(),
                });
            }
        }

        self.editor_context_menu = ContextMenuState {
            is_open: true,
            session_id,
            position_x: x,
            position_y: y,
            items,
            active_path: Vec::new(),
        };

        cx.notify();
    }

    /// Appends items received from a plugin to the active context menu.
    pub fn append_plugin_context_menu_items(&mut self, plugin_id: &str, mut plugin_items: Vec<ContextMenuItem>, cx: &mut Context<Self>) {
        if !self.editor_context_menu.is_open || plugin_items.is_empty() {
            return;
        }

        // Add a separator before the first plugin section if not already present
        if !self.editor_context_menu.items.iter().any(|i| i.id.starts_with("plugin-sep-")) {
            self.editor_context_menu.items.push(ContextMenuItem {
                id: format!("plugin-sep-{}", plugin_id),
                title: String::new(),
                icon: String::new(),
                checked: false,
                disabled: false,
                is_label: false,
                is_separator: true,
                has_callback: false,
                owner_plugin: None,
                submenu: None,
            });
        }

        // Tag owner plugin on items and recursive submenus
        fn tag_owner(items: &mut [ContextMenuItem], pid: &str) {
            for it in items {
                it.owner_plugin = Some(pid.to_string());
                if let Some(ref mut sub) = it.submenu {
                    tag_owner(sub, pid);
                }
            }
        }
        tag_owner(&mut plugin_items, plugin_id);

        self.editor_context_menu.items.extend(plugin_items);
        cx.notify();
    }

    /// Closes the editor context menu.
    pub fn close_editor_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.editor_context_menu.is_open {
            self.editor_context_menu.is_open = false;
            self.editor_context_menu.items.clear();
            self.editor_context_menu.active_path.clear();
            cx.notify();
        }
    }

    /// Wraps the current editor selection with prefix and suffix.
    fn wrap_editor_selection(&mut self, prefix: &str, suffix: &str, cx: &mut Context<Self>) {
        let sel = self.state.active_selected_text().unwrap_or_default();
        let repl = format!("{prefix}{sel}{suffix}");
        self.state.active_replace_selection(&repl);
        self.state.save_active_note();
        cx.notify();
    }

    /// Prefixes each selected line (or cursor position) with given prefix.
    fn prefix_editor_selection(&mut self, prefix: &str, cx: &mut Context<Self>) {
        let sel = self.state.active_selected_text().unwrap_or_default();
        let repl = if sel.is_empty() {
            prefix.to_string()
        } else {
            sel.lines()
                .map(|l| format!("{prefix}{l}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        self.state.active_replace_selection(&repl);
        self.state.save_active_note();
        cx.notify();
    }

    /// Handles a clicked item in the context menu.
    pub fn handle_context_menu_action(&mut self, item: &ContextMenuItem, _window: &mut Window, cx: &mut Context<Self>) {
        let action_id = item.id.clone();
        let owner = item.owner_plugin.clone();

        self.close_editor_context_menu(cx);

        match action_id.as_str() {
            "editor-cut" => {
                if let Some(text) = self.state.active_selected_text() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    self.state.active_delete_selection();
                    self.state.save_active_note();
                    cx.notify();
                }
            }
            "editor-copy" => {
                if let Some(text) = self.state.active_selected_text() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                }
            }
            "editor-paste" | "editor-paste-plain" => {
                if let Some(clipboard) = cx.read_from_clipboard() {
                    if let Some(text) = clipboard.text() {
                        self.state.active_replace_selection(&text);
                        self.state.save_active_note();
                        cx.notify();
                    }
                }
            }
            "editor-select-all" => {
                self.state.active_select_all();
                cx.notify();
            }
            "editor-insert-link" => {
                let sel = self.state.active_selected_text().unwrap_or_default();
                let repl = if sel.is_empty() { "[[]]".to_string() } else { format!("[[{sel}]]") };
                self.state.active_replace_selection(&repl);
                self.state.save_active_note();
                cx.notify();
            }
            "editor-insert-external-link" => {
                let sel = self.state.active_selected_text().unwrap_or_default();
                let repl = if sel.is_empty() { "[](url)".to_string() } else { format!("[{sel}](url)") };
                self.state.active_replace_selection(&repl);
                self.state.save_active_note();
                cx.notify();
            }
            "editor-search-vault" => {
                if let Some(sel) = self.state.active_selected_text() {
                    if !sel.trim().is_empty() {
                        self.search_dialog.is_open = true;
                        self.search_dialog.query = sel.clone();
                        self.search_dialog.cursor_pos = sel.len();
                        self.run_goto_search();
                        cx.notify();
                    }
                }
            }
            "editor-extract-selection" => {
                if let Some(sel) = self.state.active_selected_text() {
                    if !sel.trim().is_empty() {
                        self.state.active_delete_selection();
                        self.state.save_active_note();
                        if let Some(_path) = self.state.create_new_note() {
                            self.state.active_replace_selection(&sel);
                            self.state.save_active_note();
                        }
                        cx.notify();
                    }
                }
            }
            "editor-format-bold" => {
                self.wrap_editor_selection("**", "**", cx);
            }
            "editor-format-italic" => {
                self.wrap_editor_selection("*", "*", cx);
            }
            "editor-format-strikethrough" => {
                self.wrap_editor_selection("~~", "~~", cx);
            }
            "editor-format-highlight" => {
                self.wrap_editor_selection("==", "==", cx);
            }
            "editor-format-code" => {
                self.wrap_editor_selection("`", "`", cx);
            }
            "editor-paragraph-h1" => self.prefix_editor_selection("# ", cx),
            "editor-paragraph-h2" => self.prefix_editor_selection("## ", cx),
            "editor-paragraph-h3" => self.prefix_editor_selection("### ", cx),
            "editor-paragraph-h4" => self.prefix_editor_selection("#### ", cx),
            "editor-paragraph-h5" => self.prefix_editor_selection("##### ", cx),
            "editor-paragraph-h6" => self.prefix_editor_selection("###### ", cx),
            "editor-paragraph-bullet" => self.prefix_editor_selection("- ", cx),
            "editor-paragraph-number" => self.prefix_editor_selection("1. ", cx),
            "editor-paragraph-task" => self.prefix_editor_selection("- [ ] ", cx),
            "editor-paragraph-quote" => self.prefix_editor_selection("> ", cx),
            "editor-insert-callout" => {
                self.state.active_replace_selection("\n> [!NOTE]\n> \n");
                self.state.save_active_note();
                cx.notify();
            }
            "editor-insert-codeblock" => {
                self.state.active_replace_selection("\n```\n\n```\n");
                self.state.save_active_note();
                cx.notify();
            }
            "editor-insert-table" => {
                let table = "\n| Заголовок 1 | Заголовок 2 |\n| --- | --- |\n| Текст 1 | Текст 2 |\n";
                self.state.active_replace_selection(table);
                self.state.save_active_note();
                cx.notify();
            }
            "editor-insert-math" => {
                self.wrap_editor_selection("$$", "$$", cx);
            }
            "editor-insert-footnote" => {
                self.state.active_replace_selection("[^1]");
                self.state.save_active_note();
                cx.notify();
            }
            "editor-text-upper" => {
                if let Some(sel) = self.state.active_selected_text() {
                    let upper = sel.to_uppercase();
                    self.state.active_replace_selection(&upper);
                    self.state.save_active_note();
                    cx.notify();
                }
            }
            "editor-text-lower" => {
                if let Some(sel) = self.state.active_selected_text() {
                    let lower = sel.to_lowercase();
                    self.state.active_replace_selection(&lower);
                    self.state.save_active_note();
                    cx.notify();
                }
            }
            "editor-text-title" => {
                if let Some(sel) = self.state.active_selected_text() {
                    let title = sel
                        .split_whitespace()
                        .map(|word| {
                            let mut chars = word.chars();
                            match chars.next() {
                                None => String::new(),
                                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    self.state.active_replace_selection(&title);
                    self.state.save_active_note();
                    cx.notify();
                }
            }
            "editor-text-sort" => {
                if let Some(sel) = self.state.active_selected_text() {
                    let mut lines: Vec<&str> = sel.lines().collect();
                    lines.sort();
                    let sorted = lines.join("\n");
                    self.state.active_replace_selection(&sorted);
                    self.state.save_active_note();
                    cx.notify();
                }
            }
            _ => {
                // If it belongs to a plugin, dispatch action callback
                if let Some(pid) = owner {
                    if let Some(rt) = self.plugin_runtimes.iter().find(|r| r.manifest.id == pid) {
                        let _ = rt.host_events_tx.unbounded_send(HostEvent::ExecuteMenuAction {
                            action_id,
                        });
                    }
                }
            }
        }
    }

    /// Renders the editor context menu overlay with cascading submenus.
    pub fn render_editor_context_menu(&self, theme: &Theme, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        if !self.editor_context_menu.is_open {
            return div().into_any_element();
        }

        let menu_x = self.editor_context_menu.position_x;
        let menu_y = self.editor_context_menu.position_y;
        let active_path = self.editor_context_menu.active_path.clone();

        div()
            .id("editor-context-menu-backdrop")
            .absolute()
            .inset_0()
            // Backdrop dismissal
            .child(
                div()
                    .id("editor-context-backdrop-area")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.close_editor_context_menu(cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                        this.close_editor_context_menu(cx);
                    }))
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    }),
            )
            // Primary root menu popup
            .child(self.render_menu_panel(
                &self.editor_context_menu.items,
                menu_x,
                menu_y,
                Vec::new(),
                &active_path,
                theme,
                window,
                cx,
            ))
            .into_any_element()
    }

    /// Helper to compute exact total height of a slice of context menu items.
    fn calculate_menu_items_height(items: &[ContextMenuItem]) -> f32 {
        let content_h: f32 = items.iter().map(|it| {
            if it.is_separator {
                9.0
            } else if it.is_label {
                22.0
            } else {
                28.0
            }
        }).sum();
        content_h + 8.0 // 4px top + 4px bottom padding (.p_1())
    }

    /// Recursively renders a menu panel and its open submenu child panel (if any).
    fn render_menu_panel(
        &self,
        items: &[ContextMenuItem],
        panel_x: f32,
        panel_y: f32,
        current_path: Vec<usize>,
        active_path: &[usize],
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> Div {
        let vp_size = window.viewport_size();
        let vp_w: f32 = vp_size.width.into();
        let vp_h: f32 = vp_size.height.into();

        // Calculate dynamic width based on the longest item title
        let max_chars = items.iter().map(|it| it.title.chars().count()).max().unwrap_or(20);
        let menu_w: f32 = (max_chars as f32 * 8.2 + 76.0).max(280.0).min(420.0);
        let panel_h = Self::calculate_menu_items_height(items);

        let clamped_x = panel_x.min(vp_w - menu_w - 12.0).max(8.0);
        let clamped_y = if panel_y + panel_h > vp_h - 12.0 {
            (vp_h - panel_h - 12.0).max(8.0)
        } else {
            panel_y.max(8.0)
        };

        let current_depth = current_path.len();
        let active_child_index = if active_path.len() > current_depth
            && active_path[..current_depth] == current_path[..]
        {
            Some(active_path[current_depth])
        } else {
            None
        };

        let mut panel = div()
            .id(gpui::ElementId::from(gpui::SharedString::from(format!("ctx-menu-panel-{:?}", current_path))))
            .absolute()
            .left(px(clamped_x))
            .top(px(clamped_y))
            .w(px(menu_w))
            .max_h(px((vp_h - clamped_y - 12.0).max(120.0)))
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
            });

        for (idx, item) in items.iter().enumerate() {
            if item.is_separator {
                panel = panel.child(
                    div()
                        .my_1()
                        .mx_1()
                        .h(px(1.0))
                        .bg(theme.border_subtle),
                );
                continue;
            }

            if item.is_label {
                panel = panel.child(
                    div()
                        .h(px(22.0))
                        .flex()
                        .items_center()
                        .px_2p5()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_muted)
                        .child(item.title.clone()),
                );
                continue;
            }

            let mut item_path = current_path.clone();
            item_path.push(idx);

            let is_hovered = active_child_index == Some(idx);
            let has_sub = item.submenu.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
            let is_disabled = item.disabled;

            let row_item = item.clone();
            let hover_path = item_path.clone();

            let row = div()
                .id(gpui::ElementId::from(gpui::SharedString::from(format!("ctx-item-{:?}", item_path))))
                .h(px(28.0))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_2p5()
                .rounded_md()
                .bg(if is_hovered { theme.bg_hover } else { rgba(0x00000000) })
                .when(!is_disabled, |d| {
                    d.cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover))
                        .on_hover(cx.listener({
                            let p = hover_path.clone();
                            move |this, hovered: &bool, _, cx| {
                                if *hovered {
                                    this.editor_context_menu.active_path = p.clone();
                                    cx.notify();
                                }
                            }
                        }))
                        .on_click(cx.listener({
                            let it = row_item.clone();
                            move |this, _, window, cx| {
                                if !it.submenu.as_ref().map(|s| !s.is_empty()).unwrap_or(false) {
                                    this.handle_context_menu_action(&it, window, cx);
                                }
                            }
                        }))
                })
                .when(is_disabled, |d| d.opacity(0.45).cursor_not_allowed())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .flex_1()
                        .min_w_0()
                        .child({
                            let icon_char = if !item.icon.is_empty() {
                                crate::icons::map_obsidian_icon(&item.icon)
                            } else {
                                icons::ICON_FILE_TEXT
                            };
                            icons::icon(icon_char, 13.0, if is_disabled { theme.text_muted } else { theme.text_secondary })
                        })
                        .child(
                            div()
                                .text_sm()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_color(if is_disabled { theme.text_muted } else { theme.text_primary })
                                .child(item.title.clone()),
                        ),
                )
                .when(has_sub, |d| {
                    d.child(
                        div()
                            .ml_2()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .child(icons::icon(
                                icons::ICON_CHEVRON_RIGHT,
                                11.0,
                                if is_disabled { theme.text_muted } else { theme.text_secondary },
                            )),
                    )
                });

            panel = panel.child(row);
        }

        let mut root_container = div().child(panel);

        // If an active item has a submenu, render the child panel cascading to the right (or left if near screen edge)
        if let Some(child_idx) = active_child_index {
            if let Some(child_item) = items.get(child_idx) {
                if let Some(ref sub_items) = child_item.submenu {
                    if !sub_items.is_empty() {
                        let child_max_chars = sub_items.iter().map(|it| it.title.chars().count()).max().unwrap_or(18);
                        let child_w = (child_max_chars as f32 * 8.2 + 76.0).max(260.0).min(400.0);
                        let child_h = Self::calculate_menu_items_height(sub_items);

                        // Vertical position of hovered parent item inside panel
                        let item_offset_y: f32 = 4.0 + items[..child_idx].iter().map(|it| {
                            if it.is_separator {
                                9.0
                            } else if it.is_label {
                                22.0
                            } else {
                                28.0
                            }
                        }).sum::<f32>();

                        // Align first item of submenu directly with hovered parent item row
                        let ideal_sub_y = clamped_y + item_offset_y - 4.0;
                        let sub_y = if ideal_sub_y + child_h > vp_h - 12.0 {
                            (vp_h - child_h - 12.0).max(8.0)
                        } else {
                            ideal_sub_y.max(8.0)
                        };

                        let submenu_gap = 6.0;
                        let submenu_on_left = clamped_x + menu_w + child_w + submenu_gap + 12.0 > vp_w;
                        let sub_x = if submenu_on_left {
                            (clamped_x - child_w - submenu_gap).max(8.0)
                        } else {
                            clamped_x + menu_w + submenu_gap
                        };

                        let mut next_path = current_path.clone();
                        next_path.push(child_idx);

                        // Invisible bridge connecting parent item and submenu across the gap
                        let bridge = div()
                            .id(gpui::ElementId::from(gpui::SharedString::from(format!("ctx-bridge-{:?}", next_path))))
                            .absolute()
                            .top(px(ideal_sub_y.min(sub_y)))
                            .h(px((child_h + (sub_y - ideal_sub_y).abs()).max(28.0)))
                            .when(!submenu_on_left, |d| {
                                d.left(px(clamped_x + menu_w - 2.0)).w(px(submenu_gap + 4.0))
                            })
                            .when(submenu_on_left, |d| {
                                d.left(px(sub_x + child_w - 2.0)).w(px(submenu_gap + 4.0))
                            })
                            .on_hover(cx.listener({
                                let p = next_path.clone();
                                move |this, hovered: &bool, _, cx| {
                                    if *hovered {
                                        this.editor_context_menu.active_path = p.clone();
                                        cx.notify();
                                    }
                                }
                            }));

                        let sub_panel = self.render_menu_panel(
                            sub_items,
                            sub_x,
                            sub_y,
                            next_path,
                            active_path,
                            theme,
                            window,
                            cx,
                        );

                        root_container = root_container.child(bridge).child(sub_panel);
                    }
                }
            }
        }

        root_container
    }
}
