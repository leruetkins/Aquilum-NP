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
//! The offset does not jump to the cursor: it smoothly chases the target derived
//! from the cursor position. Smoothing is time based (`dt` of the real frame, not
//! a hardcoded 1/60), so 60/120/144 Hz monitors look identical. The offset is
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
    canvas, div, prelude::*, px, uniform_list, Bounds, Context, DispatchPhase, EventEmitter,
    FocusHandle, KeyDownEvent, MouseMoveEvent, Pixels, ScrollHandle, ScrollWheelEvent,
    SharedString, UniformListScrollHandle, Window,
};

use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::tabs_menu::format_bookmark_label;

/// Height of a single row (px). The keyboard navigation relies on it.
const ROW_HEIGHT: f32 = 28.0;

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
/// Natural frequency of the scroll spring (rad/s). Lower is smoother and
/// "heavier": 12 feels like the old 140 ms exponential chase, 8-10 is
/// noticeably softer.
const SPRING_OMEGA: f32 = 10.0;
/// Stiffer spring once the mouse stops: the list lands fast instead of
/// trailing behind with a tail. 0 would snap onto the target instantly,
/// 25-40 is almost tailless but without a harsh snap.
const SETTLE_OMEGA: f32 = 70.0;
/// Pause without mouse motion (ms) after which the cursor counts as idle.
const IDLE_AFTER_MS: u128 = 50;
/// The list counts as settled when the spring velocity drops below this (px/s).
const REST_VELOCITY: f32 = 8.0;
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
/// popup. See `titlebar-right-controls-group` in `app_view.rs`.
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

/// Exact step of a critically damped spring over `dt` seconds.
/// Returns the new (position, velocity). Closed-form, so it does not depend
/// on the frame rate.
fn spring_step(pos: f32, vel: f32, target: f32, dt: f32, omega: f32) -> (f32, f32) {
    let d = pos - target;
    let c = vel + omega * d;
    let decay = (-omega * dt).exp();
    (
        target + (d + c * dt) * decay,
        (vel - omega * c * dt) * decay,
    )
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
    active_ix: usize,
    /// Action header above the list; `None` hides it (e.g. locale picker).
    header: Option<TabListHeader>,
    /// Extra cap on the list viewport height; `None` lets it fill free space.
    max_list_h: Option<Pixels>,
    scroll: UniformListScrollHandle,
    /// List viewport in window coordinates; filled in prepaint through a canvas.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Wanted offset (<= 0). `None` until the mouse has moved or the keyboard acted.
    target_y: Option<Pixels>,
    /// Last observed cursor position; the very first event only primes it.
    last_mouse_y: Option<Pixels>,
    /// Timestamp of the previous animation frame (for `dt`).
    last_frame: Option<Instant>,
    /// Smoothed position in px (fractional, pre-snapping).
    pos_y: Option<f32>,
    /// Spring velocity, px/s.
    velocity_y: f32,
    /// When the mouse last moved noticeably; staleness means "idle".
    last_move: Option<Instant>,
    focus_handle: FocusHandle,
}

impl TabListPopup {
    /// Builds the popup. The caller has to focus [`TabListPopup::focus_handle`]
    /// after the entity is created, otherwise the keyboard will not work.
    pub fn new(
        tabs: Vec<TabEntry>,
        active_ix: usize,
        theme: Theme,
        header: Option<TabListHeader>,
        max_list_h: Option<Pixels>,
        cx: &mut Context<Self>,
    ) -> Self {
        let scroll = UniformListScrollHandle::new();
        let active_ix = if tabs.is_empty() {
            0
        } else {
            active_ix.min(tabs.len() - 1)
        };
        // Show the active tab right away; the first mouse move must not make the list jump.
        // scroll.scroll_to_item(active_ix, ScrollStrategy::Center);
        Self {
            theme,
            tabs,
            active_ix,
            header,
            max_list_h,
            scroll,
            bounds: Rc::new(Cell::new(Bounds::default())),
            target_y: None,
            last_mouse_y: None,
            last_frame: None,
            pos_y: None,
            velocity_y: 0.0,
            last_move: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Focus handle the parent should focus when the popup opens.
    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    /// Index of the currently highlighted tab.
    pub fn active_ix(&self) -> usize {
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
    match self.last_mouse_y {
        // The first event only primes the reference point: the cursor still sits over the
        // chevron that opened the panel, so acting on it would make the list jump.
        None => self.last_mouse_y = Some(y),
        // The distance is measured from the last *accepted* position, so a slow move
        // accumulates instead of being dropped event by event.
        Some(anchor) if (y - anchor).abs() >= px(MIN_MOVE) => {
            self.last_mouse_y = Some(y);
            self.last_move = Some(Instant::now());

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
        let count = self.tabs.len();
        if count == 0 {
            return;
        }
        self.active_ix = (self.active_ix as isize + delta).rem_euclid(count as isize) as usize;
        // The keyboard wins over the mouse: drop its target before recomputing.
        self.target_y = None;
        self.scroll_selection_into_view();
        cx.notify();
    }

    /// Picks the offset that shows the highlighted row with the least movement,
    /// then lets the same spring animation carry it there.
    fn scroll_selection_into_view(&mut self) {
        let base = self.base_handle();
        let max_scroll = base.max_offset().height;
        let viewport = base.bounds().size.height;
        // `max_offset()` is 0 until the first layout: never clamp against it there.
        if max_scroll < px(MIN_SCROLL_RANGE) || viewport <= px(0.0) {
            return;
        }

        let offset = base.offset().y;
        self.target_y = Some(nearest_offset(
            px(ROW_HEIGHT) * self.active_ix as f32,
            px(ROW_HEIGHT),
            -offset,
            viewport,
            offset,
            max_scroll,
        ));
        // Keyboard motion counts as fresh: the glide stays soft instead of
        // being mistaken for a stale mouse.
        self.reset_motion();
        self.last_move = Some(Instant::now());
    }

    /// Drops the spring state: the next animation starts from the currently
    /// shown offset with zero velocity, so a sharp direction change never yanks.
    fn reset_motion(&mut self) {
        self.last_frame = None;
        self.pos_y = None;
        self.velocity_y = 0.0;
        self.last_move = None;
    }

    /// One spring step. Returns `true` when another frame is needed.
    ///
    /// `scale` is the window scale factor: the shown offset is kept on whole
    /// device pixels (see [`snap_to_device_pixel`]).
    fn animate(&mut self, scale: f32) -> bool {
        let Some(target) = self.target_y else {
            self.reset_motion();
            return false;
        };

        // `dt` comes from the real frame time, so the smoothness does not depend
        // on the refresh rate. The first frame after a pause counts as 1/60 s.
        let now = Instant::now();
        let dt = self.last_frame.map_or(1.0 / 60.0, |previous| {
            now.duration_since(previous).as_secs_f32().min(MAX_FRAME_DT)
        });
        self.last_frame = Some(now);

        // Stale mouse → stiff spring: land fast instead of trailing a tail.
        // Fresh motion (mouse or keyboard) keeps the soft spring.
        let idle = self.last_move.map_or(true, |t| {
            now.duration_since(t).as_millis() >= IDLE_AFTER_MS
        });
        let omega = if idle { SETTLE_OMEGA } else { SPRING_OMEGA };

        let base = self.base_handle();
        let max = f32::from(base.max_offset().height);
        let target = f32::from(target);
        let mut offset = base.offset();
        // The fractional position lives apart from what is shown: after rounding
        // to pixels the accumulated precision would be lost on every frame.
        let pos = self.pos_y.unwrap_or_else(|| f32::from(offset.y));

        let (mut pos, mut vel) = spring_step(pos, self.velocity_y, target, dt, omega);

        // The spring can slightly overshoot on a sharp direction change:
        // it must never leave the list.
        let clamped = pos.clamp(-max, 0.0);
        if clamped != pos {
            vel = 0.0;
            pos = clamped;
        }
        self.pos_y = Some(pos);
        self.velocity_y = vel;

        let resting = (pos - target).abs() < 0.5 / scale && vel.abs() < REST_VELOCITY;
        let shown = if resting { target } else { pos };
        offset.y = snap_to_device_pixel(px(shown), scale);
        base.set_offset(offset);

        if resting {
            self.reset_motion();
            return false;
        }
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
            ROW_HEIGHT,
            self.tabs.len(),
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
        let is_active = ix == self.active_ix;

        let bg_hover = self.theme.bg_hover;
        let text_primary = self.theme.text_primary;
        let text_secondary = self.theme.text_secondary;
        let text_muted = self.theme.text_muted;
        let accent = self.theme.accent;
        let icon_color = if is_active { accent } else { text_secondary };

        div()
            .id(("tab-list-row", ix))
            .flex()
            // `uniform_list` lays every item out with `layout_as_root`, which sizes the item to
            // its content. Without this the row is only as wide as its text, so the hitbox (and
            // with it hover and clicks) covers the title and nothing to the right of it.
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .h(px(ROW_HEIGHT))
            .px_2()
            .rounded_md()
            .cursor_pointer()
            // Manual highlight (see `hover_tab_ix`): stock hover lights every
            // hitbox sharing the cursor point, i.e. both rows on a seam.
            .when(hovered, move |row| row.bg(bg_hover))
            .child(icons::icon(icon, ICON_SIZE, icon_color))
            .child(
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
                        .text_xs()
                        .text_color(text_muted)
                        .child(subtitle),
                )
            })
            // The check mark belongs to the active row; the hover highlight is separate.
            .when(is_active, move |row| {
                row.child(icons::icon(icons::ICON_CHECK, ICON_SIZE, text_secondary))
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
            .gap_2()
            .h(px(ROW_HEIGHT))
            .px_2()
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
}

impl Render for TabListPopup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Keep the animation alive until the offset reaches its target.
        if self.animate(window.scale_factor()) {
            window.request_animation_frame();
        }

        // Almost the whole window height when there are many tabs, content height otherwise.
        // The anchor top is not zero (the panel opens below the titlebar plus a gap), so
        // the free space below the anchor is what bounds the height, otherwise the panel
        // would run out of the window at the bottom.
        let free_height = window.viewport_size().height
            - px(WINDOW_ANCHOR_TOP + POPUP_GAP)
            - px(WINDOW_MARGIN);
        // The panel height is header + list viewport + its padding and border, so that
        // the list viewport is exactly `rows * ROW_HEIGHT` when everything fits and
        // there is nothing to scroll. Without a header the panel is list only.
        let header_h = if self.header.is_some() {
            px(HEADER_H)
        } else {
            px(0.0)
        };
        let chrome = px(PANEL_CHROME);
        let list_h = (px(ROW_HEIGHT) * self.tabs.len() as f32)
            .min((free_height - header_h - chrome).max(px(0.0)));
        let list_h = match self.max_list_h {
            Some(cap) => list_h.min(cap),
            None => list_h,
        };
        let height = header_h + list_h + chrome;

        let bounds = self.bounds.clone();
        let count = self.tabs.len();

        let bg_panel = self.theme.bg_sidebar;
        let border = self.theme.border;

        div()
            .id("tab-list-popup")
            .relative()
            .flex()
            .flex_col()
            // The panel occludes the editor behind it: without its own
            // blocking hitbox, mouse presses pass through to the editor.
            .occlude()
            .w(px(LIST_WIDTH))
            .h(height)
            .py(px(PANEL_PAD_Y))
            // Horizontal panel padding: every row (header and tabs) sits
            // inset from the border, so hover highlights never touch the edge.
            .px_1()
            .bg(bg_panel)
            .border_1()
            .border_color(border)
            .rounded_md()
            .shadow_lg()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                this.on_mouse_move(event.position.y, cx);
            }))
            // Leaving the panel clears the manual highlight (see `hover_tab_ix`):
            // without this the last row would stay lit forever.
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !hovered {
                    this.last_mouse_y = None;
                    cx.notify();
                }
            }))
            .on_mouse_down_out(cx.listener(|_this, _event, _window, cx| {
                cx.emit(TabListEvent::Dismiss);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        cx.stop_propagation();
                        cx.emit(TabListEvent::Dismiss);
                    }
                    "down" => {
                        cx.stop_propagation();
                        this.move_selection(1, cx);
                    }
                    "up" => {
                        cx.stop_propagation();
                        this.move_selection(-1, cx);
                    }
                    "enter" => {
                        cx.stop_propagation();
                        if let Some(tab) = this.tabs.get(this.active_ix) {
                            cx.emit(TabListEvent::Activate(tab.id));
                        }
                    }
                    _ => {}
                }
            }))
            .when_some(self.header.clone(), |d, header| {
                d.child(self.render_header(&header, cx))
            })
            .child(
                // The list viewport. The canvas measures exactly this box, so the
                // cursor mapping is not offset by the header, the panel padding
                // or its border.
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
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
                                    .map(|ix| this.render_row(ix, hover == Some(ix), cx))
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .track_scroll(self.scroll.clone())
                        .size_full(),
                    ),
            )
    }
}

/// Title shown for a tab: the file stem, the localized "New tab" label for
/// empty-tab sentinels, or the "untitled" placeholder as a last resort.
fn tab_title(path: &std::path::Path, new_tab: &str, untitled: &str) -> SharedString {
    SharedString::from(crate::views::new_tab::tab_display_title(path, new_tab, untitled))
}

impl AppView {
    /// Opens the tabs popup for the currently open tabs.
    ///
    /// The popup is positioned by the parent (see `render` in `app_view.rs`); this
    /// only builds the entity, subscribes to its events and gives it the focus.
    pub fn open_tabs_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.open_tabs.is_empty() {
            return;
        }

        let untitled = self.t("common.untitled");
        let new_tab = self.t("tabs.newTab");
        let tabs: Vec<TabEntry> = self
            .state
            .open_tabs
            .iter()
            .enumerate()
            .map(|(id, path)| TabEntry {
                id,
                title: tab_title(path, &new_tab, &untitled),
                subtitle: None,
                icon: icons::ICON_FILE_TEXT,
                font_family: None,
            })
            .collect();
        let active_ix = self
            .state
            .open_tabs
            .iter()
            .position(|path| {
                self.state.active_note.as_ref().map(|note| &note.path) == Some(path)
                    || self.state.active_empty_tab.as_ref() == Some(path)
            })
            .unwrap_or(0);
        let theme = self.theme.clone();
        let header = TabListHeader {
            fold: SharedString::from(self.t("tabs.fold_tabs")),
            bookmark: SharedString::from(format_bookmark_label(
                self.state.open_tabs.len(),
                &self.config.ui.language,
            )),
            close_all: SharedString::from(self.t("tabs.close_all")),
        };

        let popup = cx.new(|cx| {
            TabListPopup::new(tabs, active_ix, theme, Some(header), None, cx)
        });
        // `subscribe_in` is used instead of `subscribe`: activating a tab needs a window.
        let subscription =
            cx.subscribe_in(&popup, window, |this, _popup, event: &TabListEvent, window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(path) = this.state.open_tabs.get(*id).cloned() {
                            this.select_note(path, true, window, cx);
                        }
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
                        this.close_all_tabs(cx);
                        this.close_tabs_popup(cx);
                    }
                    TabListEvent::Dismiss => this.close_tabs_popup(cx),
                }
            });

        window.focus(&popup.read(cx).focus_handle());
        self.tabs_popup = Some(popup);
        self.tabs_popup_subscription = Some(subscription);
        cx.notify();
    }

    /// Closes the tabs popup, dropping its subscription with it.
    pub fn close_tabs_popup(&mut self, cx: &mut Context<Self>) {
        if self.tabs_popup.take().is_none() {
            return;
        }
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
            })
            .collect();
        let active_ix = SUPPORTED_LOCALES
            .iter()
            .position(|loc| loc.code == current)
            .unwrap_or(0);
        let theme = self.theme.clone();

        let popup = cx.new(|cx| {
            TabListPopup::new(tabs, active_ix, theme, None, Some(px(ROW_HEIGHT * 10.0)), cx)
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
            })
            .collect();
        let active_ix = crate::fonts::EDITOR_FONT_CHOICES
            .iter()
            .position(|name| *name == current)
            .unwrap_or(0);
        let theme = self.theme.clone();

        let popup = cx.new(|cx| TabListPopup::new(tabs, active_ix, theme, None, None, cx));
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
    fn spring_progress_does_not_depend_on_the_frame_rate() {
        let run = |frames: u32| {
            let (mut pos, mut vel) = (0.0_f32, 0.0_f32);
            for _ in 0..frames {
                (pos, vel) = spring_step(pos, vel, -1000.0, 1.0 / frames as f32, SPRING_OMEGA);
            }
            pos
        };
        assert!((run(60) - run(144)).abs() < 1.0);
        // After a second the spring is already there.
        assert!((run(60) + 1000.0).abs() < 2.0);
    }

    #[test]
    fn spring_does_not_jump_at_the_start() {
        // The first frame shifts almost nothing: velocity builds up smoothly.
        let (pos, _) = spring_step(0.0, 0.0, -1000.0, 1.0 / 60.0, SPRING_OMEGA);
        assert!(pos.abs() < 20.0);
    }

    #[test]
    fn settle_spring_lands_faster_than_the_soft_one() {
        // Half a second of soft motion, then compare: the stiff spring must
        // already sit on the target while the soft one still travels.
        let run = |omega: f32| {
            let (mut pos, mut vel) = (0.0_f32, 0.0_f32);
            for _ in 0..30 {
                (pos, vel) = spring_step(pos, vel, -1000.0, 1.0 / 60.0, omega);
            }
            pos
        };
        let soft = (run(SPRING_OMEGA) + 1000.0).abs();
        let stiff = (run(SETTLE_OMEGA) + 1000.0).abs();
        assert!(stiff < 1.0, "stiff spring still off by {stiff}");
        assert!(stiff < soft, "stiff {stiff} should beat soft {soft}");
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
}