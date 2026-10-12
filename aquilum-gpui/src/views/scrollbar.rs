//! Reusable overlay scrollbar.
//!
//! GPUI's `overflow_scroll` only *reserves layout space* for a scrollbar (the
//! `scrollbar_width` style property); it never paints one. Every scrollable surface in
//! the app therefore has to draw its own thumb. This module owns both the geometry —
//! thumb size and position, drag math, track-click math — and the element builder, so
//! the sidebar tree and any future list can share one implementation instead of each
//! re-deriving the same numbers.
//!
//! Usage: place the returned element as an `absolute` sibling of the scroll container,
//! inside a `relative` wrapper, so it never scrolls with the content.

use gpui::{
    div, point, prelude::*, px, AnyElement, App, MouseButton, MouseDownEvent, ScrollHandle,
    SharedString, Window,
};

use crate::theme::Theme;

/// Drag state of one overlay scrollbar.
///
/// A single value lives on the view that owns the scroll handle. The anchor is captured
/// on mouse-down so the thumb tracks the pointer exactly for the whole drag instead of
/// accumulating frame-to-frame error.
#[derive(Clone, Copy, Default)]
pub struct ScrollbarDrag {
    pub active: bool,
    pub start_y: f32,
    pub start_scroll: f32,
}

impl ScrollbarDrag {
    /// Starts a drag anchored at `pointer_y` with the content scrolled to `scroll_y`.
    pub fn begin(&mut self, pointer_y: f32, scroll_y: f32) {
        self.active = true;
        self.start_y = pointer_y;
        self.start_scroll = scroll_y;
    }

    /// Ends the drag. Safe to call unconditionally on every mouse-up.
    pub fn end(&mut self) {
        self.active = false;
    }
}

/// Look of an overlay scrollbar. [`Default`] reproduces the editor's scrollbar.
#[derive(Clone, Copy)]
pub struct ScrollbarStyle {
    /// Width of the invisible interaction track.
    pub track_w: f32,
    /// Thumb width at rest.
    pub thumb_w: f32,
    /// Thumb width while hovered or dragged.
    pub thumb_w_active: f32,
    /// Distance from the right edge of the track to the thumb.
    pub thumb_inset: f32,
    /// Smallest thumb height, so the thumb stays grabbable on very long content.
    pub min_thumb_h: f32,
    /// Where the track's right edge sits relative to the wrapper's right edge.
    ///
    /// `0.0` puts it flush with the wrapper. A negative value pushes the scrollbar
    /// outwards, into a parent's padding, which is how a panel whose content is inset by
    /// padding gets a scrollbar that sits on the panel border instead of over its rows.
    pub track_right_offset: f32,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self {
            track_w: 14.0,
            thumb_w: 6.0,
            thumb_w_active: 8.0,
            thumb_inset: 3.0,
            min_thumb_h: 28.0,
            track_right_offset: 0.0,
        }
    }
}

/// Thumb geometry for a single frame, derived from the live scroll handle.
#[derive(Clone, Copy)]
pub struct ScrollbarGeometry {
    /// Top edge of the track in window coordinates, in pixels.
    pub track_top_y: f32,
    /// Visible height of the scrollable area — the track length — in pixels.
    pub track_h: f32,
    /// Largest valid scroll offset (`content - track`) in pixels.
    pub max_offset: f32,
    /// Current scroll offset from the top, in pixels.
    pub scroll_y: f32,
    /// Thumb height, in pixels.
    pub thumb_h: f32,
    /// Thumb top inside the track, in pixels.
    pub thumb_top: f32,
}

impl ScrollbarGeometry {
    /// Computes the thumb geometry for a known viewport/content/offset triple.
    ///
    /// Returns `None` when there is nothing to scroll, so callers can skip the
    /// scrollbar with a plain `if let`.
    pub fn new(
        track_top_y: f32,
        track_h: f32,
        max_offset: f32,
        scroll_y: f32,
        style: &ScrollbarStyle,
    ) -> Option<Self> {
        if track_h <= 1.0 || max_offset <= 1.0 {
            return None;
        }

        let scroll_y = scroll_y.clamp(0.0, max_offset);
        let ratio = (track_h / (track_h + max_offset)).clamp(0.04, 1.0);
        let thumb_h = (track_h * ratio).max(style.min_thumb_h);
        let thumb_top = (scroll_y / max_offset).clamp(0.0, 1.0) * (track_h - thumb_h);

        Some(Self {
            track_top_y,
            track_h,
            max_offset,
            scroll_y,
            thumb_h,
            thumb_top,
        })
    }

    /// Reads the geometry straight off a [`ScrollHandle`].
    pub fn from_handle(handle: &ScrollHandle, style: &ScrollbarStyle) -> Option<Self> {
        Self::new(
            f32::from(handle.bounds().origin.y),
            f32::from(handle.bounds().size.height),
            f32::from(handle.max_offset().height),
            -f32::from(handle.offset().y),
            style,
        )
    }

    /// Scroll offset reached by dragging the thumb to `pointer_y`.
    pub fn offset_after_drag(&self, drag: &ScrollbarDrag, pointer_y: f32) -> f32 {
        let track_scrollable = (self.track_h - self.thumb_h).max(1.0);
        let delta_fraction = (pointer_y - drag.start_y) / track_scrollable;
        (drag.start_scroll + delta_fraction * self.max_offset).clamp(0.0, self.max_offset)
    }

    /// Scroll offset that centres the thumb on a click at `pointer_y` (window coords).
    pub fn offset_for_track_click(&self, pointer_y: f32) -> f32 {
        let click_y = pointer_y - self.track_top_y;
        let track_scrollable = (self.track_h - self.thumb_h).max(1.0);
        let target = (click_y - self.thumb_h / 2.0).clamp(0.0, track_scrollable);
        (target / track_scrollable) * self.max_offset
    }

    /// Applies an in-progress drag to `handle`. Returns `true` when the offset moved.
    pub fn apply_drag(&self, handle: &ScrollHandle, drag: &ScrollbarDrag, pointer_y: f32) -> bool {
        if !drag.active {
            return false;
        }
        let scroll_y = self.offset_after_drag(drag, pointer_y);
        handle.set_offset(point(px(0.0), px(-scroll_y)));
        true
    }
}

/// Builds the overlay scrollbar element.
///
/// Mouse handlers are forwarded verbatim, so the caller keeps control of the semantics
/// (stopping propagation, deciding which drag state to mutate, notifying). The thumb's
/// element id is derived from `track_id`, so distinct tracks never collide.
///
/// The thumb is deliberately flat: one gray in every state, with no hover tint and no
/// accent while dragging. A scrollbar that changes colour under the pointer draws attention
/// to itself. The width step is the only feedback left — and a caller that wants none at all
/// simply passes `thumb_w == thumb_w_active`.
pub fn overlay_scrollbar(
    track_id: &'static str,
    geometry: &ScrollbarGeometry,
    style: &ScrollbarStyle,
    theme: &Theme,
    dragging: bool,
    on_track_mouse_down: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    on_thumb_mouse_down: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let thumb_id = SharedString::from(format!("{track_id}-thumb"));
    let thumb_w = if dragging { style.thumb_w_active } else { style.thumb_w };
    let thumb_bg = theme.border;
    let thumb_hover_w = style.thumb_w_active;

    // The track paints nothing, not even on hover: a hover background reads as a stray
    // strip across the rows.
    div()
        .id(track_id)
        .absolute()
        .top_0()
        .bottom_0()
        .right(px(style.track_right_offset))
        .w(px(style.track_w))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, on_track_mouse_down)
        .child(
            div()
                .id(thumb_id)
                .absolute()
                .top(px(geometry.thumb_top))
                .right(px(style.thumb_inset))
                .w(px(thumb_w))
                .h(px(geometry.thumb_h))
                .rounded_full()
                .bg(thumb_bg)
                .hover(move |h| h.w(px(thumb_hover_w)))
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, on_thumb_mouse_down),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn no_scrollbar_when_nothing_scrolls() {
        let style = ScrollbarStyle::default();
        // Content fits the viewport.
        assert!(ScrollbarGeometry::new(0.0, 600.0, 0.0, 0.0, &style).is_none());
        // Not laid out yet.
        assert!(ScrollbarGeometry::new(0.0, 0.0, 500.0, 0.0, &style).is_none());
    }

    #[test]
    fn thumb_is_proportional_and_never_shorter_than_the_minimum() {
        let style = ScrollbarStyle::default();

        // 1000px of content in a 400px viewport -> the thumb is 40% of the track.
        let g = ScrollbarGeometry::new(0.0, 400.0, 600.0, 0.0, &style).unwrap();
        assert!(approx(g.thumb_h, 160.0), "thumb_h was {}", g.thumb_h);
        assert!(approx(g.thumb_top, 0.0), "thumb_top was {}", g.thumb_top);

        // Extremely long content clamps to the minimum thumb height.
        let g = ScrollbarGeometry::new(0.0, 400.0, 100_000.0, 0.0, &style).unwrap();
        assert!(approx(g.thumb_h, style.min_thumb_h), "thumb_h was {}", g.thumb_h);
    }

    #[test]
    fn thumb_reaches_the_bottom_at_max_offset() {
        let style = ScrollbarStyle::default();
        let g = ScrollbarGeometry::new(0.0, 400.0, 600.0, 600.0, &style).unwrap();
        assert!(approx(g.thumb_top, 400.0 - g.thumb_h), "thumb_top was {}", g.thumb_top);
    }

    #[test]
    fn offset_is_clamped_to_the_scrollable_range() {
        let style = ScrollbarStyle::default();
        let g = ScrollbarGeometry::new(0.0, 400.0, 600.0, 9_999.0, &style).unwrap();
        assert!(approx(g.scroll_y, 600.0), "scroll_y was {}", g.scroll_y);
    }

    #[test]
    fn drag_maps_pointer_travel_onto_the_full_scroll_range() {
        let style = ScrollbarStyle::default();
        let g = ScrollbarGeometry::new(0.0, 400.0, 600.0, 0.0, &style).unwrap();
        let drag = ScrollbarDrag {
            active: true,
            start_y: 100.0,
            start_scroll: 0.0,
        };

        // No pointer travel -> no scroll.
        assert!(approx(g.offset_after_drag(&drag, 100.0), 0.0));
        // Dragging the thumb to the bottom of the track scrolls to the end.
        let travel = 400.0 - g.thumb_h;
        assert!(approx(g.offset_after_drag(&drag, 100.0 + travel), 600.0));
        // Overshooting the track clamps instead of flying past the end.
        assert!(approx(g.offset_after_drag(&drag, 10_000.0), 600.0));
    }

    #[test]
    fn track_click_centres_the_thumb_on_the_pointer() {
        let style = ScrollbarStyle::default();
        // Track starts 50px down the window.
        let g = ScrollbarGeometry::new(50.0, 400.0, 600.0, 0.0, &style).unwrap();

        assert!(approx(g.offset_for_track_click(50.0), 0.0));
        assert!(approx(g.offset_for_track_click(250.0), 300.0));
        assert!(approx(g.offset_for_track_click(450.0), 600.0));
    }
}
