use std::cell::Cell;
use std::rc::Rc;
use gpui::{
    div, prelude::*, px, ClipboardItem, Context, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, SharedString, Window,
};
use crate::icons;
use crate::views::app_view::AppView;

/// Encapsulates the complete state of the quick filter input in the explorer sidebar.
#[derive(Clone)]
pub struct QuickFilterState {
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub is_mouse_selecting: bool,
    pub is_focused: bool,
    pub cursor_visible: bool,
    pub cursor_blink_epoch: usize,
    pub box_origin_x: Rc<Cell<f32>>,
}

impl Default for QuickFilterState {
    fn default() -> Self {
        Self::new()
    }
}

impl QuickFilterState {
    pub fn new() -> Self {
        Self {
            cursor_pos: 0,
            selection_anchor: None,
            is_mouse_selecting: false,
            is_focused: false,
            cursor_visible: true,
            cursor_blink_epoch: 0,
            box_origin_x: Rc::new(Cell::new(0.0)),
        }
    }

    /// Returns the normalized `(start, end)` character range of the current selection, if any.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.selection_anchor {
            let pos = self.cursor_pos;
            if anchor != pos {
                return Some((anchor.min(pos), anchor.max(pos)));
            }
        }
        None
    }

    /// Resets the cursor blink cycle and restarts the asynchronous 530ms blink ticker.
    pub fn reset_cursor_blink(&mut self, cx: &mut Context<AppView>) {
        self.cursor_visible = true;
        self.cursor_blink_epoch = self.cursor_blink_epoch.wrapping_add(1);
        let epoch = self.cursor_blink_epoch;
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();
        cx.foreground_executor().spawn(async move {
            loop {
                timer_executor.timer(std::time::Duration::from_millis(530)).await;
                let should_continue = entity.update(&mut async_cx, |this, cx| {
                    if this.quick_filter.cursor_blink_epoch != epoch || !this.quick_filter.is_focused {
                        false
                    } else {
                        this.quick_filter.cursor_visible = !this.quick_filter.cursor_visible;
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

    /// Synchronizes internal focus state with GPUI focus handle state.
    pub fn sync_focus(&mut self, is_focused: bool, cx: &mut Context<AppView>) {
        if !self.is_focused && is_focused {
            self.is_focused = true;
            self.reset_cursor_blink(cx);
        } else if self.is_focused && !is_focused {
            self.is_focused = false;
            self.selection_anchor = None;
            self.is_mouse_selecting = false;
        }
    }

    /// Focuses the quick filter and selects all existing text (e.g. for Ctrl+F).
    pub fn focus_and_select_all(&mut self, query: &str, cx: &mut Context<AppView>) {
        self.is_focused = true;
        self.selection_anchor = Some(0);
        self.cursor_pos = query.chars().count();
        self.reset_cursor_blink(cx);
        cx.notify();
    }

    /// Handles mouse drag selection when cursor moves across or outside the window.
    pub fn handle_mouse_move(&mut self, event: &MouseMoveEvent, query: &str, window: &mut Window, cx: &mut Context<AppView>) {
        if !self.is_mouse_selecting {
            return;
        }
        let x: f32 = event.position.x.into();
        let box_x = self.box_origin_x.get();
        let rel_x = (x - box_x).max(0.0);
        let new_pos = if query.is_empty() {
            0
        } else {
            let run = gpui::TextRun {
                len: query.len(),
                font: gpui::font("Segoe UI"),
                color: Default::default(),
                underline: Default::default(),
                strikethrough: None,
                background_color: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(query.to_string()),
                px(12.0),
                &[run],
                None,
            );
            let byte_idx = shaped.closest_index_for_x(px(rel_x));
            query[..byte_idx.min(query.len())].chars().count()
        };
        if self.cursor_pos != new_pos {
            self.cursor_pos = new_pos;
            self.cursor_visible = true;
            cx.notify();
        }
    }

    /// Handles mouse up: terminates drag-to-select and collapses zero-width selection.
    pub fn handle_mouse_up(&mut self, cx: &mut Context<AppView>) {
        if self.is_mouse_selecting {
            self.is_mouse_selecting = false;
            if self.selection_anchor == Some(self.cursor_pos) {
                self.selection_anchor = None;
            }
            cx.notify();
        }
    }

    /// Clears the search query and resets all selection and cursor positions.
    pub fn clear(&mut self, query: &mut String, cx: &mut Context<AppView>) {
        query.clear();
        self.cursor_pos = 0;
        self.selection_anchor = None;
        self.is_mouse_selecting = false;
        cx.notify();
    }
}

impl AppView {
    /// Dispatches keyboard events for the sidebar quick filter input.
    pub fn handle_search_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.quick_filter.reset_cursor_blink(cx);
        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let shift = event.keystroke.modifiers.shift;
        let alt = event.keystroke.modifiers.alt;

        // Escape: clear query and focus editor
        if key == "escape" {
            self.quick_filter.selection_anchor = None;
            self.state.search_query.clear();
            self.quick_filter.cursor_pos = 0;
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        // Enter: open first matching note
        if key == "enter" {
            if let Some(first_match) = self.first_matching_file(&self.state.file_tree) {
                self.select_note(first_match, true, window, cx);
            }
            return;
        }

        // Ctrl + A (Select All)
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.quick_filter.selection_anchor = Some(0);
            self.quick_filter.cursor_pos = self.state.search_query.chars().count();
            cx.notify();
            return;
        }

        // Ctrl + C (Copy selected text)
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s_start, s_end)) = self.quick_filter.selection_range() {
                let selected: String = self.state.search_query.chars().skip(s_start).take(s_end - s_start).collect();
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
            }
            return;
        }

        // Ctrl + X (Cut selected text)
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s_start, s_end)) = self.quick_filter.selection_range() {
                let selected: String = self.state.search_query.chars().skip(s_start).take(s_end - s_start).collect();
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
                let mut chars: Vec<char> = self.state.search_query.chars().collect();
                chars.drain(s_start..s_end);
                self.state.search_query = chars.into_iter().collect();
                self.quick_filter.cursor_pos = s_start;
                self.quick_filter.selection_anchor = None;
                cx.notify();
            }
            return;
        }

        // Ctrl + V (Paste over selection or at cursor)
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text
                        .chars()
                        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                        .collect();
                    if !filtered.is_empty() {
                        let mut chars: Vec<char> = self.state.search_query.chars().collect();
                        let pos = if let Some((s_start, s_end)) = self.quick_filter.selection_range() {
                            chars.drain(s_start..s_end);
                            self.quick_filter.selection_anchor = None;
                            s_start
                        } else {
                            self.quick_filter.cursor_pos.min(chars.len())
                        };
                        for (i, c) in filtered.chars().enumerate() {
                            chars.insert(pos + i, c);
                        }
                        self.quick_filter.cursor_pos = pos + filtered.chars().count();
                        self.state.search_query = chars.into_iter().collect();
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Other Ctrl shortcuts
        if ctrl {
            if key.eq_ignore_ascii_case("s") || key == "ы" || key == "Ы" {
                self.save_note(window, cx);
            } else if key.eq_ignore_ascii_case("o") || key == "щ" || key == "Щ" {
                self.open_folder_dialog(window, cx);
            } else if key.eq_ignore_ascii_case("n") || key == "т" || key == "Т" {
                self.create_new_note(window, cx);
            }
            return;
        }

        // Left Arrow
        if key == "left" {
            if shift {
                if self.quick_filter.selection_anchor.is_none() {
                    self.quick_filter.selection_anchor = Some(self.quick_filter.cursor_pos);
                }
                if self.quick_filter.cursor_pos > 0 {
                    self.quick_filter.cursor_pos -= 1;
                }
            } else {
                if let Some((s_start, _)) = self.quick_filter.selection_range() {
                    self.quick_filter.cursor_pos = s_start;
                    self.quick_filter.selection_anchor = None;
                } else if self.quick_filter.cursor_pos > 0 {
                    self.quick_filter.cursor_pos -= 1;
                }
            }
            cx.notify();
            return;
        }

        // Right Arrow
        if key == "right" {
            let total_chars = self.state.search_query.chars().count();
            if shift {
                if self.quick_filter.selection_anchor.is_none() {
                    self.quick_filter.selection_anchor = Some(self.quick_filter.cursor_pos);
                }
                if self.quick_filter.cursor_pos < total_chars {
                    self.quick_filter.cursor_pos += 1;
                }
            } else {
                if let Some((_, s_end)) = self.quick_filter.selection_range() {
                    self.quick_filter.cursor_pos = s_end;
                    self.quick_filter.selection_anchor = None;
                } else if self.quick_filter.cursor_pos < total_chars {
                    self.quick_filter.cursor_pos += 1;
                }
            }
            cx.notify();
            return;
        }

        // Home
        if key == "home" {
            if shift {
                if self.quick_filter.selection_anchor.is_none() {
                    self.quick_filter.selection_anchor = Some(self.quick_filter.cursor_pos);
                }
                self.quick_filter.cursor_pos = 0;
            } else {
                self.quick_filter.selection_anchor = None;
                self.quick_filter.cursor_pos = 0;
            }
            cx.notify();
            return;
        }

        // End
        if key == "end" {
            let total_chars = self.state.search_query.chars().count();
            if shift {
                if self.quick_filter.selection_anchor.is_none() {
                    self.quick_filter.selection_anchor = Some(self.quick_filter.cursor_pos);
                }
                self.quick_filter.cursor_pos = total_chars;
            } else {
                self.quick_filter.selection_anchor = None;
                self.quick_filter.cursor_pos = total_chars;
            }
            cx.notify();
            return;
        }

        // Backspace
        if key == "backspace" {
            let mut chars: Vec<char> = self.state.search_query.chars().collect();
            if let Some((s_start, s_end)) = self.quick_filter.selection_range() {
                chars.drain(s_start..s_end);
                self.quick_filter.cursor_pos = s_start;
                self.quick_filter.selection_anchor = None;
                self.state.search_query = chars.into_iter().collect();
                cx.notify();
            } else {
                let pos = self.quick_filter.cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.quick_filter.cursor_pos = pos - 1;
                    self.state.search_query = chars.into_iter().collect();
                    cx.notify();
                }
            }
            return;
        }

        // Delete
        if key == "delete" {
            let mut chars: Vec<char> = self.state.search_query.chars().collect();
            if let Some((s_start, s_end)) = self.quick_filter.selection_range() {
                chars.drain(s_start..s_end);
                self.quick_filter.cursor_pos = s_start;
                self.quick_filter.selection_anchor = None;
                self.state.search_query = chars.into_iter().collect();
                cx.notify();
            } else {
                let pos = self.quick_filter.cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                    self.state.search_query = chars.into_iter().collect();
                    cx.notify();
                }
            }
            return;
        }

        // Normal character input
        if !ctrl && !alt && !event.keystroke.modifiers.platform {
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
                    .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                    .collect();
                if !filtered.is_empty() {
                    let mut chars: Vec<char> = self.state.search_query.chars().collect();
                    let pos = if let Some((s_start, s_end)) = self.quick_filter.selection_range() {
                        chars.drain(s_start..s_end);
                        self.quick_filter.selection_anchor = None;
                        s_start
                    } else {
                        self.quick_filter.cursor_pos.min(chars.len())
                    };
                    for (i, c) in filtered.chars().enumerate() {
                        chars.insert(pos + i, c);
                    }
                    self.quick_filter.cursor_pos = pos + filtered.chars().count();
                    self.state.search_query = chars.into_iter().collect();
                    cx.notify();
                }
            }
        }
    }

    /// Renders the complete quick filter component.
    pub fn render_quick_filter(&self, window: &mut Window, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let is_search_focused = self.search_focus_handle.is_focused(window);

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
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                window.focus(&this.search_focus_handle);
                this.quick_filter.is_focused = true;
                this.quick_filter.reset_cursor_blink(cx);

                let click_x: f32 = event.position.x.into();
                let box_x = this.quick_filter.box_origin_x.get();
                let rel_x = (click_x - box_x).max(0.0);

                let clicked_pos = if this.state.search_query.is_empty() {
                    0
                } else {
                    let run = gpui::TextRun {
                        len: this.state.search_query.len(),
                        font: gpui::font("Segoe UI"),
                        color: Default::default(),
                        underline: Default::default(),
                        strikethrough: None,
                        background_color: None,
                    };
                    let shaped = window.text_system().shape_line(
                        SharedString::from(this.state.search_query.clone()),
                        px(12.0),
                        &[run],
                        None,
                    );
                    let byte_idx = shaped.closest_index_for_x(px(rel_x));
                    this.state.search_query[..byte_idx.min(this.state.search_query.len())].chars().count()
                };

                let total_chars = this.state.search_query.chars().count();
                if event.click_count == 2 {
                    let chars: Vec<char> = this.state.search_query.chars().collect();
                    if !chars.is_empty() {
                        let pos = clicked_pos.min(chars.len().saturating_sub(1));
                        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
                        let target_is_word = is_word_char(chars[pos]);
                        let mut start = pos;
                        while start > 0 && is_word_char(chars[start - 1]) == target_is_word && !chars[start - 1].is_whitespace() {
                            start -= 1;
                        }
                        let mut end = pos + 1;
                        while end < chars.len() && is_word_char(chars[end]) == target_is_word && !chars[end].is_whitespace() {
                            end += 1;
                        }
                        this.quick_filter.selection_anchor = Some(start);
                        this.quick_filter.cursor_pos = end;
                    }
                    this.quick_filter.is_mouse_selecting = false;
                } else if event.click_count >= 3 {
                    this.quick_filter.selection_anchor = Some(0);
                    this.quick_filter.cursor_pos = total_chars;
                    this.quick_filter.is_mouse_selecting = false;
                } else if event.modifiers.shift {
                    if this.quick_filter.selection_anchor.is_none() {
                        this.quick_filter.selection_anchor = Some(this.quick_filter.cursor_pos);
                    }
                    this.quick_filter.cursor_pos = clicked_pos;
                    this.quick_filter.is_mouse_selecting = true;
                } else {
                    this.quick_filter.cursor_pos = clicked_pos;
                    this.quick_filter.selection_anchor = Some(clicked_pos);
                    this.quick_filter.is_mouse_selecting = true;
                }
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_search_key(event, window, cx);
            }))
            .child(icons::icon(icons::ICON_SEARCH, 12.0, theme.text_muted))
            .child({
                let query = &self.state.search_query;
                let query_chars: Vec<char> = query.chars().collect();
                let cursor_pos = self.quick_filter.cursor_pos.min(query_chars.len());
                let selection_range = self.quick_filter.selection_range();

                let run = gpui::TextRun {
                    len: query.len(),
                    font: gpui::font("Segoe UI"),
                    color: Default::default(),
                    underline: Default::default(),
                    strikethrough: None,
                    background_color: None,
                };
                let shaped = window.text_system().shape_line(
                    SharedString::from(query.clone()),
                    px(12.0),
                    &[run],
                    None,
                );

                let cursor_x = if query.is_empty() {
                    0.0
                } else {
                    let byte_index: usize = query_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
                    f32::from(shaped.x_for_index(byte_index))
                };

                let (sel_x1, sel_w) = if let Some((s_start, s_end)) = selection_range {
                    if !query.is_empty() {
                        let b1: usize = query_chars[..s_start.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                        let b2: usize = query_chars[..s_end.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                        let x1 = f32::from(shaped.x_for_index(b1));
                        let x2 = f32::from(shaped.x_for_index(b2));
                        (x1, (x2 - x1).max(0.0))
                    } else {
                        (0.0, 0.0)
                    }
                } else {
                    (0.0, 0.0)
                };

                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h(px(16.0))
                    .child(
                        gpui::canvas(
                            {
                                let cell = self.quick_filter.box_origin_x.clone();
                                move |bounds, _, _| {
                                    cell.set(f32::from(bounds.origin.x));
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    // 1. Selection background
                    .when(selection_range.is_some() && sel_w > 0.0, |s| {
                        s.child(
                            div()
                                .absolute()
                                .left(px(sel_x1))
                                .top(px(0.0))
                                .w(px(sel_w))
                                .h(px(16.0))
                                .bg(theme.selection_bg)
                                .rounded_xs(),
                        )
                    })
                    // 2. Text or placeholder
                    .child(
                        if query.is_empty() {
                            div()
                                .font_family("Segoe UI")
                                .text_xs()
                                .line_height(px(16.0))
                                .text_color(theme.text_muted)
                                .child("Quick filter...")
                        } else {
                            div()
                                .font_family("Segoe UI")
                                .text_xs()
                                .line_height(px(16.0))
                                .text_color(theme.text_primary)
                                .child(query.clone())
                        }
                    )
                    // 3. Caret (shown when focused and cursor_visible)
                    .when(is_search_focused && self.quick_filter.cursor_visible, |s| {
                        s.child(
                            div()
                                .absolute()
                                .left(px(cursor_x))
                                .top(px(0.0))
                                .w(px(1.5))
                                .h(px(16.0))
                                .bg(theme.accent)
                                .rounded_xs(),
                        )
                    })
            })
            .when(!self.state.search_query.is_empty(), |s| {
                s.child(
                    div()
                        .id("btn-clear-search")
                        .cursor_pointer()
                        .p_0p5()
                        .hover(|h| h.bg(theme.bg_hover))
                        .rounded_xs()
                        .child(icons::icon(icons::ICON_CLOSE, 10.0, theme.text_muted))
                        .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                            cx.stop_propagation();
                        }))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.quick_filter.clear(&mut this.state.search_query, cx);
                        }))
                )
            })
    }
}
