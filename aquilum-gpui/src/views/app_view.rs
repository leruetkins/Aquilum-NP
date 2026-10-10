use crate::icons;
use crate::state::{AppState, FileNode};
use crate::theme::Theme;
use gpui::{
    div, point, prelude::*, px, rgb, rgba, uniform_list, AnyElement, ClipboardItem, Context, Div, FocusHandle,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Render, ScrollHandle, ScrollStrategy,
    SharedString, UniformListScrollHandle, Window, WindowControlArea,
};
use std::cell::Cell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct AppView {
    pub state: AppState,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
    pub search_focus_handle: FocusHandle,
    pub editor_scroll_handle: UniformListScrollHandle,
    pub sidebar_scroll_handle: ScrollHandle,
    pub need_scroll_sidebar_to_active: Cell<bool>,
    pub initial_sidebar_scroll_done: Cell<bool>,
    pub edit_mode: bool,
    pub is_mouse_selecting: bool,
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    pub is_resizing_sidebar: bool,
    pub is_hovering_resizer: bool,
    pub word_wrap: bool,
    pub last_wrap_cols: usize,
    pub is_settings_open: bool,
    pub settings_tab: crate::views::settings_view::SettingsTab,
    pub is_dragging_scrollbar: bool,
    pub scrollbar_drag_start_y: f32,
    pub scrollbar_drag_start_scroll_y: f32,
    pub config: aquilum_core::settings::models::AppConfig,
    pub last_saved_geometry: Option<(f32, f32, Option<f32>, Option<f32>, bool)>,
    pub initial_scroll_done: bool,
    pub initial_scroll_top: f32,
    pub is_hovering_close: bool,
    pub title_focus_handle: FocusHandle,
    pub title_edit_text: String,
    pub title_editing_path: Option<PathBuf>,
    pub title_cursor_pos: usize,
    pub was_title_focused: bool,
    pub title_selection_anchor: Option<usize>,
    pub title_is_mouse_selecting: bool,
    pub title_is_focused: bool,
    pub title_cursor_visible: bool,
    pub title_cursor_blink_epoch: usize,
    pub auto_save_epoch: usize,
}

impl AppView {
    /// Creates and initializes a new AppView instance with loaded configuration.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state = AppState::new();
        let config = state.core.settings.get_config();
        let theme = Theme::from_config(&config.ui.theme, None);
        let ws = crate::state::WindowState::load(&crate::state::app_data_dir());
        let last_saved_geometry = Some((ws.width, ws.height, ws.x, ws.y, ws.maximized));

        let mut edit_mode = true;
        let mut word_wrap = true;
        let mut initial_scroll_top = 0.0;
        if let Some(ref note) = state.active_note {
            if let Some(settings) = state.note_states.get(&note.path) {
                edit_mode = settings.edit_mode;
                word_wrap = settings.word_wrap;
                initial_scroll_top = settings.scroll_top;
            }
        }

        let title_editing_path = state.active_note.as_ref().map(|n| n.path.clone());
        let title_edit_text = state.active_note.as_ref().map(|n| n.title.clone()).unwrap_or_default();
        let title_cursor_pos = title_edit_text.chars().count();

        Self {
            state,
            theme,
            focus_handle: cx.focus_handle(),
            search_focus_handle: cx.focus_handle(),
            editor_scroll_handle: UniformListScrollHandle::new(),
            sidebar_scroll_handle: ScrollHandle::new(),
            need_scroll_sidebar_to_active: Cell::new(false),
            initial_sidebar_scroll_done: Cell::new(false),
            edit_mode,
            is_mouse_selecting: false,
            sidebar_width: 260.0,
            sidebar_collapsed: false,
            is_resizing_sidebar: false,
            is_hovering_resizer: false,
            word_wrap,
            last_wrap_cols: 80,
            is_settings_open: false,
            settings_tab: crate::views::settings_view::SettingsTab::Ui,
            is_dragging_scrollbar: false,
            scrollbar_drag_start_y: 0.0,
            scrollbar_drag_start_scroll_y: 0.0,
            config,
            last_saved_geometry,
            initial_scroll_done: false,
            initial_scroll_top,
            is_hovering_close: false,
            title_focus_handle: cx.focus_handle(),
            title_edit_text,
            title_editing_path,
            title_cursor_pos,
            was_title_focused: false,
            title_selection_anchor: None,
            title_is_mouse_selecting: false,
            title_is_focused: false,
            title_cursor_visible: true,
            title_cursor_blink_epoch: 0,
            auto_save_epoch: 0,
        }
    }

    pub fn title_selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.title_selection_anchor {
            let pos = self.title_cursor_pos;
            if anchor != pos {
                return Some((anchor.min(pos), anchor.max(pos)));
            }
        }
        None
    }

    pub fn reset_title_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.title_cursor_visible = true;
        self.title_cursor_blink_epoch = self.title_cursor_blink_epoch.wrapping_add(1);
        let epoch = self.title_cursor_blink_epoch;
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();
        cx.foreground_executor().spawn(async move {
            loop {
                timer_executor.timer(std::time::Duration::from_millis(530)).await;
                let should_continue = entity.update(&mut async_cx, |this, cx| {
                    if this.title_cursor_blink_epoch != epoch || !this.title_is_focused {
                        false
                    } else {
                        this.title_cursor_visible = !this.title_cursor_visible;
                        cx.notify();
                        true
                    }
                }).unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }).detach();
    }

    pub fn schedule_auto_save(&mut self, cx: &mut Context<Self>) {
        self.auto_save_epoch = self.auto_save_epoch.wrapping_add(1);
        let epoch = self.auto_save_epoch;
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();
        cx.foreground_executor().spawn(async move {
            timer_executor.timer(std::time::Duration::from_millis(500)).await;
            let _ = entity.update(&mut async_cx, |this, cx| {
                if this.auto_save_epoch == epoch {
                    if this.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
                        this.state.save_active_note();
                        cx.notify();
                    }
                }
            });
        }).detach();
    }

    pub fn current_scroll_top(&self) -> f32 {
        let offset = self.editor_scroll_handle.0.borrow().base_handle.offset();
        f32::from(-offset.y).max(0.0)
    }

    pub fn apply_note_settings_for_path(&mut self, path: &Path) {
        if let Some(settings) = self.state.note_states.get(path).cloned() {
            self.edit_mode = settings.edit_mode;
            self.word_wrap = settings.word_wrap;
            if settings.scroll_top > 0.0 {
                self.set_editor_scroll_top(settings.scroll_top);
            } else if settings.cursor_offset > 0 {
                self.scroll_cursor_into_view();
            } else {
                self.set_editor_scroll_top(0.0);
            }
        } else {
            self.edit_mode = true;
            self.word_wrap = true;
            self.set_editor_scroll_top(0.0);
        }
    }

    pub fn save_on_exit(&mut self, window: &Window) {
        let is_maximized = window.is_maximized();
        let bounds = window.bounds();
        let mut window_state = crate::state::WindowState::load(&crate::state::app_data_dir());
        window_state.maximized = is_maximized;
        if !is_maximized {
            window_state.x = Some(bounds.origin.x.into());
            window_state.y = Some(bounds.origin.y.into());
            window_state.width = bounds.size.width.into();
            window_state.height = bounds.size.height.into();
        }
        window_state.save(&crate::state::app_data_dir());

        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }

        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        let sidebar_offset_y: f32 = self.sidebar_scroll_handle.offset().y.into();
        self.state.session.sidebar_scroll_top = sidebar_offset_y;
        self.state.save_session_state();
    }

    /// Toggles the left sidebar visibility.
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        cx.notify();
    }

    /// Toggles word wrapping mode in the editor.
    pub fn toggle_word_wrap(&mut self, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        cx.notify();
    }

    fn compute_wrap_cols(&self, window_width: f32, char_width: f32) -> usize {
        let editor_left_w = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
        let char_w = if char_width > 1.0 { char_width } else { 7.7 };
        // Доступная ширина текста: ширина окна минус левые колонки и отступы (контейнер 32px + строка 8px + номера строк 40px + правые 40px)
        let available_text_w = (window_width - editor_left_w - 120.0).max(100.0);
        ((available_text_w / char_w).floor() as usize).max(20)
    }

    fn move_cursor_up_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        note.ensure_visual_cache(self.word_wrap, self.last_wrap_cols);
        let visual_rows = Arc::clone(&note.cached_visual_rows);
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
            note.sync_cursor();
            if !shift {
                note.selection_anchor = None;
            }
            return;
        }

        let target_v = &visual_rows[curr_v_idx - 1];
        let target_len = target_v.char_end.saturating_sub(target_v.char_start);
        let max_col = if target_v.is_last_sub {
            target_len
        } else {
            target_len.saturating_sub(1)
        };
        let target_col = col_in_row.min(max_col);
        let target_char = target_v.char_start + target_col;
        let offset = note.offset_for_line_col(target_v.line_idx, target_char);
        note.cursor_offset = offset;
        note.sync_cursor();
        if !shift {
            note.selection_anchor = None;
        }
    }

    fn move_cursor_down_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        note.ensure_visual_cache(self.word_wrap, self.last_wrap_cols);
        let visual_rows = Arc::clone(&note.cached_visual_rows);
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
            note.sync_cursor();
            if !shift {
                note.selection_anchor = None;
            }
            return;
        }

        let target_v = &visual_rows[curr_v_idx + 1];
        let target_len = target_v.char_end.saturating_sub(target_v.char_start);
        let max_col = if target_v.is_last_sub {
            target_len
        } else {
            target_len.saturating_sub(1)
        };
        let target_col = col_in_row.min(max_col);
        let target_char = target_v.char_start + target_col;
        let offset = note.offset_for_line_col(target_v.line_idx, target_char);
        note.cursor_offset = offset;
        note.sync_cursor();
        if !shift {
            note.selection_anchor = None;
        }
    }

    fn move_cursor_home_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        note.ensure_visual_cache(self.word_wrap, self.last_wrap_cols);
        let visual_rows = Arc::clone(&note.cached_visual_rows);
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
        note.sync_cursor();
        if !shift {
            note.selection_anchor = None;
        }
    }

    fn move_cursor_end_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        note.ensure_visual_cache(self.word_wrap, self.last_wrap_cols);
        let visual_rows = Arc::clone(&note.cached_visual_rows);
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

        let line_len = note.cached_lines.get(v.line_idx).map(|l| l.chars().count()).unwrap_or(0);
        let sub_end = if v.is_last_sub {
            v.char_end
        } else {
            v.char_end.saturating_sub(1)
        };
        let target_char = if cursor_col >= sub_end && !v.is_last_sub {
            line_len
        } else {
            sub_end
        };
        note.cursor_offset = note.offset_for_line_col(v.line_idx, target_char);
        note.sync_cursor();
        if !shift {
            note.selection_anchor = None;
        }
    }

    pub fn scroll_cursor_into_view_with_strategy(&self, strategy: ScrollStrategy) {
        if let Some(ref note) = self.state.active_note {
            let (cursor_line, cursor_col) = note.line_col();
            let visual_rows = &note.cached_visual_rows;
            if let Some((v_idx, _)) = crate::markdown::find_visual_row_for_cursor(visual_rows, cursor_line, cursor_col) {
                self.editor_scroll_handle.scroll_to_item(v_idx, strategy);
            }
        }
    }

    fn scroll_cursor_into_view(&self) {
        if let Some(ref note) = self.state.active_note {
            let (cursor_line, cursor_col) = note.line_col();
            let visual_rows = &note.cached_visual_rows;
            if let Some((v_idx, _)) = crate::markdown::find_visual_row_for_cursor(visual_rows, cursor_line, cursor_col) {
                let state = self.editor_scroll_handle.0.borrow();
                let scroll_y = f32::from(-state.base_handle.offset().y).max(0.0);
                let viewport_h = state.last_item_size.map(|s| f32::from(s.item.height)).unwrap_or(600.0).max(100.0);
                drop(state);

                let item_height = 24.0;
                let first_visible_v_idx = (scroll_y / item_height).floor().max(0.0) as usize;
                let visible_count = (viewport_h / item_height).floor().max(1.0) as usize;
                let last_visible_v_idx = first_visible_v_idx + visible_count.saturating_sub(1);

                // Если каретка уже внутри видимой области экрана (с запасом 1 строки), скролл не требуется!
                if v_idx > first_visible_v_idx && v_idx < last_visible_v_idx {
                    return;
                }

                if v_idx <= first_visible_v_idx {
                    self.editor_scroll_handle.scroll_to_item(v_idx, ScrollStrategy::Top);
                } else if v_idx >= last_visible_v_idx {
                    self.editor_scroll_handle.scroll_to_item(v_idx, ScrollStrategy::Bottom);
                }
            }
        }
    }

    fn set_editor_scroll_top(&self, scroll_y: f32) {
        let mut state = self.editor_scroll_handle.0.borrow_mut();
        state.deferred_scroll_to_item = None;
        let base = &state.base_handle;
        let offset_x = base.offset().x;
        base.set_offset(point(offset_x, px(-scroll_y)));
    }

    fn handle_scrollbar_drag(&mut self, current_y: f32, window: &Window, cx: &mut Context<Self>) {
        if !self.is_dragging_scrollbar {
            return;
        }
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        note.ensure_visual_cache(self.word_wrap, self.last_wrap_cols);
        let total_lines = note.cached_visual_rows.len().max(1);
        let viewport_h: f32 = window.viewport_size().height.into();
        let editor_h = (viewport_h - 170.0).max(100.0);
        let total_content_h = (total_lines as f32) * 24.0 + 48.0;
        let ratio = (editor_h / total_content_h).clamp(0.04, 1.0);
        let thumb_h = (editor_h * ratio).max(28.0);
        let max_scroll_content = (total_content_h - editor_h).max(1.0);
        let track_scrollable = (editor_h - thumb_h).max(1.0);

        let delta_y = current_y - self.scrollbar_drag_start_y;
        let delta_fraction = delta_y / track_scrollable;
        let new_scroll_y = (self.scrollbar_drag_start_scroll_y + delta_fraction * max_scroll_content).clamp(0.0, max_scroll_content);
        self.set_editor_scroll_top(new_scroll_y);
        cx.notify();
    }

    fn open_folder_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            self.state.open_workspace(folder);
            if self.state.active_note.is_none() {
                if let Some(first_file) = AppState::first_file_in_tree(&self.state.file_tree) {
                    self.state.open_note(first_file);
                }
            }
            self.need_scroll_sidebar_to_active.set(true);
            cx.notify();
        }
    }

    fn create_new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        if let Some(_) = self.state.create_new_note() {
            if let Some(ref note) = self.state.active_note {
                self.title_editing_path = Some(note.path.clone());
                self.title_edit_text = note.title.clone();
                self.title_cursor_pos = self.title_edit_text.chars().count();
                self.title_selection_anchor = None;
                self.title_is_mouse_selecting = false;
            }
            self.edit_mode = true;
            self.word_wrap = true;
            self.set_editor_scroll_top(0.0);
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    fn select_note(&mut self, path: PathBuf, center_in_sidebar: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        let sidebar_offset_y: f32 = self.sidebar_scroll_handle.offset().y.into();
        self.state.session.sidebar_scroll_top = sidebar_offset_y;
        self.state.open_note(path.clone());
        self.apply_note_settings_for_path(&path);
        if let Some(ref note) = self.state.active_note {
            self.title_editing_path = Some(note.path.clone());
            self.title_edit_text = note.title.clone();
            self.title_cursor_pos = self.title_edit_text.chars().count();
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
        }
        if center_in_sidebar {
            self.need_scroll_sidebar_to_active.set(true);
        }
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn close_tab(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.state.active_note.as_ref().map(|n| n.path == path).unwrap_or(false) {
            self.commit_title_rename_internal();
        }
        if self.state.active_note.as_ref().map(|n| n.is_dirty && n.path == path).unwrap_or(false) {
            self.state.save_active_note();
        }
        let is_closing_active = self.state.active_note.as_ref().map(|n| n.path == path).unwrap_or(false);
        if is_closing_active {
            let current_scroll = self.current_scroll_top();
            self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        }
        self.state.close_tab(path);
        if let Some((target_path, title)) = self.state.active_note.as_ref().map(|n| (n.path.clone(), n.title.clone())) {
            self.apply_note_settings_for_path(&target_path);
            self.title_editing_path = Some(target_path);
            self.title_edit_text = title;
            self.title_cursor_pos = self.title_edit_text.chars().count();
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
            self.need_scroll_sidebar_to_active.set(true);
        } else {
            self.title_editing_path = None;
            self.title_edit_text.clear();
            self.title_cursor_pos = 0;
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
        }
        cx.notify();
    }

    fn navigate_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        if let Some(target) = self.state.navigate_back() {
            self.apply_note_settings_for_path(&target);
            if let Some(ref note) = self.state.active_note {
                self.title_editing_path = Some(note.path.clone());
                self.title_edit_text = note.title.clone();
                self.title_cursor_pos = self.title_edit_text.chars().count();
                self.title_selection_anchor = None;
                self.title_is_mouse_selecting = false;
            }
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    fn navigate_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        if let Some(target) = self.state.navigate_forward() {
            self.apply_note_settings_for_path(&target);
            if let Some(ref note) = self.state.active_note {
                self.title_editing_path = Some(note.path.clone());
                self.title_edit_text = note.title.clone();
                self.title_cursor_pos = self.title_edit_text.chars().count();
                self.title_selection_anchor = None;
                self.title_is_mouse_selecting = false;
            }
            self.need_scroll_sidebar_to_active.set(true);
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

    fn update_mouse_selection(&mut self, pos_x: f32, pos_y: f32, viewport_h: f32, cx: &mut Context<Self>) {
        if !self.is_mouse_selecting {
            return;
        }
        if self.state.active_note.is_none() {
            return;
        }

        // Top of rows area: Titlebar (38px) + Nav bar (40px) + note-title-panel (54px) = 132px
        let editor_rows_top_y = 132.0;
        let editor_bottom_y = (viewport_h - 28.0).max(editor_rows_top_y + 100.0);
        let scroll_y = self.current_scroll_top();

        // Edge autoscroll when dragging outside or near the top/bottom bounds
        if pos_y < editor_rows_top_y && scroll_y > 0.0 {
            let step = ((editor_rows_top_y - pos_y).max(1.0) * 0.8).min(30.0);
            self.set_editor_scroll_top((scroll_y - step).max(0.0));
        } else if pos_y > editor_bottom_y - 16.0 {
            let step = ((pos_y - (editor_bottom_y - 16.0)).max(1.0) * 0.8).min(30.0);
            self.set_editor_scroll_top(scroll_y + step);
        }

        let current_scroll_y = self.current_scroll_top();
        let rel_y = pos_y - editor_rows_top_y + current_scroll_y;

        let font_id = cx.text_system().resolve_font(&gpui::font("Consolas"));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);
        let editor_left_x = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
        let text_start_x = editor_left_x + 32.0 + 8.0 + 40.0;
        let line_h = 24.0;
        let word_wrap = self.word_wrap;
        let wrap_cols = self.last_wrap_cols;

        let Some(ref mut note) = self.state.active_note else {
            return;
        };

        note.ensure_visual_cache(word_wrap, wrap_cols);
        let visual_rows = Arc::clone(&note.cached_visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let target_v_idx = if rel_y <= 0.0 {
            0
        } else {
            let idx = (rel_y / line_h).floor() as usize;
            idx.min(visual_rows.len().saturating_sub(1))
        };

        let v = &visual_rows[target_v_idx];
        let v_len = v.char_end.saturating_sub(v.char_start);

        let (cursor_line, _) = note.line_col();
        let is_active_line = v.line_idx == cursor_line;
        let line = &note.cached_lines[v.line_idx];
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
            note.sync_cursor();
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
                self.select_note(first_match, true, window, cx);
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
            if key == "space" {
                self.state.search_query.push(' ');
                cx.notify();
            } else if let Some(ch) = &event.keystroke.key_char {
                self.state.search_query.push_str(ch);
                cx.notify();
            } else if key.chars().count() == 1 {
                self.state.search_query.push_str(key);
                cx.notify();
            }
        }
    }

    fn commit_title_rename_internal(&mut self) -> bool {
        let Some(ref note) = self.state.active_note else {
            return false;
        };
        let current_path = note.path.clone();
        let current_title = note.title.clone();
        let new_title = self.title_edit_text.trim();

        if new_title.is_empty() {
            self.title_edit_text = current_title;
            self.title_cursor_pos = self.title_edit_text.chars().count();
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
            return false;
        }

        if new_title != current_title {
            match self.state.rename_note(&current_path, new_title) {
                Ok(new_path) => {
                    self.title_editing_path = Some(new_path);
                    self.title_edit_text = new_title.to_string();
                    self.title_cursor_pos = self.title_edit_text.chars().count();
                    self.title_selection_anchor = None;
                    self.title_is_mouse_selecting = false;
                    return true;
                }
                Err(err) => {
                    self.state.status_text = format!("Rename failed: {}", err);
                    self.title_edit_text = current_title;
                    self.title_cursor_pos = self.title_edit_text.chars().count();
                    self.title_selection_anchor = None;
                    self.title_is_mouse_selecting = false;
                    return false;
                }
            }
        }
        self.title_selection_anchor = None;
        self.title_is_mouse_selecting = false;
        false
    }

    pub fn commit_title_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_title_rename_internal() {
            cx.notify();
        }
    }

    fn handle_title_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let shift = event.keystroke.modifiers.shift;
        let alt = event.keystroke.modifiers.alt;

        if key == "enter" || key == "tab" {
            self.title_selection_anchor = None;
            self.commit_title_rename(window, cx);
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        if key == "escape" {
            self.title_selection_anchor = None;
            if let Some(ref note) = self.state.active_note {
                self.title_edit_text = note.title.clone();
                self.title_cursor_pos = self.title_edit_text.chars().count();
            }
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        if key == "down" {
            self.title_selection_anchor = None;
            self.commit_title_rename(window, cx);
            if let Some(ref mut note) = self.state.active_note {
                note.cursor_offset = 0;
                note.selection_anchor = None;
            }
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        // Ctrl + A (Select All)
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.title_selection_anchor = Some(0);
            self.title_cursor_pos = self.title_edit_text.chars().count();
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Ctrl + C (Copy selected text)
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s_start, s_end)) = self.title_selection_range() {
                let selected: String = self.title_edit_text.chars().skip(s_start).take(s_end - s_start).collect();
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
            }
            return;
        }

        // Ctrl + X (Cut selected text)
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s_start, s_end)) = self.title_selection_range() {
                let selected: String = self.title_edit_text.chars().skip(s_start).take(s_end - s_start).collect();
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
                let mut chars: Vec<char> = self.title_edit_text.chars().collect();
                chars.drain(s_start..s_end);
                self.title_edit_text = chars.into_iter().collect();
                self.title_cursor_pos = s_start;
                self.title_selection_anchor = None;
                self.reset_title_cursor_blink(cx);
                cx.notify();
            }
            return;
        }

        // Ctrl + S (Save note)
        if ctrl && (key.eq_ignore_ascii_case("s") || key == "ы" || key == "Ы") {
            self.commit_title_rename(window, cx);
            self.save_note(window, cx);
            return;
        }

        // Ctrl + V (Paste over selection or at cursor)
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\n', '\r', '\t'];
                    let filtered: String = text
                        .chars()
                        .filter(|c| !invalid_chars.contains(c) && !c.is_control())
                        .collect();
                    if !filtered.is_empty() {
                        let mut chars: Vec<char> = self.title_edit_text.chars().collect();
                        let pos = if let Some((s_start, s_end)) = self.title_selection_range() {
                            chars.drain(s_start..s_end);
                            self.title_selection_anchor = None;
                            s_start
                        } else {
                            self.title_cursor_pos.min(chars.len())
                        };
                        for (i, c) in filtered.chars().enumerate() {
                            chars.insert(pos + i, c);
                        }
                        self.title_cursor_pos = pos + filtered.chars().count();
                        self.title_edit_text = chars.into_iter().collect();
                        self.reset_title_cursor_blink(cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Left Arrow
        if key == "left" {
            if shift {
                if self.title_selection_anchor.is_none() {
                    self.title_selection_anchor = Some(self.title_cursor_pos);
                }
                if self.title_cursor_pos > 0 {
                    self.title_cursor_pos -= 1;
                }
            } else {
                if let Some((s_start, _)) = self.title_selection_range() {
                    self.title_cursor_pos = s_start;
                    self.title_selection_anchor = None;
                } else if self.title_cursor_pos > 0 {
                    self.title_cursor_pos -= 1;
                }
            }
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Right Arrow
        if key == "right" {
            let total_chars = self.title_edit_text.chars().count();
            if shift {
                if self.title_selection_anchor.is_none() {
                    self.title_selection_anchor = Some(self.title_cursor_pos);
                }
                if self.title_cursor_pos < total_chars {
                    self.title_cursor_pos += 1;
                }
            } else {
                if let Some((_, s_end)) = self.title_selection_range() {
                    self.title_cursor_pos = s_end;
                    self.title_selection_anchor = None;
                } else if self.title_cursor_pos < total_chars {
                    self.title_cursor_pos += 1;
                }
            }
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Home
        if key == "home" {
            if shift {
                if self.title_selection_anchor.is_none() {
                    self.title_selection_anchor = Some(self.title_cursor_pos);
                }
                self.title_cursor_pos = 0;
            } else {
                self.title_selection_anchor = None;
                self.title_cursor_pos = 0;
            }
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // End
        if key == "end" {
            let total_chars = self.title_edit_text.chars().count();
            if shift {
                if self.title_selection_anchor.is_none() {
                    self.title_selection_anchor = Some(self.title_cursor_pos);
                }
                self.title_cursor_pos = total_chars;
            } else {
                self.title_selection_anchor = None;
                self.title_cursor_pos = total_chars;
            }
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Backspace
        if key == "backspace" {
            let mut chars: Vec<char> = self.title_edit_text.chars().collect();
            if let Some((s_start, s_end)) = self.title_selection_range() {
                chars.drain(s_start..s_end);
                self.title_cursor_pos = s_start;
                self.title_selection_anchor = None;
                self.title_edit_text = chars.into_iter().collect();
                self.reset_title_cursor_blink(cx);
                cx.notify();
            } else {
                let pos = self.title_cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.title_cursor_pos = pos - 1;
                    self.title_edit_text = chars.into_iter().collect();
                    self.reset_title_cursor_blink(cx);
                    cx.notify();
                }
            }
            return;
        }

        // Delete
        if key == "delete" {
            let mut chars: Vec<char> = self.title_edit_text.chars().collect();
            if let Some((s_start, s_end)) = self.title_selection_range() {
                chars.drain(s_start..s_end);
                self.title_cursor_pos = s_start;
                self.title_selection_anchor = None;
                self.title_edit_text = chars.into_iter().collect();
                self.reset_title_cursor_blink(cx);
                cx.notify();
            } else {
                let pos = self.title_cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                    self.title_edit_text = chars.into_iter().collect();
                    self.reset_title_cursor_blink(cx);
                    cx.notify();
                }
            }
            return;
        }

        // Normal character input
        if !ctrl && !alt && !event.keystroke.modifiers.platform {
            let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\n', '\r', '\t'];
            let text_to_insert = if key == "space" {
                Some(" ")
            } else if let Some(ch) = &event.keystroke.key_char {
                Some(ch.as_str())
            } else if key.chars().count() == 1 {
                Some(key.as_str())
            } else {
                None
            };

            if let Some(text) = text_to_insert {
                let filtered: String = text
                    .chars()
                    .filter(|c| !invalid_chars.contains(c) && !c.is_control())
                    .collect();
                if !filtered.is_empty() {
                    let mut chars: Vec<char> = self.title_edit_text.chars().collect();
                    let pos = if let Some((s_start, s_end)) = self.title_selection_range() {
                        chars.drain(s_start..s_end);
                        self.title_selection_anchor = None;
                        s_start
                    } else {
                        self.title_cursor_pos.min(chars.len())
                    };
                    for (i, c) in filtered.chars().enumerate() {
                        chars.insert(pos + i, c);
                    }
                    self.title_cursor_pos = pos + filtered.chars().count();
                    self.title_edit_text = chars.into_iter().collect();
                    self.reset_title_cursor_blink(cx);
                    cx.notify();
                }
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
        selected_index: &mut Option<usize>,
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
                        self.render_tree_nodes(children, depth + 1, active_path, theme, acc, selected_index, cx);
                    }
                }
                FileNode::File { name, path } => {
                    if !self.state.search_query.is_empty()
                        && !name.to_lowercase().contains(&self.state.search_query.to_lowercase())
                    {
                        continue;
                    }

                    let is_selected = active_path.map(|p| p == path).unwrap_or(false);
                    if is_selected {
                        *selected_index = Some(acc.len());
                    }
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
                            this.select_note(file_path.clone(), false, window, cx);
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
            self.schedule_auto_save(cx);
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
            self.schedule_auto_save(cx);
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
            self.schedule_auto_save(cx);
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
            self.schedule_auto_save(cx);
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
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.schedule_auto_save(cx);
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
            .h(px(40.0))
            .min_w_0()
            .overflow_hidden()
            .px_3()
            .bg(theme.bg_editor)
            // Left: Back/Forward buttons + Breadcrumbs
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .min_w_0()
                    .overflow_hidden()
                    // Back button
                    .child(
                        div()
                            .id("btn-nav-back")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .when(can_go_back, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_ARROW_LEFT, 16.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.navigate_back(window, cx);
                                    }))
                            })
                            .when(!can_go_back, |d| {
                                d.cursor_default()
                                    .child(icons::icon(icons::ICON_ARROW_LEFT, 16.0, theme.border))
                            }),
                    )
                    // Forward button
                    .child(
                        div()
                            .id("btn-nav-forward")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .when(can_go_forward, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_ARROW_RIGHT, 16.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.navigate_forward(window, cx);
                                    }))
                            })
                            .when(!can_go_forward, |d| {
                                d.cursor_default()
                                    .child(icons::icon(icons::ICON_ARROW_RIGHT, 16.0, theme.border))
                            }),
                    )
                    // Spacer between arrows and breadcrumbs
                    .child(div().w(px(10.0)))
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
                                        .text_sm()
                                        .text_color(theme.text_muted)
                                        .px_1p5()
                                        .flex_shrink_0()
                                        .child("/"),
                                );
                            }
                            let is_last = idx == count.saturating_sub(1);
                            crumbs_bar = crumbs_bar.child(
                                div()
                                    .text_sm()
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
                                    .text_sm()
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
                    .gap_1p5()
                    .flex_shrink_0()
                    .when_some(self.state.active_note.as_ref(), |r, note| {
                        let words: usize = note.content.split_whitespace().count();
                        let chars = note.content.chars().count();
                        r.child(
                            div()
                                .text_sm()
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
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_CODE,
                                15.0,
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
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_BOOK_OPEN,
                                16.0,
                                if !is_editing { theme.accent } else { theme.text_muted },
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit_mode = !this.edit_mode;
                                let current_scroll = this.current_scroll_top();
                                this.state.snapshot_active_note_settings(current_scroll, this.edit_mode, this.word_wrap);
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
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_MORE_VERTICAL, 16.0, theme.text_muted))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_note(window, cx);
                            })),
                    ),
            )
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.initial_scroll_done {
            self.initial_scroll_done = true;
            if self.initial_scroll_top > 0.0 {
                self.set_editor_scroll_top(self.initial_scroll_top);
            } else {
                self.scroll_cursor_into_view();
            }
        }

        let is_maximized = window.is_maximized();
        let bounds = window.bounds();
        let geom = (
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            Some(f32::from(bounds.origin.x)),
            Some(f32::from(bounds.origin.y)),
            is_maximized,
        );
        if self.last_saved_geometry != Some(geom) {
            self.last_saved_geometry = Some(geom);
            let mut ws = crate::state::WindowState::load(&crate::state::app_data_dir());
            ws.maximized = is_maximized;
            if !is_maximized {
                ws.width = geom.0;
                ws.height = geom.1;
                ws.x = geom.2;
                ws.y = geom.3;
            }
            ws.save(&crate::state::app_data_dir());
        }

        let is_title_focused = self.title_focus_handle.is_focused(window);
        if !self.title_is_focused && is_title_focused {
            self.title_is_focused = true;
            self.reset_title_cursor_blink(cx);
        } else if self.title_is_focused && !is_title_focused {
            self.title_is_focused = false;
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
        }

        if self.was_title_focused && !is_title_focused {
            self.commit_title_rename_internal();
        }
        self.was_title_focused = is_title_focused;

        if let Some(ref note) = self.state.active_note {
            if self.title_editing_path.as_ref() != Some(&note.path) {
                self.title_editing_path = Some(note.path.clone());
                self.title_edit_text = note.title.clone();
                self.title_cursor_pos = self.title_edit_text.chars().count();
                self.title_selection_anchor = None;
                self.title_is_mouse_selecting = false;
            }
        } else {
            self.title_editing_path = None;
            self.title_edit_text.clear();
            self.title_cursor_pos = 0;
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
        }

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
        let wrap_cols = self.compute_wrap_cols(viewport_w, line_char_width);
        self.last_wrap_cols = wrap_cols;
        if let Some(ref mut note) = self.state.active_note {
            note.ensure_visual_cache(self.word_wrap, wrap_cols);
        }

        let is_resizer_active = self.is_resizing_sidebar || self.is_hovering_resizer;
        let resizer_color = if is_resizer_active { theme.accent } else { theme.border };

        div()
            .flex()
            .flex_col()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.bg_app)
            .text_color(theme.text_primary)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
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
                } else if this.is_dragging_scrollbar {
                    let y: f32 = event.position.y.into();
                    this.handle_scrollbar_drag(y, window, cx);
                } else if this.title_is_mouse_selecting {
                    let x: f32 = event.position.x.into();
                    let editor_left_x = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                    let title_left_x = editor_left_x + 48.0;
                    let rel_x = (x - title_left_x).max(0.0);
                    let new_pos = if this.title_edit_text.is_empty() {
                        0
                    } else {
                        let run = gpui::TextRun {
                            len: this.title_edit_text.len(),
                            font: gpui::font("Segoe UI").bold(),
                            color: Default::default(),
                            underline: Default::default(),
                            strikethrough: None,
                            background_color: None,
                        };
                        let shaped = window.text_system().shape_line(
                            SharedString::from(this.title_edit_text.clone()),
                            px(28.0),
                            &[run],
                            None,
                        );
                        let byte_idx = shaped.closest_index_for_x(px(rel_x));
                        this.title_edit_text[..byte_idx.min(this.title_edit_text.len())].chars().count()
                    };
                    if this.title_cursor_pos != new_pos {
                        this.title_cursor_pos = new_pos;
                        this.title_cursor_visible = true;
                        cx.notify();
                    }
                } else if this.is_mouse_selecting {
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    let viewport_h: f32 = window.viewport_size().height.into();
                    this.update_mouse_selection(x, y, viewport_h, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.is_settings_open {
                    return;
                }
                if this.title_is_mouse_selecting {
                    this.title_is_mouse_selecting = false;
                    if this.title_selection_anchor == Some(this.title_cursor_pos) {
                        this.title_selection_anchor = None;
                    }
                    cx.notify();
                }
                this.is_resizing_sidebar = false;
                this.is_mouse_selecting = false;
                this.is_dragging_scrollbar = false;
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
                if this.title_is_mouse_selecting {
                    this.title_is_mouse_selecting = false;
                    if this.title_selection_anchor == Some(this.title_cursor_pos) {
                        this.title_selection_anchor = None;
                    }
                    cx.notify();
                }
                this.is_resizing_sidebar = false;
                this.is_hovering_resizer = false;
                this.is_mouse_selecting = false;
                this.is_dragging_scrollbar = false;
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
                                .border_b_1()
                                .border_color(theme.border)
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
                                // 2. File tree header (VIBECODIUM badge and drag region)
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
                                                .child("VIBECODIUM"),
                                        )
                                        .child(
                                            div()
                                                .id("sidebar-titlebar-drag-region")
                                                .flex_1()
                                                .h_full()
                                                .window_control_area(WindowControlArea::Drag),
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
                                .border_b_1()
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
                    // Vertical divider (1px border line + 7px transparent hitbox) matching the sidebar-resizer below exactly
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("titlebar-resizer-divider")
                                .relative()
                                .w(px(1.0))
                                .h_full()
                                .flex_shrink_0()
                                .bg(resizer_color)
                                .border_b_1()
                                .border_color(theme.border)
                                .when(is_resizer_active, |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .bottom_0()
                                            .left(px(-1.0))
                                            .w(px(3.0))
                                            .bg(theme.accent),
                                    )
                                })
                                .child(
                                    div()
                                        .id("titlebar-resizer-hitbox")
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(px(-3.0))
                                        .w(px(7.0))
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
                                        })),
                                ),
                        )
                    })
                    // Right area: Editor header column containing Tab Bar, Drag Area, and Window Controls
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .overflow_hidden()
                            .bg(theme.bg_sidebar)
                            // Continuous bottom border line running across the editor header column
                            .child(
                                div()
                                    .id("titlebar-editor-bottom-border")
                                    .absolute()
                                    .bottom_0()
                                    .left_0()
                                    .right_0()
                                    .h(px(1.0))
                                    .bg(theme.border),
                            )
                            // Browser-style Tabs Area starting directly at the boundary
                            .child({
                                let mut tabs_bar = div()
                                    .flex()
                                    .flex_row()
                                    .items_end()
                                    .gap_2()
                                    .h_full()
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

                                    let mut tab_item = div()
                                        .id(gpui::ElementId::from(("tab-item", idx)))
                                        .relative()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .h(px(34.0))
                                        .when(!is_active, |tab| {
                                            tab.bg(theme.bg_sidebar)
                                                .rounded_t_md()
                                                .border_b_1()
                                                .border_color(theme.border)
                                                .hover(|h| h.bg(theme.bg_hover))
                                        });

                                    if is_active {
                                        // 1. Solid center body fill (stops 7px before tab edges so it never bleeds into corners/vertical borders, and starts at top 1px below top border)
                                        tab_item = tab_item.child(
                                            div()
                                                .id("tab-body-fill")
                                                .absolute()
                                                .top(px(1.0))
                                                .bottom_0()
                                                .left(px(7.0))
                                                .right(px(7.0))
                                                .bg(theme.bg_editor),
                                        );
                                        // 2. Solid fill mask at the baseline connecting active tab into the editor
                                        tab_item = tab_item.child(
                                            div()
                                                .id("tab-bottom-cover")
                                                .absolute()
                                                .bottom(px(-2.0))
                                                .left(px(7.0))
                                                .right(px(7.0))
                                                .h(px(3.0))
                                                .bg(theme.bg_editor),
                                        );
                                        // 3. Continuous top border between corner curves (from x=7.0 to x=W-7.0)
                                        tab_item = tab_item.child(
                                            div()
                                                .id("tab-border-top")
                                                .absolute()
                                                .top_0()
                                                .left(px(7.0))
                                                .right(px(7.0))
                                                .h(px(1.0))
                                                .bg(theme.border),
                                        );
                                        // 4. Left cap (bottom concave flair + vertical line + top convex corner + 2px horizontal top extension)
                                        tab_item = tab_item.child(
                                            div()
                                                .id("tab-cap-left")
                                                .absolute()
                                                .top_0()
                                                .left(px(-8.0))
                                                .w(px(16.0))
                                                .h(px(36.0))
                                                .child(
                                                    icons::svg_sized(icons::ICON_TAB_ACTIVE_LEFT_FILL, 16.0, 36.0, theme.bg_editor)
                                                        .absolute()
                                                        .top_0()
                                                        .left_0(),
                                                )
                                                .child(
                                                    icons::svg_sized(icons::ICON_TAB_ACTIVE_LEFT_STROKE, 16.0, 36.0, theme.border)
                                                        .absolute()
                                                        .top_0()
                                                        .left_0(),
                                                ),
                                        );
                                        // 5. Right cap (2px horizontal top extension + top convex corner + vertical line + bottom concave flair)
                                        tab_item = tab_item.child(
                                            div()
                                                .id("tab-cap-right")
                                                .absolute()
                                                .top_0()
                                                .right(px(-8.0))
                                                .w(px(16.0))
                                                .h(px(36.0))
                                                .child(
                                                    icons::svg_sized(icons::ICON_TAB_ACTIVE_RIGHT_FILL, 16.0, 36.0, theme.bg_editor)
                                                        .absolute()
                                                        .top_0()
                                                        .left_0(),
                                                )
                                                .child(
                                                    icons::svg_sized(icons::ICON_TAB_ACTIVE_RIGHT_STROKE, 16.0, 36.0, theme.border)
                                                        .absolute()
                                                        .top_0()
                                                        .left_0(),
                                                ),
                                        );
                                    }

                                    tabs_bar = tabs_bar.child(
                                        tab_item
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
                                                            this.select_note(tab_path_for_select.clone(), true, window, cx);
                                                        } else {
                                                            this.need_scroll_sidebar_to_active.set(true);
                                                            this.state.reveal_path_in_tree(&tab_path_for_select);
                                                            cx.notify();
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
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .h(px(34.0))
                                        .ml_1()
                                        .child(
                                            div()
                                                .id("btn-new-tab")
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .w(px(28.0))
                                                .h(px(28.0))
                                                .rounded_md()
                                                .cursor_pointer()
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .child(icons::icon(icons::ICON_PLUS, 16.0, theme.text_muted))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.create_new_note(window, cx);
                                                })),
                                        ),
                                )
                            })
                            // Drag Area (fills remaining space between tabs and window controls)
                            .child(
                                div()
                                    .id("titlebar-drag-region")
                                    .flex_1()
                                    .h_full()
                                    .window_control_area(WindowControlArea::Drag),
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
                                            .child(icons::icon(icons::ICON_MINIMIZE, 12.0, theme.text_secondary))
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
                                                12.0,
                                                theme.text_secondary,
                                            ))
                                            .on_click(cx.listener(|_, _, window, _| {
                                                window.zoom_window();
                                            })),
                                    )
                                    .child({
                                        let is_hovering_close = self.is_hovering_close;
                                        div()
                                            .id("win-ctrl-close")
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(46.0))
                                            .h_full()
                                            .cursor_pointer()
                                            .window_control_area(WindowControlArea::Close)
                                            .hover(|h| h.bg(rgb(0xe81123)))
                                            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                                if this.is_hovering_close != *hovered {
                                                    this.is_hovering_close = *hovered;
                                                    cx.notify();
                                                }
                                            }))
                                            .child(icons::icon(
                                                icons::ICON_WINDOW_CLOSE,
                                                12.0,
                                                if is_hovering_close { rgb(0xffffff) } else { theme.text_secondary },
                                            ))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.save_on_exit(window);
                                                cx.quit();
                                            }))
                                    }),
                            ),
                    ),
            )
            // ================= MAIN BODY =================
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_row()
                    .w_full()
                    .min_w_0()
                    .min_h_0()
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
                            let mut selected_tree_index = None;
                            self.render_tree_nodes(
                                &self.state.file_tree,
                                0,
                                active_path.as_ref(),
                                theme,
                                &mut tree_elements,
                                &mut selected_tree_index,
                                cx,
                            );

                            if !self.initial_sidebar_scroll_done.get() {
                                self.initial_sidebar_scroll_done.set(true);
                                if self.state.session.sidebar_scroll_top < 0.0 {
                                    self.sidebar_scroll_handle.set_offset(point(px(0.0), px(self.state.session.sidebar_scroll_top)));
                                } else if let Some(ix) = selected_tree_index {
                                    let tree_viewport_h = (f32::from(window.bounds().size.height) - 144.0).max(100.0);
                                    let visible_items = (tree_viewport_h / 26.0).floor() as usize;
                                    let half = (visible_items / 2).max(1);
                                    let target_top_ix = ix.saturating_sub(half);
                                    self.sidebar_scroll_handle.scroll_to_top_of_item(target_top_ix);
                                }
                            } else if self.need_scroll_sidebar_to_active.get() {
                                self.need_scroll_sidebar_to_active.set(false);
                                if let Some(ix) = selected_tree_index {
                                    let tree_viewport_h = (f32::from(window.bounds().size.height) - 144.0).max(100.0);
                                    let visible_items = (tree_viewport_h / 26.0).floor() as usize;
                                    let half = (visible_items / 2).max(1);
                                    let target_top_ix = ix.saturating_sub(half);
                                    self.sidebar_scroll_handle.scroll_to_top_of_item(target_top_ix);
                                }
                            }

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
                                    .track_scroll(&self.sidebar_scroll_handle)
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
                    // ----- Sidebar Resizer Divider (1px border line + 7px transparent hitbox) -----
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("sidebar-resizer")
                                .relative()
                                .flex_shrink_0()
                                .w(px(1.0))
                                .h_full()
                                .bg(resizer_color)
                                .when(is_resizer_active, |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .bottom_0()
                                            .left(px(-1.0))
                                            .w(px(3.0))
                                            .bg(theme.accent),
                                    )
                                })
                                .child(
                                    div()
                                        .id("sidebar-resizer-hitbox")
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(px(-3.0))
                                        .w(px(7.0))
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
                                        })),
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
                            .min_h_0()
                            .overflow_hidden()
                            .bg(theme.bg_editor)
                            // Панель навигации (Obsidian-style) сразу под вкладками
                            .child(self.render_navigation_bar(theme, cx))
                            .when_some(self.state.active_note.as_ref(), |this, note| {
                                let note_title = note
                                    .path
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| "Untitled".to_string());

                                let lines_arc = Arc::clone(&note.cached_lines);
                                let visual_rows_arc = Arc::clone(&note.cached_visual_rows);
                                let line_starts_arc = Arc::clone(&note.cached_line_starts);
                                let code_states_arc = Arc::clone(&note.cached_code_states);
                                let visual_rows_count = visual_rows_arc.len();

                                let (cursor_line, cursor_col) = note.line_col();
                                let (active_v_idx, active_col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows_arc, cursor_line, cursor_col)
                                    .unwrap_or((0, 0));

                                let active_block_id: Option<usize> = if is_editing {
                                    code_states_arc.get(cursor_line).and_then(|s| s.as_ref()).map(|s| s.block_id)
                                } else {
                                    None
                                };
                                let line_h = px(24.0);
                                let cursor_h = px(16.0);
                                let cursor_top = 4.0;
                                let editor_left_x = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
                                let text_start_x = editor_left_x + 32.0 + 8.0 + 40.0;

                                let theme_copy = theme.clone();
                                let theme_scrollbar = theme.clone();

                                let viewport_h: f32 = window.viewport_size().height.into();
                                let editor_h = (viewport_h - 170.0).max(100.0);
                                let total_content_h = (visual_rows_count as f32) * 24.0 + 48.0;
                                let show_scrollbar = total_content_h > editor_h;
                                let max_scroll_content = (total_content_h - editor_h).max(1.0);
                                let (thumb_top, thumb_h) = if show_scrollbar {
                                    let ratio = (editor_h / total_content_h).clamp(0.04, 1.0);
                                    let th = (editor_h * ratio).max(28.0);
                                    let scroll_y = {
                                        let offset = self.editor_scroll_handle.0.borrow().base_handle.offset();
                                        f32::from(-offset.y).max(0.0)
                                    };
                                    let scroll_fraction = (scroll_y / max_scroll_content).clamp(0.0, 1.0);
                                    let tt = scroll_fraction * (editor_h - th);
                                    (tt, th)
                                } else {
                                    (0.0, 28.0)
                                };
                                let is_scrollbar_dragging = self.is_dragging_scrollbar;
                                let scroll_y_current = {
                                    let offset = self.editor_scroll_handle.0.borrow().base_handle.offset();
                                    f32::from(-offset.y).max(0.0)
                                };

                                let is_title_focused = self.title_focus_handle.is_focused(window);
                                let title_text = &self.title_edit_text;
                                let title_chars: Vec<char> = title_text.chars().collect();
                                let cursor_pos = self.title_cursor_pos.min(title_chars.len());
                                let selection_range = self.title_selection_range();

                                let run = gpui::TextRun {
                                    len: title_text.len(),
                                    font: gpui::font("Segoe UI").bold(),
                                    color: Default::default(),
                                    underline: Default::default(),
                                    strikethrough: None,
                                    background_color: None,
                                };
                                let shaped = window.text_system().shape_line(
                                    SharedString::from(title_text.clone()),
                                    px(28.0),
                                    &[run],
                                    None,
                                );

                                let cursor_x = if title_text.is_empty() {
                                    0.0
                                } else {
                                    let byte_index: usize = title_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
                                    f32::from(shaped.x_for_index(byte_index))
                                };

                                let (sel_x1, sel_w) = if let Some((s_start, s_end)) = selection_range {
                                    if !title_text.is_empty() {
                                        let b1: usize = title_chars[..s_start.min(title_chars.len())].iter().map(|c| c.len_utf8()).sum();
                                        let b2: usize = title_chars[..s_end.min(title_chars.len())].iter().map(|c| c.len_utf8()).sum();
                                        let x1 = f32::from(shaped.x_for_index(b1));
                                        let x2 = f32::from(shaped.x_for_index(b2));
                                        (x1, (x2 - x1).max(0.0))
                                    } else {
                                        (0.0, 0.0)
                                    }
                                } else {
                                    (0.0, 0.0)
                                };

                                let display_title = if title_text.is_empty() {
                                    if is_title_focused {
                                        String::new()
                                    } else {
                                        note_title.clone()
                                    }
                                } else {
                                    title_text.clone()
                                };

                                this.child(
                                    div()
                                        .id("note-title-panel")
                                        .w_full()
                                        .flex()
                                        .flex_row()
                                        .items_start()
                                        .pl(px(48.0))
                                        .pr_4()
                                        .pt(px(2.0))
                                        .pb(px(16.0))
                                        .cursor(gpui::CursorStyle::IBeam)
                                        .bg(theme.bg_editor)
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                            window.focus(&this.title_focus_handle);
                                            this.title_is_focused = true;
                                            this.reset_title_cursor_blink(cx);
                                            this.title_cursor_pos = this.title_edit_text.chars().count();
                                            this.title_selection_anchor = None;
                                            cx.notify();
                                        }))
                                        .child(
                                            div()
                                                .id("note-title-input")
                                                .relative()
                                                .flex_1()
                                                .min_w_0()
                                                .h(px(36.0))
                                                .track_focus(&self.title_focus_handle)
                                                .cursor(gpui::CursorStyle::IBeam)
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                                    cx.stop_propagation();
                                                    window.focus(&this.title_focus_handle);
                                                    this.title_is_focused = true;
                                                    this.reset_title_cursor_blink(cx);

                                                    let click_x: f32 = event.position.x.into();
                                                    let editor_left_x = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                                                    let title_left_x = editor_left_x + 48.0;
                                                    let rel_x = (click_x - title_left_x).max(0.0);

                                                    let clicked_pos = if this.title_edit_text.is_empty() {
                                                        0
                                                    } else {
                                                        let run = gpui::TextRun {
                                                            len: this.title_edit_text.len(),
                                                            font: gpui::font("Segoe UI").bold(),
                                                            color: Default::default(),
                                                            underline: Default::default(),
                                                            strikethrough: None,
                                                            background_color: None,
                                                        };
                                                        let shaped = window.text_system().shape_line(
                                                            SharedString::from(this.title_edit_text.clone()),
                                                            px(28.0),
                                                            &[run],
                                                            None,
                                                        );
                                                        let byte_idx = shaped.closest_index_for_x(px(rel_x));
                                                        this.title_edit_text[..byte_idx.min(this.title_edit_text.len())].chars().count()
                                                    };

                                                    if event.click_count == 2 {
                                                        this.title_selection_anchor = Some(0);
                                                        this.title_cursor_pos = this.title_edit_text.chars().count();
                                                        this.title_is_mouse_selecting = false;
                                                    } else if event.modifiers.shift {
                                                        if this.title_selection_anchor.is_none() {
                                                            this.title_selection_anchor = Some(this.title_cursor_pos);
                                                        }
                                                        this.title_cursor_pos = clicked_pos;
                                                        this.title_is_mouse_selecting = true;
                                                    } else {
                                                        this.title_cursor_pos = clicked_pos;
                                                        this.title_selection_anchor = Some(clicked_pos);
                                                        this.title_is_mouse_selecting = true;
                                                    }
                                                    cx.notify();
                                                }))
                                                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                                    cx.stop_propagation();
                                                    this.handle_title_key(event, window, cx);
                                                }))
                                                .when(selection_range.is_some() && sel_w > 0.0, |s| {
                                                    s.child(
                                                        div()
                                                            .absolute()
                                                            .left(px(sel_x1))
                                                            .top(px(4.0))
                                                            .w(px(sel_w))
                                                            .h(px(28.0))
                                                            .bg(theme.selection_bg)
                                                            .rounded_xs(),
                                                    )
                                                })
                                                .child(
                                                    div()
                                                        .font_family("Segoe UI")
                                                        .font_weight(gpui::FontWeight::BOLD)
                                                        .text_size(px(28.0))
                                                        .line_height(px(36.0))
                                                        .text_color(rgb(0xf44336))
                                                        .child(display_title),
                                                )
                                                .when(title_text.is_empty() && is_title_focused, |s| {
                                                    s.child(
                                                        div()
                                                            .absolute()
                                                            .left_0()
                                                            .top_0()
                                                            .h(px(36.0))
                                                            .font_family("Segoe UI")
                                                            .font_weight(gpui::FontWeight::BOLD)
                                                            .text_size(px(28.0))
                                                            .line_height(px(36.0))
                                                            .text_color(rgba(0xf4433666))
                                                            .child("Untitled"),
                                                    )
                                                })
                                                .when(is_title_focused && self.title_cursor_visible && selection_range.is_none(), |s| {
                                                    s.child(
                                                        div()
                                                            .absolute()
                                                            .left(px(cursor_x))
                                                            .top(px(5.0))
                                                            .w(px(2.0))
                                                            .h(px(26.0))
                                                            .bg(rgb(0xf44336))
                                                            .rounded_xs(),
                                                    )
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("note-editor-container")
                                        .relative()
                                        .track_focus(&self.focus_handle)
                                        .cursor(gpui::CursorStyle::IBeam)
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .w_full()
                                        .min_w_0()
                                        .min_h_0()
                                        .overflow_hidden()
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                            if this.is_resizing_sidebar {
                                                return;
                                            }
                                            this.commit_title_rename(window, cx);
                                            window.focus(&this.focus_handle);
                                            this.is_mouse_selecting = true;
                                            if let Some(ref mut n) = this.state.active_note {
                                                let end_offset = n.content.len();
                                                if event.modifiers.shift {
                                                    if n.selection_anchor.is_none() {
                                                        n.selection_anchor = Some(n.cursor_offset);
                                                    }
                                                } else {
                                                    n.selection_anchor = Some(end_offset);
                                                }
                                                n.cursor_offset = end_offset;
                                                n.sync_cursor();
                                            }
                                            cx.notify();
                                        }))
                                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                                            if this.is_resizing_sidebar {
                                                let x: f32 = event.position.x.into();
                                                if x < 100.0 {
                                                    this.sidebar_collapsed = true;
                                                } else {
                                                    this.sidebar_collapsed = false;
                                                    this.sidebar_width = x.clamp(160.0, 600.0);
                                                }
                                                cx.notify();
                                            } else if this.is_dragging_scrollbar {
                                                let y: f32 = event.position.y.into();
                                                this.handle_scrollbar_drag(y, window, cx);
                                            } else if this.is_mouse_selecting {
                                                let x: f32 = event.position.x.into();
                                                let y: f32 = event.position.y.into();
                                                let viewport_h: f32 = window.viewport_size().height.into();
                                                this.update_mouse_selection(x, y, viewport_h, cx);
                                            }
                                        }))
                                        .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                            this.is_resizing_sidebar = false;
                                            this.is_dragging_scrollbar = false;
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
                                            this.is_dragging_scrollbar = false;
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

                                                    crate::perf::PERF.shape_calls.fetch_add(range.len() as u64, std::sync::atomic::Ordering::Relaxed);
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
                                                                .w_full()
                                                                .flex()
                                                                .flex_row()
                                                                .items_center()
                                                                .h(line_h)
                                                                .py_0()
                                                                .px_2()
                                                                .rounded_sm()
                                                                .cursor(gpui::CursorStyle::IBeam)
                                                                .when(is_current, |s| s.bg(theme.bg_hover))
                                                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                                    cx.stop_propagation();
                                                                    if this.is_resizing_sidebar {
                                                                        return;
                                                                    }
                                                                    this.is_mouse_selecting = true;
                                                                    let click_x: f32 = event.position.x.into();

                                                                    let col_in_row = if click_x > text_start_x {
                                                                        let raw_col = ((click_x - text_start_x) / line_char_width).round() as usize;
                                                                        crate::markdown::visual_col_to_row_char(&row_spans_down, raw_col, is_active_line, hidden_prefix_len).min(row_len)
                                                                    } else {
                                                                        0
                                                                    };
                                                                    let target_char = v_char_start + col_in_row;
                                                                    if let Some(ref mut n) = this.state.active_note {
                                                                        let offset = n.offset_for_line_col(v_line_idx, target_char);
                                                                        if event.click_count == 2 {
                                                                            let (w_start, w_end) = n.word_range_at(offset);
                                                                            n.selection_anchor = Some(w_start);
                                                                            n.cursor_offset = w_end;
                                                                        } else if event.click_count >= 3 {
                                                                            let (l_start, l_end) = n.line_range_at(offset);
                                                                            n.selection_anchor = Some(l_start);
                                                                            n.cursor_offset = l_end;
                                                                        } else if event.modifiers.shift {
                                                                            if n.selection_anchor.is_none() {
                                                                                n.selection_anchor = Some(n.cursor_offset);
                                                                            }
                                                                            n.cursor_offset = offset;
                                                                        } else {
                                                                            // Single click anywhere on row places cursor and CLEARS existing selection
                                                                            n.cursor_offset = offset;
                                                                            n.selection_anchor = Some(offset);
                                                                        }
                                                                        n.sync_cursor();
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
                                            .pt_0()
                                            .pb_6()
                                        )
                                        .when(show_scrollbar, |c| {
                                            c.child(
                                                div()
                                                    .id("editor-scrollbar-track")
                                                    .absolute()
                                                    .top_0()
                                                    .bottom_0()
                                                    .right_0()
                                                    .w(px(14.0))
                                                    .h_full()
                                                    .cursor_pointer()
                                                    .hover(|h| h.bg(rgba(0x00000015)))
                                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let click_y: f32 = event.position.y.into();
                                                        let click_rel_y = (click_y - 132.0).max(0.0);
                                                        let target_fraction = (click_rel_y / editor_h).clamp(0.0, 1.0);
                                                        let target_scroll_y = target_fraction * max_scroll_content;
                                                        this.set_editor_scroll_top(target_scroll_y);
                                                        cx.notify();
                                                    }))
                                                    .child(
                                                        div()
                                                            .id("editor-scrollbar-thumb")
                                                            .absolute()
                                                            .top(px(thumb_top))
                                                            .right(px(3.0))
                                                            .w(px(if is_scrollbar_dragging { 8.0 } else { 6.0 }))
                                                            .h(px(thumb_h))
                                                            .rounded_full()
                                                            .bg(if is_scrollbar_dragging {
                                                                theme_scrollbar.accent
                                                            } else {
                                                                theme_scrollbar.border
                                                            })
                                                            .hover(|h| h.bg(theme_scrollbar.text_secondary).w(px(8.0)))
                                                            .cursor_pointer()
                                                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                                                cx.stop_propagation();
                                                                this.is_dragging_scrollbar = true;
                                                                this.scrollbar_drag_start_y = event.position.y.into();
                                                                this.scrollbar_drag_start_scroll_y = scroll_y_current;
                                                                cx.notify();
                                                            }))
                                                    )
                                            )
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
                                                .child("Welcome to Vibecodium"),
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
                    .id("app-status-bar")
                    .flex()
                    .flex_shrink_0()
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
                crate::markdown::SyntaxSpan::LinkText(l) => {
                    Some(
                        div()
                            .underline()
                            .text_color(theme.accent)
                            .child(l),
                    )
                }
                crate::markdown::SyntaxSpan::Underline(u) => {
                    Some(
                        div()
                            .underline()
                            .text_color(theme.text_primary)
                            .child(u),
                    )
                }
                crate::markdown::SyntaxSpan::Highlight(h) => {
                    let hl_bg = if theme.is_dark { gpui::rgba(0xf9e2af44) } else { gpui::rgba(0xdf8e1d33) };
                    Some(
                        div()
                            .bg(hl_bg)
                            .text_color(theme.text_primary)
                            .rounded_xs()
                            .child(h),
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
        crate::markdown::LineKind::Heading5(_) => {
            let rest = line.strip_prefix("##### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(6));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("##### "));
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
                    char_start.saturating_sub(6),
                    char_end.saturating_sub(6),
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
        crate::markdown::LineKind::Heading6(_) => {
            let rest = line.strip_prefix("###### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(7));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("###### "));
                }
                row.child(
                    div()
                        .italic()
                        .text_color(theme.text_muted)
                        .child(render_syntax_spans(row_spans, is_active_line, theme)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(7),
                    char_end.saturating_sub(7),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(
                        div()
                            .italic()
                            .text_color(theme.text_muted)
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
            if is_active_line {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .text_color(theme.text_muted)
                    .child(line.to_string())
            } else {
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        div()
                            .h(px(1.0))
                            .w_full()
                            .bg(theme.border_subtle),
                    )
            }
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

