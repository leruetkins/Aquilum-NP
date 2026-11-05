//! Obsidian-style sidebar search panel.
//!
//! Toggled with the folder/magnifier buttons next to the `VIBECODIUM` badge
//! (or `Ctrl+Shift+F`). Reuses the [`crate::views::goto_search`] engine —
//! name hits first, then content hits — plus `path:` / `file:` / `tag:` /
//! `line:` / `section:` operators, a case toggle, query history and
//! collapsible per-file groups with highlighted excerpts.

use std::cell::Cell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    div, point, prelude::*, px, Context, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, SharedString, Styled, Window,
};

use crate::blink_manager::{BlinkManager, BlinkTarget};
use crate::icons;
use crate::search::worker::{SearchConsumer, SearchRequest};
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::goto_search::{
    highlight_query_terms_cased, highlight_terms_cased, parse_search_op, SearchOp, MAX_RESULTS,
};
use crate::views::scrollbar::{
    overlay_scrollbar, ScrollbarGeometry, ScrollbarStyle,
};

/// Scrollbar look for the panel results.
///
/// Same lane idea as the Go to file dialog (8px gutter, constant 6px thumb),
/// but the track is pushed outwards into the sidebar column's own `p_2` with
/// a negative offset — exactly how the sidebar tree parks its thumb on the
/// panel border instead of over the rows (`track_right_offset: -SIDEBAR_PADDING`).
/// Without this the thumb floats a full gutter away from the sidebar edge.
fn panel_scrollbar_style() -> ScrollbarStyle {
    ScrollbarStyle {
        track_w: 8.0,
        thumb_w: 6.0,
        thumb_w_active: 6.0,
        thumb_inset: 2.0,
        track_right_offset: -8.0,
        ..ScrollbarStyle::default()
    }
}

/// Sidebar content mode: file tree, search panel, bookmarks, recent files, or active plugin leaf view.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SidebarMode {
    #[default]
    Files,
    Search,
    Bookmarks,
    RecentFiles,
    Plugin(String),
}

/// State of the sidebar search panel.
#[derive(Clone)]
pub struct SidebarSearchState {
    pub query: String,
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub is_mouse_selecting: bool,
    pub box_origin_x: Rc<Cell<f32>>,
    /// Visible width of the text area, recorded by its canvas after layout.
    pub box_width: Rc<Cell<f32>>,
    /// Horizontal scroll of the text and caret; keeps the caret inside the field.
    pub scroll_x: Rc<Cell<f32>>,
    pub blink: BlinkManager,
    pub results: Vec<crate::views::goto_search::GotoResult>,
    /// The query the current `results` answer (highlights follow it while a
    /// newer query is in flight — see `SearchDialogState::results_query`).
    pub results_query: String,
    /// Keyboard/mouse cursor over the results. `None` until the user moves
    /// with arrows or hovers — unlike the Go to file dialog, a fresh result
    /// list shows no highlight. `Enter` with `None` opens the first hit.
    pub selected_ix: Option<usize>,
    /// Collapsed result groups (file paths); groups start expanded.
    pub collapsed: HashSet<PathBuf>,
    /// Submitted queries, most recent first (in-memory, capped).
    pub history: Vec<String>,
    pub case_sensitive: bool,
}

impl Default for SidebarSearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            cursor_pos: 0,
            selection_anchor: None,
            is_mouse_selecting: false,
            box_origin_x: Rc::new(Cell::new(0.0)),
            box_width: Rc::new(Cell::new(0.0)),
            scroll_x: Rc::new(Cell::new(0.0)),
            blink: BlinkManager::new(),
            results: Vec::new(),
            results_query: String::new(),
            selected_ix: None,
            collapsed: HashSet::new(),
            history: Vec::new(),
            case_sensitive: false,
        }
    }
}

/// Operators shown in the hints box: prefix, locale key for the description.
/// The prefix itself is syntax (same in every language, like the screenshot).
const SEARCH_OPS: &[(&str, SearchOp, &str)] = &[
    ("path:", SearchOp::Path, "search.op_path_desc"),
    ("file:", SearchOp::File, "search.op_file_desc"),
    ("tag:", SearchOp::Tag, "search.op_tag_desc"),
    ("line:", SearchOp::Line, "search.op_line_desc"),
    ("section:", SearchOp::Section, "search.op_section_desc"),
];

/// Stored query history entries.
const MAX_HISTORY: usize = 20;

impl AppView {
    /// Switches the sidebar to the search panel and focuses its input.
    pub fn open_sidebar_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_collapsed = false;
        self.sidebar_mode = SidebarMode::Search;
        self.state.session.sidebar_mode = SidebarMode::Search;
        self.state.save_session_state();
        self.run_sidebar_search();
        window.focus(&self.search_panel_focus_handle);
        self.sidebar_search
            .blink
            .enable(BlinkTarget::SidebarSearch, cx);
        cx.notify();
    }

    /// Opens the sidebar search panel with a tag query (e.g. `tag:#name`) and runs the search.
    pub fn open_tag_search(&mut self, tag: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_collapsed = false;
        self.sidebar_mode = SidebarMode::Search;
        self.state.session.sidebar_mode = SidebarMode::Search;
        let formatted_query = if tag.starts_with('#') {
            format!("tag:{}", tag)
        } else {
            format!("tag:#{}", tag)
        };
        self.sidebar_search.query = formatted_query.clone();
        self.sidebar_search.cursor_pos = formatted_query.chars().count();
        self.sidebar_search.selection_anchor = None;
        self.sidebar_search.is_mouse_selecting = false;
        self.state.save_session_state();
        self.run_sidebar_search();
        window.focus(&self.search_panel_focus_handle);
        self.sidebar_search
            .blink
            .enable(BlinkTarget::SidebarSearch, cx);
        cx.notify();
    }

    /// Switches the sidebar back to the file tree.
    pub fn close_sidebar_search(&mut self, cx: &mut Context<Self>) {
        self.sidebar_mode = SidebarMode::Files;
        self.state.session.sidebar_mode = SidebarMode::Files;
        self.sidebar_search.is_mouse_selecting = false;
        self.sidebar_search.selection_anchor = None;
        self.sidebar_search.blink.disable(cx);
        self.state.save_session_state();
        cx.notify();
    }

    /// Requests a fresh panel search on the worker (immediate entry points:
    /// panel open, tag search, history/operator clicks, the `Aa` toggle).
    ///
    /// The scan runs off the UI thread; results land via
    /// [`AppView::poll_search_replies`]. Stale replies are dropped by
    /// sequence number. An empty query clears synchronously and invalidates
    /// any in-flight reply.
    pub fn run_sidebar_search(&mut self) {
        let raw = self.sidebar_search.query.trim().to_string();
        if raw.is_empty() {
            self.panel_search_seq += 1;
            self.sidebar_search.results.clear();
            self.sidebar_search.selected_ix = None;
            self.set_panel_scroll_top(0.0);
            return;
        }
        let (op, rest) = parse_search_op(&raw);
        self.panel_search_seq += 1;
        self.state.search_worker.ask(SearchRequest {
            consumer: SearchConsumer::Panel,
            seq: self.panel_search_seq,
            query: rest,
            op,
            case_sensitive: self.sidebar_search.case_sensitive,
            limit: MAX_RESULTS,
        });
    }

    /// Arms the debounced search for the panel input.
    ///
    /// Called on EVERY keystroke: the sequence bumps and the in-flight scan
    /// is cancelled right here (a reply for an already-superseded query can
    /// never surface). Only the newest arm fires a scan
    /// ([`Self::run_sidebar_search`], off the UI thread); the text and caret
    /// were already updated by the caller.
    pub fn request_sidebar_search(&mut self, cx: &mut Context<Self>) {
        self.panel_search_seq += 1;
        self.state.search_worker.cancel(SearchConsumer::Panel);
        self.panel_debounce_epoch += 1;
        let epoch = self.panel_debounce_epoch;
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();
        cx.foreground_executor()
            .spawn(async move {
                timer_executor.timer(Duration::from_millis(70)).await;
                let _ = entity.update(&mut async_cx, |this, _cx| {
                    if this.panel_debounce_epoch == epoch {
                        this.run_sidebar_search();
                    }
                });
            })
            .detach();
    }

    /// Submits the query: records history and opens the selected hit.
    pub fn submit_sidebar_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let raw = self.sidebar_search.query.trim().to_string();
        if raw.is_empty() {
            return;
        }
        self.record_panel_history();
        if let Some(hit) = self
            .sidebar_search
            .results
            .get(self.sidebar_search.selected_ix.unwrap_or(0))
            .cloned()
        {
            self.open_panel_result(hit.path, window, cx);
        } else {
            cx.notify();
        }
    }

    /// Records the current query in history (most recent first, deduped).
    ///
    /// Called both on `Enter` and when a hit is opened with the mouse, so
    /// the history fills however results get opened — like Obsidian.
    fn record_panel_history(&mut self) {
        let raw = self.sidebar_search.query.trim().to_string();
        if raw.is_empty() || self.sidebar_search.history.iter().any(|h| h == &raw) {
            return;
        }
        self.sidebar_search.history.insert(0, raw);
        while self.sidebar_search.history.len() > MAX_HISTORY {
            self.sidebar_search.history.pop();
        }
    }

    /// Opens a panel hit, replacing an active empty tab like Go to file does.
    pub fn open_panel_result(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_panel_history();
        if let Some(empty) = self.state.active_empty_tab.clone() {
            self.state.open_tabs.retain(|p| p != &empty);
            self.state.active_empty_tab = None;
        }
        self.select_note(path, true, window, cx);
    }

    /// Scrolls the panel results to `scroll_y` pixels from the top.
    pub(crate) fn set_panel_scroll_top(&self, scroll_y: f32) {
        self.panel_scroll_handle
            .set_offset(point(px(0.0), px(-scroll_y)));
    }

    /// Follows the pointer while the panel results thumb is dragged.
    pub(crate) fn handle_panel_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.panel_scrollbar_drag.active {
            return;
        }
        let style = panel_scrollbar_style();
        let Some(geometry) = ScrollbarGeometry::from_handle(&self.panel_scroll_handle, &style)
        else {
            return;
        };
        geometry.apply_drag(
            &self.panel_scroll_handle,
            &self.panel_scrollbar_drag,
            pointer_y,
        );
        cx.notify();
    }

    /// Follows the pointer while the panel input selection is dragged.
    pub fn handle_panel_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_mode != SidebarMode::Search || !self.sidebar_search.is_mouse_selecting {
            return;
        }
        let x: f32 = event.position.x.into();
        let new_pos = self.panel_click_pos(x, window);
        if self.sidebar_search.cursor_pos != new_pos {
            self.sidebar_search.cursor_pos = new_pos;
            self.sidebar_search
                .blink
                .pause_and_show(BlinkTarget::SidebarSearch, cx);
            cx.notify();
        }
    }

    /// Ends a panel input drag selection, collapsing a zero-width range.
    pub fn handle_panel_mouse_up(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_search.is_mouse_selecting {
            self.sidebar_search.is_mouse_selecting = false;
            if self.sidebar_search.selection_anchor == Some(self.sidebar_search.cursor_pos) {
                self.sidebar_search.selection_anchor = None;
            }
            cx.notify();
        }
    }

    /// Maps a click x (window coords) to a panel query char index.
    pub fn panel_click_pos(&self, click_x: f32, window: &Window) -> usize {
        let query = &self.sidebar_search.query;
        if query.is_empty() {
            return 0;
        }
        let box_x = self.sidebar_search.box_origin_x.get();
        let rel_x = (click_x - box_x + self.sidebar_search.scroll_x.get()).max(0.0);
        let run = gpui::TextRun {
            len: query.len(),
            font: gpui::font("Segoe UI"),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        // 12px: must match the `text_xs` size the input is rendered with,
        // otherwise the caret drifts away from the letters.
        let shaped = window.text_system().shape_line(
            SharedString::from(query.clone()),
            px(12.0),
            &[run],
            None,
        );
        let byte_idx = shaped.closest_index_for_x(px(rel_x));
        query[..byte_idx.min(query.len())].chars().count()
    }

    /// Inserts text at the panel cursor, replacing the selection first.
    fn insert_panel_text(&mut self, text: &str) {
        let mut chars: Vec<char> = self.sidebar_search.query.chars().collect();
        let pos = if let Some((s, e)) = self.sidebar_search.selection_range() {
            chars.drain(s..e);
            self.sidebar_search.selection_anchor = None;
            s
        } else {
            self.sidebar_search.cursor_pos.min(chars.len())
        };
        for (i, c) in text.chars().enumerate() {
            chars.insert(pos + i, c);
        }
        self.sidebar_search.cursor_pos = pos + text.chars().count();
        self.sidebar_search.query = chars.into_iter().collect();
    }

    /// Handles keyboard events inside the panel input.
    pub fn handle_panel_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_search
            .blink
            .pause_and_show(BlinkTarget::SidebarSearch, cx);
        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let shift = event.keystroke.modifiers.shift;
        let alt = event.keystroke.modifiers.alt;

        // Escape clears the query first, then returns focus to the editor.
        if key == "escape" {
            if !self.sidebar_search.query.is_empty() {
                self.sidebar_search.query.clear();
                self.sidebar_search.cursor_pos = 0;
                self.sidebar_search.selection_anchor = None;
                self.request_sidebar_search(cx);
            } else {
                window.focus(&self.focus_handle);
            }
            cx.notify();
            return;
        }

        // Arrow navigation through results (wraps around). From no
        // selection, Down starts at the first hit and Up at the last.
        if key == "down" || key == "up" {
            let len = self.sidebar_search.results.len();
            if len > 0 {
                self.sidebar_search.selected_ix = Some(match self.sidebar_search.selected_ix {
                    None => {
                        if key == "down" {
                            0
                        } else {
                            len - 1
                        }
                    }
                    Some(cur) => {
                        let delta = if key == "down" { 1isize } else { -1 };
                        (cur as isize + delta).rem_euclid(len as isize) as usize
                    }
                });
                cx.notify();
            }
            return;
        }

        if key == "enter" {
            self.submit_sidebar_search(window, cx);
            return;
        }

        if key == "home" || key == "end" {
            let total = self.sidebar_search.query.chars().count();
            let pos = if key == "home" { 0 } else { total };
            if shift {
                if self.sidebar_search.selection_anchor.is_none() {
                    self.sidebar_search.selection_anchor =
                        Some(self.sidebar_search.cursor_pos);
                }
            } else {
                self.sidebar_search.selection_anchor = None;
            }
            self.sidebar_search.cursor_pos = pos;
            cx.notify();
            return;
        }

        if key == "left" || key == "right" {
            let total = self.sidebar_search.query.chars().count();
            if shift && self.sidebar_search.selection_anchor.is_none() {
                self.sidebar_search.selection_anchor = Some(self.sidebar_search.cursor_pos);
            }
            if key == "left" {
                if shift {
                    self.sidebar_search.cursor_pos =
                        self.sidebar_search.cursor_pos.saturating_sub(1);
                } else if let Some((start, _)) = self.sidebar_search.selection_range() {
                    self.sidebar_search.cursor_pos = start;
                    self.sidebar_search.selection_anchor = None;
                } else {
                    self.sidebar_search.cursor_pos =
                        self.sidebar_search.cursor_pos.saturating_sub(1);
                }
            } else if shift {
                self.sidebar_search.cursor_pos =
                    (self.sidebar_search.cursor_pos + 1).min(total);
            } else if let Some((_, end)) = self.sidebar_search.selection_range() {
                self.sidebar_search.cursor_pos = end;
                self.sidebar_search.selection_anchor = None;
            } else {
                self.sidebar_search.cursor_pos =
                    (self.sidebar_search.cursor_pos + 1).min(total);
            }
            cx.notify();
            return;
        }

        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.sidebar_search.selection_anchor = Some(0);
            self.sidebar_search.cursor_pos = self.sidebar_search.query.chars().count();
            cx.notify();
            return;
        }

        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s, e)) = self.sidebar_search.selection_range() {
                let text: String =
                    self.sidebar_search.query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
            return;
        }

        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s, e)) = self.sidebar_search.selection_range() {
                let text: String =
                    self.sidebar_search.query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                let mut chars: Vec<char> = self.sidebar_search.query.chars().collect();
                chars.drain(s..e);
                self.sidebar_search.query = chars.into_iter().collect();
                self.sidebar_search.cursor_pos = s;
                self.sidebar_search.selection_anchor = None;
                self.request_sidebar_search(cx);
                cx.notify();
            }
            return;
        }

        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text
                        .chars()
                        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                        .collect();
                    if !filtered.is_empty() {
                        self.insert_panel_text(&filtered);
                        self.request_sidebar_search(cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        if ctrl || alt || event.keystroke.modifiers.platform {
            return;
        }

        if key == "backspace" || key == "delete" {
            let mut chars: Vec<char> = self.sidebar_search.query.chars().collect();
            if let Some((s, e)) = self.sidebar_search.selection_range() {
                chars.drain(s..e);
                self.sidebar_search.cursor_pos = s;
                self.sidebar_search.selection_anchor = None;
            } else if key == "backspace" {
                let pos = self.sidebar_search.cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.sidebar_search.cursor_pos = pos - 1;
                } else {
                    return;
                }
            } else {
                let pos = self.sidebar_search.cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                } else {
                    return;
                }
            }
            self.sidebar_search.query = chars.into_iter().collect();
            self.request_sidebar_search(cx);
            cx.notify();
            return;
        }

        let text_to_insert = if key == "space" {
            Some(" ".to_string())
        } else if let Some(ch) = &event.keystroke.key_char {
            Some(ch.to_string())
        } else if key.chars().count() == 1 {
            Some(key.clone())
        } else {
            None
        };
        if let Some(text) = text_to_insert {
            let filtered: String = text
                .chars()
                .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                .collect();
            if !filtered.is_empty() {
                self.insert_panel_text(&filtered);
                self.request_sidebar_search(cx);
                cx.notify();
            }
        }
    }
}

impl SidebarSearchState {
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.selection_anchor {
            if anchor != self.cursor_pos {
                return Some((anchor.min(self.cursor_pos), anchor.max(self.cursor_pos)));
            }
        }
        None
    }
}

/// Word boundaries around char index `pos` for double-click selection.
fn panel_word_range_at(text: &str, pos: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return (0, 0);
    }
    let pos = pos.min(chars.len().saturating_sub(1));
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let target_is_word = is_word(chars[pos]);
    let mut start = pos;
    while start > 0 && is_word(chars[start - 1]) == target_is_word && !chars[start - 1].is_whitespace() {
        start -= 1;
    }
    let mut end = pos + 1;
    while end < chars.len() && is_word(chars[end]) == target_is_word && !chars[end].is_whitespace() {
        end += 1;
    }
    (start, end)
}

impl AppView {
    /// Renders the sidebar search panel (input, hints/history, grouped results).
    pub fn render_search_panel(
        &self,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let query = self.sidebar_search.query.clone();
        let cs = self.sidebar_search.case_sensitive;
        let trimmed = query.trim().to_string();
        let lowered = trimmed.to_lowercase();
        // Hints stay visible for an empty query or a bare `op:` prefix.
        let show_hints = trimmed.is_empty()
            || SEARCH_OPS.iter().any(|(prefix, _, _)| {
                lowered == *prefix || lowered == prefix.trim_end_matches(':')
            });
        // Highlights follow the RESULTS' query (see the Go to file dialog).
        let (_, results_rest) = parse_search_op(&self.sidebar_search.results_query);
        let terms = highlight_query_terms_cased(&results_rest, cs);
        // A newer query is in flight: the displayed list answers the OLD one.
        // Hide it behind the searching state (with the LIVE index-build
        // percent, not a reply-frozen one) so it can't read as new results.
        let search_pending = !trimmed.is_empty()
            && self.sidebar_search.results_query.trim() != trimmed;
        let pending_label = match self.state.search_worker.build_progress() {
            Some(pct) => format!("{} {}%", self.t("search.searching"), pct),
            None => self.t("search.searching").to_string(),
        };
        let pixel_scale = window.scale_factor();

        // Input caret geometry (Segoe UI 12px = `text_xs`, same shaping as
        // click mapping above).
        let query_chars: Vec<char> = query.chars().collect();
        let cursor_pos = self.sidebar_search.cursor_pos.min(query_chars.len());
        let selection_range = self.sidebar_search.selection_range();
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
            crate::snap::snap_x(f32::from(shaped.x_for_index(byte_index)), pixel_scale)
        };

        // Keep the caret inside the field: the typed text scrolls instead of
        // wrapping or spilling past the buttons on the right.
        let area_w = self.sidebar_search.box_width.get();
        let mut scroll_x = self.sidebar_search.scroll_x.get();
        // The caret's right stop is taken from the device grid, so its sub-pixel
        // phase cannot cross a rounding boundary on some keystrokes and make the
        // caret jump between two physical pixels at the end of a scrolled line.
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
        // Snap the scroll to the physical pixel grid: the text and the caret are
        // shifted by it, and a fractional offset rasterizes the 1.5px caret as
        // two or three pixels depending on the letter typed.
        scroll_x = crate::snap::snap_x(scroll_x, pixel_scale);
        self.sidebar_search.scroll_x.set(scroll_x);

        // Whole device pixels: the caret keeps one thickness instead of straddling
        // two or three physical columns on every keystroke.
        let caret_left = crate::snap::snap_x(cursor_x - scroll_x, pixel_scale);
        let caret_w = crate::snap::device_thickness(
            (1.5 * pixel_scale).round().max(1.0),
            pixel_scale,
        );
        let caret_w = if caret_w > 0.0 { caret_w } else { 1.5 };

        let (sel_x1, sel_w) = if let Some((s, e)) = selection_range {
            if !query.is_empty() {
                let b1: usize = query_chars[..s.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                let b2: usize = query_chars[..e.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                // Both edges go through the same device-pixel snapping as the caret
                // above; the rectangle is rounded, so it is painted antialiased and an
                // unsnapped edge would breathe while the caret stays crisp.
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
        let is_input_focused = self.search_panel_focus_handle.is_focused(window);
        let caret_visible = is_input_focused && self.sidebar_search.blink.visible();

        div()
            .id("sidebar-search-panel")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_2()
            // ---- Input row ----
            .child(
                div()
                    .id("search-panel-box")
                    .track_focus(&self.search_panel_focus_handle)
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .px_2()
                    .py_1p5()
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(if is_input_focused {
                        theme.accent
                    } else {
                        theme.border
                    })
                    .rounded_md()
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            window.focus(&this.search_panel_focus_handle);
                            this.sidebar_search
                                .blink
                                .pause_and_show(BlinkTarget::SidebarSearch, cx);
                            let click_x: f32 = event.position.x.into();
                            let pos = this.panel_click_pos(click_x, window);
                            let total = this.sidebar_search.query.chars().count();
                            if event.click_count >= 3 {
                                this.sidebar_search.selection_anchor = Some(0);
                                this.sidebar_search.cursor_pos = total;
                                this.sidebar_search.is_mouse_selecting = false;
                            } else if event.click_count == 2 {
                                let (s, e) =
                                    panel_word_range_at(&this.sidebar_search.query, pos);
                                this.sidebar_search.selection_anchor = Some(s);
                                this.sidebar_search.cursor_pos = e;
                                this.sidebar_search.is_mouse_selecting = false;
                            } else if event.modifiers.shift {
                                if this.sidebar_search.selection_anchor.is_none() {
                                    this.sidebar_search.selection_anchor =
                                        Some(this.sidebar_search.cursor_pos);
                                }
                                this.sidebar_search.cursor_pos = pos;
                                this.sidebar_search.is_mouse_selecting = true;
                            } else {
                                this.sidebar_search.cursor_pos = pos;
                                this.sidebar_search.selection_anchor = Some(pos);
                                this.sidebar_search.is_mouse_selecting = true;
                            }
                            cx.notify();
                        }),
                    )
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.handle_panel_search_key(event, window, cx);
                    }))
                    .child(icons::icon(icons::ICON_SEARCH, 12.0, theme.text_muted))
                    .child(
                        div()
                            .id("search-panel-input")
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h(px(16.0))
                            .overflow_hidden()
                            .child(
                                gpui::canvas(
                                    {
                                        let cell =
                                            self.sidebar_search.box_origin_x.clone();
                                        let width =
                                            self.sidebar_search.box_width.clone();
                                        move |bounds, _, _| {
                                            cell.set(f32::from(bounds.origin.x));
                                            width.set(f32::from(bounds.size.width));
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .when(selection_range.is_some() && sel_w > 0.0, |s| {
                                s.child(
                                    div()
                                        .absolute()
                                        .left(px(sel_x1 - scroll_x))
                                        .top(px(0.0))
                                        .w(px(sel_w))
                                        .h(px(16.0))
                                        .bg(theme.selection_bg)
                                        .rounded_xs(),
                                )
                            })
                            .child(if query.is_empty() {
                                div()
                                    .font_family("Segoe UI")
                                    .text_xs()
                                    .line_height(px(16.0))
                                    .text_color(theme.text_muted)
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(self.t("search.panel_hint"))
                            } else {
                                div()
                                    .font_family("Segoe UI")
                                    .text_xs()
                                    .line_height(px(16.0))
                                    .text_color(theme.text_primary)
                                    .whitespace_nowrap()
                                    .relative()
                                    .left(px(-scroll_x))
                                    .child(query.clone())
                            })
                            .when(caret_visible, |s| {
                                s.child(
                                    div()
                                        .absolute()
                                        .left(px(caret_left))
                                        .top(px(0.0))
                                        .w(px(caret_w))
                                        .h(px(16.0))
                                        .bg(theme.accent)
                                        .rounded_xs(),
                                )
                            }),
                    )
                    // Aa — case sensitive toggle
                    .child(
                        div()
                            .id("search-panel-case")
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(if cs {
                                theme.accent
                            } else {
                                theme.text_muted
                            })
                            .child("Aa")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar_search.case_sensitive =
                                    !this.sidebar_search.case_sensitive;
                                this.run_sidebar_search();
                                cx.notify();
                            })),
                    )
                    .when(!query.is_empty(), |row| {
                        row.child(
                            div()
                                .id("search-panel-clear")
                                .p_0p5()
                                .rounded_xs()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(
                                    icons::ICON_CLOSE,
                                    10.0,
                                    theme.text_muted,
                                ))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.sidebar_search.query.clear();
                                    this.sidebar_search.cursor_pos = 0;
                                    this.sidebar_search.selection_anchor = None;
                                    this.run_sidebar_search();
                                    cx.notify();
                                })),
                        )
                    }),
            )
            // ---- Hints (operators + history) ----
            .when(show_hints, |panel| {
                panel.child(
                    div()
                        .id("search-panel-hints")
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .p_1p5()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border)
                        .rounded_md()
                        .child(
                            div()
                                .px_1p5()
                                .py_1()
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(theme.text_secondary)
                                .child(self.t("search.operators_title")),
                        )
                        .children(SEARCH_OPS.iter().map(|(prefix, _, desc_key)| {
                            let prefix_owned = prefix.to_string();
                            div()
                                .id(SharedString::from(format!("search-op-{}", prefix_owned)))
                                .flex()
                                .flex_row()
                                .items_baseline()
                                .gap_1p5()
                                .px_1p5()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(theme.accent)
                                        .child(prefix_owned.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .child(self.t(desc_key)),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.sidebar_search.query = prefix_owned.clone();
                                    this.sidebar_search.cursor_pos =
                                        this.sidebar_search.query.chars().count();
                                    this.sidebar_search.selection_anchor = None;
                                    this.run_sidebar_search();
                                    window.focus(&this.search_panel_focus_handle);
                                    this.sidebar_search
                                        .blink
                                        .pause_and_show(BlinkTarget::SidebarSearch, cx);
                                    cx.notify();
                                }))
                        }))
                        .child(
                            div()
                                .h_px()
                                .bg(theme.border_subtle)
                                .my_1()
                                .mx_1(),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .px_1p5()
                                .py_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(theme.text_secondary)
                                        .child(self.t("search.history")),
                                )
                                .when(!self.sidebar_search.history.is_empty(), |row| {
                                    row.child(
                                        div()
                                            .id("search-history-clear")
                                            .p_0p5()
                                            .rounded_xs()
                                            .cursor_pointer()
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .child(icons::icon(
                                                icons::ICON_CLOSE,
                                                10.0,
                                                theme.text_muted,
                                            ))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.sidebar_search.history.clear();
                                                cx.notify();
                                            })),
                                    )
                                }),
                        )
                        .children(self.sidebar_search.history.iter().map(|item| {
                            let item_owned = item.clone();
                            div()
                                .id(SharedString::from(format!("search-history-{}", item_owned)))
                                .px_1p5()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(
                                    div()
                                        .text_xs()
                                        .truncate()
                                        .text_color(theme.text_primary)
                                        .child(item_owned.clone()),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.sidebar_search.query = item_owned.clone();
                                    this.sidebar_search.cursor_pos = this
                                        .sidebar_search
                                        .query
                                        .chars()
                                        .count();
                                    this.sidebar_search.selection_anchor = None;
                                    this.run_sidebar_search();
                                    window.focus(&this.search_panel_focus_handle);
                                    cx.notify();
                                }))
                        })),
                )
            })
            // ---- Results ----
            .when(!show_hints, |panel| {
                panel
                    .child(
                        div()
                            .id("search-panel-found")
                            .px_1()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(if search_pending {
                                // While a newer query is in flight the count
                                // belongs to the old query — replace the
                                // whole line with the searching state.
                                pending_label.clone()
                            } else {
                                self.t_fmt(
                                    "search.found",
                                    &[&self.sidebar_search.results.len().to_string()],
                                )
                            }),
                    )
                    .when(search_pending, |panel| {
                        // The stale list (with old-query highlights) is
                        // hidden while the fresh reply is in flight.
                        panel.child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(icons::icon(
                                    icons::ICON_SEARCH,
                                    14.0,
                                    theme.text_muted,
                                )),
                        )
                    })
                    .when(!search_pending, |panel| {
                        panel.child({
                        let mut viewport = div()
                            .id("search-panel-viewport")
                            .relative()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .child(
                                div()
                                    .id("search-panel-scroll")
                                    .track_scroll(&self.panel_scroll_handle)
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_scroll()
                                    // Right gutter hosts the scrollbar lane so
                                    // cards never run into the thumb (same
                                    // recipe as the Go to file dialog).
                                    .p_2()
                                    .gap_1p5()
                                    .children(self.sidebar_search.results.iter().enumerate().map(
                                        |(ix, hit)| {
                                            let is_selected =
                                                self.sidebar_search.selected_ix == Some(ix);
                                            let hit_path = hit.path.clone();
                                            let hit_path_open = hit.path.clone();
                                            let hit_title = hit.title.clone();
                                            let hit_snippet = hit.snippet.clone();
                                            let hit_count = hit.match_count;
                                            let is_collapsed = self
                                                .sidebar_search
                                                .collapsed
                                                .contains(&hit.path);
                                            let chevron = if is_collapsed {
                                                icons::ICON_CHEVRON_RIGHT
                                            } else {
                                                icons::ICON_CHEVRON_DOWN
                                            };
                                            // One bordered card per file, like the
                                            // Go to file dialog: header row plus
                                            // excerpt inside a single frame.
                                            div()
                                                .id(SharedString::from(format!("search-panel-hit-{}", ix)))
                                                .flex()
                                                .flex_col()
                                                .gap_1()
                                                .px_3()
                                                .py_2()
                                                .mr(px(4.0))
                                                .rounded_lg()
                                                .border_1()
                                                .border_color(theme.border_subtle)
                                                .bg(theme.bg_card)
                                                .cursor_pointer()
                                                .when(is_selected, |d| d.bg(theme.bg_hover))
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        this.open_panel_result(
                                                            hit_path_open.clone(),
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                ))
                                                .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1p5()
                                                        .child(
                                                            div()
                                                                .id(SharedString::from(format!(
                                                                    "search-panel-chevron-{}",
                                                                    ix
                                                                )))
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .w(px(18.0))
                                                                .h(px(18.0))
                                                                .rounded_sm()
                                                                .cursor_pointer()
                                                                .hover(|h| h.bg(theme.bg_hover))
                                                                .child(icons::icon(
                                                                    chevron,
                                                                    11.0,
                                                                    theme.text_muted,
                                                                ))
                                                                .on_mouse_down(
                                                                    MouseButton::Left,
                                                                    |_, _, cx| {
                                                                        cx.stop_propagation();
                                                                    },
                                                                )
                                                                .on_click(cx.listener(
                                                                    move |this, _, _, cx| {
                                                                        cx.stop_propagation();
                                                                        if !this
                                                                            .sidebar_search
                                                                            .collapsed
                                                                            .remove(&hit_path)
                                                                        {
                                                                            this.sidebar_search
                                                                                .collapsed
                                                                                .insert(
                                                                                    hit_path.clone(),
                                                                                );
                                                                        }
                                                                        cx.notify();
                                                                    },
                                                                )),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .truncate()
                                                                .text_sm()
                                                                .font_weight(
                                                                    gpui::FontWeight::SEMIBOLD,
                                                                )
                                                                .child(highlight_terms_cased(
                                                                    &hit_title,
                                                                    &terms,
                                                                    theme.text_primary,
                                                                    is_selected,
                                                                    cs,
                                                                    theme,
                                                                )),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_shrink_0()
                                                                .text_xs()
                                                                .text_color(theme.text_muted)
                                                                .child(
                                                                    self.format_match_count(
                                                                        hit_count,
                                                                    ),
                                                                ),
                                                        ),
                                                )
                                                .when(
                                                    !is_collapsed && !hit_snippet.is_empty(),
                                                    |d| {
                                                        d.child(
                                                            div()
                                                                .pl(px(24.0))
                                                                .pr_1()
                                                                .text_xs()
                                                                .line_height(px(18.0))
                                                                .child(
                                                                    highlight_terms_cased(
                                                                        &hit_snippet,
                                                                        &terms,
                                                                        theme.text_secondary,
                                                                        is_selected,
                                                                        cs,
                                                                        theme,
                                                                    ),
                                                                ),
                                                        )
                                                    },
                                                )
                                        },
                                    )),
                            );
                        let scroll_style = panel_scrollbar_style();
                        if let Some(geometry) = ScrollbarGeometry::from_handle(
                            &self.panel_scroll_handle,
                            &scroll_style,
                        ) {
                            let dragging = self.panel_scrollbar_drag.active;
                            viewport = viewport.child(overlay_scrollbar(
                                "search-panel-scrollbar-track",
                                &geometry,
                                &scroll_style,
                                theme,
                                dragging,
                                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    let pointer_y: f32 = event.position.y.into();
                                    this.set_panel_scroll_top(
                                        geometry.offset_for_track_click(pointer_y),
                                    );
                                    cx.notify();
                                }),
                                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.panel_scrollbar_drag
                                        .begin(event.position.y.into(), geometry.scroll_y);
                                    cx.notify();
                                }),
                            ));
                        }
                        viewport
                    })
                    })
            })
    }
}
