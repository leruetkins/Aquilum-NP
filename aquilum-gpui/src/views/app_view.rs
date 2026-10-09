use crate::icons;
use crate::state::{AppState, FileNode};
use crate::theme::Theme;
use gpui::{
    div, prelude::*, px, rgba, AnyElement, ClipboardItem, Context, FocusHandle, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Render, SharedString, Window,
};
use std::path::PathBuf;

pub struct AppView {
    pub state: AppState,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
    pub search_focus_handle: FocusHandle,
    pub edit_mode: bool,
    pub is_mouse_selecting: bool,
    pub editor_top_y: Option<f32>,
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    pub is_resizing_sidebar: bool,
    pub word_wrap: bool,
    pub last_wrap_cols: usize,
}

impl AppView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            state: AppState::new(),
            theme: Theme::default(),
            focus_handle: cx.focus_handle(),
            search_focus_handle: cx.focus_handle(),
            edit_mode: true,
            is_mouse_selecting: false,
            editor_top_y: None,
            sidebar_width: 260.0,
            sidebar_collapsed: false,
            is_resizing_sidebar: false,
            word_wrap: true,
            last_wrap_cols: 80,
        }
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        cx.notify();
    }

    fn toggle_word_wrap(&mut self, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        cx.notify();
    }

    fn compute_wrap_cols(&self, window_width: f32, char_width: f32) -> usize {
        let current_sidebar_w = if self.sidebar_collapsed { 0.0 } else { self.sidebar_width };
        let char_w = if char_width > 1.0 { char_width } else { 7.7 };
        // Доступная ширина текста:
        // Ширина окна минус сайдбар, разделитель (4px), левые отступы (32px контейнер + 8px строка + 40px номера строк = 80px)
        // и правые отступы (32px контейнер + 8px строка = 40px). Всего отступов: 124px.
        let available_text_w = (window_width - current_sidebar_w - 124.0).max(100.0);
        ((available_text_w / char_w).floor() as usize).max(20)
    }

    fn move_cursor_up_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        let lines: Vec<&str> = note.content.split('\n').collect();
        let visual_rows = crate::markdown::build_visual_rows(&lines, self.word_wrap, self.last_wrap_cols);
        if visual_rows.is_empty() {
            return;
        }

        let (cursor_line, cursor_col) = note.line_col();
        let (curr_v_idx, col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));

        if shift && note.selection_anchor.is_none() {
            note.selection_anchor = Some(note.cursor_offset);
        }

        if curr_v_idx == 0 {
            note.cursor_offset = 0;
            if !shift {
                note.selection_anchor = None;
            }
            return;
        }

        let target_v = &visual_rows[curr_v_idx - 1];
        let target_len = target_v.char_end.saturating_sub(target_v.char_start);
        let target_col = col_in_row.min(target_len);
        let target_char = target_v.char_start + target_col;
        let offset = note.offset_for_line_col(target_v.line_idx, target_char);
        note.cursor_offset = offset;
        if !shift {
            note.selection_anchor = None;
        }
    }

    fn move_cursor_down_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        let lines: Vec<&str> = note.content.split('\n').collect();
        let visual_rows = crate::markdown::build_visual_rows(&lines, self.word_wrap, self.last_wrap_cols);
        if visual_rows.is_empty() {
            return;
        }

        let (cursor_line, cursor_col) = note.line_col();
        let (curr_v_idx, col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));

        if shift && note.selection_anchor.is_none() {
            note.selection_anchor = Some(note.cursor_offset);
        }

        if curr_v_idx + 1 >= visual_rows.len() {
            note.cursor_offset = note.content.len();
            if !shift {
                note.selection_anchor = None;
            }
            return;
        }

        let target_v = &visual_rows[curr_v_idx + 1];
        let target_len = target_v.char_end.saturating_sub(target_v.char_start);
        let target_col = col_in_row.min(target_len);
        let target_char = target_v.char_start + target_col;
        let offset = note.offset_for_line_col(target_v.line_idx, target_char);
        note.cursor_offset = offset;
        if !shift {
            note.selection_anchor = None;
        }
    }

    fn move_cursor_home_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        let lines: Vec<&str> = note.content.split('\n').collect();
        let visual_rows = crate::markdown::build_visual_rows(&lines, self.word_wrap, self.last_wrap_cols);
        if visual_rows.is_empty() {
            return;
        }
        let (cursor_line, cursor_col) = note.line_col();
        let (curr_v_idx, _) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));
        let v = &visual_rows[curr_v_idx];

        if shift && note.selection_anchor.is_none() {
            note.selection_anchor = Some(note.cursor_offset);
        }

        let target_char = if cursor_col == v.char_start && v.sub_idx > 0 {
            0
        } else {
            v.char_start
        };
        note.cursor_offset = note.offset_for_line_col(v.line_idx, target_char);
        if !shift {
            note.selection_anchor = None;
        }
    }

    fn move_cursor_end_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        let lines: Vec<&str> = note.content.split('\n').collect();
        let visual_rows = crate::markdown::build_visual_rows(&lines, self.word_wrap, self.last_wrap_cols);
        if visual_rows.is_empty() {
            return;
        }
        let (cursor_line, cursor_col) = note.line_col();
        let (curr_v_idx, _) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));
        let v = &visual_rows[curr_v_idx];

        if shift && note.selection_anchor.is_none() {
            note.selection_anchor = Some(note.cursor_offset);
        }

        let line_len = lines.get(v.line_idx).map(|l| l.chars().count()).unwrap_or(0);
        let target_char = if cursor_col == v.char_end && !v.is_last_sub {
            line_len
        } else {
            v.char_end
        };
        note.cursor_offset = note.offset_for_line_col(v.line_idx, target_char);
        if !shift {
            note.selection_anchor = None;
        }
    }

    fn open_folder_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            self.state.open_workspace(folder);
            cx.notify();
        }
    }

    fn create_new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        if let Some(_) = self.state.create_new_note() {
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    fn select_note(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        self.state.open_note(path);
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn reload_files(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.reload_file_tree();
        cx.notify();
    }

    fn save_note(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.save_active_note();
        cx.notify();
    }

    fn update_mouse_selection(&mut self, pos_x: f32, pos_y: f32, cx: &mut Context<Self>) {
        if !self.is_mouse_selecting {
            return;
        }
        let Some(ref mut note) = self.state.active_note else {
            return;
        };

        let lines: Vec<&str> = note.content.split('\n').collect();
        if lines.is_empty() {
            return;
        }

        let visual_rows = crate::markdown::build_visual_rows(&lines, self.word_wrap, self.last_wrap_cols);
        if visual_rows.is_empty() {
            return;
        }

        let font_id = cx.text_system().resolve_font(&gpui::font("Consolas"));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);
        let current_sidebar_w = if self.sidebar_collapsed { 0.0 } else { self.sidebar_width };
        let text_start_x = current_sidebar_w + 4.0 + 32.0 + 8.0 + 41.0;
        let line_h = 24.0;

        let target_v_idx = if let Some(top_y) = self.editor_top_y {
            if pos_y < top_y {
                0
            } else {
                let idx = ((pos_y - top_y) / line_h).floor() as usize;
                idx.min(visual_rows.len().saturating_sub(1))
            }
        } else {
            0
        };

        let v = &visual_rows[target_v_idx];
        let v_len = v.char_end.saturating_sub(v.char_start);

        let col_in_row = if pos_x > text_start_x {
            (((pos_x - text_start_x) / line_char_width).round() as usize).min(v_len)
        } else {
            0
        };

        let target_char = v.char_start + col_in_row;
        let offset = note.offset_for_line_col(v.line_idx, target_char);
        note.cursor_offset = offset;
        cx.notify();
    }

    fn handle_search_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        if key == "backspace" {
            self.state.search_query.pop();
            cx.notify();
        } else if key == "escape" {
            self.state.search_query.clear();
            window.focus(&self.focus_handle);
            cx.notify();
        } else if key == "enter" {
            if let Some(first_match) = self.first_matching_file(&self.state.file_tree) {
                self.select_note(first_match, window, cx);
            }
        } else if event.keystroke.modifiers.control {
            if key.eq_ignore_ascii_case("s") || key == "ы" || key == "Ы" {
                self.save_note(window, cx);
            } else if key.eq_ignore_ascii_case("o") || key == "щ" || key == "Щ" {
                self.open_folder_dialog(window, cx);
            } else if key.eq_ignore_ascii_case("n") || key == "т" || key == "Т" {
                self.create_new_note(window, cx);
            }
        } else {
            if let Some(ch) = &event.keystroke.key_char {
                self.state.search_query.push_str(ch);
                cx.notify();
            } else if key.chars().count() == 1 {
                self.state.search_query.push_str(key);
                cx.notify();
            }
        }
    }

    fn first_matching_file(&self, nodes: &[FileNode]) -> Option<PathBuf> {
        for node in nodes {
            match node {
                FileNode::File { name, path } => {
                    if self.state.search_query.is_empty()
                        || name.to_lowercase().contains(&self.state.search_query.to_lowercase())
                    {
                        return Some(path.clone());
                    }
                }
                FileNode::Folder { children, .. } => {
                    if let Some(p) = self.first_matching_file(children) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    fn render_tree_nodes(
        &self,
        nodes: &[FileNode],
        depth: usize,
        active_path: Option<&PathBuf>,
        theme: &Theme,
        acc: &mut Vec<AnyElement>,
        cx: &Context<Self>,
    ) {
        for node in nodes {
            match node {
                FileNode::Folder {
                    name,
                    path,
                    children,
                } => {
                    let is_expanded = self.state.expanded_folders.contains(path);
                    let chevron = if is_expanded {
                        icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted)
                    } else {
                        icons::icon(icons::ICON_CHEVRON_RIGHT, 12.0, theme.text_muted)
                    };
                    let folder_svg = if is_expanded {
                        icons::icon(icons::ICON_FOLDER_OPEN, 14.0, theme.accent)
                    } else {
                        icons::icon(icons::ICON_FOLDER, 14.0, theme.accent)
                    };
                    let indent = (depth * 14) as f32 + 4.0;
                    let folder_path = path.clone();

                    let folder_row = div()
                        .id(SharedString::from(format!("fld-{}", folder_path.to_string_lossy())))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .pl(px(indent))
                        .pr_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover))
                        .child(chevron)
                        .child(folder_svg)
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_primary)
                                .truncate()
                                .child(name.clone()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.toggle_folder(folder_path.clone());
                            cx.notify();
                        }));

                    acc.push(folder_row.into_any_element());

                    if is_expanded {
                        self.render_tree_nodes(children, depth + 1, active_path, theme, acc, cx);
                    }
                }
                FileNode::File { name, path } => {
                    if !self.state.search_query.is_empty()
                        && !name.to_lowercase().contains(&self.state.search_query.to_lowercase())
                    {
                        continue;
                    }

                    let is_selected = active_path.map(|p| p == path).unwrap_or(false);
                    let indent = (depth * 14 + 18) as f32 + 4.0;
                    let file_path = path.clone();
                    let display_name = name.strip_suffix(".md").unwrap_or(name).to_string();

                    let file_svg = icons::icon(
                        icons::ICON_FILE_TEXT,
                        13.0,
                        if is_selected { theme.accent } else { theme.text_muted },
                    );

                    let file_row = div()
                        .id(SharedString::from(format!("fil-{}", file_path.to_string_lossy())))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .pl(px(indent))
                        .pr_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .when(is_selected, |s| {
                            s.bg(theme.bg_selected)
                                .text_color(theme.accent)
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                        })
                        .when(!is_selected, |s| {
                            s.hover(|h| h.bg(theme.bg_hover))
                                .text_color(theme.text_primary)
                        })
                        .child(file_svg)
                        .child(
                            div()
                                .text_xs()
                                .truncate()
                                .child(display_name),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_note(file_path.clone(), window, cx);
                        }));

                    acc.push(file_row.into_any_element());
                }
            }
        }
    }

    fn handle_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let ctrl = keystroke.modifiers.control;
        let shift = keystroke.modifiers.shift;

        // Ctrl + \ или Ctrl + B (Свернуть / развернуть сайдбар)
        if ctrl && (keystroke.key == "\\" || keystroke.key.eq_ignore_ascii_case("b") || keystroke.key == "и" || keystroke.key == "И") {
            self.toggle_sidebar(cx);
            return;
        }

        // Ctrl + S (Сохранение)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("s") || keystroke.key == "ы" || keystroke.key == "Ы") {
            self.state.save_active_note();
            cx.notify();
            return;
        }

        // Ctrl + O (Открыть хранилище)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("o") || keystroke.key == "щ" || keystroke.key == "Щ") {
            self.open_folder_dialog(_window, cx);
            return;
        }

        // Ctrl + N (Новая заметка)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("n") || keystroke.key == "т" || keystroke.key == "Т") {
            self.create_new_note(_window, cx);
            return;
        }

        // Ctrl + F (Поиск / фильтр)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("f") || keystroke.key == "а" || keystroke.key == "А") {
            _window.focus(&self.search_focus_handle);
            cx.notify();
            return;
        }

        // Ctrl + Z (Undo)
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я") {
            if let Some(ref mut note) = self.state.active_note {
                note.undo();
                cx.notify();
            }
            return;
        }

        // Ctrl + Y или Ctrl + Shift + Z (Redo)
        if (ctrl && (keystroke.key.eq_ignore_ascii_case("y") || keystroke.key == "н" || keystroke.key == "Н"))
            || (ctrl && shift && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я"))
        {
            if let Some(ref mut note) = self.state.active_note {
                note.redo();
                cx.notify();
            }
            return;
        }

        // Ctrl + A (Выделить всё)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("a") || keystroke.key == "ф" || keystroke.key == "Ф") {
            if let Some(ref mut note) = self.state.active_note {
                note.select_all();
                cx.notify();
            }
            return;
        }

        // Ctrl + C (Копировать)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("c") || keystroke.key == "с" || keystroke.key == "С") {
            if let Some(ref note) = self.state.active_note {
                if let Some(text) = note.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            return;
        }

        // Ctrl + X (Вырезать)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("x") || keystroke.key == "ч" || keystroke.key == "Ч") {
            if let Some(ref mut note) = self.state.active_note {
                if let Some(text) = note.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    note.delete_selection();
                    cx.notify();
                }
            }
            return;
        }

        // Alt + Z (Переключение переноса строк)
        if keystroke.modifiers.alt && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я") {
            self.toggle_word_wrap(cx);
            return;
        }

        // Ctrl + V (Вставить)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("v") || keystroke.key == "м" || keystroke.key == "М") {
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

        if self.state.active_note.is_none() {
            return;
        }

        match keystroke.key.as_str() {
            "backspace" => { if let Some(ref mut n) = self.state.active_note { n.delete_backwards(); } }
            "delete" => { if let Some(ref mut n) = self.state.active_note { n.delete_forwards(); } }
            "enter" => { if let Some(ref mut n) = self.state.active_note { n.insert_newline_auto_indent(); } }
            "tab" => { if let Some(ref mut n) = self.state.active_note { n.insert_text("    "); } }
            "space" => { if let Some(ref mut n) = self.state.active_note { n.insert_text(" "); } }
            "left" => { if let Some(ref mut n) = self.state.active_note { n.move_cursor_left(shift); } }
            "right" => { if let Some(ref mut n) = self.state.active_note { n.move_cursor_right(shift); } }
            "up" => self.move_cursor_up_visual(shift),
            "down" => self.move_cursor_down_visual(shift),
            "home" => self.move_cursor_home_visual(shift),
            "end" => self.move_cursor_end_visual(shift),
            _ => {
                if !ctrl && !keystroke.modifiers.alt && !keystroke.modifiers.platform {
                    if let Some(ref mut n) = self.state.active_note {
                        if let Some(ch) = &keystroke.key_char {
                            n.insert_text(ch);
                        } else if keystroke.key.chars().count() == 1 {
                            n.insert_text(&keystroke.key);
                        }
                    }
                }
            }
        }
        cx.notify();
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let active_path = self.state.active_note.as_ref().map(|n| n.path.clone());
        let is_editing = self.edit_mode;
        let font_id = cx.text_system().resolve_font(&gpui::font("Consolas"));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);

        let viewport_w: f32 = window.viewport_size().width.into();
        let wrap_cols = self.compute_wrap_cols(viewport_w, line_char_width);
        self.last_wrap_cols = wrap_cols;

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_app)
            .text_color(theme.text_primary)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.is_resizing_sidebar {
                    let x: f32 = event.position.x.into();
                    if x < 100.0 {
                        this.sidebar_collapsed = true;
                    } else {
                        this.sidebar_collapsed = false;
                        this.sidebar_width = x.clamp(160.0, 600.0);
                    }
                    cx.notify();
                } else if this.is_mouse_selecting {
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.update_mouse_selection(x, y, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.is_resizing_sidebar = false;
                this.is_mouse_selecting = false;
                if let Some(ref mut n) = this.state.active_note {
                    if n.selection_anchor == Some(n.cursor_offset) {
                        n.selection_anchor = None;
                    }
                }
                cx.notify();
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.is_resizing_sidebar = false;
                this.is_mouse_selecting = false;
                cx.notify();
            }))
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
                                    .id("btn-toggle-sidebar")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .p_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(
                                        icons::ICON_PANEL_LEFT,
                                        15.0,
                                        if self.sidebar_collapsed { theme.text_muted } else { theme.accent },
                                    ))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_sidebar(cx);
                                    })),
                            )
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
                                    .gap_1p5()
                                    .text_sm()
                                    .text_color(theme.text_secondary)
                                    .child(icons::icon(icons::ICON_FOLDER, 14.0, theme.text_secondary))
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
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .px_3()
                                    .py_1()
                                    .bg(theme.bg_card)
                                    .hover(|s| s.bg(theme.bg_hover))
                                    .border_1()
                                    .border_color(theme.border)
                                    .rounded_md()
                                    .text_xs()
                                    .cursor_pointer()
                                    .child(icons::icon(icons::ICON_FOLDER_OPEN, 13.0, theme.text_primary))
                                    .child("Open Folder")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_folder_dialog(window, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id("btn-reload")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .px_2()
                                    .py_1()
                                    .bg(theme.bg_card)
                                    .hover(|s| s.bg(theme.bg_hover))
                                    .border_1()
                                    .border_color(theme.border)
                                    .rounded_md()
                                    .text_xs()
                                    .cursor_pointer()
                                    .child(icons::icon(icons::ICON_REFRESH, 13.0, theme.text_primary))
                                    .child("Reload")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.reload_files(window, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .px_2()
                                    .py_1()
                                    .bg(theme.bg_card)
                                    .border_1()
                                    .border_color(theme.success)
                                    .text_color(theme.success)
                                    .rounded_md()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(icons::icon(icons::ICON_BOLT, 12.0, theme.success))
                                    .child("0 WebView • 120 FPS"),
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
                    // ----- Сайдбар файлов (Дерево каталогов) -----
                    .when(!self.sidebar_collapsed, |s| {
                        s.child({
                            let mut tree_elements = Vec::new();
                            self.render_tree_nodes(
                                &self.state.file_tree,
                                0,
                                active_path.as_ref(),
                                theme,
                                &mut tree_elements,
                                cx,
                            );

                            let is_search_focused = self.search_focus_handle.is_focused(window);

                            div()
                                .flex()
                                .flex_col()
                                .w(px(self.sidebar_width))
                                .h_full()
                                .bg(theme.bg_sidebar)
                                .p_2()
                                .gap_2()
                                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                                    if this.is_resizing_sidebar {
                                        let x: f32 = event.position.x.into();
                                        if x < 100.0 {
                                            this.sidebar_collapsed = true;
                                        } else {
                                            this.sidebar_collapsed = false;
                                            this.sidebar_width = x.clamp(160.0, 600.0);
                                        }
                                        cx.notify();
                                    }
                                }))
                                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                    if this.is_resizing_sidebar {
                                        this.is_resizing_sidebar = false;
                                        cx.notify();
                                    }
                                }))
                            // Vault Header
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .py_1()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(icons::icon(icons::ICON_FOLDER, 15.0, theme.accent))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(theme.text_primary)
                                                    .truncate()
                                                    .child(self.state.workspace_name.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .id("btn-new-note")
                                                    .p_1()
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .hover(|h| h.bg(theme.bg_hover))
                                                    .child(icons::icon(icons::ICON_PLUS, 13.0, theme.text_primary))
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.create_new_note(window, cx);
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("btn-open-vault")
                                                    .p_1()
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .hover(|h| h.bg(theme.bg_hover))
                                                    .child(icons::icon(icons::ICON_FOLDER_OPEN, 13.0, theme.text_primary))
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.open_folder_dialog(window, cx);
                                                    })),
                                            ),
                                    ),
                            )
                            // Search Filter Bar
                            .child(
                                div()
                                    .id("search-filter-box")
                                    .track_focus(&self.search_focus_handle)
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .px_2()
                                    .py_1p5()
                                    .bg(theme.bg_card)
                                    .border_1()
                                    .border_color(if is_search_focused { theme.accent } else { theme.border })
                                    .rounded_md()
                                    .cursor_pointer()
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                        window.focus(&this.search_focus_handle);
                                        cx.notify();
                                    }))
                                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                        this.handle_search_key(event, window, cx);
                                    }))
                                    .child(icons::icon(icons::ICON_SEARCH, 12.0, theme.text_muted))
                                    .child(
                                        if self.state.search_query.is_empty() {
                                            div()
                                                .flex_1()
                                                .text_xs()
                                                .text_color(theme.text_muted)
                                                .child("Quick filter...")
                                        } else {
                                            div()
                                                .flex_1()
                                                .text_xs()
                                                .text_color(theme.text_primary)
                                                .child(self.state.search_query.clone())
                                        }
                                    )
                                    .when(!self.state.search_query.is_empty(), |s| {
                                        s.child(
                                            div()
                                                .id("btn-clear-search")
                                                .cursor_pointer()
                                                .p_0p5()
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .rounded_xs()
                                                .child(icons::icon(icons::ICON_CLOSE, 10.0, theme.text_muted))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.state.search_query.clear();
                                                    cx.notify();
                                                }))
                                        )
                                    })
                            )
                            // Tree Scroll
                            .child(
                                div()
                                    .id("sidebar-tree-scroll")
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .overflow_scroll()
                                    .gap_0p5()
                                    .children(tree_elements)
                            )
                        })
                    })
                    // ----- Разделитель / Ресайзер сайдбара (как в Obsidian) -----
                    .child(
                        div()
                            .id("sidebar-resizer")
                            .w(px(4.0))
                            .h_full()
                            .bg(if self.is_resizing_sidebar { theme.accent } else { theme.border })
                            .hover(|h| h.bg(theme.accent))
                            .cursor(gpui::CursorStyle::ResizeColumn)
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                if event.click_count == 2 {
                                    this.toggle_sidebar(cx);
                                } else {
                                    this.is_resizing_sidebar = true;
                                    cx.notify();
                                }
                            }))
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
                                let line_h = px(24.0);
                                let cursor_h = px(16.0);
                                let cursor_top = 4.0;
                                let current_sidebar_w = if self.sidebar_collapsed { 0.0 } else { self.sidebar_width };
                                let text_start_x = current_sidebar_w + 4.0 + 32.0 + 8.0 + 41.0;

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
                                                        .flex()
                                                        .flex_row()
                                                        .items_center()
                                                        .gap_1p5()
                                                        .px_3()
                                                        .py_1()
                                                        .bg(if is_editing { theme.bg_selected } else { theme.bg_card })
                                                        .hover(|s| s.bg(theme.bg_hover))
                                                        .border_1()
                                                        .border_color(theme.border)
                                                        .rounded_md()
                                                        .text_xs()
                                                        .cursor_pointer()
                                                        .child(if is_editing {
                                                            icons::icon(icons::ICON_PENCIL, 12.0, theme.accent)
                                                        } else {
                                                            icons::icon(icons::ICON_EYE, 12.0, theme.text_secondary)
                                                        })
                                                        .child(if is_editing { "Edit Mode" } else { "Preview Mode" })
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.edit_mode = !this.edit_mode;
                                                            cx.notify();
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-toggle-wrap")
                                                        .flex()
                                                        .flex_row()
                                                        .items_center()
                                                        .gap_1p5()
                                                        .px_3()
                                                        .py_1()
                                                        .bg(if self.word_wrap { theme.bg_selected } else { theme.bg_card })
                                                        .hover(|s| s.bg(theme.bg_hover))
                                                        .border_1()
                                                        .border_color(theme.border)
                                                        .rounded_md()
                                                        .text_xs()
                                                        .cursor_pointer()
                                                        .child(icons::icon(icons::ICON_CODE, 12.0, if self.word_wrap { theme.accent } else { theme.text_secondary }))
                                                        .child(if self.word_wrap { "Wrap: On (Alt+Z)" } else { "Wrap: Off (Alt+Z)" })
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.toggle_word_wrap(cx);
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-save-note")
                                                        .flex()
                                                        .flex_row()
                                                        .items_center()
                                                        .gap_1p5()
                                                        .px_3()
                                                        .py_1()
                                                        .bg(theme.accent)
                                                        .text_color(theme.bg_sidebar)
                                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                                        .rounded_md()
                                                        .text_xs()
                                                        .cursor_pointer()
                                                        .child(icons::icon(icons::ICON_SAVE, 13.0, theme.bg_sidebar))
                                                        .child("Save (Ctrl+S)")
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
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                            if this.is_resizing_sidebar {
                                                return;
                                            }
                                            window.focus(&this.focus_handle);
                                            this.is_mouse_selecting = true;
                                            let x: f32 = event.position.x.into();
                                            let y: f32 = event.position.y.into();
                                            this.update_mouse_selection(x, y, cx);
                                            if let Some(ref mut n) = this.state.active_note {
                                                if !event.modifiers.shift {
                                                    n.selection_anchor = Some(n.cursor_offset);
                                                }
                                            }
                                            cx.notify();
                                        }))
                                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                                            if this.is_resizing_sidebar {
                                                let x: f32 = event.position.x.into();
                                                if x < 100.0 {
                                                    this.sidebar_collapsed = true;
                                                } else {
                                                    this.sidebar_collapsed = false;
                                                    this.sidebar_width = x.clamp(160.0, 600.0);
                                                }
                                                cx.notify();
                                            } else if this.is_mouse_selecting {
                                                let x: f32 = event.position.x.into();
                                                let y: f32 = event.position.y.into();
                                                this.update_mouse_selection(x, y, cx);
                                            }
                                        }))
                                        .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                            this.is_resizing_sidebar = false;
                                            this.is_mouse_selecting = false;
                                            if let Some(ref mut n) = this.state.active_note {
                                                if n.selection_anchor == Some(n.cursor_offset) {
                                                    n.selection_anchor = None;
                                                }
                                            }
                                            cx.notify();
                                        }))
                                        .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                            this.is_resizing_sidebar = false;
                                            this.is_mouse_selecting = false;
                                            cx.notify();
                                        }))
                                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                            this.handle_key(event, window, cx);
                                        }))
                                        .children({
                                            let visual_rows = crate::markdown::build_visual_rows(&lines, self.word_wrap, wrap_cols);
                                            let (active_v_idx, active_col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
                                                .unwrap_or((0, 0));

                                            visual_rows.into_iter().enumerate().map(move |(v_idx, v)| {
                                                let line = lines[v.line_idx];
                                                let is_current = is_editing && (v_idx == active_v_idx);
                                                let is_active_line = is_editing && (v.line_idx == cursor_line);
                                                let code_state = code_states[v.line_idx].as_ref().map(|s| (s.kind, s.lang.as_deref()));
                                                let classified = crate::markdown::classify_line(line, code_state);
                                                let is_opening_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: true, .. });
                                                let is_closing_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: false, .. });
                                                let is_code_line = matches!(classified, crate::markdown::LineKind::CodeLine(_));
                                                let is_in_active_block = code_states[v.line_idx].as_ref().map(|s| Some(s.block_id) == active_block_id).unwrap_or(false);
                                                let row_len = v.char_end.saturating_sub(v.char_start);

                                                let hidden_prefix_len = if !is_active_line && v.sub_idx == 0 {
                                                    match classified {
                                                        crate::markdown::LineKind::Heading1(_) => 2,
                                                        crate::markdown::LineKind::Heading2(_) => 3,
                                                        crate::markdown::LineKind::Heading3(_) => 4,
                                                        crate::markdown::LineKind::Heading4(_) => 5,
                                                        _ => 0,
                                                    }
                                                } else {
                                                    0
                                                };

                                                let line_number_text = if v.sub_idx == 0 {
                                                    format!("{:>3} ", v.line_idx + 1)
                                                } else {
                                                    "    ".to_string()
                                                };

                                                let line_start = line_starts[v.line_idx];
                                                let line_end = line_start + line.len();

                                                // Проверяем, пересекается ли выделение с этой визуальной строкой
                                                let row_sel_chars = selection_range.as_ref().and_then(|sel| {
                                                    if sel.end > line_start && sel.start < line_end {
                                                        let byte_s = sel.start.saturating_sub(line_start).min(line.len());
                                                        let byte_e = sel.end.saturating_sub(line_start).min(line.len());
                                                        if byte_s < byte_e {
                                                            let char_s = line[..byte_s].chars().count();
                                                            let char_e = char_s + line[byte_s..byte_e].chars().count();
                                                            let intersect_s = char_s.max(v.char_start);
                                                            let intersect_e = char_e.min(v.char_end);
                                                            if intersect_s < intersect_e {
                                                                let vis_s = (intersect_s - v.char_start).saturating_sub(hidden_prefix_len);
                                                                let vis_e = (intersect_e - v.char_start).saturating_sub(hidden_prefix_len);
                                                                if vis_s < vis_e {
                                                                    Some((vis_s, vis_e))
                                                                } else {
                                                                    None
                                                                }
                                                            } else {
                                                                None
                                                            }
                                                        } else {
                                                            None
                                                        }
                                                    } else {
                                                        None
                                                    }
                                                });

                                                let v_char_start = v.char_start;
                                                let v_char_end = v.char_end;
                                                let v_line_idx = v.line_idx;
                                                let v_sub_idx = v.sub_idx;

                                                div()
                                                    .id(gpui::ElementId::from(("v-row", v_idx)))
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .h(line_h)
                                                    .py_0()
                                                    .px_2()
                                                    .rounded_sm()
                                                    .when(is_current, |s| s.bg(theme.bg_hover))
                                                    // Обработка начала выделения / клика мыши на всей строке
                                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                        if this.is_resizing_sidebar {
                                                            return;
                                                        }
                                                        this.is_mouse_selecting = true;
                                                        let click_x: f32 = event.position.x.into();
                                                        let click_y: f32 = event.position.y.into();
                                                        this.editor_top_y = Some(click_y - (v_idx as f32) * 24.0);

                                                        let col_in_row = if click_x > text_start_x {
                                                            let raw_col = ((click_x - text_start_x) / line_char_width).round() as usize;
                                                            (raw_col + hidden_prefix_len).min(row_len)
                                                        } else {
                                                            0
                                                        };
                                                        let target_char = v_char_start + col_in_row;
                                                        if let Some(ref mut n) = this.state.active_note {
                                                            let offset = n.offset_for_line_col(v_line_idx, target_char);
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
                                                    // Обработка протягивания мыши (Drag to Select) на всей строке (включая номера строк)
                                                    .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                                                        if this.is_resizing_sidebar {
                                                            let x: f32 = event.position.x.into();
                                                            if x < 100.0 {
                                                                this.sidebar_collapsed = true;
                                                            } else {
                                                                this.sidebar_collapsed = false;
                                                                this.sidebar_width = x.clamp(160.0, 600.0);
                                                            }
                                                            cx.notify();
                                                            return;
                                                        }
                                                        if this.is_mouse_selecting {
                                                            let click_x: f32 = event.position.x.into();
                                                            let click_y: f32 = event.position.y.into();
                                                            this.editor_top_y = Some(click_y - (v_idx as f32) * 24.0);

                                                            let col_in_row = if click_x > text_start_x {
                                                                let raw_col = ((click_x - text_start_x) / line_char_width).round() as usize;
                                                                (raw_col + hidden_prefix_len).min(row_len)
                                                            } else {
                                                                0
                                                            };
                                                            let target_char = v_char_start + col_in_row;
                                                            if let Some(ref mut n) = this.state.active_note {
                                                                let offset = n.offset_for_line_col(v_line_idx, target_char);
                                                                n.cursor_offset = offset;
                                                                cx.notify();
                                                            }
                                                        }
                                                    }))
                                                    // Номер строки
                                                    .child(
                                                        div()
                                                            .id(gpui::ElementId::from(("line-num", v_idx)))
                                                            .w(px(40.0))
                                                            .h(line_h)
                                                            .flex()
                                                            .items_center()
                                                            .text_xs()
                                                            .text_color(if is_current { theme.accent } else { theme.text_muted })
                                                            .font_family("Consolas")
                                                            .cursor_pointer()
                                                            .child(line_number_text),
                                                    )
                                                    // Текст строки
                                                    .child(
                                                        div()
                                                            .id(gpui::ElementId::from(("line-content", v_idx)))
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
                                                            // 1. Фоновый слой выделения (не ломает верстку строки)
                                                            .when_some(row_sel_chars, |s, (sel_s, sel_e)| {
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
                                                                    .child(render_live_styled_row(
                                                                        line,
                                                                        &classified,
                                                                        v_char_start,
                                                                        v_char_end,
                                                                        v_sub_idx,
                                                                        is_active_line,
                                                                        is_in_active_block,
                                                                        theme,
                                                                    ))
                                                            )
                                                            // 3. Слой курсора (absolute overlay, никогда не раздвигает буквы в словах)
                                                            .when(is_current, |s| {
                                                                let cursor_x = (active_col_in_row as f32) * line_char_width;
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
                                            })
                                        })
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

fn render_live_styled_row(
    line: &str,
    classified: &crate::markdown::LineKind<'_>,
    char_start: usize,
    char_end: usize,
    sub_idx: usize,
    is_active_line: bool,
    show_fences: bool,
    theme: &Theme,
) -> gpui::Div {
    if line.is_empty() || char_start >= char_end {
        return div().h_full().flex().items_center().child(" ");
    }

    match classified {
        crate::markdown::LineKind::Heading1(_) => {
            let rest = line.strip_prefix("# ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(2));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("# "));
                }
                row.child(
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.accent)
                        .child(render_syntax_spans(row_spans, theme)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(2),
                    char_end.saturating_sub(2),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme.accent)
                            .child(render_syntax_spans(row_spans, theme)),
                    )
            }
        }
        crate::markdown::LineKind::Heading2(_) => {
            let rest = line.strip_prefix("## ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(3));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("## "));
                }
                row.child(
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.accent_hover)
                        .child(render_syntax_spans(row_spans, theme)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(3),
                    char_end.saturating_sub(3),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme.accent_hover)
                            .child(render_syntax_spans(row_spans, theme)),
                    )
            }
        }
        crate::markdown::LineKind::Heading3(_) => {
            let rest = line.strip_prefix("### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(4));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("### "));
                }
                row.child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(render_syntax_spans(row_spans, theme)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(4),
                    char_end.saturating_sub(4),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(render_syntax_spans(row_spans, theme)),
                    )
            }
        }
        crate::markdown::LineKind::Heading4(_) => {
            let rest = line.strip_prefix("#### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(5));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("#### "));
                }
                row.child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_secondary)
                        .child(render_syntax_spans(row_spans, theme)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(5),
                    char_end.saturating_sub(5),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_secondary)
                            .child(render_syntax_spans(row_spans, theme)),
                    )
            }
        }
        crate::markdown::LineKind::Quote(_) => {
            let rest = line.strip_prefix("> ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(2));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .italic()
                    .child(div().text_color(theme.accent).child("> "))
                    .child(div().text_color(theme.text_secondary).child(render_syntax_spans(row_spans, theme)))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(2),
                    char_end.saturating_sub(2),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .italic()
                    .child(div().text_color(theme.border_subtle).child("  "))
                    .child(div().text_color(theme.text_secondary).child(render_syntax_spans(row_spans, theme)))
            }
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
            let rest = if line.len() >= prefix_len { &line[prefix_len..] } else { "" };
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let prefix_color = if *checked { theme.success } else { theme.text_muted };
            let prefix_str = if line.len() >= prefix_len { &line[..prefix_len] } else { "" };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(prefix_len));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().font_weight(gpui::FontWeight::BOLD).text_color(prefix_color).child(prefix_str.to_string()))
                    .child(
                        div()
                            .when(*checked, |s| s.text_color(theme.text_muted).line_through())
                            .child(render_syntax_spans(row_spans, theme)),
                    )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(prefix_len),
                    char_end.saturating_sub(prefix_len),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child("      "))
                    .child(
                        div()
                            .when(*checked, |s| s.text_color(theme.text_muted).line_through())
                            .child(render_syntax_spans(row_spans, theme)),
                    )
            }
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
            let prefix_len = indent_len + bullet.len();
            let spans = crate::markdown::parse_inline_syntax_spans(rest);

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(prefix_len));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(indent.to_string()))
                    .child(div().font_weight(gpui::FontWeight::BOLD).text_color(theme.accent).child(bullet.to_string()))
                    .child(render_syntax_spans(row_spans, theme))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(prefix_len),
                    char_end.saturating_sub(prefix_len),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(format!("{}  ", indent)))
                    .child(render_syntax_spans(row_spans, theme))
            }
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
            let prefix_len = indent_len + num_part.len();
            let spans = crate::markdown::parse_inline_syntax_spans(rest);

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(prefix_len));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(indent.to_string()))
                    .child(div().font_weight(gpui::FontWeight::BOLD).text_color(theme.accent).child(num_part.to_string()))
                    .child(render_syntax_spans(row_spans, theme))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(prefix_len),
                    char_end.saturating_sub(prefix_len),
                );
                let spaces = " ".repeat(prefix_len);
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(spaces))
                    .child(render_syntax_spans(row_spans, theme))
            }
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
                                    .child(lang_label),
                            ),
                    )
                    .child(icons::icon(icons::ICON_CODE, 12.0, theme.text_muted))
            } else {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .h_full()
                    .child(div().h_px().w_full().bg(theme.border_subtle))
            }
        }
        crate::markdown::LineKind::CodeLine(_) => {
            let row_slice: String = line.chars().skip(char_start).take(char_end - char_start).collect();
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .text_color(theme.success)
                .child(row_slice)
        }
        crate::markdown::LineKind::Paragraph(_) => {
            let spans = crate::markdown::parse_inline_syntax_spans(line);
            let row_spans = crate::markdown::slice_syntax_spans(&spans, char_start, char_end);
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(render_syntax_spans(row_spans, theme))
        }
    }
}

