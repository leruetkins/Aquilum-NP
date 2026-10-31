//! Dropdown list of open tabs (Obsidian-style tab switcher).
//!
//! The list is scrolled by the **absolute cursor position** inside it. The wheel,
//! held buttons and dragging are not involved: the cursor near the top edge shows
//! the beginning of the list, near the bottom edge the end, and in between the
//! offset is proportional to the cursor position.
//!
//! The top and bottom edge have a dead zone (see [`edge_pad`]): while the cursor
//! is inside it the list already sits at its extreme position, so the first and
//! the last row are fully visible without pushing the mouse against the border.
//!
//! The offset follows the target derived from the cursor position directly:
//! no spring, no inertia, no tail. The mouse sets the position, the list stands
//! where it was put and stops dead the moment input stops. The offset is
//! always snapped to whole device pixels, so text and icons move together.
//!
//! The parent is responsible for positioning: this component only renders the
//! panel itself (`deferred(anchored(...))` lives in the parent).
//!
//! The native wheel scrolling of the list is swallowed in the capture phase
//! (see the `canvas` paint callback): gpui binds it to the list element itself
//! and it runs before any ancestor listener, so a regular `on_scroll_wheel`
//! from above cannot intercept it. Scrolling is cursor-driven only.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    canvas, div, point, prelude::*, px, uniform_list, AnyElement, Bounds, ClipboardItem, Context,
    DispatchPhase, EventEmitter, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, Rgba, ScrollHandle, ScrollWheelEvent, SharedString,
    UniformListScrollHandle, Window,
};

use crate::blink_manager::BlinkManager;
use crate::icons;
use crate::snap::{snap_x, snapped_span};
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::tabs_menu::format_bookmark_label;

/// Height of a single row (px). The keyboard navigation relies on it.
pub const ROW_HEIGHT: f32 = 28.0;

/// Dead zone at the top and bottom edge, as a fraction of the list height...
const EDGE_PAD_FRACTION: f32 = 0.10;
/// ...but never smaller than one row, so the last row ends up fully under the
/// cursor before it reaches the border...
const EDGE_PAD_MIN: f32 = ROW_HEIGHT;
/// ...and never larger than this on very tall lists.
const EDGE_PAD_MAX: f32 = 56.0;

/// Number of action rows in the popup header (fold, bookmark, close all).
const HEADER_ROWS: usize = 3;
/// Number of dividers in the popup header.
const HEADER_DIVIDERS: usize = 2;
/// Height of one header divider (px): 1px line plus `my_1` (4px each side).
const DIVIDER_H: f32 = 9.0;
/// Total height of the popup header (px): action rows plus dividers.
const HEADER_H: f32 = ROW_HEIGHT * HEADER_ROWS as f32 + DIVIDER_H * HEADER_DIVIDERS as f32;

/// Mouse movements shorter than this (px) are ignored to suppress jitter.
const MIN_MOVE: f32 = 0.5;
/// Upper bound for a single frame step (s): keeps a long stall from producing a jump.
const MAX_FRAME_DT: f32 = 0.05;
/// A scroll range below this (px) counts as "everything fits": no cursor scrolling.
const MIN_SCROLL_RANGE: f32 = 1.0;

/// Panel width (px).
const LIST_WIDTH: f32 = 280.0;
/// Vertical padding of the panel (px). Together with the border it is subtracted
/// from the panel height to get the list viewport, so the height must include it.
const PANEL_PAD_Y: f32 = 4.0;
/// Border width of the panel (px).
const PANEL_BORDER: f32 = 1.0;
/// Total vertical space taken by the panel padding and border.
const PANEL_CHROME: f32 = PANEL_PAD_Y * 2.0 + PANEL_BORDER * 2.0;

/// Margin from the window edges (px) when the list wants to be taller than the
/// window. The parent uses it to place the panel so that the heights match.
pub const WINDOW_MARGIN: f32 = 16.0;
/// Distance from the top of the window to the top of the panel (px): the height of
/// the unified titlebar, so that the panel opens *below* the tabs instead of
/// covering them. Same anchor the old tabs dropdown menu uses.
pub const WINDOW_ANCHOR_TOP: f32 = 38.0;
/// Extra gap (px) below the titlebar: the "+" and the chevron that opens this popup
/// live at the right end of that row, and the panel must visibly hang from them
/// rather than glue itself to the row's bottom border.
pub const POPUP_GAP: f32 = 4.0;
/// Right offset (px) of the panel's right edge, measured from the window edge. It
/// sums the three native window controls plus the action group's padding, the
/// sidebar toggle and its gap, so the edge lines up with the chevron that opens the
/// popup. Applies while the right sidebar is closed; when it is open the parent
/// computes the offset from the sidebar width instead (the buttons shift left).
/// See `titlebar-right-controls-group` in `app_view.rs`.
pub const POPUP_RIGHT_OFFSET: f32 = 46.0 * 3.0 + 4.0 + 32.0 + 4.0;
/// File and check mark icon size (px).
const ICON_SIZE: f32 = 14.0;

/// A single row of the tab list.
#[derive(Clone)]
pub struct TabEntry {
    /// Identifier the parent understands; passed back in [`TabListEvent::Activate`].
    pub id: usize,
    /// Row title shown in the row.
    pub title: SharedString,
    /// Optional right-aligned muted subtitle (e.g. the English locale name).
    pub subtitle: Option<SharedString>,
    /// Leading icon (file for tabs, globe for locales, ...).
    pub icon: &'static str,
    /// Font family the title is previewed in; `None` uses the UI font.
    /// Lets e.g. every font choice render in itself.
    pub font_family: Option<SharedString>,
    /// Colored leading dot instead of the icon (filter swatches, status
    /// pickers); `None` renders `icon` as before.
    pub dot_color: Option<Rgba>,
}

/// Events emitted by [`TabListPopup`] towards the parent.
#[derive(Clone, Copy, Debug)]
pub enum TabListEvent {
    /// The user picked the tab with this id (click or Enter).
    Activate(usize),
    /// The user clicked "Fold tabs" in the header.
    FoldTabs,
    /// The user clicked the bookmark action in the header.
    BookmarkTabs,
    /// The user clicked "Close all tabs" in the header.
    CloseAllTabs,
    /// The list should be closed (Escape or a click outside of it).
    Dismiss,
}

impl EventEmitter<TabListEvent> for TabListPopup {}

/// Localized labels of the popup header (Obsidian-style actions above the
/// tab list). Computed once by the parent, which owns the locale.
#[derive(Clone)]
pub struct TabListHeader {
    pub fold: SharedString,
    pub bookmark: SharedString,
    pub close_all: SharedString,
}

/// Size of the dead zone at the top and at the bottom of the list.
///
/// A fraction of the list height, but at least one row (so the last row is fully
/// visible while the cursor is still a row away from the border) and at most
/// [`EDGE_PAD_MAX`]. On very short lists it never takes more than a third of the
/// height, otherwise nothing would be left for the proportional part.
fn edge_pad(list_height: Pixels) -> Pixels {
    (list_height * EDGE_PAD_FRACTION)
        .clamp(px(EDGE_PAD_MIN), px(EDGE_PAD_MAX))
        .min(list_height / 3.0)
}

/// Target offset for the given cursor position: `0` at the top of the list,
/// `-max_scroll` at the bottom, proportional in between.
///
/// The scroll speed follows from this formula by itself: it equals
/// `max_scroll / (list_height - 2 * edge_pad)`, so a long list moves faster than a
/// short one for the same cursor travel.
fn cursor_target_offset(
    cursor_y: Pixels,
    list_top: Pixels,
    list_height: Pixels,
    max_scroll: Pixels,
) -> Pixels {
    let pad = edge_pad(list_height);
    let usable = (list_height - pad * 2.0).max(px(1.0));
    let t = ((cursor_y - list_top - pad) / usable).clamp(0.0, 1.0);
    // In gpui the offset is negative: 0 is the top, -max is the bottom.
    -(max_scroll * t)
}

/// Row index at a cursor height, or `None` outside `[top, top + height]`.
///
/// Pure geometry shared by the hover highlight: `floor` gives the lower row
/// a deterministic win exactly on a boundary. That matters because gpui
/// hitboxes include both edges (`Bounds::contains` uses `>=`/`<=`), so a
/// cursor parked exactly on a row seam belongs to two hitboxes and the stock
/// CSS `:hover` lights both rows at once.
fn hover_row_at(
    cursor_y: f32,
    top: f32,
    height: f32,
    offset_y: f32,
    row_h: f32,
    count: usize,
) -> Option<usize> {
    if count == 0 || height <= 0.0 || row_h <= 0.0 {
        return None;
    }
    if cursor_y < top || cursor_y > top + height {
        return None;
    }
    let content_y = (cursor_y - top) + (-offset_y);
    let ix = (content_y / row_h).floor() as isize;
    if ix < 0 || ix >= count as isize {
        None
    } else {
        Some(ix as usize)
    }
}

/// Header action row at a height relative to the header top, or `None` on
/// dividers, padding, or outside. Layout mirrors `render_header` exactly:
/// three 28 px rows split by two 9 px dividers.
fn hover_header_at(rel_y: f32) -> Option<usize> {
    // (exclusive upper bound, row) segments from the header top.
    const BOUNDS: [(f32, Option<usize>); 5] = [
        (ROW_HEIGHT, Some(0)),
        (ROW_HEIGHT * 2.0, Some(1)),
        (ROW_HEIGHT * 2.0 + DIVIDER_H, None),
        (ROW_HEIGHT * 3.0 + DIVIDER_H, Some(2)),
        (HEADER_H, None),
    ];
    if rel_y < 0.0 {
        return None;
    }
    for (end, row) in BOUNDS {
        if rel_y < end {
            return row;
        }
    }
    None
}

/// Fraction of the remaining distance covered in one `dt`-second frame.
/// Time-based, so identical wall-clock motion converges equally at any fps.
/// A zero (or negative) tau means "already there".
fn smoothing_alpha(dt_secs: f32, tau_ms: f32) -> f32 {
    if tau_ms <= 0.0 {
        return 1.0;
    }
    1.0 - (-dt_secs * 1000.0 / tau_ms).exp()
}

/// Offset that brings the row `[item_top, item_top + item_height]` into the
/// viewport with the least movement, clamped to the scrollable range.
///
/// This is the keyboard equivalent of `ScrollStrategy::Nearest`, which gpui 0.2.2
/// does not have.
fn nearest_offset(
    item_top: Pixels,
    item_height: Pixels,
    view_top: Pixels,
    view_height: Pixels,
    current_offset: Pixels,
    max_scroll: Pixels,
) -> Pixels {
    let item_bottom = item_top + item_height;
    let view_bottom = view_top + view_height;
    let target = if item_top < view_top {
        -item_top
    } else if item_bottom > view_bottom {
        -(item_bottom - view_height)
    } else {
        // Already fully visible: keep the current offset.
        current_offset
    };
    target.clamp(-max_scroll, px(0.0))
}

/// Rounds a value to a whole number of device pixels. A fractional offset makes
/// text and SVG icons snap to the pixel grid differently, which shows up as them
/// shimmering out of sync while the list moves.
fn snap_to_device_pixel(value: Pixels, scale: f32) -> Pixels {
    (value * scale).round() / scale
}

/// Dropdown list of open tabs with cursor driven scrolling.
pub struct TabListPopup {
    theme: Theme,
    tabs: Vec<TabEntry>,
    active_ix: Option<usize>,
    /// Action header above the list; `None` hides it (e.g. locale picker).
    header: Option<TabListHeader>,
    /// Extra cap on the list viewport height; `None` lets it fill free space.
    max_list_h: Option<Pixels>,
    /// Light exponential smoothing (time constant, ms) instead of direct
    /// follow; `None` stands exactly where the cursor put it. Only small
    /// popups (e.g. locales) opt in — the tabs list stays instant.
    smooth_tau_ms: Option<f32>,
    /// Timestamp of the previous animation frame (for `dt`); only used when
    /// `smooth_tau_ms` is set.
    last_frame: Option<Instant>,
    scroll: UniformListScrollHandle,
    /// List viewport in window coordinates; filled in prepaint through a canvas.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Wanted offset (<= 0). `None` until the mouse has moved or the keyboard acted.
    target_y: Option<Pixels>,
    /// Last observed cursor position; the very first event only primes it.
    last_mouse_y: Option<Pixels>,
    focus_handle: FocusHandle,
    custom_width: Option<Pixels>,
    width_full: bool,
    show_icons: bool,
    subtitle_below: bool,
    custom_row_height: Option<f32>,
    search_enabled: bool,
    search_placeholder: SharedString,
    search_query: String,
    no_matches_label: SharedString,
    filtered_indices: Vec<usize>,
    cursor_filtered_ix: Option<usize>,
    search_cursor_pos: usize,
    search_selection_anchor: Option<usize>,
    search_is_mouse_selecting: bool,
    search_is_focused: bool,
    search_box_origin_x: Rc<Cell<f32>>,
    blink: BlinkManager,
}

/// Returns the normalized word range `(start, end)` around character index `pos`.
fn search_word_range_at(text: &str, pos: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return (0, 0);
    }
    let pos = pos.min(chars.len().saturating_sub(1));
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
    (start, end)
}

impl TabListPopup {
    /// Builds the popup. The caller has to focus [`TabListPopup::focus_handle`]
    /// after the entity is created, otherwise the keyboard will not work.
    pub fn new(
        tabs: Vec<TabEntry>,
        active_ix: Option<usize>,
        theme: Theme,
        header: Option<TabListHeader>,
        max_list_h: Option<Pixels>,
        smooth_tau_ms: Option<f32>,
        cx: &mut Context<Self>,
    ) -> Self {
        let scroll = UniformListScrollHandle::new();
        let filtered_indices: Vec<usize> = (0..tabs.len()).collect();
        let active_ix = match active_ix {
            Some(ix) if !tabs.is_empty() && ix < tabs.len() => Some(ix),
            _ => None,
        };
        // Show the active tab right away; the first mouse move must not make the list jump.
        // scroll.scroll_to_item(active_ix, ScrollStrategy::Center);
        Self {
            theme,
            tabs,
            active_ix,
            header,
            max_list_h,
            smooth_tau_ms,
            scroll,
            bounds: Rc::new(Cell::new(Bounds::default())),
            target_y: None,
            last_mouse_y: None,
            last_frame: None,
            focus_handle: cx.focus_handle(),
            custom_width: None,
            width_full: false,
            show_icons: true,
            subtitle_below: false,
            custom_row_height: None,
            search_enabled: false,
            search_placeholder: SharedString::from("Поиск..."),
            search_query: String::new(),
            no_matches_label: SharedString::from("Ничего не найдено"),
            filtered_indices,
            cursor_filtered_ix: None,
            search_cursor_pos: 0,
            search_selection_anchor: None,
            search_is_mouse_selecting: false,
            search_is_focused: false,
            search_box_origin_x: Rc::new(Cell::new(0.0)),
            blink: BlinkManager::new(),
        }
    }

    /// Resets the cursor blink cycle, showing the cursor and pausing blink.
    pub fn reset_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.blink.pause_and_show_view(|this| &mut this.blink, cx);
    }

    /// Enables cursor blinking when focused.
    pub fn enable_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.blink.enable_view(|this| &mut this.blink, cx);
    }

    /// Disables cursor blinking when losing focus.
    pub fn disable_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.blink.disable_view(cx);
    }

    /// Helper for cursor visibility according to BlinkManager state.
    #[inline]
    pub fn cursor_visible(&self) -> bool {
        self.blink.visible()
    }

    /// Returns the normalized `(start, end)` character range of the current selection, if any.
    pub fn search_selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.search_selection_anchor {
            let pos = self.search_cursor_pos;
            if anchor != pos {
                return Some((anchor.min(pos), anchor.max(pos)));
            }
        }
        None
    }

    /// Handles mouse drag text selection across the search input.
    fn handle_search_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.search_is_mouse_selecting {
            return;
        }
        let x: f32 = event.position.x.into();
        let box_x = self.search_box_origin_x.get();
        let rel_x = (x - box_x).max(0.0);
        let new_pos = if self.search_query.is_empty() {
            0
        } else {
            let run = gpui::TextRun {
                len: self.search_query.len(),
                font: gpui::font("Segoe UI"),
                color: Default::default(),
                underline: Default::default(),
                strikethrough: None,
                background_color: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(self.search_query.clone()),
                px(12.0),
                &[run],
                None,
            );
            let byte_idx = shaped.closest_index_for_x(px(rel_x));
            self.search_query[..byte_idx.min(self.search_query.len())].chars().count()
        };
        if self.search_cursor_pos != new_pos {
            self.search_cursor_pos = new_pos;
            self.reset_cursor_blink(cx);
            cx.notify();
        }
    }

    /// Finishes mouse drag selection and collapses zero-width selection.
    fn handle_search_mouse_up(&mut self, cx: &mut Context<Self>) {
        if self.search_is_mouse_selecting {
            self.search_is_mouse_selecting = false;
            if self.search_selection_anchor == Some(self.search_cursor_pos) {
                self.search_selection_anchor = None;
            }
            cx.notify();
        }
    }

    /// Overrides the default width for this popup.
    pub fn with_width(mut self, width: Pixels) -> Self {
        self.custom_width = Some(width);
        self
    }

    /// Makes the popup expand to full parent width (`.w_full()`).
    pub fn with_full_width(mut self) -> Self {
        self.width_full = true;
        self
    }

    /// Enables a search filter bar at the top of the popup.
    pub fn with_search(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.search_enabled = true;
        let ph: SharedString = placeholder.into();
        self.no_matches_label = if ph.starts_with("Поиск") {
            SharedString::from("Ничего не найдено")
        } else {
            SharedString::from("No matches found")
        };
        self.search_placeholder = ph;
        self
    }

    /// Updates filtered tab indices based on current search query.
    fn update_filter(&mut self) {
        if !self.search_enabled || self.search_query.trim().is_empty() {
            self.filtered_indices = (0..self.tabs.len()).collect();
        } else {
            let q = self.search_query.trim().to_lowercase();
            self.filtered_indices = self
                .tabs
                .iter()
                .enumerate()
                .filter_map(|(ix, tab)| {
                    let title_match = tab.title.to_lowercase().contains(&q);
                    let sub_match = tab
                        .subtitle
                        .as_ref()
                        .map(|s| s.to_lowercase().contains(&q))
                        .unwrap_or(false);
                    if title_match || sub_match {
                        Some(ix)
                    } else {
                        None
                    }
                })
                .collect();
        }
        self.cursor_filtered_ix = if self.filtered_indices.is_empty() {
            None
        } else {
            Some(0)
        };
        let total_chars = self.search_query.chars().count();
        if self.search_cursor_pos > total_chars {
            self.search_cursor_pos = total_chars;
        }
        if let Some(anchor) = self.search_selection_anchor {
            if anchor > total_chars {
                self.search_selection_anchor = Some(total_chars);
            }
            if self.search_selection_anchor == Some(self.search_cursor_pos) {
                self.search_selection_anchor = None;
            }
        }
        let base = self.base_handle();
        base.set_offset(point(px(0.0), px(0.0)));
        self.target_y = None;
    }

    /// Hides leading icons/dots on list rows.
    pub fn without_icons(mut self) -> Self {
        self.show_icons = false;
        self
    }

    /// Displays the subtitle underneath the title in a two-line layout.
    pub fn with_subtitle_below(mut self) -> Self {
        self.subtitle_below = true;
        if self.custom_row_height.is_none() {
            self.custom_row_height = Some(42.0);
        }
        self
    }

    /// Sets a custom row height (defaults to [`ROW_HEIGHT`]).
    pub fn with_row_height(mut self, height: f32) -> Self {
        self.custom_row_height = Some(height);
        self
    }

    /// Current height of each row in the list.
    pub fn row_h(&self) -> f32 {
        self.custom_row_height.unwrap_or(ROW_HEIGHT)
    }

    /// Focus handle the parent should focus when the popup opens.
    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    /// Index of the currently highlighted tab, if any.
    pub fn active_ix(&self) -> Option<usize> {
        self.active_ix
    }

    /// The base scroll handle of the list: the single source of truth for the offset.
    fn base_handle(&self) -> ScrollHandle {
        self.scroll.0.borrow().base_handle.clone()
    }

    /// Maps the absolute cursor position to a target offset.
    ///
    /// The repaint at the end is unconditional, and that is not an optimisation detail:
    /// the hover highlight is computed from the cursor position while a frame is built
    /// (see `hover_tab_ix`), and the window is only redrawn when a listener asks for
    /// it. A cursor that lands on a row in a single jump produces exactly one event,
    /// so gating `notify` behind "the offset changed" would leave that row unlit.
    /// Everything above only decides the offset.
    fn on_mouse_move(&mut self, y: Pixels, cx: &mut Context<Self>) {
        if self.search_enabled && self.cursor_filtered_ix.is_some() {
            self.cursor_filtered_ix = None;
        }
        match self.last_mouse_y {
            // The first event only primes the reference point: the cursor still sits over the
            // chevron that opened the panel, so acting on it would make the list jump.
            None => self.last_mouse_y = Some(y),
            // The distance is measured from the last *accepted* position, so a slow move
            // accumulates instead of being dropped event by event.
            Some(anchor) if (y - anchor).abs() >= px(MIN_MOVE) => {
                self.last_mouse_y = Some(y);

                let bounds = self.bounds.get();
                let base = self.base_handle();
                let max_scroll = base.max_offset().height;
                if bounds.size.height > px(0.0) && max_scroll >= px(MIN_SCROLL_RANGE) {
                    self.target_y = Some(cursor_target_offset(
                        y,
                        bounds.origin.y,
                        bounds.size.height,
                        max_scroll,
                    ));
                }
            }
            Some(_) => {}
        }

        // Unconditional repaint: the hover highlight needs it (see the note on this function).
        cx.notify();
    }

    /// Moves the highlight and keeps it visible.
    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.filtered_indices.len();
        if count == 0 {
            return;
        }
        let cur = if self.search_enabled {
            self.cursor_filtered_ix
        } else {
            self.active_ix
        };
        let next = match cur {
            Some(ix) => (ix as isize + delta).rem_euclid(count as isize) as usize,
            None => {
                if delta >= 0 {
                    0
                } else {
                    count - 1
                }
            }
        };
        if self.search_enabled {
            self.cursor_filtered_ix = Some(next);
        } else {
            self.active_ix = Some(next);
        }
        // The keyboard wins over the mouse: drop its target before recomputing.
        self.target_y = None;
        self.scroll_selection_into_view();
        cx.notify();
    }

    /// Picks the offset that shows the highlighted row with the least movement.
    /// Applied directly on the next frame (see [`animate`]).
    fn scroll_selection_into_view(&mut self) {
        let target_pos = if self.search_enabled {
            self.cursor_filtered_ix
        } else {
            self.active_ix
        };
        let Some(pos) = target_pos else {
            return;
        };
        let base = self.base_handle();
        let max_scroll = base.max_offset().height;
        let viewport = base.bounds().size.height;
        // `max_offset()` is 0 until the first layout: never clamp against it there.
        if max_scroll < px(MIN_SCROLL_RANGE) || viewport <= px(0.0) {
            return;
        }

        let offset = base.offset().y;
        let row_h = self.row_h();
        self.target_y = Some(nearest_offset(
            px(row_h) * pos as f32,
            px(row_h),
            -offset,
            viewport,
            offset,
            max_scroll,
        ));
    }

    /// Applies the pending target offset. Without smoothing the list stands
    /// exactly where the cursor (or keyboard) put it and stops dead the
    /// moment input stops; with `smooth_tau_ms` set it chases the target with
    /// a light exponential glide instead. Returns `false` when there is no
    /// animation running and no further frame is needed.
    ///
    /// `scale` is the window scale factor: the shown offset is kept on whole
    /// device pixels (see [`snap_to_device_pixel`]).
    fn animate(&mut self, scale: f32) -> bool {
        let Some(target) = self.target_y.take() else {
            self.last_frame = None;
            return false;
        };

        if self.smooth_tau_ms.is_none() {
            let base = self.base_handle();
            let max = f32::from(base.max_offset().height);
            let clamped = f32::from(target).clamp(-max, 0.0);
            let mut offset = base.offset();
            offset.y = snap_to_device_pixel(px(clamped), scale);
            base.set_offset(offset);
            return false;
        }
        let tau_ms = self.smooth_tau_ms.unwrap_or(50.0);

        // `dt` comes from the real frame time, so the glide does not depend
        // on the refresh rate. The first frame after a pause counts as 1/60 s.
        let now = Instant::now();
        let dt = self.last_frame.map_or(1.0 / 60.0, |previous| {
            now.duration_since(previous).as_secs_f32().min(MAX_FRAME_DT)
        });
        self.last_frame = Some(now);

        let base = self.base_handle();
        let max = f32::from(base.max_offset().height);
        let clamped = f32::from(target).clamp(-max, 0.0);
        let mut offset = base.offset();
        let pos = f32::from(offset.y);
        let shown = pos + (clamped - pos) * smoothing_alpha(dt, tau_ms);
        // Within a device pixel: snap onto the target and consume it.
        if (clamped - shown).abs() < 1.0 / scale {
            offset.y = snap_to_device_pixel(px(clamped), scale);
            base.set_offset(offset);
            self.last_frame = None;
            return false;
        }
        offset.y = snap_to_device_pixel(px(shown), scale);
        base.set_offset(offset);
        self.target_y = Some(target);
        true
    }

    /// Tab row under the last known cursor position, recomputed every frame.
    ///
    /// The highlight follows sliding rows during the scroll animation even
    /// with a stationary mouse, and resolves to a single row on seams (unlike
    /// CSS `:hover`, whose hitboxes include both edges).
    fn hover_tab_ix(&self) -> Option<usize> {
        let y = self.last_mouse_y?;
        let bounds = self.bounds.get();
        hover_row_at(
            f32::from(y),
            f32::from(bounds.origin.y),
            f32::from(bounds.size.height),
            f32::from(self.base_handle().offset().y),
            self.row_h(),
            self.filtered_indices.len(),
        )
    }

    /// Header action row under the cursor (0, 1 or 2), if the popup has
    /// a header at all.
    fn hover_header_ix(&self) -> Option<usize> {
        let bounds = self.bounds.get();
        if bounds.size.height <= px(0.0) {
            return None;
        }
        let y = self.last_mouse_y?;
        hover_header_at(f32::from(y) - (f32::from(bounds.origin.y) - HEADER_H))
    }

    /// Renders a single row: icon, title, optional subtitle, and a check mark
    /// for the active row only.
    fn render_row(&self, ix: usize, hovered: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = &self.tabs[ix];
        let id = tab.id;
        let title = tab.title.clone();
        let subtitle = tab.subtitle.clone();
        let icon = tab.icon;
        let font_family = tab
            .font_family
            .clone()
            .unwrap_or_else(|| SharedString::from("Segoe UI"));
        let is_active = self.active_ix == Some(ix);

        let bg_hover = self.theme.bg_hover;
        let text_primary = self.theme.text_primary;
        let text_secondary = self.theme.text_secondary;
        let text_muted = self.theme.text_muted;
        let accent = self.theme.accent;
        let icon_color = if is_active { accent } else { text_secondary };
        let leading: Option<AnyElement> = if self.show_icons {
            Some(match tab.dot_color {
                Some(color) => div()
                    .w(px(7.0))
                    .h(px(7.0))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(color)
                    .into_any_element(),
                None => icons::icon(icon, ICON_SIZE, icon_color).into_any_element(),
            })
        } else {
            None
        };

        let row = div()
            .id(("tab-list-row", ix))
            .flex()
            // `uniform_list` lays every item out with `layout_as_root`, which sizes the item to
            // its content. Without this the row is only as wide as its text, so the hitbox (and
            // with it hover and clicks) covers the title and nothing to the right of it.
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap_2p5()
            .h(px(self.row_h()))
            .px_3()
            .rounded_md()
            .cursor_pointer()
            // Manual highlight (see `hover_tab_ix`): stock hover lights every
            // hitbox sharing the cursor point, i.e. both rows on a seam.
            .when(hovered, move |row| row.bg(bg_hover))
            .when_some(leading, |row, icon_el| row.child(icon_el));

        let row = if self.subtitle_below {
            row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_0p5()
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .font_family(font_family)
                            .text_color(text_primary)
                            .child(title),
                    )
                    .when_some(subtitle, |col, subtitle| {
                        col.child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(text_muted)
                                .child(subtitle),
                        )
                    }),
            )
        } else {
            row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(text_primary)
                    .child(title),
            )
            .when_some(subtitle, |row, subtitle| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .ml_1p5()
                        .text_xs()
                        .text_color(text_muted)
                        .child(subtitle),
                )
            })
        };

        row.when(is_active, move |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .child(icons::icon(icons::ICON_CHECK, ICON_SIZE, text_secondary)),
            )
        })
        .on_click(cx.listener(move |_this, _event, _window, cx| {
            cx.emit(TabListEvent::Activate(id));
        }))
    }

    /// Renders one header action row: icon plus label, same metrics as a tab
    /// row so the whole panel reads as one menu. Emits the given event.
    fn render_header_row(
        &self,
        id: &'static str,
        icon: &'static str,
        label: SharedString,
        event: TabListEvent,
        hovered: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bg_hover = self.theme.bg_hover;
        div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap_2p5()
            .h(px(ROW_HEIGHT))
            .px_3()
            .rounded_md()
            .cursor_pointer()
            .when(hovered, move |row| row.bg(bg_hover))
            .child(icons::icon(icon, ICON_SIZE, self.theme.text_secondary))
            .child(
                div()
                    .text_xs()
                    .text_color(self.theme.text_primary)
                    .child(label),
            )
            .on_click(cx.listener(move |_this, _event, _window, cx| {
                cx.emit(event);
            }))
    }

    /// Renders a hairline divider between header sections.
    fn render_header_divider(&self, id: &'static str) -> impl IntoElement {
        div()
            .id(id)
            .h_px()
            .bg(self.theme.border_subtle)
            .my_1()
            .mx_1()
    }

    /// Renders the popup header: Obsidian-style tab actions above the list.
    fn render_header(&self, header: &TabListHeader, cx: &mut Context<Self>) -> impl IntoElement {
        let hover = self.hover_header_ix();
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(self.render_header_row(
                "tab-list-fold",
                icons::ICON_LAYERS,
                header.fold.clone(),
                TabListEvent::FoldTabs,
                hover == Some(0),
                cx,
            ))
            .child(self.render_header_row(
                "tab-list-bookmark",
                icons::ICON_BOOKMARK,
                header.bookmark.clone(),
                TabListEvent::BookmarkTabs,
                hover == Some(1),
                cx,
            ))
            .child(self.render_header_divider("tab-list-divider-1"))
            .child(self.render_header_row(
                "tab-list-close-all",
                icons::ICON_CLOSE,
                header.close_all.clone(),
                TabListEvent::CloseAllTabs,
                hover == Some(2),
                cx,
            ))
            .child(self.render_header_divider("tab-list-divider-2"))
    }

    /// Renders the search input row at the top of the popup.
    fn render_search_bar(
        &self,
        window: &Window,
        pixel_scale: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        let is_search_focused = self.search_is_focused;
        let query = &self.search_query;
        let query_chars: Vec<char> = query.chars().collect();
        let cursor_pos = self.search_cursor_pos.min(query_chars.len());
        let selection_range = self.search_selection_range();

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
            snap_x(f32::from(shaped.x_for_index(byte_index)), pixel_scale)
        };

        let (sel_x1, sel_w) = if let Some((s_start, s_end)) = selection_range {
            if !query.is_empty() {
                let b1: usize = query_chars[..s_start.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                let b2: usize = query_chars[..s_end.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                snapped_span(
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

        let box_cell = self.search_box_origin_x.clone();

        div()
            .id("tab-list-search-box")
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(32.0))
            .px_3()
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                window.focus(&this.focus_handle);
                this.search_is_focused = true;
                this.reset_cursor_blink(cx);

                let click_x: f32 = event.position.x.into();
                let box_x = this.search_box_origin_x.get();
                let rel_x = (click_x - box_x).max(0.0);

                let clicked_pos = if this.search_query.is_empty() {
                    0
                } else {
                    let run = gpui::TextRun {
                        len: this.search_query.len(),
                        font: gpui::font("Segoe UI"),
                        color: Default::default(),
                        underline: Default::default(),
                        strikethrough: None,
                        background_color: None,
                    };
                    let shaped = window.text_system().shape_line(
                        SharedString::from(this.search_query.clone()),
                        px(12.0),
                        &[run],
                        None,
                    );
                    let byte_idx = shaped.closest_index_for_x(px(rel_x));
                    this.search_query[..byte_idx.min(this.search_query.len())].chars().count()
                };

                let total_chars = this.search_query.chars().count();
                if event.click_count == 2 {
                    let (start, end) = search_word_range_at(&this.search_query, clicked_pos);
                    this.search_selection_anchor = Some(start);
                    this.search_cursor_pos = end;
                    this.search_is_mouse_selecting = false;
                } else if event.click_count >= 3 {
                    this.search_selection_anchor = Some(0);
                    this.search_cursor_pos = total_chars;
                    this.search_is_mouse_selecting = false;
                } else if event.modifiers.shift {
                    if this.search_selection_anchor.is_none() {
                        this.search_selection_anchor = Some(this.search_cursor_pos);
                    }
                    this.search_cursor_pos = clicked_pos;
                    this.search_is_mouse_selecting = true;
                } else {
                    this.search_cursor_pos = clicked_pos;
                    this.search_selection_anchor = Some(clicked_pos);
                    this.search_is_mouse_selecting = true;
                }
                cx.notify();
            }))
            .child(icons::icon(icons::ICON_SEARCH, 13.0, theme.text_muted))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h(px(16.0))
                    .child(
                        gpui::canvas(
                            move |bounds, _, _| {
                                box_cell.set(f32::from(bounds.origin.x));
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
                                .whitespace_nowrap()
                                .child(self.search_placeholder.clone())
                        } else {
                            div()
                                .font_family("Segoe UI")
                                .text_xs()
                                .line_height(px(16.0))
                                .text_color(theme.text_primary)
                                .whitespace_nowrap()
                                .child(query.clone())
                        }
                    )
                    // 3. Crisp Caret
                    .when(is_search_focused && self.cursor_visible(), |s| {
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
                    }),
            )
            .when(!query.is_empty(), |bar| {
                bar.child(
                    div()
                        .id("tab-list-search-clear")
                        .w(px(16.0))
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .rounded_xs()
                        .hover(|h| h.bg(theme.bg_hover))
                        .child(icons::icon(icons::ICON_CLOSE, 10.0, theme.text_muted))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.search_query.clear();
                            this.search_cursor_pos = 0;
                            this.search_selection_anchor = None;
                            this.search_is_mouse_selecting = false;
                            this.reset_cursor_blink(cx);
                            this.update_filter();
                            window.focus(&this.focus_handle);
                            cx.notify();
                        })),
                )
            })
    }

    /// Renders a horizontal divider separating the search box from list rows.
    fn render_search_divider(&self) -> impl IntoElement {
        div()
            .id("tab-list-search-divider")
            .h_px()
            .w_full()
            .bg(self.theme.border)
            .mb_1()
    }
}

impl Render for TabListPopup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Keep the animation alive until the offset reaches its target.
        if self.animate(window.scale_factor()) {
            window.request_animation_frame();
        }

        let pixel_scale = window.scale_factor();
        let is_popup_focused = self.focus_handle.is_focused(window);
        if self.search_enabled {
            if is_popup_focused {
                if !self.search_is_focused || !self.blink.is_enabled() {
                    self.search_is_focused = true;
                    self.blink.enable_view(|s| &mut s.blink, cx);
                }
            } else if self.search_is_focused {
                self.search_is_focused = false;
                self.blink.disable_view(cx);
                self.search_selection_anchor = None;
                self.search_is_mouse_selecting = false;
            }
        }

        // Almost the whole window height when there are many tabs, content height otherwise.
        // The anchor top is not zero (the panel opens below the titlebar plus a gap), so
        // the free space below the anchor is what bounds the height, otherwise the panel
        // would run out of the window at the bottom.
        let free_height = window.viewport_size().height
            - px(WINDOW_ANCHOR_TOP + POPUP_GAP)
            - px(WINDOW_MARGIN);
        // The panel height is header + search + list viewport + its padding and border.
        let header_h = if self.header.is_some() {
            px(HEADER_H)
        } else {
            px(0.0)
        };
        let search_h = if self.search_enabled {
            px(37.0)
        } else {
            px(0.0)
        };
        let chrome = px(PANEL_CHROME);
        let row_h = self.row_h();
        let has_no_matches = self.search_enabled && self.filtered_indices.is_empty();
        let list_h = if has_no_matches {
            px(44.0)
        } else {
            let lh = (px(row_h) * self.filtered_indices.len() as f32)
                .min((free_height - header_h - search_h - chrome).max(px(0.0)));
            match self.max_list_h {
                Some(cap) => lh.min(cap),
                None => lh,
            }
        };
        let height = header_h + search_h + list_h + chrome;

        let bounds = self.bounds.clone();
        let count = self.filtered_indices.len();

        let bg_panel = self.theme.bg_sidebar;
        let border = self.theme.border;

        let mut popup = div()
            .id("tab-list-popup")
            .relative()
            .flex()
            .flex_col()
            // The panel occludes the editor behind it: without its own
            // blocking hitbox, mouse presses pass through to the editor.
            .occlude();
        if self.width_full {
            popup = popup.w_full();
        } else {
            popup = popup.w(self.custom_width.unwrap_or(px(LIST_WIDTH)));
        }
        popup
            .h(height)
            .py(px(PANEL_PAD_Y))
            // Horizontal panel padding: every row (header and tabs) sits
            // inset from the border, so hover highlights never touch the edge.
            .px_1p5()
            .bg(bg_panel)
            .border_1()
            .border_color(border)
            .rounded_md()
            .shadow_lg()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.search_enabled && this.search_is_mouse_selecting {
                    this.handle_search_mouse_move(event, window, cx);
                } else {
                    this.on_mouse_move(event.position.y, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.search_enabled && this.search_is_mouse_selecting {
                    this.handle_search_mouse_up(cx);
                }
            }))
            // Leaving the panel clears the manual highlight (see `hover_tab_ix`):
            // without this the last row would stay lit forever.
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !hovered {
                    this.last_mouse_y = None;
                    if this.search_is_mouse_selecting {
                        this.search_is_mouse_selecting = false;
                    }
                    cx.notify();
                }
            }))
            .on_mouse_down_out(cx.listener(|_this, _event, _window, cx| {
                cx.emit(TabListEvent::Dismiss);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                let key = event.keystroke.key.as_str();
                let ctrl = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
                let shift = event.keystroke.modifiers.shift;
                let alt = event.keystroke.modifiers.alt;

                match key {
                    "escape" => {
                        cx.stop_propagation();
                        cx.emit(TabListEvent::Dismiss);
                        return;
                    }
                    "down" => {
                        cx.stop_propagation();
                        this.move_selection(1, cx);
                        return;
                    }
                    "up" => {
                        cx.stop_propagation();
                        this.move_selection(-1, cx);
                        return;
                    }
                    "enter" => {
                        cx.stop_propagation();
                        let target_tab = if this.search_enabled {
                            this.cursor_filtered_ix
                                .and_then(|c_ix| this.filtered_indices.get(c_ix))
                                .and_then(|&tab_ix| this.tabs.get(tab_ix))
                        } else {
                            this.active_ix.and_then(|ix| this.tabs.get(ix))
                        };
                        if let Some(tab) = target_tab {
                            cx.emit(TabListEvent::Activate(tab.id));
                        }
                        return;
                    }
                    _ => {}
                }

                if !this.search_enabled {
                    return;
                }

                this.reset_cursor_blink(cx);

                // Ctrl + A (Select All)
                if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
                    cx.stop_propagation();
                    this.search_selection_anchor = Some(0);
                    this.search_cursor_pos = this.search_query.chars().count();
                    cx.notify();
                    return;
                }

                // Ctrl + C (Copy)
                if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
                    cx.stop_propagation();
                    if let Some((s_start, s_end)) = this.search_selection_range() {
                        let selected: String = this.search_query.chars().skip(s_start).take(s_end - s_start).collect();
                        cx.write_to_clipboard(ClipboardItem::new_string(selected));
                    }
                    return;
                }

                // Ctrl + X (Cut)
                if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
                    cx.stop_propagation();
                    if let Some((s_start, s_end)) = this.search_selection_range() {
                        let selected: String = this.search_query.chars().skip(s_start).take(s_end - s_start).collect();
                        cx.write_to_clipboard(ClipboardItem::new_string(selected));
                        let mut chars: Vec<char> = this.search_query.chars().collect();
                        chars.drain(s_start..s_end);
                        this.search_query = chars.into_iter().collect();
                        this.search_cursor_pos = s_start;
                        this.search_selection_anchor = None;
                        this.update_filter();
                        cx.notify();
                    }
                    return;
                }

                // Ctrl + V (Paste)
                if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
                    cx.stop_propagation();
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            let filtered: String = text
                                .chars()
                                .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                                .collect();
                            if !filtered.is_empty() {
                                let mut chars: Vec<char> = this.search_query.chars().collect();
                                let pos = if let Some((s_start, s_end)) = this.search_selection_range() {
                                    chars.drain(s_start..s_end);
                                    this.search_selection_anchor = None;
                                    s_start
                                } else {
                                    this.search_cursor_pos.min(chars.len())
                                };
                                for (i, c) in filtered.chars().enumerate() {
                                    chars.insert(pos + i, c);
                                }
                                this.search_cursor_pos = pos + filtered.chars().count();
                                this.search_query = chars.into_iter().collect();
                                this.update_filter();
                                cx.notify();
                            }
                        }
                    }
                    return;
                }

                // Ctrl + U (Clear line)
                if ctrl && key.eq_ignore_ascii_case("u") {
                    cx.stop_propagation();
                    this.search_query.clear();
                    this.search_cursor_pos = 0;
                    this.search_selection_anchor = None;
                    this.update_filter();
                    cx.notify();
                    return;
                }

                if ctrl {
                    return;
                }

                // Left Arrow
                if key == "left" {
                    cx.stop_propagation();
                    if shift {
                        if this.search_selection_anchor.is_none() {
                            this.search_selection_anchor = Some(this.search_cursor_pos);
                        }
                        if this.search_cursor_pos > 0 {
                            this.search_cursor_pos -= 1;
                        }
                    } else if let Some((s_start, _)) = this.search_selection_range() {
                        this.search_cursor_pos = s_start;
                        this.search_selection_anchor = None;
                    } else if this.search_cursor_pos > 0 {
                        this.search_cursor_pos -= 1;
                    }
                    cx.notify();
                    return;
                }

                // Right Arrow
                if key == "right" {
                    cx.stop_propagation();
                    let total_chars = this.search_query.chars().count();
                    if shift {
                        if this.search_selection_anchor.is_none() {
                            this.search_selection_anchor = Some(this.search_cursor_pos);
                        }
                        if this.search_cursor_pos < total_chars {
                            this.search_cursor_pos += 1;
                        }
                    } else if let Some((_, s_end)) = this.search_selection_range() {
                        this.search_cursor_pos = s_end;
                        this.search_selection_anchor = None;
                    } else if this.search_cursor_pos < total_chars {
                        this.search_cursor_pos += 1;
                    }
                    cx.notify();
                    return;
                }

                // Home
                if key == "home" {
                    cx.stop_propagation();
                    if shift {
                        if this.search_selection_anchor.is_none() {
                            this.search_selection_anchor = Some(this.search_cursor_pos);
                        }
                        this.search_cursor_pos = 0;
                    } else {
                        this.search_selection_anchor = None;
                        this.search_cursor_pos = 0;
                    }
                    cx.notify();
                    return;
                }

                // End
                if key == "end" {
                    cx.stop_propagation();
                    let total_chars = this.search_query.chars().count();
                    if shift {
                        if this.search_selection_anchor.is_none() {
                            this.search_selection_anchor = Some(this.search_cursor_pos);
                        }
                        this.search_cursor_pos = total_chars;
                    } else {
                        this.search_selection_anchor = None;
                        this.search_cursor_pos = total_chars;
                    }
                    cx.notify();
                    return;
                }

                // Backspace
                if key == "backspace" {
                    cx.stop_propagation();
                    let mut chars: Vec<char> = this.search_query.chars().collect();
                    if let Some((s_start, s_end)) = this.search_selection_range() {
                        chars.drain(s_start..s_end);
                        this.search_cursor_pos = s_start;
                        this.search_selection_anchor = None;
                        this.search_query = chars.into_iter().collect();
                        this.update_filter();
                        cx.notify();
                    } else {
                        let pos = this.search_cursor_pos.min(chars.len());
                        if pos > 0 {
                            chars.remove(pos - 1);
                            this.search_cursor_pos = pos - 1;
                            this.search_query = chars.into_iter().collect();
                            this.update_filter();
                            cx.notify();
                        }
                    }
                    return;
                }

                // Delete
                if key == "delete" {
                    cx.stop_propagation();
                    let mut chars: Vec<char> = this.search_query.chars().collect();
                    if let Some((s_start, s_end)) = this.search_selection_range() {
                        chars.drain(s_start..s_end);
                        this.search_cursor_pos = s_start;
                        this.search_selection_anchor = None;
                        this.search_query = chars.into_iter().collect();
                        this.update_filter();
                        cx.notify();
                    } else {
                        let pos = this.search_cursor_pos.min(chars.len());
                        if pos < chars.len() {
                            chars.remove(pos);
                            this.search_query = chars.into_iter().collect();
                            this.update_filter();
                            cx.notify();
                        }
                    }
                    return;
                }

                // Normal Character input
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

                    if let Some(text) = text_to_insert {
                        let filtered: String = text
                            .chars()
                            .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                            .collect();
                        if !filtered.is_empty() {
                            cx.stop_propagation();
                            let mut chars: Vec<char> = this.search_query.chars().collect();
                            let pos = if let Some((s_start, s_end)) = this.search_selection_range() {
                                chars.drain(s_start..s_end);
                                this.search_selection_anchor = None;
                                s_start
                            } else {
                                this.search_cursor_pos.min(chars.len())
                            };
                            for (i, c) in filtered.chars().enumerate() {
                                chars.insert(pos + i, c);
                            }
                            this.search_cursor_pos = pos + filtered.chars().count();
                            this.search_query = chars.into_iter().collect();
                            this.update_filter();
                            cx.notify();
                        }
                    }
                }
            }))
            .when(self.search_enabled, |d| {
                d.child(self.render_search_bar(window, pixel_scale, cx))
                    .child(self.render_search_divider())
            })
            .when_some(self.header.clone(), |d, header| {
                d.child(self.render_header(&header, cx))
            })
            .child(
                // The list viewport. The canvas measures exactly this box, so the
                // cursor mapping is not offset by the header, the search bar, the panel padding
                // or its border.
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .when(has_no_matches, |parent| {
                        parent.child(
                            div()
                                .w_full()
                                .h(px(44.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xs()
                                .text_color(self.theme.text_muted)
                                .child(self.no_matches_label.clone()),
                        )
                    })
                    .when(!has_no_matches, |parent| {
                        parent
                            .child(
                                canvas(
                                    move |viewport, _window, _cx| bounds.set(viewport),
                                    |viewport, _state, window, _cx| {
                                        // Гасим колесо и тачпад в фазе capture: до того, как событие
                                        // дойдёт до слушателя uniform_list (он срабатывает раньше любого
                                        // ancestor-слушателя, так что обычным on_scroll_wheel сверху его
                                        // не перехватить). Прокруткой управляет только положение курсора.
                                        // Слушатель живёт один кадр и регистрируется заново в каждом
                                        // paint — так задумано в gpui, утечки нет. contains() нужен,
                                        // чтобы не глушить колесо вне списка: слушатель висит на всём окне.
                                        window.on_mouse_event(
                                            move |event: &ScrollWheelEvent, phase, _window, cx| {
                                                if phase == DispatchPhase::Capture
                                                    && viewport.contains(&event.position)
                                                {
                                                    cx.stop_propagation();
                                                }
                                            },
                                        );
                                    },
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(
                                uniform_list(
                                    "tab-list-popup-rows",
                                    count,
                                    cx.processor(|this, range: Range<usize>, _window, cx| {
                                        let hover = this.hover_tab_ix();
                                        range
                                            .map(|filtered_pos| {
                                                let tab_ix = this.filtered_indices[filtered_pos];
                                                let is_hovered = hover == Some(filtered_pos)
                                                    || this.cursor_filtered_ix == Some(filtered_pos);
                                                this.render_row(tab_ix, is_hovered, cx)
                                            })
                                            .collect::<Vec<_>>()
                                    }),
                                )
                                .track_scroll(self.scroll.clone())
                                .size_full(),
                            )
                    }),
            )
    }
}

/// Title shown for a tab: the file stem, the localized "New tab" label for
/// empty-tab sentinels, or the "untitled" placeholder as a last resort.
#[allow(dead_code)]
fn tab_title(path: &std::path::Path, new_tab: &str, untitled: &str) -> SharedString {
    SharedString::from(crate::views::new_tab::tab_display_title(path, new_tab, untitled))
}

impl AppView {
    /// Opens the tabs popup for the currently open tabs in the active leaf.
    pub fn open_tabs_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_leaf = self.state.workspace.active_leaf;
        self.open_leaf_tabs_popup(active_leaf, 0.0, 0.0, window, cx);
        self.tabs_popup_pos = None;
    }

    /// Opens the tabs popup for a specific leaf/window at the specified anchor coordinates.
    pub fn open_leaf_tabs_popup(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        click_x: f32,
        click_y: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let leaf = match self.state.leaves.get(&leaf_id) {
            Some(l) => l,
            None => return,
        };
        if leaf.tabs.is_empty() {
            return;
        }
        self.state.workspace.active_leaf = leaf_id;

        let untitled = self.t("common.untitled");
        let new_tab = self.t("tabs.newTab");
        let tabs: Vec<TabEntry> = leaf
            .tabs
            .iter()
            .enumerate()
            .map(|(id, tab)| {
                let (title, icon) = match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        let t = self
                            .state
                            .documents
                            .get(id)
                            .map(|d| d.title.clone())
                            .unwrap_or_else(|| untitled.clone());
                        (t, icons::ICON_FILE_TEXT)
                    }
                    crate::workspace::TabContent::Empty => (new_tab.clone(), icons::ICON_FILE_TEXT),
                    crate::workspace::TabContent::Plugin { view_id } => {
                        (view_id.clone(), icons::ICON_PLUG)
                    }
                };
                TabEntry {
                    id,
                    title: SharedString::from(title),
                    subtitle: None,
                    icon,
                    font_family: None,
                    dot_color: None,
                }
            })
            .collect();

        let active_ix = leaf.active_tab_index.min(leaf.tabs.len().saturating_sub(1));
        let theme = self.theme.clone();
        let header = TabListHeader {
            fold: SharedString::from(self.t("tabs.fold_tabs")),
            bookmark: SharedString::from(format_bookmark_label(
                leaf.tabs.len(),
                &self.config.ui.language,
            )),
            close_all: SharedString::from(self.t("tabs.close_all")),
        };

        let popup = cx.new(|cx| {
            TabListPopup::new(tabs, Some(active_ix), theme, Some(header), None, None, cx)
        });
        // `subscribe_in` is used instead of `subscribe`: activating a tab needs a window.
        let subscription =
            cx.subscribe_in(&popup, window, move |this, _popup, event: &TabListEvent, window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        this.select_leaf_tab(leaf_id, *id, window, cx);
                        this.close_tabs_popup(cx);
                    }
                    TabListEvent::FoldTabs => {
                        this.state.status_text = this.t("tabs.fold_tabs_mode");
                        this.close_tabs_popup(cx);
                    }
                    TabListEvent::BookmarkTabs => {
                        this.state.status_text = this.t("tabs.tabs_added_to_bookmarks");
                        this.close_tabs_popup(cx);
                    }
                    TabListEvent::CloseAllTabs => {
                        this.close_leaf_all_tabs(leaf_id, window, cx);
                        this.close_tabs_popup(cx);
                    }
                    TabListEvent::Dismiss => this.close_tabs_popup(cx),
                }
            });

        window.focus(&popup.read(cx).focus_handle());
        self.tabs_popup = Some(popup);
        self.tabs_popup_leaf = Some(leaf_id);
        self.tabs_popup_pos = Some((click_x, click_y));
        self.tabs_popup_subscription = Some(subscription);
        cx.notify();
    }

    /// Closes the tabs popup, dropping its subscription with it.
    pub fn close_tabs_popup(&mut self, cx: &mut Context<Self>) {
        if self.tabs_popup.take().is_none() {
            self.tabs_popup_leaf = None;
            self.tabs_popup_pos = None;
            return;
        }
        self.tabs_popup_leaf = None;
        self.tabs_popup_pos = None;
        self.tabs_popup_subscription = None;
        cx.notify();
    }

    /// Whether the tabs popup is currently shown.
    pub fn tabs_popup_is_open(&self) -> bool {
        self.tabs_popup.is_some()
    }

    /// Opens the locale picker: the same smart popup, but with locales and
    /// no action header. The parent anchors it to the language button (see
    /// the settings UI tab), so unlike the tabs popup it needs no free-space
    /// math: the list viewport is capped to ~10 rows.
    pub fn open_locale_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::i18n::{LocaleInfo, SUPPORTED_LOCALES};

        let current = LocaleInfo::normalize(&self.config.ui.language);
        let tabs: Vec<TabEntry> = SUPPORTED_LOCALES
            .iter()
            .enumerate()
            .map(|(id, loc)| TabEntry {
                id,
                title: SharedString::from(loc.native_name),
                subtitle: Some(SharedString::from(loc.english_name)),
                icon: icons::ICON_GLOBE,
                font_family: None,
                dot_color: None,
            })
            .collect();
        let active_ix = SUPPORTED_LOCALES
            .iter()
            .position(|loc| loc.code == current)
            .unwrap_or(0);
        let theme = self.theme.clone();

        let popup = cx.new(|cx| {
            TabListPopup::new(
                tabs,
                Some(active_ix),
                theme,
                None,
                Some(px(ROW_HEIGHT * 10.0)),
                Some(50.0),
                cx,
            )
        });
        let subscription = cx.subscribe_in(
            &popup,
            window,
            |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(loc) = SUPPORTED_LOCALES.get(*id) {
                            this.config.ui.language = loc.code.to_string();
                            this.save_settings(cx);
                        }
                        this.close_locale_popup(cx);
                    }
                    // The locale picker has no header actions.
                    TabListEvent::FoldTabs
                    | TabListEvent::BookmarkTabs
                    | TabListEvent::CloseAllTabs => {}
                    TabListEvent::Dismiss => this.close_locale_popup(cx),
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.locale_popup = Some(popup);
        self.locale_popup_subscription = Some(subscription);
        cx.notify();
    }

    /// Closes the locale picker, dropping its subscription with it.
    pub fn close_locale_popup(&mut self, cx: &mut Context<Self>) {
        if self.locale_popup.take().is_none() {
            return;
        }
        self.locale_popup_subscription = None;
        cx.notify();
    }

    /// Whether the locale picker is currently shown.
    pub fn locale_popup_is_open(&self) -> bool {
        self.locale_popup.is_some()
    }

    /// Opens the editor font picker: the same smart popup, each row previewed
    /// in its own font. The list is short, so no height cap is needed.
    pub fn open_font_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.config.editor.font.font_family.clone();
        let tabs: Vec<TabEntry> = crate::fonts::EDITOR_FONT_CHOICES
            .iter()
            .enumerate()
            .map(|(id, name)| TabEntry {
                id,
                title: SharedString::from(*name),
                subtitle: None,
                icon: icons::ICON_TYPE,
                font_family: Some(SharedString::from(*name)),
                dot_color: None,
            })
            .collect();
        let active_ix = crate::fonts::EDITOR_FONT_CHOICES
            .iter()
            .position(|name| *name == current)
            .unwrap_or(0);
        let theme = self.theme.clone();

        let popup = cx.new(|cx| TabListPopup::new(tabs, Some(active_ix), theme, None, None, None, cx));
        let subscription = cx.subscribe_in(
            &popup,
            window,
            |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(name) = crate::fonts::EDITOR_FONT_CHOICES.get(*id) {
                            this.config.editor.font.font_family = name.to_string();
                            this.save_settings(cx);
                        }
                        this.close_font_popup(cx);
                    }
                    // The font picker has no header actions.
                    TabListEvent::FoldTabs
                    | TabListEvent::BookmarkTabs
                    | TabListEvent::CloseAllTabs => {}
                    TabListEvent::Dismiss => this.close_font_popup(cx),
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.font_popup = Some(popup);
        self.font_popup_subscription = Some(subscription);
        cx.notify();
    }

    /// Closes the font picker, dropping its subscription with it.
    pub fn close_font_popup(&mut self, cx: &mut Context<Self>) {
        if self.font_popup.take().is_none() {
            return;
        }
        self.font_popup_subscription = None;
        cx.notify();
    }

    /// Whether the font picker is currently shown.
    pub fn font_popup_is_open(&self) -> bool {
        self.font_popup.is_some()
    }

    /// Opens the plugin select picker: the same smart popup, anchored to the select button.
    pub fn open_plugin_select(
        &mut self,
        plugin_id: String,
        view_id: u64,
        node_id: String,
        options: Vec<crate::plugins::ui::SelectOption>,
        current_value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_ix = options.iter().position(|o| o.value == current_value).unwrap_or(0);
        let tabs: Vec<TabEntry> = options
            .iter()
            .enumerate()
            .map(|(id, opt)| TabEntry {
                id,
                title: SharedString::from(opt.label.clone()),
                subtitle: None,
                icon: icons::ICON_CHEVRON_RIGHT,
                font_family: None,
                dot_color: None,
            })
            .collect();

        let theme = self.theme.clone();
        let max_rows = options.len().clamp(1, 10) as f32;
        let popup = cx.new(|cx| {
            TabListPopup::new(
                tabs,
                Some(active_ix),
                theme,
                None,
                Some(px(ROW_HEIGHT * max_rows)),
                Some(50.0),
                cx,
            )
        });

        let p_id_activate = plugin_id.clone();
        let n_id_activate = node_id.clone();
        let opts_activate = options.clone();

        let subscription = cx.subscribe_in(
            &popup,
            window,
            move |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(opt) = opts_activate.get(*id) {
                            let chosen_val = opt.value.clone();
                            if let Some(tree) = this.plugin_views.get_mut(&(p_id_activate.clone(), view_id)) {
                                tree.update_input_value(&n_id_activate, &chosen_val);
                            }
                            let _ = this.state.plugin_host.send_event(
                                &p_id_activate,
                                crate::plugins::api::HostEvent::UiEvent {
                                    view_id,
                                    node_id: n_id_activate.clone(),
                                    event: "change".to_string(),
                                    value: Some(chosen_val),
                                },
                            );
                        }
                        this.close_plugin_select(cx);
                    }
                    TabListEvent::Dismiss => this.close_plugin_select(cx),
                    _ => {}
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.active_plugin_select = Some((plugin_id, view_id, node_id));
        self.plugin_select_popup = Some(popup);
        self.plugin_select_subscription = Some(subscription);
        cx.notify();
    }

    /// Closes the plugin select picker, dropping its subscription.
    pub fn close_plugin_select(&mut self, cx: &mut Context<Self>) {
        if self.plugin_select_popup.take().is_none() && self.active_plugin_select.take().is_none() {
            return;
        }
        self.active_plugin_select = None;
        self.plugin_select_subscription = None;
        cx.notify();
    }

    /// Whether the plugin select picker is currently shown for this node.
    pub fn is_plugin_select_open(&self, plugin_id: &str, view_id: u64, node_id: &str) -> bool {
        self.active_plugin_select.as_ref() == Some(&(plugin_id.to_string(), view_id, node_id.to_string()))
            && self.plugin_select_popup.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(left: Pixels, right: Pixels) -> bool {
        (left - right).abs() < px(0.01)
    }

    // The tests below use list_top = 100 and list_height = 300, so the dead zone is
    // clamp(300 * 0.10, 28, 56) = 30 px: the proportional band is 130..370.

    #[test]
    fn hover_row_resolves_to_a_single_row() {
        let at = |y: f32| hover_row_at(y, 100.0, 280.0, 0.0, ROW_HEIGHT, 10);
        assert_eq!(at(100.0), Some(0));
        assert_eq!(at(100.0 + 3.0 * ROW_HEIGHT + 1.0), Some(3));
        // Exactly on a seam: floor wins deterministically, never both rows.
        assert_eq!(at(100.0 + 2.0 * ROW_HEIGHT), Some(2));
        // Exactly on the bottom edge there is no row anymore (past the last one).
        assert_eq!(at(100.0 + 280.0), None);
        // ...but a hair above still hits the last row.
        assert_eq!(at(100.0 + 279.9), Some(9));
    }

    #[test]
    fn hover_row_follows_a_scrolled_offset() {
        // Viewport scrolled by 56 px (two rows): content [56, 336).
        let at = |y: f32| hover_row_at(y, 100.0, 280.0, -56.0, ROW_HEIGHT, 10);
        assert_eq!(at(100.0), Some(2));
        // Bottom edge shows content 336: clamps into the list.
        // Bottom edge shows content past the list: nothing hovered.
        assert_eq!(at(100.0 + 280.0), None);
    }

    #[test]
    fn hover_row_is_none_outside_the_list() {
        let at = |y: f32| hover_row_at(y, 100.0, 280.0, 0.0, ROW_HEIGHT, 10);
        assert_eq!(at(99.9), None);
        assert_eq!(at(100.0 + 280.0 + 0.1), None);
        assert_eq!(hover_row_at(150.0, 100.0, 280.0, 0.0, ROW_HEIGHT, 0), None);
        assert_eq!(hover_row_at(150.0, 100.0, 0.0, 0.0, ROW_HEIGHT, 10), None);
    }

    #[test]
    fn hover_header_maps_rows_and_skips_dividers() {
        assert_eq!(hover_header_at(-0.1), None);
        assert_eq!(hover_header_at(0.0), Some(0));
        assert_eq!(hover_header_at(27.9), Some(0));
        assert_eq!(hover_header_at(28.0), Some(1));
        // First divider: no row.
        assert_eq!(hover_header_at(56.0), None);
        assert_eq!(hover_header_at(64.9), None);
        assert_eq!(hover_header_at(65.0), Some(2));
        assert_eq!(hover_header_at(92.9), Some(2));
        // Second divider and below: nothing.
        assert_eq!(hover_header_at(93.0), None);
        assert_eq!(hover_header_at(101.9), None);
        assert_eq!(hover_header_at(HEADER_H), None);
        assert_eq!(hover_header_at(HEADER_H + 10.0), None);
    }

    #[test]
    fn edge_pad_is_a_fraction_of_the_height_within_bounds() {
        assert!(approx(edge_pad(px(300.0)), px(30.0)));
        // 10% of 1000 is 100, capped at EDGE_PAD_MAX.
        assert!(approx(edge_pad(px(1000.0)), px(EDGE_PAD_MAX)));
        // 10% of 150 is 15, raised to one row...
        assert!(approx(edge_pad(px(150.0)), px(ROW_HEIGHT)));
        // ...but never more than a third of a very short list.
        assert!(approx(edge_pad(px(60.0)), px(20.0)));
    }

    #[test]
    fn cursor_at_the_top_of_the_band_shows_the_beginning() {
        let target = cursor_target_offset(px(130.0), px(100.0), px(300.0), px(1000.0));
        assert!(approx(target, px(0.0)));
    }

    #[test]
    fn cursor_at_the_bottom_of_the_band_shows_the_end() {
        let target = cursor_target_offset(px(370.0), px(100.0), px(300.0), px(1000.0));
        assert!(approx(target, px(-1000.0)));
    }

    #[test]
    fn the_last_row_is_fully_visible_one_row_before_the_edge() {
        // One row above the bottom border (400 - 28 = 372) is already the very end.
        let target =
            cursor_target_offset(px(400.0 - ROW_HEIGHT), px(100.0), px(300.0), px(1000.0));
        assert!(approx(target, px(-1000.0)));
    }

    #[test]
    fn the_first_row_is_fully_visible_one_row_after_the_edge() {
        let target =
            cursor_target_offset(px(100.0 + ROW_HEIGHT), px(100.0), px(300.0), px(1000.0));
        assert!(approx(target, px(0.0)));
    }

    #[test]
    fn cursor_in_the_middle_lands_in_the_middle() {
        let target = cursor_target_offset(px(250.0), px(100.0), px(300.0), px(1000.0));
        assert!(approx(target, px(-500.0)));
    }

    #[test]
    fn cursor_outside_the_list_is_clamped() {
        let top = cursor_target_offset(px(0.0), px(100.0), px(300.0), px(1000.0));
        assert!(approx(top, px(0.0)));
        let bottom = cursor_target_offset(px(900.0), px(100.0), px(300.0), px(1000.0));
        assert!(approx(bottom, px(-1000.0)));
    }

    #[test]
    fn a_longer_list_moves_faster_for_the_same_cursor_travel() {
        // Same cursor travel and same list height, but a longer scrollable range:
        // the offset delta grows with max_scroll, which is the whole point of
        // deriving the speed from max_scroll / list_height.
        let from = px(150.0);
        let to = px(200.0);
        let list_top = px(100.0);
        let list_height = px(300.0);

        let few = cursor_target_offset(to, list_top, list_height, px(200.0))
            - cursor_target_offset(from, list_top, list_height, px(200.0));
        let many = cursor_target_offset(to, list_top, list_height, px(2000.0))
            - cursor_target_offset(from, list_top, list_height, px(2000.0));

        assert!(many.abs() > few.abs() * 5.0);
    }

    #[test]
    fn a_visible_row_is_not_moved() {
        // Viewport 0..300, row 200..228 is fully visible.
        let target = nearest_offset(px(200.0), px(28.0), px(0.0), px(300.0), px(0.0), px(1000.0));
        assert!(approx(target, px(0.0)));
    }

    #[test]
    fn a_row_above_the_viewport_scrolls_up_by_the_minimum() {
        // Viewport 500..800, row 200..228.
        let target =
            nearest_offset(px(200.0), px(28.0), px(500.0), px(300.0), px(-500.0), px(1000.0));
        assert!(approx(target, px(-200.0)));
    }

    #[test]
    fn a_row_below_the_viewport_scrolls_down_by_the_minimum() {
        // Viewport 0..300, row 400..428.
        let target = nearest_offset(px(400.0), px(28.0), px(0.0), px(300.0), px(0.0), px(1000.0));
        assert!(approx(target, px(-128.0)));
    }

    #[test]
    fn nearest_is_clamped_to_the_scrollable_range() {
        // The minimum movement would be -128, but only 100 px are scrollable.
        let target = nearest_offset(px(400.0), px(28.0), px(0.0), px(300.0), px(0.0), px(100.0));
        assert!(approx(target, px(-100.0)));
    }

    #[test]
    fn smoothing_alpha_converges_equally_at_any_fps() {
        // One second of motion towards -1000 lands on the target at both rates.
        let run = |frames: u32| {
            let mut pos = 0.0_f32;
            let target = -1000.0;
            for _ in 0..frames {
                pos += (target - pos) * smoothing_alpha(1.0 / frames as f32, 50.0);
            }
            pos
        };
        assert!((run(60) - run(144)).abs() < 1.0);
        assert!((run(60) + 1000.0).abs() < 2.0);
        // Zero tau means "already there".
        assert_eq!(smoothing_alpha(1.0 / 60.0, 0.0), 1.0);
    }

    #[test]
    fn snapping_lands_on_device_pixels() {
        // 150%: a device pixel is 1/1.5 px. 10.3 * 1.5 = 15.45 -> 15 -> 10.0.
        assert!(approx(snap_to_device_pixel(px(10.3), 1.5), px(10.0)));
        assert!(approx(snap_to_device_pixel(px(10.4), 1.0), px(10.0)));
        assert!(approx(snap_to_device_pixel(px(-13.47), 2.0), px(-13.5)));
    }

    #[test]
    fn panel_chrome_matches_padding_and_border() {
        // Viewport = panel height - padding (top + bottom) - border (top + bottom).
        assert!(approx(px(PANEL_CHROME), px(10.0)));
    }

    #[test]
    fn test_custom_row_height_hover_geometry() {
        let at = |y: f32| hover_row_at(y, 100.0, 420.0, 0.0, 42.0, 10);
        assert_eq!(at(100.0 + 3.0 * 42.0 + 1.0), Some(3));
        assert_eq!(at(100.0 + 2.0 * 42.0), Some(2));
        assert_eq!(at(99.0), None);
        assert_eq!(at(521.0), None);
    }

    #[test]
    fn test_tab_search_filter_matching() {
        let entries = vec![
            TabEntry {
                id: 0,
                title: SharedString::from("Task list"),
                subtitle: Some(SharedString::from("editor:list-task")),
                icon: "",
                font_family: None,
                dot_color: None,
            },
            TabEntry {
                id: 1,
                title: SharedString::from("Toggle preview"),
                subtitle: Some(SharedString::from("markdown:preview")),
                icon: "",
                font_family: None,
                dot_color: None,
            },
        ];

        // Match by title
        let q = "task";
        let matches: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(ix, e)| {
                let title_match = e.title.to_lowercase().contains(q);
                let sub_match = e.subtitle.as_ref().map(|s| s.to_lowercase().contains(q)).unwrap_or(false);
                if title_match || sub_match { Some(ix) } else { None }
            })
            .collect();
        assert_eq!(matches, vec![0]);

        // Match by subtitle
        let q2 = "markdown";
        let matches2: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(ix, e)| {
                let title_match = e.title.to_lowercase().contains(q2);
                let sub_match = e.subtitle.as_ref().map(|s| s.to_lowercase().contains(q2)).unwrap_or(false);
                if title_match || sub_match { Some(ix) } else { None }
            })
            .collect();
        assert_eq!(matches2, vec![1]);

        // No matches
        let q3 = "nonexistent";
        let matches3: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(ix, e)| {
                let title_match = e.title.to_lowercase().contains(q3);
                let sub_match = e.subtitle.as_ref().map(|s| s.to_lowercase().contains(q3)).unwrap_or(false);
                if title_match || sub_match { Some(ix) } else { None }
            })
            .collect();
        assert!(matches3.is_empty());
    }

    #[test]
    fn test_search_word_range_at() {
        let text = "hello_world 123 foo.bar";
        // Selecting "hello_world"
        assert_eq!(search_word_range_at(text, 0), (0, 11));
        assert_eq!(search_word_range_at(text, 5), (0, 11));
        assert_eq!(search_word_range_at(text, 10), (0, 11));

        // Selecting space at 11
        assert_eq!(search_word_range_at(text, 11), (11, 12));

        // Selecting "123"
        assert_eq!(search_word_range_at(text, 12), (12, 15));
        assert_eq!(search_word_range_at(text, 14), (12, 15));

        // Empty text
        assert_eq!(search_word_range_at("", 0), (0, 0));
    }
}