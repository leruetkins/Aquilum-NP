use crate::icons;
use crate::state::{AppState, FileNode};
use crate::theme::Theme;
use gpui::{
    div, prelude::*, px, rgb, rgba, uniform_list, AnyElement, ClipboardItem, Context, Div, FocusHandle,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Render, ScrollStrategy,
    SharedString, UniformListScrollHandle, Window, WindowControlArea,
};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct AppView {
    pub state: AppState,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
    pub search_focus_handle: FocusHandle,
    pub editor_scroll_handle: UniformListScrollHandle,
    pub edit_mode: bool,
    pub is_mouse_selecting: bool,
    pub editor_top_y: Option<f32>,
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    pub is_resizing_sidebar: bool,
    pub is_hovering_resizer: bool,
    pub word_wrap: bool,
    pub last_wrap_cols: usize,
    pub is_settings_open: bool,
    pub settings_tab: crate::views::settings_view::SettingsTab,
    pub config: aquilum_core::settings::models::AppConfig,
}

impl AppView {
    /// Creates and initializes a new AppView instance with loaded configuration.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state = AppState::new();
        let config = state.core.settings.get_config();
        let theme = Theme::from_config(&config.ui.theme, None);
        Self {
            state,
            theme,
            focus_handle: cx.focus_handle(),
            search_focus_handle: cx.focus_handle(),
            editor_scroll_handle: UniformListScrollHandle::new(),
            edit_mode: true,
            is_mouse_selecting: false,
            editor_top_y: None,
            sidebar_width: 260.0,
            sidebar_collapsed: false,
            is_resizing_sidebar: false,
            is_hovering_resizer: false,
            word_wrap: true,
            last_wrap_cols: 80,
            is_settings_open: false,
            settings_tab: crate::views::settings_view::SettingsTab::Ui,
            config,
        }
    }

    /// Toggles the left sidebar visibility.
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        cx.notify();
    }

    /// Toggles word wrapping mode in the editor.
    pub fn toggle_word_wrap(&mut self, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        cx.notify();
    }

    fn compute_wrap_cols(&self, window_width: f32, char_width: f32) -> usize {
        let current_sidebar_w = if self.sidebar_collapsed { 40.0 } else { self.sidebar_width };
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

    fn scroll_cursor_into_view(&self) {
        if let Some(ref note) = self.state.active_note {
            let (cursor_line, cursor_col) = note.line_col();
            let lines: Vec<&str> = note.content.split('\n').collect();
            let visual_rows = crate::markdown::build_visual_rows(&lines, self.word_wrap, self.last_wrap_cols);
            if let Some((v_idx, _)) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col) {
                self.editor_scroll_handle.scroll_to_item(v_idx, ScrollStrategy::Center);
            }
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

    fn close_tab(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.state.active_note.as_ref().map(|n| n.is_dirty && n.path == path).unwrap_or(false) {
            self.state.save_active_note();
        }
        self.state.close_tab(path);
        cx.notify();
    }

    fn navigate_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        if let Some(_) = self.state.navigate_back() {
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    fn navigate_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        if let Some(_) = self.state.navigate_forward() {
            window.focus(&self.focus_handle);
            cx.notify();
        }
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
        let current_sidebar_w = if self.sidebar_collapsed { 40.0 } else { self.sidebar_width };
        let text_start_x = current_sidebar_w + 4.0 + 32.0 + 8.0 + 40.0;
        let line_h = 24.0;

        let top_y = self.editor_top_y.unwrap_or(126.0);
        let target_v_idx = if pos_y < top_y {
            0
        } else {
            let idx = ((pos_y - top_y) / line_h).floor() as usize;
            idx.min(visual_rows.len().saturating_sub(1))
        };

        let v = &visual_rows[target_v_idx];
        let v_len = v.char_end.saturating_sub(v.char_start);

        let (cursor_line, _) = note.line_col();
        let is_active_line = v.line_idx == cursor_line;
        let line = lines[v.line_idx];
        let classified = crate::markdown::classify_line(line, None);
        let (row_spans, hidden_prefix_len) = crate::markdown::row_spans_and_hidden_prefix(line, &classified, v, is_active_line);

        let col_in_row = if pos_x > text_start_x {
            let raw_col = ((pos_x - text_start_x) / line_char_width).round() as usize;
            crate::markdown::visual_col_to_row_char(&row_spans, raw_col, is_active_line, hidden_prefix_len).min(v_len)
        } else {
            0
        };

        let target_char = v.char_start + col_in_row;
        let offset = note.offset_for_line_col(v.line_idx, target_char);
        if note.cursor_offset != offset {
            note.cursor_offset = offset;
            cx.notify();
        }
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
        let alt = keystroke.modifiers.alt;
        let shift = keystroke.modifiers.shift;

        // Alt + Left (Navigate Back in note history)
        if alt && keystroke.key == "left" {
            self.navigate_back(_window, cx);
            return;
        }

        // Alt + Right (Navigate Forward in note history)
        if alt && keystroke.key == "right" {
            self.navigate_forward(_window, cx);
            return;
        }

        // Escape closes settings modal if open
        if keystroke.key == "escape" && self.is_settings_open {
            self.close_settings(cx);
            return;
        }

        // Ctrl + , (Toggle Settings modal)
        if ctrl && (keystroke.key == "," || keystroke.key == "<" || keystroke.key == "б" || keystroke.key == "Б") {
            self.toggle_settings(cx);
            return;
        }

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

        // Ctrl + W (Закрыть текущую вкладку)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("w") || keystroke.key == "ц" || keystroke.key == "Ц") {
            if let Some(ref note) = self.state.active_note {
                let path = note.path.clone();
                self.close_tab(&path, cx);
            }
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
        self.scroll_cursor_into_view();
        cx.notify();
    }

    fn render_navigation_bar(&self, theme: &Theme, cx: &Context<Self>) -> gpui::Stateful<Div> {
        let can_go_back = self.state.can_navigate_back();
        let can_go_forward = self.state.can_navigate_forward();
        let breadcrumbs = self.state.note_breadcrumbs();
        let is_dirty = self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false);
        let is_editing = self.edit_mode;

        div()
            .id("editor-navigation-bar")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(34.0))
            .min_w_0()
            .overflow_hidden()
            .px_3()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.bg_editor)
            // Left: Back/Forward buttons + Breadcrumbs
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .min_w_0()
                    .overflow_hidden()
                    // Back button
                    .child(
                        div()
                            .id("btn-nav-back")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(24.0))
                            .h(px(24.0))
                            .rounded_sm()
                            .when(can_go_back, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_ARROW_LEFT, 13.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.navigate_back(window, cx);
                                    }))
                            })
                            .when(!can_go_back, |d| {
                                d.cursor_default()
                                    .child(icons::icon(icons::ICON_ARROW_LEFT, 13.0, theme.border))
                            }),
                    )
                    // Forward button
                    .child(
                        div()
                            .id("btn-nav-forward")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(24.0))
                            .h(px(24.0))
                            .rounded_sm()
                            .when(can_go_forward, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_ARROW_RIGHT, 13.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.navigate_forward(window, cx);
                                    }))
                            })
                            .when(!can_go_forward, |d| {
                                d.cursor_default()
                                    .child(icons::icon(icons::ICON_ARROW_RIGHT, 13.0, theme.border))
                            }),
                    )
                    // Spacer between arrows and breadcrumbs
                    .child(div().w(px(8.0)))
                    // Breadcrumbs trail
                    .child({
                        let mut crumbs_bar = div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .min_w_0()
                            .overflow_hidden();

                        let count = breadcrumbs.len();
                        for (idx, part) in breadcrumbs.iter().enumerate() {
                            if idx > 0 {
                                crumbs_bar = crumbs_bar.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .px_1p5()
                                        .flex_shrink_0()
                                        .child("/"),
                                );
                            }
                            let is_last = idx == count.saturating_sub(1);
                            crumbs_bar = crumbs_bar.child(
                                div()
                                    .text_xs()
                                    .font_weight(if is_last {
                                        gpui::FontWeight::SEMIBOLD
                                    } else {
                                        gpui::FontWeight::NORMAL
                                    })
                                    .text_color(if is_last {
                                        theme.text_primary
                                    } else {
                                        theme.text_muted
                                    })
                                    .truncate()
                                    .hover(|h| {
                                        if !is_last {
                                            h.text_color(theme.text_secondary)
                                        } else {
                                            h
                                        }
                                    })
                                    .child(part.clone()),
                            );
                        }

                        if is_dirty {
                            crumbs_bar = crumbs_bar.child(
                                div()
                                    .text_xs()
                                    .text_color(theme.accent)
                                    .ml_1p5()
                                    .child("●"),
                            );
                        }

                        crumbs_bar
                    }),
            )
            // Right: Info and Obsidian-style action buttons
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .when_some(self.state.active_note.as_ref(), |r, note| {
                        let words: usize = note.content.split_whitespace().count();
                        let chars = note.content.chars().count();
                        r.child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .mr_2()
                                .child(format!("{}w • {}c", words, chars)),
                        )
                    })
                    // Toggle wrap
                    .child(
                        div()
                            .id("btn-toggle-wrap")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(24.0))
                            .h(px(24.0))
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_CODE,
                                12.0,
                                if self.word_wrap { theme.accent } else { theme.text_muted },
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_word_wrap(cx);
                            })),
                    )
                    // Toggle reader / edit view
                    .child(
                        div()
                            .id("btn-toggle-reading-mode")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(24.0))
                            .h(px(24.0))
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_BOOK_OPEN,
                                13.0,
                                if !is_editing { theme.accent } else { theme.text_muted },
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit_mode = !this.edit_mode;
                                cx.notify();
                            })),
                    )
                    // More options (3 vertical dots - saves active note)
                    .child(
                        div()
                            .id("btn-more-options")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(24.0))
                            .h(px(24.0))
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_MORE_VERTICAL, 13.0, theme.text_muted))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_note(window, cx);
                            })),
                    ),
            )
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.theme = Theme::from_config(&self.config.ui.theme, Some(window.appearance()));
        let cx = &*cx;
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
        let is_maximized = window.is_maximized();
        let wrap_cols = self.compute_wrap_cols(viewport_w, line_char_width);
        self.last_wrap_cols = wrap_cols;

        let is_resizer_active = self.is_resizing_sidebar || self.is_hovering_resizer;
        let resizer_line_w = if is_resizer_active { 3.0 } else { 1.0 };
        let resizer_color = if is_resizer_active { theme.accent } else { theme.border };

        div()
            .flex()
            .flex_col()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.bg_app)
            .text_color(theme.text_primary)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.is_settings_open {
                    return;
                }
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
                if this.is_settings_open {
                    return;
                }
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
                if this.is_settings_open {
                    return;
                }
                this.is_resizing_sidebar = false;
                this.is_hovering_resizer = false;
                this.is_mouse_selecting = false;
                cx.notify();
            }))
            // ================= WINDOW TITLEBAR & TAB BAR (CUSTOM CSD FRAME) =================
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(38.0))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .bg(theme.bg_sidebar)
                    .border_b_1()
                    .border_color(theme.border)
                    // Left area: Sidebar header column in titlebar (width matches sidebar)
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .w(px(self.sidebar_width))
                                .flex_shrink_0()
                                .h_full()
                                .bg(theme.bg_sidebar)
                                // 1. Rail header cell (44px) matching the rail below
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(44.0))
                                        .h_full()
                                        .flex_shrink_0()
                                        .border_r_1()
                                        .border_color(theme.border)
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
                                                    theme.accent,
                                                ))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.toggle_sidebar(cx);
                                                })),
                                        ),
                                )
                                // 2. File tree header (AQUILUM badge and drag region)
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .flex_1()
                                        .h_full()
                                        .px_3()
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
                                                .id("sidebar-titlebar-drag-region")
                                                .flex_1()
                                                .h_full()
                                                .window_control_area(WindowControlArea::Drag)
                                                .on_mouse_down(MouseButton::Left, cx.listener(|_, event: &MouseDownEvent, window, _| {
                                                    if event.click_count == 2 {
                                                        window.zoom_window();
                                                    }
                                                })),
                                        ),
                                ),
                        )
                    })
                    .when(self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(44.0))
                                .h_full()
                                .flex_shrink_0()
                                .border_r_1()
                                .border_color(theme.border)
                                .bg(theme.bg_sidebar)
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
                                            theme.text_muted,
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.toggle_sidebar(cx);
                                        })),
                                ),
                        )
                    })
                    // Vertical divider (4px hitbox) matching the sidebar-resizer below exactly
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("titlebar-resizer-divider")
                                .w(px(4.0))
                                .h_full()
                                .flex_shrink_0()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_center()
                                .cursor(gpui::CursorStyle::ResizeColumn)
                                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                    this.is_hovering_resizer = *hovered;
                                    cx.notify();
                                }))
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                    if event.click_count == 2 {
                                        this.toggle_sidebar(cx);
                                    } else {
                                        this.is_resizing_sidebar = true;
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    div()
                                        .w(px(resizer_line_w))
                                        .h_full()
                                        .bg(resizer_color),
                                ),
                        )
                    })
                    // Right area: Editor header column containing Tab Bar, Drag Area, and Window Controls
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .overflow_hidden()
                            .bg(theme.bg_sidebar)
                            // Browser-style Tabs Area starting directly at the boundary
                            .child({
                                let mut tabs_bar = div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .h_full()
                                    .pt_1()
                                    .pl_2()
                                    .min_w_0()
                                    .overflow_hidden();

                                let active_path = self.state.active_note.as_ref().map(|n| n.path.clone());
                                let is_active_dirty = self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false);

                                for (idx, tab_path) in self.state.open_tabs.iter().enumerate() {
                                    let is_active = active_path.as_ref() == Some(tab_path);
                                    let title = tab_path
                                        .file_stem()
                                        .map(|s| s.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| "Untitled".to_owned());
                                    let tab_path_for_select = tab_path.clone();
                                    let tab_path_for_close = tab_path.clone();

                                    tabs_bar = tabs_bar.child(
                                        div()
                                            .id(gpui::ElementId::from(("tab-item", idx)))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .h(px(32.0))
                                            .bg(if is_active { theme.bg_editor } else { theme.bg_sidebar })
                                            .border_t_1()
                                            .border_x_1()
                                            .border_color(if is_active { theme.border } else { rgba(0x00000000) })
                                            .rounded_t_md()
                                            .hover(|h| {
                                                if !is_active {
                                                    h.bg(theme.bg_hover)
                                                } else {
                                                    h
                                                }
                                            })
                                            // Tab selection area (icon + title + dirty dot)
                                            .child(
                                                div()
                                                    .id(gpui::ElementId::from(("tab-select", idx)))
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .h_full()
                                                    .pl_2p5()
                                                    .pr_1()
                                                    .cursor_pointer()
                                                    .child(icons::icon(
                                                        icons::ICON_FILE_TEXT,
                                                        12.0,
                                                        if is_active { theme.accent } else { theme.text_muted },
                                                    ))
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .font_weight(if is_active {
                                                                gpui::FontWeight::SEMIBOLD
                                                            } else {
                                                                gpui::FontWeight::NORMAL
                                                            })
                                                            .text_color(if is_active {
                                                                theme.text_primary
                                                            } else {
                                                                theme.text_secondary
                                                            })
                                                            .truncate()
                                                            .max_w(px(160.0))
                                                            .child(title),
                                                    )
                                                    .when(is_active && is_active_dirty, |tab| {
                                                        tab.child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(theme.accent)
                                                                .child("●"),
                                                        )
                                                    })
                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                        if this.state.active_note.as_ref().map(|n| &n.path != &tab_path_for_select).unwrap_or(true) {
                                                            this.select_note(tab_path_for_select.clone(), window, cx);
                                                        }
                                                    })),
                                            )
                                            // Tab close button (isolated sibling)
                                            .child(
                                                div()
                                                    .id(gpui::ElementId::from(("btn-close-tab", idx)))
                                                    .p_0p5()
                                                    .mr_1p5()
                                                    .rounded_sm()
                                                    .cursor_pointer()
                                                    .hover(|h| h.bg(theme.bg_hover))
                                                    .child(icons::icon(icons::ICON_CLOSE, 10.0, theme.text_muted))
                                                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                                        cx.stop_propagation();
                                                    })
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        cx.stop_propagation();
                                                        this.close_tab(&tab_path_for_close, cx);
                                                    })),
                                            ),
                                    );
                                }

                                tabs_bar.child(
                                    div()
                                        .id("btn-new-tab")
                                        .p_1()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|h| h.bg(theme.bg_hover))
                                        .child(icons::icon(icons::ICON_PLUS, 12.0, theme.text_muted))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.create_new_note(window, cx);
                                        })),
                                )
                            })
                            // Drag Area (fills remaining space between tabs and window controls)
                            .child(
                                div()
                                    .id("titlebar-drag-region")
                                    .flex_1()
                                    .h_full()
                                    .window_control_area(WindowControlArea::Drag)
                                    .on_mouse_down(MouseButton::Left, cx.listener(|_, event: &MouseDownEvent, window, _| {
                                        if event.click_count == 2 {
                                            window.zoom_window();
                                        }
                                    })),
                            )
                            // Native Windows Titlebar Control Buttons (Minimize, Maximize/Restore, Close)
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .h_full()
                                    .flex_shrink_0()
                                    .child(
                                        div()
                                            .id("win-ctrl-min")
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(46.0))
                                            .h_full()
                                            .cursor_pointer()
                                            .window_control_area(WindowControlArea::Min)
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .child(icons::icon(icons::ICON_MINIMIZE, 11.0, theme.text_secondary))
                                            .on_click(cx.listener(|_, _, window, _| {
                                                window.minimize_window();
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("win-ctrl-max")
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(46.0))
                                            .h_full()
                                            .cursor_pointer()
                                            .window_control_area(WindowControlArea::Max)
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .child(icons::icon(
                                                if is_maximized { icons::ICON_RESTORE } else { icons::ICON_MAXIMIZE },
                                                11.0,
                                                theme.text_secondary,
                                            ))
                                            .on_click(cx.listener(|_, _, window, _| {
                                                window.zoom_window();
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("win-ctrl-close")
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(46.0))
                                            .h_full()
                                            .cursor_pointer()
                                            .window_control_area(WindowControlArea::Close)
                                            .hover(|h| h.bg(rgb(0xe81123)).text_color(rgb(0xffffff)))
                                            .child(icons::icon(icons::ICON_CLOSE, 11.0, theme.text_secondary))
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                cx.quit();
                                            })),
                                    ),
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
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    // ----- Left Action Rail / Ribbon (Obsidian-style) -----
                    .child(
                        div()
                            .id("sidebar-action-rail")
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
                            // 1. Create New Note
                            .child(
                                div()
                                    .id("rail-btn-new-note")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(32.0))
                                    .h(px(32.0))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_NOTE_CREATE, 17.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.create_new_note(window, cx);
                                    })),
                            )
                            // 2. Daily Note / Calendar
                            .child(
                                div()
                                    .id("rail-btn-calendar")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(32.0))
                                    .h(px(32.0))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_CALENDAR, 17.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.create_new_note(window, cx);
                                    })),
                            )
                            // 3. Command Palette
                            .child(
                                div()
                                    .id("rail-btn-commands")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(32.0))
                                    .h(px(32.0))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_TERMINAL, 17.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_settings(cx);
                                    })),
                            )
                            // 4. Sync / Refresh
                            .child(
                                div()
                                    .id("rail-btn-refresh")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(32.0))
                                    .h(px(32.0))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_REFRESH, 17.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.reload_file_tree();
                                        cx.notify();
                                    })),
                            )
                            // 5. Trash
                            .child(
                                div()
                                    .id("rail-btn-trash")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(32.0))
                                    .h(px(32.0))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_TRASH_2, 17.0, theme.text_secondary)),
                            )
                            // Spacer to push settings to bottom
                            .child(div().flex_1())
                            // Settings at bottom
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
                                        if self.is_settings_open { theme.accent } else { theme.text_muted },
                                    ))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_settings(cx);
                                    })),
                            ),
                    )
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
                                .w(px((self.sidebar_width - 44.0).max(100.0)))
                                .flex_shrink_0()
                                .overflow_hidden()
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
                                            )
                                            .child(
                                                div()
                                                    .id("btn-reload-vault")
                                                    .p_1()
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .hover(|h| h.bg(theme.bg_hover))
                                                    .child(icons::icon(icons::ICON_REFRESH, 13.0, theme.text_primary))
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.reload_files(window, cx);
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
                            // Sidebar Footer (Vault display + Settings button)
                            .child(
                                div()
                                    .id("sidebar-footer")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .py_1p5()
                                    .border_t_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1p5()
                                            .text_xs()
                                            .text_color(theme.text_secondary)
                                            .child(icons::icon(icons::ICON_FOLDER, 12.0, theme.text_muted))
                                            .child(self.state.workspace_name.clone()),
                                    )
                                    .child(
                                        div()
                                            .id("btn-sidebar-settings")
                                            .p_1()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .child(icons::icon(
                                                icons::ICON_SETTINGS,
                                                13.0,
                                                if self.is_settings_open { theme.accent } else { theme.text_muted },
                                            ))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_settings(cx);
                                            })),
                                    ),
                            )
                        })
                    })
                    // ----- Sidebar Resizer Divider (Obsidian-style) -----
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("sidebar-resizer")
                                .flex_shrink_0()
                                .w(px(4.0))
                                .h_full()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_center()
                                .cursor(gpui::CursorStyle::ResizeColumn)
                                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                    this.is_hovering_resizer = *hovered;
                                    cx.notify();
                                }))
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                    if event.click_count == 2 {
                                        this.toggle_sidebar(cx);
                                    } else {
                                        this.is_resizing_sidebar = true;
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    div()
                                        .w(px(resizer_line_w))
                                        .h_full()
                                        .bg(resizer_color),
                                ),
                        )
                    })
                    // ----- Центральная область редактора -----
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .bg(theme.bg_editor)
                            // Панель навигации (Obsidian-style) сразу под вкладками
                            .child(self.render_navigation_bar(theme, cx))
                            .when_some(self.state.active_note.as_ref(), |this, note| {
                                let lines: Vec<String> = note.content.split('\n').map(String::from).collect();
                                let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                                let (cursor_line, cursor_col) = note.line_col();
                                let visual_rows = crate::markdown::build_visual_rows(&line_refs, self.word_wrap, wrap_cols);
                                let (active_v_idx, active_col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
                                    .unwrap_or((0, 0));

                                let mut line_starts: Vec<usize> = Vec::with_capacity(lines.len());
                                let mut running_offset = 0;
                                for line in &lines {
                                    line_starts.push(running_offset);
                                    running_offset += line.len() + 1;
                                }

                                let code_states = crate::markdown::analyze_code_blocks(&line_refs);
                                let active_block_id: Option<usize> = if is_editing {
                                    code_states.get(cursor_line).and_then(|s| s.as_ref()).map(|s| s.block_id)
                                } else {
                                    None
                                };
                                let line_h = px(24.0);
                                let cursor_h = px(16.0);
                                let cursor_top = 4.0;
                                let current_sidebar_w = if self.sidebar_collapsed { 40.0 } else { self.sidebar_width };
                                // Left column + Resizer (4px) + Container Padding Left (32px) + Row Padding Left (8px) + Line Number Width (40px) = current_sidebar_w + 84px
                                let text_start_x = current_sidebar_w + 4.0 + 32.0 + 8.0 + 40.0;

                                let lines_arc = Arc::new(lines);
                                let visual_rows_arc = Arc::new(visual_rows);
                                let line_starts_arc = Arc::new(line_starts);
                                let code_states_arc = Arc::new(code_states);
                                let theme_copy = theme.clone();
                                let visual_rows_count = visual_rows_arc.len();

                                this.child(
                                    div()
                                        .id("note-editor-container")
                                        .track_focus(&self.focus_handle)
                                        .cursor(gpui::CursorStyle::IBeam)
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .w_full()
                                        .min_w_0()
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                                            if this.is_resizing_sidebar {
                                                return;
                                            }
                                            window.focus(&this.focus_handle);
                                            this.is_mouse_selecting = true;
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
                                        .child(
                                            uniform_list(
                                                "note-editor-uniform-list",
                                                visual_rows_count,
                                                cx.processor(move |this, range: Range<usize>, _window, cx| {
                                                    let theme = &theme_copy;
                                                    let is_editing = this.edit_mode;
                                                    let selection_range = this.state.active_note.as_ref().and_then(|n| n.selection_range());
                                                    let (cursor_line, _) = this.state.active_note.as_ref().map(|n| n.line_col()).unwrap_or((0, 0));

                                                    let mut items = Vec::with_capacity(range.len());
                                                    for v_idx in range {
                                                        let v = &visual_rows_arc[v_idx];
                                                        let line = &lines_arc[v.line_idx];
                                                        let is_current = is_editing && (v_idx == active_v_idx);
                                                        let is_active_line = is_editing && (v.line_idx == cursor_line);
                                                        let code_state = code_states_arc[v.line_idx].as_ref().map(|s| (s.kind, s.lang.as_deref()));
                                                        let classified = crate::markdown::classify_line(line, code_state);
                                                        let is_opening_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: true, .. });
                                                        let is_closing_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: false, .. });
                                                        let is_code_line = matches!(classified, crate::markdown::LineKind::CodeLine(_));
                                                        let is_in_active_block = code_states_arc[v.line_idx].as_ref().map(|s| Some(s.block_id) == active_block_id).unwrap_or(false);
                                                        let row_len = v.char_end.saturating_sub(v.char_start);

                                                        let (row_spans, hidden_prefix_len) = crate::markdown::row_spans_and_hidden_prefix(line, &classified, v, is_active_line);
                                                        let row_spans_down = row_spans.clone();

                                                        let line_number_text = if v.sub_idx == 0 {
                                                            format!("{:>3} ", v.line_idx + 1)
                                                        } else {
                                                            "    ".to_string()
                                                        };

                                                        let line_start = line_starts_arc[v.line_idx];
                                                        let line_end = line_start + line.len();

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
                                                                        let raw_s = intersect_s - v.char_start;
                                                                        let raw_e = intersect_e - v.char_start;
                                                                        let vis_s = crate::markdown::row_char_to_visual_col(&row_spans, raw_s, is_active_line, hidden_prefix_len);
                                                                        let vis_e = crate::markdown::row_char_to_visual_col(&row_spans, raw_e, is_active_line, hidden_prefix_len);
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

                                                        items.push(
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
                                                                        crate::markdown::visual_col_to_row_char(&row_spans_down, raw_col, is_active_line, hidden_prefix_len).min(row_len)
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
                                                                        .child(line_number_text),
                                                                )
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
                                                                                    .bg(theme.selection_bg)
                                                                                    .rounded_xs(),
                                                                            )
                                                                        })
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
                                                                                )),
                                                                        )
                                                                        .when(is_current, |s| {
                                                                            let vis_cursor = crate::markdown::row_char_to_visual_col(&row_spans, active_col_in_row, is_active_line, hidden_prefix_len);
                                                                            let cursor_x = (vis_cursor as f32) * line_char_width;
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
                                                                        }),
                                                                ),
                                                        );
                                                    }
                                                    items
                                                }),
                                            )
                                            .track_scroll(self.editor_scroll_handle.clone())
                                            .size_full()
                                            .px_8()
                                            .py_6()
                                        )
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
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .px_4()
                    .bg(theme.bg_sidebar)
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(self.state.status_text.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .gap_4()
                            .when_some(self.state.active_note.as_ref(), |s, note| {
                                let (line, col) = note.line_col();
                                s.child(format!("Ln {}, Col {}", line + 1, col + 1))
                            })
                            .child("SQLite WAL")
                            .child("Tantivy")
                            .child("120 FPS"),
                    ),
            )
            // ================= SETTINGS MODAL =================
            .when(self.is_settings_open, |d| {
                d.child(self.render_settings_modal(cx))
            })
    }
}

fn render_syntax_spans(spans: Vec<crate::markdown::SyntaxSpan>, is_active_line: bool, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .children(spans.into_iter().filter_map(|span| {
            match span {
                crate::markdown::SyntaxSpan::Marker(m) => {
                    if is_active_line {
                        Some(div().text_color(theme.text_muted).child(m))
                    } else {
                        None
                    }
                }
                crate::markdown::SyntaxSpan::Text(t) => {
                    Some(div().text_color(theme.text_primary).child(t))
                }
                crate::markdown::SyntaxSpan::Bold(b) => {
                    Some(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child(b),
                    )
                }
                crate::markdown::SyntaxSpan::Italic(i) => {
                    Some(
                        div()
                            .italic()
                            .text_color(theme.text_primary)
                            .child(i),
                    )
                }
                crate::markdown::SyntaxSpan::BoldItalic(bi) => {
                    Some(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .italic()
                            .text_color(theme.text_primary)
                            .child(bi),
                    )
                }
                crate::markdown::SyntaxSpan::InlineCode(c) => {
                    Some(
                        div()
                            .text_color(theme.success)
                            .bg(theme.code_bg)
                            .rounded_xs()
                            .child(c),
                    )
                }
                crate::markdown::SyntaxSpan::Strikethrough(s) => {
                    Some(
                        div()
                            .line_through()
                            .text_color(theme.text_muted)
                            .child(s),
                    )
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
                        .child(render_syntax_spans(row_spans, is_active_line, theme)),
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
                            .child(render_syntax_spans(row_spans, is_active_line, theme)),
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
                        .child(render_syntax_spans(row_spans, is_active_line, theme)),
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
                            .child(render_syntax_spans(row_spans, is_active_line, theme)),
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
                        .child(render_syntax_spans(row_spans, is_active_line, theme)),
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
                            .child(render_syntax_spans(row_spans, is_active_line, theme)),
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
                        .child(render_syntax_spans(row_spans, is_active_line, theme)),
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
                            .child(render_syntax_spans(row_spans, is_active_line, theme)),
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
                    .child(div().text_color(theme.text_secondary).child(render_syntax_spans(row_spans, is_active_line, theme)))
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
                    .child(div().text_color(theme.text_secondary).child(render_syntax_spans(row_spans, is_active_line, theme)))
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
                            .child(render_syntax_spans(row_spans, is_active_line, theme)),
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
                            .child(render_syntax_spans(row_spans, is_active_line, theme)),
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
                    .child(render_syntax_spans(row_spans, is_active_line, theme))
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
                    .child(render_syntax_spans(row_spans, is_active_line, theme))
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
                    .child(render_syntax_spans(row_spans, is_active_line, theme))
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
                    .child(render_syntax_spans(row_spans, is_active_line, theme))
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
                .child(render_syntax_spans(row_spans, is_active_line, theme))
        }
    }
}

