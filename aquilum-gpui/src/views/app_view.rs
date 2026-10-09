use crate::state::AppState;
use crate::theme::Theme;
use aquilum_core::files::models::FileItemType;
use gpui::{
    div, prelude::*, px, rgba, ClipboardItem, Context, FocusHandle, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, Render, Window,
};
use std::path::PathBuf;

pub struct AppView {
    pub state: AppState,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
    pub edit_mode: bool,
    pub is_mouse_selecting: bool,
}

impl AppView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            state: AppState::new(),
            theme: Theme::default(),
            focus_handle: cx.focus_handle(),
            edit_mode: true,
            is_mouse_selecting: false,
        }
    }

    fn open_folder_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            self.state.open_workspace(folder);
            cx.notify();
        }
    }

    fn select_note(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.state.open_note(path);
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn reload_files(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.reload_files();
        cx.notify();
    }

    fn save_note(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.save_active_note();
        cx.notify();
    }

    fn handle_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let ctrl = keystroke.modifiers.control;
        let shift = keystroke.modifiers.shift;

        // Ctrl + S (Сохранение)
        if ctrl && keystroke.key.eq_ignore_ascii_case("s") {
            self.state.save_active_note();
            cx.notify();
            return;
        }

        // Ctrl + Z (Undo)
        if ctrl && !shift && keystroke.key.eq_ignore_ascii_case("z") {
            if let Some(ref mut note) = self.state.active_note {
                note.undo();
                cx.notify();
            }
            return;
        }

        // Ctrl + Y или Ctrl + Shift + Z (Redo)
        if (ctrl && keystroke.key.eq_ignore_ascii_case("y"))
            || (ctrl && shift && keystroke.key.eq_ignore_ascii_case("z"))
        {
            if let Some(ref mut note) = self.state.active_note {
                note.redo();
                cx.notify();
            }
            return;
        }

        // Ctrl + A (Выделить всё)
        if ctrl && keystroke.key.eq_ignore_ascii_case("a") {
            if let Some(ref mut note) = self.state.active_note {
                note.select_all();
                cx.notify();
            }
            return;
        }

        // Ctrl + C (Копировать)
        if ctrl && keystroke.key.eq_ignore_ascii_case("c") {
            if let Some(ref note) = self.state.active_note {
                if let Some(text) = note.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            return;
        }

        // Ctrl + X (Вырезать)
        if ctrl && keystroke.key.eq_ignore_ascii_case("x") {
            if let Some(ref mut note) = self.state.active_note {
                if let Some(text) = note.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    note.delete_selection();
                    cx.notify();
                }
            }
            return;
        }

        // Ctrl + V (Вставить)
        if ctrl && keystroke.key.eq_ignore_ascii_case("v") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    if let Some(ref mut note) = self.state.active_note {
                        note.insert_text(&text);
                        cx.notify();
                    }
                }
            }
            return;
        }

        let Some(ref mut note) = self.state.active_note else {
            return;
        };

        match keystroke.key.as_str() {
            "backspace" => note.delete_backwards(),
            "delete" => note.delete_forwards(),
            "enter" => note.insert_newline_auto_indent(),
            "tab" => note.insert_text("    "),
            "space" => note.insert_text(" "),
            "left" => note.move_cursor_left(shift),
            "right" => note.move_cursor_right(shift),
            "up" => note.move_cursor_up(shift),
            "down" => note.move_cursor_down(shift),
            "home" => note.home(shift),
            "end" => note.end(shift),
            _ => {
                if !ctrl && !keystroke.modifiers.alt && !keystroke.modifiers.platform {
                    if let Some(ch) = &keystroke.key_char {
                        note.insert_text(ch);
                    } else if keystroke.key.chars().count() == 1 {
                        note.insert_text(&keystroke.key);
                    }
                }
            }
        }
        cx.notify();
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let active_path = self.state.active_note.as_ref().map(|n| n.path.clone());
        let is_editing = self.edit_mode;

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_app)
            .text_color(theme.text_primary)
            // ================= HEADER =================
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(46.0))
                    .px_4()
                    .bg(theme.bg_sidebar)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .bg(theme.accent)
                                            .text_color(theme.bg_sidebar)
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .rounded_md()
                                            .text_xs()
                                            .child("AQUILUM"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .child("NATIVE GPUI EDITOR"),
                                    ),
                            )
                            .child(
                                div()
                                    .h(px(16.0))
                                    .w_px()
                                    .bg(theme.border),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .text_sm()
                                    .text_color(theme.text_secondary)
                                    .child("📁")
                                    .child(self.state.workspace_name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("btn-open-folder")
                                    .px_3()
                                    .py_1()
                                    .bg(theme.bg_card)
                                    .hover(|s| s.bg(theme.bg_hover))
                                    .border_1()
                                    .border_color(theme.border)
                                    .rounded_md()
                                    .text_xs()
                                    .cursor_pointer()
                                    .child("📂 Open Folder")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_folder_dialog(window, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id("btn-reload")
                                    .px_2()
                                    .py_1()
                                    .bg(theme.bg_card)
                                    .hover(|s| s.bg(theme.bg_hover))
                                    .border_1()
                                    .border_color(theme.border)
                                    .rounded_md()
                                    .text_xs()
                                    .cursor_pointer()
                                    .child("🔄 Reload")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.reload_files(window, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .bg(theme.bg_card)
                                    .border_1()
                                    .border_color(theme.success)
                                    .text_color(theme.success)
                                    .rounded_md()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("⚡ 0 WebView • 120 FPS"),
                            ),
                    ),
            )
            // ================= MAIN BODY =================
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_row()
                    .size_full()
                    .overflow_hidden()
                    // ----- Сайдбар файлов -----
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w(px(260.0))
                            .h_full()
                            .bg(theme.bg_sidebar)
                            .border_r_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .px_4()
                                    .py_2()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.text_muted)
                                    .child(format!("NOTES & FILES ({})", self.state.files.len())),
                            )
                            .child(
                                div()
                                    .id("sidebar-file-list")
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .overflow_scroll()
                                    .px_2()
                                    .gap_0p5()
                                    .children(self.state.files.iter().map(|item| {
                                        let item_path = PathBuf::from(&item.id);
                                        let is_active = active_path
                                            .as_ref()
                                            .map(|p| p == &item_path)
                                            .unwrap_or(false);

                                        let icon = match item.item_type {
                                            FileItemType::Folder => "📁 ",
                                            FileItemType::File => "📝 ",
                                        };

                                        let target_path = item_path.clone();
                                        let is_folder = item.item_type == FileItemType::Folder;

                                        div()
                                            .id(gpui::SharedString::from(item.id.clone()))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .text_sm()
                                            .cursor_pointer()
                                            .when(is_active, |s| {
                                                s.bg(theme.bg_selected)
                                                    .text_color(theme.accent)
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                            })
                                            .when(!is_active, |s| {
                                                s.hover(|h| h.bg(theme.bg_hover))
                                                    .text_color(if is_folder {
                                                        theme.text_secondary
                                                    } else {
                                                        theme.text_primary
                                                    })
                                            })
                                            .child(icon)
                                            .child(item.name.clone())
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                if !is_folder {
                                                    this.select_note(target_path.clone(), window, cx);
                                                }
                                            }))
                                    })),
                            ),
                    )
                    // ----- Центральная область редактора -----
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .h_full()
                            .bg(theme.bg_editor)
                            .when_some(self.state.active_note.as_ref(), |this, note| {
                                let lines: Vec<&str> = note.content.split('\n').collect();
                                let (cursor_line, cursor_col) = note.line_col();
                                let words: usize = note.content.split_whitespace().count();
                                let chars = note.content.chars().count();
                                let is_dirty = note.is_dirty;
                                let selection_range = note.selection_range();

                                // Вычисляем смещения начала каждой строки
                                let mut line_starts: Vec<usize> = Vec::with_capacity(lines.len());
                                let mut running_offset = 0;
                                for line in &lines {
                                    line_starts.push(running_offset);
                                    running_offset += line.len() + 1;
                                }

                                let code_states = crate::markdown::analyze_code_blocks(&lines);
                                let active_block_id: Option<usize> = if is_editing {
                                    code_states.get(cursor_line).and_then(|s| s.as_ref()).map(|s| s.block_id)
                                } else {
                                    None
                                };
                                let font_id = cx.text_system().resolve_font(&gpui::font("Consolas"));
                                let line_char_width = cx
                                    .text_system()
                                    .advance(font_id, px(14.0), 'x')
                                    .map(|s| f32::from(s.width))
                                    .unwrap_or(8.42);
                                let line_h = px(24.0);
                                let cursor_h = px(16.0);
                                let cursor_top = 4.0;
                                let text_start_x = 341.0;

                                this.child(
                                    // Верхняя панель открытой заметки
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .px_8()
                                        .py_3()
                                        .border_b_1()
                                        .border_color(theme.border_subtle)
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .text_2xl()
                                                        .font_weight(gpui::FontWeight::BOLD)
                                                        .text_color(theme.text_primary)
                                                        .child(note.title.clone()),
                                                )
                                                .when(is_dirty, |s| {
                                                    s.child(
                                                        div()
                                                            .text_sm()
                                                            .text_color(theme.accent)
                                                            .child("● unsaved"),
                                                    )
                                                }),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_3()
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(theme.text_muted)
                                                        .child(format!("{} words • {} chars", words, chars)),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-toggle-mode")
                                                        .px_3()
                                                        .py_1()
                                                        .bg(if is_editing { theme.bg_selected } else { theme.bg_card })
                                                        .hover(|s| s.bg(theme.bg_hover))
                                                        .border_1()
                                                        .border_color(theme.border)
                                                        .rounded_md()
                                                        .text_xs()
                                                        .cursor_pointer()
                                                        .child(if is_editing { "✏️ Edit Mode" } else { "👁️ Preview Mode" })
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.edit_mode = !this.edit_mode;
                                                            cx.notify();
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-save-note")
                                                        .px_3()
                                                        .py_1()
                                                        .bg(theme.accent)
                                                        .text_color(theme.bg_sidebar)
                                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                                        .rounded_md()
                                                        .text_xs()
                                                        .cursor_pointer()
                                                        .child("💾 Save (Ctrl+S)")
                                                        .on_click(cx.listener(|this, _, window, cx| {
                                                            this.save_note(window, cx);
                                                        })),
                                                ),
                                        ),
                                )
                                .child(
                                    // Область редактора
                                    div()
                                        .id("note-editor-container")
                                        .track_focus(&self.focus_handle)
                                        .cursor(gpui::CursorStyle::IBeam)
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .overflow_scroll()
                                        .px_8()
                                        .py_6()
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                            window.focus(&this.focus_handle);
                                            cx.notify();
                                        }))
                                        .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                            this.is_mouse_selecting = false;
                                            if let Some(ref mut n) = this.state.active_note {
                                                if n.selection_anchor == Some(n.cursor_offset) {
                                                    n.selection_anchor = None;
                                                }
                                            }
                                            cx.notify();
                                        }))
                                        .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                            this.is_mouse_selecting = false;
                                            cx.notify();
                                        }))
                                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                            this.handle_key(event, window, cx);
                                        }))
                                        .children(lines.into_iter().enumerate().map(|(line_idx, line)| {
                                                let is_current = is_editing && (line_idx == cursor_line);
                                                let code_state = code_states[line_idx].as_ref().map(|s| (s.kind, s.lang.as_deref()));
                                                let classified = crate::markdown::classify_line(line, code_state);
                                                let is_opening_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: true, .. });
                                                let is_closing_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: false, .. });
                                                let is_code_line = matches!(classified, crate::markdown::LineKind::CodeLine(_));
                                                let is_in_active_block = code_states[line_idx].as_ref().map(|s| Some(s.block_id) == active_block_id).unwrap_or(false);
                                                let char_count = line.chars().count();

                                                let line_number = line_idx + 1;
                                                let line_start = line_starts[line_idx];
                                                let line_end = line_start + line.len();

                                                // Проверяем, пересекается ли выделение с этой строкой (в символах, а не байтах)
                                                let line_sel_chars = selection_range.as_ref().and_then(|sel| {
                                                    if sel.end > line_start && sel.start < line_end {
                                                        let byte_s = sel.start.saturating_sub(line_start).min(line.len());
                                                        let byte_e = sel.end.saturating_sub(line_start).min(line.len());
                                                        if byte_s < byte_e {
                                                            let char_s = line[..byte_s].chars().count();
                                                            let char_e = char_s + line[byte_s..byte_e].chars().count();
                                                            Some((char_s, char_e))
                                                        } else {
                                                            None
                                                        }
                                                    } else {
                                                        None
                                                    }
                                                });

                                                div()
                                                    .id(line_idx)
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .h(line_h)
                                                    .py_0()
                                                    .px_2()
                                                    .rounded_sm()
                                                    .when(is_current, |s| s.bg(theme.bg_hover))
                                                    // Номер строки (клик по нему ставит курсор в начало строки)
                                                    .child(
                                                        div()
                                                            .id(gpui::ElementId::from(("line-num", line_idx)))
                                                            .w(px(40.0))
                                                            .h(line_h)
                                                            .flex()
                                                            .items_center()
                                                            .text_xs()
                                                            .text_color(if is_current { theme.accent } else { theme.text_muted })
                                                            .font_family("Consolas")
                                                            .cursor_pointer()
                                                            .child(format!("{:>3} ", line_number))
                                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                                if let Some(ref mut n) = this.state.active_note {
                                                                    n.set_cursor_to_line_start(line_idx);
                                                                }
                                                                window.focus(&this.focus_handle);
                                                                cx.notify();
                                                            })),
                                                    )
                                                    // Текст строки с попиксельным позиционированием клика мыши и drag-to-select
                                                    .child(
                                                        div()
                                                            .id(gpui::ElementId::from(("line-content", line_idx)))
                                                            .relative()
                                                            .flex()
                                                            .flex_row()
                                                            .items_center()
                                                            .flex_1()
                                                            .h(line_h)
                                                            .py_0()
                                                            .font_family("Consolas")
                                                            .text_sm()
                                                            .cursor(gpui::CursorStyle::IBeam)
                                                            .when(is_opening_fence, |s| s.bg(theme.bg_card).rounded_t_sm())
                                                            .when(is_closing_fence, |s| s.bg(theme.bg_card).rounded_b_sm())
                                                            .when(is_code_line, |s| s.bg(theme.code_bg))
                                                            // Обработка начала выделения / клика мыши
                                                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                                this.is_mouse_selecting = true;
                                                                let click_x: f32 = event.position.x.into();
                                                                let col = if click_x > text_start_x {
                                                                    (((click_x - text_start_x) / line_char_width).round() as usize).min(char_count)
                                                                } else {
                                                                    0
                                                                };
                                                                if let Some(ref mut n) = this.state.active_note {
                                                                    let offset = n.offset_for_line_col(line_idx, col);
                                                                    if event.modifiers.shift {
                                                                        if n.selection_anchor.is_none() {
                                                                            n.selection_anchor = Some(n.cursor_offset);
                                                                        }
                                                                    } else {
                                                                        n.selection_anchor = Some(offset);
                                                                    }
                                                                    n.cursor_offset = offset;
                                                                }
                                                                window.focus(&this.focus_handle);
                                                                cx.notify();
                                                            }))
                                                            // Обработка протягивания мыши (Drag to Select)
                                                            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                                                                if this.is_mouse_selecting {
                                                                    let click_x: f32 = event.position.x.into();
                                                                    let col = if click_x > text_start_x {
                                                                        (((click_x - text_start_x) / line_char_width).round() as usize).min(char_count)
                                                                    } else {
                                                                        0
                                                                    };
                                                                    if let Some(ref mut n) = this.state.active_note {
                                                                        let offset = n.offset_for_line_col(line_idx, col);
                                                                        n.cursor_offset = offset;
                                                                        cx.notify();
                                                                    }
                                                                }
                                                            }))
                                                            // 1. Фоновый слой выделения (не ломает верстку строки)
                                                            .when_some(line_sel_chars, |s, (sel_s, sel_e)| {
                                                                let sel_x = (sel_s as f32) * line_char_width;
                                                                let sel_w = ((sel_e - sel_s) as f32) * line_char_width;
                                                                s.child(
                                                                    div()
                                                                        .absolute()
                                                                        .left(px(sel_x))
                                                                        .top_0()
                                                                        .h_full()
                                                                        .w(px(sel_w))
                                                                        .bg(rgba(0x89b4fa44))
                                                                        .rounded_xs(),
                                                                )
                                                            })
                                                            // 2. Слой текста (строка рендерится целиком, не разрезается)
                                                            .child(
                                                                div()
                                                                    .h_full()
                                                                    .flex()
                                                                    .flex_row()
                                                                    .items_center()
                                                                    .child(render_live_styled_line(line, &classified, is_in_active_block, theme))
                                                            )
                                                            // 3. Слой курсора (absolute overlay, никогда не раздвигает буквы в словах)
                                                            .when(is_current, |s| {
                                                                let char_count = line.chars().count();
                                                                let col = cursor_col.min(char_count);
                                                                let cursor_x = (col as f32) * line_char_width;
                                                                s.child(
                                                                    div()
                                                                        .absolute()
                                                                        .left(px(cursor_x))
                                                                        .top(px(cursor_top))
                                                                        .w(px(2.0))
                                                                        .h(cursor_h)
                                                                        .bg(theme.accent)
                                                                        .rounded_xs(),
                                                                )
                                                            })
                                                    )
                                            })),
                                )
                            })
                            .when(self.state.active_note.is_none(), |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .size_full()
                                        .justify_center()
                                        .items_center()
                                        .gap_3()
                                        .child(
                                            div()
                                                .text_3xl()
                                                .child("🦅"),
                                        )
                                        .child(
                                            div()
                                                .text_xl()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(theme.text_primary)
                                                .child("Welcome to Aquilum Native"),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(theme.text_muted)
                                                .child("Select a note from the sidebar or click 'Open Folder' to choose a workspace."),
                                        ),
                                )
                            }),
                    ),
            )
            // ================= FOOTER / STATUS BAR =================
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(28.0))
                    .px_4()
                    .bg(theme.bg_sidebar)
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(self.state.status_text.clone())
                    .child(
                        div()
                            .flex()
                            .gap_4()
                            .when_some(self.state.active_note.as_ref(), |s, note| {
                                let (line, col) = note.line_col();
                                s.child(format!("Ln {}, Col {}", line + 1, col + 1))
                            })
                            .child("SQLite WAL: Connected")
                            .child("Tantivy: Ready")
                            .child("GPUI DirectX: 120 FPS"),
                    ),
            )
    }
}

fn render_syntax_spans(spans: Vec<crate::markdown::SyntaxSpan>, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .children(spans.into_iter().map(|span| {
            match span {
                crate::markdown::SyntaxSpan::Marker(m) => {
                    div().text_color(theme.text_muted).child(m)
                }
                crate::markdown::SyntaxSpan::Text(t) => {
                    div().text_color(theme.text_primary).child(t)
                }
                crate::markdown::SyntaxSpan::Bold(b) => {
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .child(b)
                }
                crate::markdown::SyntaxSpan::Italic(i) => {
                    div()
                        .italic()
                        .text_color(theme.text_primary)
                        .child(i)
                }
                crate::markdown::SyntaxSpan::BoldItalic(bi) => {
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .italic()
                        .text_color(theme.text_primary)
                        .child(bi)
                }
                crate::markdown::SyntaxSpan::InlineCode(c) => {
                    div()
                        .text_color(theme.success)
                        .bg(theme.code_bg)
                        .rounded_xs()
                        .child(c)
                }
                crate::markdown::SyntaxSpan::Strikethrough(s) => {
                    div()
                        .line_through()
                        .text_color(theme.text_muted)
                        .child(s)
                }
            }
        }))
}

fn render_live_styled_line(
    line: &str,
    classified: &crate::markdown::LineKind<'_>,
    show_fences: bool,
    theme: &Theme,
) -> gpui::Div {
    if line.is_empty() {
        return div().h_full().flex().items_center().child(" ");
    }

    match classified {
        crate::markdown::LineKind::Heading1(_) => {
            let (prefix, rest) = if let Some(stripped) = line.strip_prefix("# ") {
                ("# ", stripped)
            } else {
                ("", line)
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(div().text_color(theme.text_muted).child(prefix.to_string()))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.accent)
                        .child(render_syntax_spans(crate::markdown::parse_inline_syntax_spans(rest), theme))
                )
        }
        crate::markdown::LineKind::Heading2(_) => {
            let (prefix, rest) = if let Some(stripped) = line.strip_prefix("## ") {
                ("## ", stripped)
            } else {
                ("", line)
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(div().text_color(theme.text_muted).child(prefix.to_string()))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.accent_hover)
                        .child(render_syntax_spans(crate::markdown::parse_inline_syntax_spans(rest), theme))
                )
        }
        crate::markdown::LineKind::Heading3(_) => {
            let (prefix, rest) = if let Some(stripped) = line.strip_prefix("### ") {
                ("### ", stripped)
            } else {
                ("", line)
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(div().text_color(theme.text_muted).child(prefix.to_string()))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(render_syntax_spans(crate::markdown::parse_inline_syntax_spans(rest), theme))
                )
        }
        crate::markdown::LineKind::Heading4(_) => {
            let (prefix, rest) = if let Some(stripped) = line.strip_prefix("#### ") {
                ("#### ", stripped)
            } else {
                ("", line)
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(div().text_color(theme.text_muted).child(prefix.to_string()))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_secondary)
                        .child(render_syntax_spans(crate::markdown::parse_inline_syntax_spans(rest), theme))
                )
        }
        crate::markdown::LineKind::Quote(_) => {
            let (prefix, rest) = if let Some(stripped) = line.strip_prefix("> ") {
                ("> ", stripped)
            } else {
                ("", line)
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .italic()
                .child(div().text_color(theme.accent).child(prefix.to_string()))
                .child(div().text_color(theme.text_secondary).child(render_syntax_spans(crate::markdown::parse_inline_syntax_spans(rest), theme)))
        }
        crate::markdown::LineKind::Task { checked, .. } => {
            let prefix_len = if line.starts_with("- [ ] ")
                || line.starts_with("* [ ] ")
                || line.starts_with("- [x] ")
                || line.starts_with("- [X] ")
                || line.starts_with("* [x] ")
                || line.starts_with("* [X] ")
            {
                6
            } else {
                0
            };
            let (prefix, rest) = line.split_at(prefix_len);
            let prefix_color = if *checked { theme.success } else { theme.text_muted };
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(div().font_weight(gpui::FontWeight::BOLD).text_color(prefix_color).child(prefix.to_string()))
                .child(
                    div()
                        .when(*checked, |s| s.text_color(theme.text_muted).line_through())
                        .child(render_syntax_spans(crate::markdown::parse_inline_syntax_spans(rest), theme))
                )
        }
        crate::markdown::LineKind::BulletList { .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            let (bullet, rest) = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
                (&trimmed[..2], &trimmed[2..])
            } else {
                ("", trimmed)
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(div().child(indent.to_string()))
                .child(div().font_weight(gpui::FontWeight::BOLD).text_color(theme.accent).child(bullet.to_string()))
                .child(render_syntax_spans(crate::markdown::parse_inline_syntax_spans(rest), theme))
        }
        crate::markdown::LineKind::NumberedList { num, .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            let num_prefix_len = num.len() + 2;
            let (num_part, rest) = if trimmed.len() >= num_prefix_len {
                (&trimmed[..num_prefix_len], &trimmed[num_prefix_len..])
            } else {
                ("", trimmed)
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(div().child(indent.to_string()))
                .child(div().font_weight(gpui::FontWeight::BOLD).text_color(theme.accent).child(num_part.to_string()))
                .child(render_syntax_spans(crate::markdown::parse_inline_syntax_spans(rest), theme))
        }
        crate::markdown::LineKind::HorizontalRule => {
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .text_color(theme.border)
                .child(line.to_string())
        }
        crate::markdown::LineKind::CodeFence { is_opening, lang } => {
            if show_fences {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.accent)
                    .child(line.to_string())
            } else if *is_opening {
                let lang_label = lang.unwrap_or("CODE").to_uppercase();
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .h_full()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.accent)
                                    .child(lang_label)
                            )
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child("‹/›")
                    )
            } else {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .h_full()
                    .child(
                        div()
                            .h_px()
                            .w_full()
                            .bg(theme.border_subtle)
                    )
            }
        }
        crate::markdown::LineKind::CodeLine(_) => {
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .text_color(theme.success)
                .child(line.to_string())
        }
        crate::markdown::LineKind::Paragraph(_) => {
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(render_syntax_spans(crate::markdown::parse_inline_syntax_spans(line), theme))
        }
    }
}

