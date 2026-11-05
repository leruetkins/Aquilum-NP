use std::cell::Cell;
use std::rc::Rc;
use gpui::{
    div, prelude::*, px, ClipboardItem, Context, FocusHandle, KeyDownEvent,
    MouseDownEvent, MouseMoveEvent, SharedString, Window,
};

use crate::blink_manager::BlinkManager;
use crate::theme::Theme;
use crate::views::goto_search::word_range_at;

/// Reusable single-line text input component with caret blinking, keyboard navigation,
/// mouse drag/double-click selection, multi-DPI snapping, and clipboard integration.
#[derive(Clone)]
pub struct TextInput {
    pub text: String,
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub is_mouse_selecting: bool,
    pub box_origin_x: Rc<Cell<f32>>,
    /// Visible width of the text area, recorded by its canvas after layout.
    pub box_width: Rc<Cell<f32>>,
    /// Horizontal scroll of the text and caret; keeps the caret inside the field.
    pub scroll_x: Rc<Cell<f32>>,
    pub blink: BlinkManager,
    pub focus_handle: Option<FocusHandle>,
}

impl TextInput {
    pub fn new(focus_handle: FocusHandle) -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
            selection_anchor: None,
            is_mouse_selecting: false,
            box_origin_x: Rc::new(Cell::new(0.0)),
            box_width: Rc::new(Cell::new(0.0)),
            scroll_x: Rc::new(Cell::new(0.0)),
            blink: BlinkManager::new(),
            focus_handle: Some(focus_handle),
        }
    }

    #[cfg(test)]
    pub fn for_testing() -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
            selection_anchor: None,
            is_mouse_selecting: false,
            box_origin_x: Rc::new(Cell::new(0.0)),
            box_width: Rc::new(Cell::new(0.0)),
            scroll_x: Rc::new(Cell::new(0.0)),
            blink: BlinkManager::new(),
            focus_handle: None,
        }
    }

    #[inline]
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor_pos = self.text.chars().count();
        self.selection_anchor = None;
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor_pos = 0;
        self.selection_anchor = None;
        self.is_mouse_selecting = false;
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    #[inline]
    pub fn cursor_visible(&self) -> bool {
        self.blink.visible()
    }

    #[inline]
    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle
            .as_ref()
            .map(|h| h.is_focused(window))
            .unwrap_or(false)
    }

    pub fn focus(&self, window: &mut Window) {
        if let Some(h) = &self.focus_handle {
            window.focus(h);
        }
    }

    /// Normalized `(start, end)` character indices of the current selection, if any.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.selection_anchor {
            let pos = self.cursor_pos;
            if anchor != pos {
                return Some((anchor.min(pos), anchor.max(pos)));
            }
        }
        None
    }

    /// Returns the currently selected substring, if any.
    pub fn selected_text(&self) -> Option<String> {
        let (s, e) = self.selection_range()?;
        Some(self.text.chars().skip(s).take(e - s).collect())
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor_pos = self.text.chars().count();
    }

    pub fn enable_blink<V: 'static>(
        &mut self,
        get_bm: fn(&mut V) -> &mut BlinkManager,
        cx: &mut Context<V>,
    ) {
        self.blink.enable_view(get_bm, cx);
    }

    pub fn disable_blink<V: 'static>(&mut self, cx: &mut Context<V>) {
        self.blink.disable_view(cx);
    }

    pub fn pause_and_show<V: 'static>(
        &mut self,
        get_bm: fn(&mut V) -> &mut BlinkManager,
        cx: &mut Context<V>,
    ) {
        self.blink.pause_and_show_view(get_bm, cx);
    }

    /// Maps a click's x-coordinate (window coordinates) to a char index in the text.
    pub fn click_pos(
        &self,
        click_x: f32,
        window: &Window,
        font_size: f32,
        font_family: &'static str,
    ) -> usize {
        if self.text.is_empty() {
            return 0;
        }
        let box_x = self.box_origin_x.get();
        let rel_x = (click_x - box_x + self.scroll_x.get()).max(0.0);
        let run = gpui::TextRun {
            len: self.text.len(),
            font: gpui::font(font_family),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(self.text.clone()),
            px(font_size),
            &[run],
            None,
        );
        let byte_idx = shaped.closest_index_for_x(px(rel_x));
        self.text[..byte_idx.min(self.text.len())].chars().count()
    }

    /// Handles mouse down event (single click, double-click word, triple-click all, shift-click).
    pub fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &Window,
        font_size: f32,
        font_family: &'static str,
    ) {
        let clicked_pos = self.click_pos(f32::from(event.position.x), window, font_size, font_family);
        let total_chars = self.text.chars().count();

        if event.click_count == 2 {
            let (s, e) = word_range_at(&self.text, clicked_pos);
            self.selection_anchor = Some(s);
            self.cursor_pos = e;
            self.is_mouse_selecting = false;
        } else if event.click_count >= 3 {
            self.selection_anchor = Some(0);
            self.cursor_pos = total_chars;
            self.is_mouse_selecting = false;
        } else if event.modifiers.shift {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_pos);
            }
            self.cursor_pos = clicked_pos;
            self.is_mouse_selecting = true;
        } else {
            self.cursor_pos = clicked_pos;
            self.selection_anchor = Some(clicked_pos);
            self.is_mouse_selecting = true;
        }
    }

    /// Handles mouse move during selection drag.
    pub fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        font_size: f32,
        font_family: &'static str,
    ) {
        if !self.is_mouse_selecting {
            return;
        }
        let pos = self.click_pos(f32::from(event.position.x), window, font_size, font_family);
        self.cursor_pos = pos;
    }

    pub fn handle_mouse_up(&mut self) {
        self.is_mouse_selecting = false;
        // A click without a drag leaves a collapsed selection; keeping the
        // anchor set would make the next caret move select one character.
        if self.selection_anchor == Some(self.cursor_pos) {
            self.selection_anchor = None;
        }
    }

    /// Inserts text at the current cursor or replaces the active selection.
    pub fn insert_text(&mut self, text_to_insert: &str) {
        let mut chars: Vec<char> = self.text.chars().collect();
        let pos = if let Some((s, e)) = self.selection_range() {
            // Caret and anchor can lag behind the text after an edit made while
            // the selection was collapsed; never index past the buffer.
            let s = s.min(chars.len());
            let e = e.min(chars.len());
            if s < e {
                chars.drain(s..e);
            }
            self.selection_anchor = None;
            s
        } else {
            self.cursor_pos.min(chars.len())
        };

        for (i, c) in text_to_insert.chars().enumerate() {
            chars.insert(pos + i, c);
        }
        self.cursor_pos = pos + text_to_insert.chars().count();
        self.text = chars.into_iter().collect();
    }

    /// Handles keystroke in the input field. Returns `true` if text changed.
    ///
    /// Blinking is the caller's concern: the caller owns the blink manager and
    /// can reset it without this method borrowing the whole view.
    pub fn handle_key_down<V: 'static>(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<V>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        let ctrl = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        let shift = event.keystroke.modifiers.shift;
        let alt = event.keystroke.modifiers.alt;

        // Ctrl + A (Select All)
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.select_all();
            cx.notify();
            return false;
        }

        // Ctrl + C (Copy)
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some(selected) = self.selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
            }
            return false;
        }

        // Ctrl + X (Cut)
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some(selected) = self.selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
                self.insert_text("");
                cx.notify();
                return true;
            }
            return false;
        }

        // Ctrl + V (Paste)
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text
                        .chars()
                        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                        .collect();
                    if !filtered.is_empty() {
                        self.insert_text(&filtered);
                        cx.notify();
                        return true;
                    }
                }
            }
            return false;
        }

        if ctrl {
            return false;
        }

        // Left Arrow
        if key == "left" {
            if shift {
                if self.selection_anchor.is_none() {
                    self.selection_anchor = Some(self.cursor_pos);
                }
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            } else if let Some((s, _)) = self.selection_range() {
                self.cursor_pos = s;
                self.selection_anchor = None;
            } else if self.cursor_pos > 0 {
                self.cursor_pos -= 1;
                self.selection_anchor = None;
            }
            cx.notify();
            return false;
        }

        // Right Arrow
        if key == "right" {
            let total_chars = self.text.chars().count();
            if shift {
                if self.selection_anchor.is_none() {
                    self.selection_anchor = Some(self.cursor_pos);
                }
                if self.cursor_pos < total_chars {
                    self.cursor_pos += 1;
                }
            } else if let Some((_, e)) = self.selection_range() {
                self.cursor_pos = e;
                self.selection_anchor = None;
            } else if self.cursor_pos < total_chars {
                self.cursor_pos += 1;
                self.selection_anchor = None;
            }
            cx.notify();
            return false;
        }

        // Home
        if key == "home" {
            if shift {
                if self.selection_anchor.is_none() {
                    self.selection_anchor = Some(self.cursor_pos);
                }
            } else {
                self.selection_anchor = None;
            }
            self.cursor_pos = 0;
            cx.notify();
            return false;
        }

        // End
        if key == "end" {
            let total_chars = self.text.chars().count();
            if shift {
                if self.selection_anchor.is_none() {
                    self.selection_anchor = Some(self.cursor_pos);
                }
            } else {
                self.selection_anchor = None;
            }
            self.cursor_pos = total_chars;
            cx.notify();
            return false;
        }

        // Backspace
        if key == "backspace" {
            if self.selection_range().is_some() {
                self.insert_text("");
                cx.notify();
                return true;
            }
            let mut chars: Vec<char> = self.text.chars().collect();
            let pos = self.cursor_pos.min(chars.len());
            if pos > 0 {
                chars.remove(pos - 1);
                self.cursor_pos = pos - 1;
                // A collapsed selection still carries an anchor from the last
                // click; drop it so it cannot point past the shortened text.
                self.selection_anchor = None;
                self.text = chars.into_iter().collect();
                cx.notify();
                return true;
            }
            return false;
        }

        // Delete
        if key == "delete" {
            if self.selection_range().is_some() {
                self.insert_text("");
                cx.notify();
                return true;
            }
            let mut chars: Vec<char> = self.text.chars().collect();
            let pos = self.cursor_pos.min(chars.len());
            if pos < chars.len() {
                chars.remove(pos);
                self.selection_anchor = None;
                self.text = chars.into_iter().collect();
                cx.notify();
                return true;
            }
            return false;
        }

        // Character typing
        if !alt {
            let text_to_insert = if key == "space" {
                Some(" ")
            } else if let Some(ch) = &event.keystroke.key_char {
                if ch.chars().all(|c| !c.is_control()) {
                    Some(ch.as_str())
                } else {
                    None
                }
            } else if key.chars().count() == 1 {
                let c = key.chars().next().unwrap();
                if !c.is_control() {
                    Some(key)
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(s) = text_to_insert {
                self.insert_text(s);
                cx.notify();
                return true;
            }
        }

        false
    }

    /// Renders the styled text input box with caret and selection highlighting.
    pub fn render(
        &self,
        placeholder: &str,
        theme: &Theme,
        window: &Window,
        pixel_scale: f32,
        font_size: f32,
        font_family: &'static str,
    ) -> gpui::Div {
        let is_focused = self.is_focused(window);
        let query = &self.text;
        let query_chars: Vec<char> = query.chars().collect();
        let cursor_pos = self.cursor_pos.min(query_chars.len());
        let selection_range = self.selection_range();

        let run = gpui::TextRun {
            len: query.len(),
            font: gpui::font(font_family),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(query.clone()),
            px(font_size),
            &[run],
            None,
        );

        let cursor_x = if query.is_empty() {
            0.0
        } else {
            let byte_index: usize = query_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
            crate::snap::snap_x(f32::from(shaped.x_for_index(byte_index)), pixel_scale)
        };

        // Keep the caret inside the field: the typed text scrolls instead of
        // wrapping or spilling past the field. The right stop is taken from the
        // device grid, so the caret cannot jump between two physical pixels at
        // the end of a scrolled line.
        let area_w = self.box_width.get();
        let mut scroll_x = self.scroll_x.get();
        let caret_stop = crate::snap::snap_x((area_w - 2.0).max(0.0), pixel_scale);
        if query.is_empty() || area_w <= 0.0 {
            scroll_x = 0.0;
        } else {
            if cursor_x - scroll_x > caret_stop {
                scroll_x = cursor_x - caret_stop;
            } else if cursor_x - scroll_x < 0.0 {
                scroll_x = cursor_x;
            }
            let text_w = f32::from(shaped.x_for_index(query.len()));
            let max_scroll = (text_w - caret_stop).max(0.0);
            scroll_x = scroll_x.clamp(0.0, max_scroll);
        }
        scroll_x = crate::snap::snap_x(scroll_x, pixel_scale);
        self.scroll_x.set(scroll_x);

        // Whole device pixels: the caret keeps one thickness instead of
        // straddling two or three physical columns on every keystroke.
        let caret_left = crate::snap::snap_x(cursor_x - scroll_x, pixel_scale);
        let caret_w = crate::snap::device_thickness(
            (1.5 * pixel_scale).round().max(1.0),
            pixel_scale,
        );
        let caret_w = if caret_w > 0.0 { caret_w } else { 1.5 };

        let (sel_x1, sel_w) = if let Some((s_start, s_end)) = selection_range {
            if !query.is_empty() {
                let b1: usize = query_chars[..s_start.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                let b2: usize = query_chars[..s_end.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                crate::snap::snapped_span(
                    f32::from(shaped.x_for_index(b1)),
                    f32::from(shaped.x_for_index(b2)),
                    pixel_scale,
                )
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };

        let box_cell = self.box_origin_x.clone();
        let width_cell = self.box_width.clone();
        let caret_visible = is_focused && self.cursor_visible();

        // One fixed line box centered in the field, the way the sidebar quick
        // filter draws its text: a bare text node would stick to the top of
        // the taller find field, and a caret measured from that same line
        // would look shorter than the text it belongs to.
        let line_height = font_size + 4.0;

        div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .justify_center()
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(line_height))
                    .child(
                        gpui::canvas(
                            move |bounds, _, _| {
                                box_cell.set(f32::from(bounds.origin.x));
                                width_cell.set(f32::from(bounds.size.width));
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
                                .left(px(sel_x1 - scroll_x))
                                .top(px(0.0))
                                .w(px(sel_w))
                                .h(px(line_height))
                                .bg(theme.selection_bg)
                                .rounded_xs(),
                        )
                    })
                    // 2. Text or placeholder
                    .child(
                        if query.is_empty() {
                            div()
                                .font_family(SharedString::from(font_family.to_string()))
                                // The caret and selection are measured with the normal
                                // weight; an inherited bold weight would paint wider
                                // glyphs and leave the caret inside a letter.
                                .font_weight(gpui::FontWeight::NORMAL)
                                .text_size(px(font_size))
                                .line_height(px(line_height))
                                .text_color(theme.text_muted)
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(placeholder.to_string())
                        } else {
                            div()
                                .font_family(SharedString::from(font_family.to_string()))
                                .font_weight(gpui::FontWeight::NORMAL)
                                .text_size(px(font_size))
                                .line_height(px(line_height))
                                .text_color(theme.text_primary)
                                .whitespace_nowrap()
                                .relative()
                                .left(px(-scroll_x))
                                .child(query.clone())
                        }
                    )
                    // 3. Crisp Caret
                    .when(caret_visible, |s| {
                        s.child(
                            div()
                                .absolute()
                                .left(px(caret_left))
                                .top(px(0.0))
                                .w(px(caret_w))
                                .h(px(line_height))
                                .bg(theme.accent)
                                .rounded_xs(),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_input_selection_range() {
        let mut input = TextInput::for_testing();
        input.set_text("hello world");
        assert_eq!(input.cursor_pos, 11);
        assert_eq!(input.selection_range(), None);

        input.cursor_pos = 5;
        input.selection_anchor = Some(0);
        assert_eq!(input.selection_range(), Some((0, 5)));
        assert_eq!(input.selected_text().as_deref(), Some("hello"));

        input.cursor_pos = 0;
        input.selection_anchor = Some(5);
        assert_eq!(input.selection_range(), Some((0, 5)));
        assert_eq!(input.selected_text().as_deref(), Some("hello"));
    }

    #[test]
    fn test_text_input_insert_and_replace() {
        let mut input = TextInput::for_testing();
        input.insert_text("hello");
        assert_eq!(input.text(), "hello");
        assert_eq!(input.cursor_pos, 5);

        input.selection_anchor = Some(1);
        input.cursor_pos = 4;
        input.insert_text("a");
        assert_eq!(input.text(), "hao");
        assert_eq!(input.cursor_pos, 2);
    }

    #[test]
    fn test_text_input_tolerates_stale_caret_past_text_end() {
        let mut input = TextInput::for_testing();
        input.set_text("hello");
        // A collapsed selection (anchor == caret) left over after the text
        // shrank by one character: inserting must not index past the buffer.
        input.cursor_pos = 6;
        input.selection_anchor = Some(6);
        input.insert_text("x");
        assert_eq!(input.text(), "hellox");
        assert_eq!(input.cursor_pos, 6);

        input.set_text("hello");
        input.cursor_pos = 7;
        input.selection_anchor = Some(5);
        input.insert_text("y");
        assert_eq!(input.text(), "helloy");
        assert_eq!(input.cursor_pos, 6);
    }
}
