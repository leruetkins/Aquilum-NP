use gpui::{px, Pixels, Rgba, SharedString};

/// Height of a single row (px). The keyboard navigation relies on it.
pub const ROW_HEIGHT: f32 = 28.0;

/// Dead zone at the top and bottom edge, as a fraction of the list height...
pub(crate) const EDGE_PAD_FRACTION: f32 = 0.10;
/// ...but never smaller than one row, so the last row ends up fully under the
/// cursor before it reaches the border...
pub(crate) const EDGE_PAD_MIN: f32 = ROW_HEIGHT;
/// ...and never larger than this on very tall lists.
pub(crate) const EDGE_PAD_MAX: f32 = 56.0;

/// Number of action rows in the popup header (fold, bookmark, close all).
pub(crate) const HEADER_ROWS: usize = 3;
/// Number of dividers in the popup header.
pub(crate) const HEADER_DIVIDERS: usize = 2;
/// Height of one header divider (px): 1px line plus `my_1` (4px each side).
pub(crate) const DIVIDER_H: f32 = 9.0;
/// Total height of the popup header (px): action rows plus dividers.
pub(crate) const HEADER_H: f32 = ROW_HEIGHT * HEADER_ROWS as f32 + DIVIDER_H * HEADER_DIVIDERS as f32;

/// Mouse movements shorter than this (px) are ignored to suppress jitter.
pub(crate) const MIN_MOVE: f32 = 0.5;
/// Upper bound for a single frame step (s): keeps a long stall from producing a jump.
pub(crate) const MAX_FRAME_DT: f32 = 0.05;
/// A scroll range below this (px) counts as "everything fits": no cursor scrolling.
pub(crate) const MIN_SCROLL_RANGE: f32 = 1.0;

/// Panel width (px).
pub(crate) const LIST_WIDTH: f32 = 280.0;
/// Vertical padding of the panel (px). Together with the border it is subtracted
/// from the panel height to get the list viewport, so the height must include it.
pub(crate) const PANEL_PAD_Y: f32 = 4.0;
/// Border width of the panel (px).
pub(crate) const PANEL_BORDER: f32 = 1.0;
/// Total vertical space taken by the panel padding and border.
pub(crate) const PANEL_CHROME: f32 = PANEL_PAD_Y * 2.0 + PANEL_BORDER * 2.0;

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
pub(crate) const ICON_SIZE: f32 = 14.0;

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
    /// Trailing icon rendered to the right of title/subtitle (e.g. internal command vs plugin icon).
    pub trailing_icon: Option<&'static str>,
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
pub(crate) fn edge_pad(list_height: Pixels) -> Pixels {
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
pub(crate) fn cursor_target_offset(
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
pub(crate) fn hover_row_at(
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
pub(crate) fn hover_header_at(rel_y: f32) -> Option<usize> {
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
pub(crate) fn smoothing_alpha(dt_secs: f32, tau_ms: f32) -> f32 {
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
pub(crate) fn nearest_offset(
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
pub(crate) fn snap_to_device_pixel(value: Pixels, scale: f32) -> Pixels {
    (value * scale).round() / scale
}
