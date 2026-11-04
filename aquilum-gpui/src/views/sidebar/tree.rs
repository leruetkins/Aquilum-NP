//! Virtualized file-tree component of the sidebar.
//!
//! A vault can hold thousands of notes, so the tree is never rendered as a tree of
//! elements. Once per frame [`AppView::render_sidebar_tree`] flattens it into a list of
//! [`SidebarRow`] values (respecting collapsed folders and the active search filter),
//! wraps that list in a [`VisualRowHeightIndex`], and lays out only the rows that
//! intersect the viewport — bracketed by two spacers that keep the scrollable content as
//! tall as the whole list, so the scrollbar stays proportional.

use std::path::{Path, PathBuf};

use gpui::{
    canvas, div, point, prelude::*, px, rgba, Animation, AnimationExt, AnyElement, Context,
    DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, ScrollWheelEvent,
    SharedString, Window,
};

use crate::icons;
use crate::markdown::VisualRowHeightIndex;
use crate::state::FileNode;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::scrollbar::{overlay_scrollbar, ScrollbarGeometry, ScrollbarStyle};

/// Fixed height of one sidebar tree row, in pixels.
///
/// The file tree is virtualized: the whole tree is flattened into a list of rows once per
/// frame and only the rows intersecting the viewport are laid out. A fixed row height
/// makes the geometry a plain stride — row `i` occupies
/// `[i * SIDEBAR_ROW_H, (i + 1) * SIDEBAR_ROW_H)` — and keeps the two spacer heights
/// exact, so the scrollbar still reflects the full list.
///
/// The value reproduces the row stride used before virtualization: the label line box
/// (`text_xs` = 12px at gpui's default `phi()` line height -> 19px) plus 8px of vertical
/// padding (`py_1`), plus the 2px the container gap used to add. Text rhythm and total
/// content height are therefore unchanged, while the explicit height makes the geometry
/// exact instead of relying on text metrics.
const SIDEBAR_ROW_H: f32 = 29.0;

/// Pixels of rows rendered above and below the viewport, so fast scrolling does not
/// expose blank edges before the next frame catches up.
const SIDEBAR_OVERSCAN_PX: f32 = 300.0;

/// Horizontal padding of the sidebar column (`p_2`), in pixels.
///
/// The tree viewport ends this far short of the panel border, which would leave the
/// scrollbar floating over the rows. Pushing it out by the same amount parks it in that
/// padding gutter instead, clear of the row backgrounds.
const SIDEBAR_PADDING: f32 = 8.0;

/// Gap the rows leave between their background and the scrollbar lane, in pixels.
///
/// A row's background stretches to the edge of the content box, which is exactly where the
/// scrollbar lane begins — so a highlighted row would run straight into the thumb. The rows
/// give up this much of their right edge to open the gap. Only the background moves: the label
/// keeps its own `pr_2` padding and does not shift.
pub(crate) const SIDEBAR_ROW_SCROLLBAR_GAP: f32 = 4.0;

/// Visible width of the sidebar thumb, in pixels.
const SIDEBAR_THUMB_W: f32 = 6.0;

/// Gap between the thumb and the panel border, in pixels.
///
/// Without it the thumb sits flush on the divider and reads as part of the border instead of
/// as a control. The inset leaves a sliver of panel background on the border side, while
/// [`SIDEBAR_ROW_SCROLLBAR_GAP`] does the same on the row side.
const SIDEBAR_SCROLLBAR_BORDER_GAP: f32 = 2.0;

/// Scrollbar look for the sidebar tree.
///
/// The lane is the whole padding gutter ([`SIDEBAR_PADDING`]), but the thumb is narrower than
/// the lane and pushed left, so it clears the panel border by
/// [`SIDEBAR_SCROLLBAR_BORDER_GAP`] and the row highlight by [`SIDEBAR_ROW_SCROLLBAR_GAP`].
/// `thumb_w == thumb_w_active` keeps the width constant — the bar never changes size under the
/// pointer.
pub(crate) fn sidebar_scrollbar_style() -> ScrollbarStyle {
    ScrollbarStyle {
        track_w: SIDEBAR_PADDING,
        thumb_w: SIDEBAR_THUMB_W,
        thumb_w_active: SIDEBAR_THUMB_W,
        thumb_inset: SIDEBAR_SCROLLBAR_BORDER_GAP,
        track_right_offset: -SIDEBAR_PADDING,
        ..ScrollbarStyle::default()
    }
}

/// Dead zone at the top and bottom edge as a fraction of the list height.
///
/// Mirrors the cursor-driven list (`tab_list.rs`): while the cursor is inside
/// the dead zone the list already sits at its extreme position, so the first
/// and the last row are fully visible without pushing the mouse to the border.
const SIDEBAR_SMART_EDGE_PAD_FRACTION: f32 = 0.10;
/// ...but never larger than this on very tall lists.
const SIDEBAR_SMART_EDGE_PAD_MAX: f32 = 56.0;

/// Mouse movements shorter than this (px) are ignored to suppress jitter.
const SIDEBAR_SMART_MIN_MOVE: f32 = 0.5;
/// A scroll range below this (px) counts as "everything fits": no cursor scrolling.
const SIDEBAR_SMART_MIN_SCROLL_RANGE: f32 = 1.0;

/// Size of the dead zone at the top and at the bottom of the file list.
///
/// A fraction of the list height, but at least one row (so the last row is
/// fully visible while the cursor is still a row away from the border) and at
/// most [`SIDEBAR_SMART_EDGE_PAD_MAX`]. On very short lists it never takes
/// more than a third of the height.
fn sidebar_smart_edge_pad(list_height: Pixels) -> Pixels {
    (list_height * SIDEBAR_SMART_EDGE_PAD_FRACTION)
        .clamp(px(SIDEBAR_ROW_H), px(SIDEBAR_SMART_EDGE_PAD_MAX))
        .min(list_height / 3.0)
}

/// Target `scroll_top` for the given cursor position: `0` at the top of the
/// list, `max_scroll` at the bottom, proportional in between.
///
/// Same formula as the tabs popup (`cursor_target_offset` in `tab_list.rs`),
/// but expressed as a top offset instead of gpui's negative scroll offset.
fn sidebar_smart_cursor_target(
    cursor_y: Pixels,
    list_top: Pixels,
    list_height: Pixels,
    max_scroll: Pixels,
) -> Pixels {
    let pad = sidebar_smart_edge_pad(list_height);
    let usable = (list_height - pad * 2.0).max(px(1.0));
    let t = ((cursor_y - list_top - pad) / usable).clamp(0.0, 1.0);
    max_scroll * t
}

/// Rounds a value to whole device pixels, so text and icons move together
/// while the list follows the cursor (same as the tabs popup).
fn sidebar_smart_snap(value: Pixels, scale: f32) -> Pixels {
    (value * scale).round() / scale
}

/// Clamp helper shared with tests: keeps a cursor-driven target inside `[0, max]`.
fn clamp_sidebar_smart_target(raw: f32, max_offset: f32) -> f32 {
    raw.clamp(0.0, max_offset.max(0.0))
}

/// Clamps a raw scroll offset (pixels from the top of the list) into the scrollable range.
///
/// Split out of the renderer so the invariant is testable: the visible-range slice must never
/// be computed from an offset outside `[0, max_offset]`, because GPUI leaves the offset
/// unclamped until prepaint and a stale overshoot makes the list flicker at whichever end is
/// being pushed against.
///
/// `max_offset` stays 0 until the tree has been laid out once, so it is only a usable bound
/// once positive; before that only the lower bound applies.
fn clamp_sidebar_scroll_top(raw: f32, max_offset: f32) -> f32 {
    if max_offset > 0.0 {
        raw.clamp(0.0, max_offset)
    } else {
        raw.max(0.0)
    }
}

/// One flattened line of the sidebar file tree.
///
/// Built by [`AppView::flatten_sidebar_tree`] and turned into an element by
/// [`AppView::render_sidebar_row`]. The tree is flattened instead of walked recursively
/// during render, so collapsed folders and the active search filter cost nothing beyond
/// this cheap data row.
struct SidebarRow {
    is_folder: bool,
    /// Folders only: whether the folder is currently expanded.
    is_expanded: bool,
    /// Whether this row is selected (active note or selected folder).
    is_selected: bool,
    name: String,
    path: PathBuf,
    depth: usize,
}

/// Active drag of an entry inside the sidebar file tree.
///
/// Mirrors the bookmarks drag ([`crate::views::sidebar::bookmarks::BookmarkDragState`]):
/// the pointer has to travel past a small threshold before the gesture counts as a drag,
/// so a plain click keeps opening notes and toggling folders.
#[derive(Clone, Debug)]
pub struct SidebarTreeDragState {
    /// Entry being dragged.
    pub source_path: PathBuf,
    /// Folders are moved with `move_folder`, files with `move_file`.
    pub is_folder: bool,
    /// Y position of the mouse-down that started the gesture, in window coordinates.
    pub start_y: f32,
    /// Current pointer position in window coordinates; drives the drag ghost.
    pub pointer_x: f32,
    pub pointer_y: f32,
    /// Set once the pointer moved past the drag threshold.
    pub is_dragging: bool,
    /// Folder currently highlighted as the drop destination.
    pub target: Option<PathBuf>,
}

/// Distance in pixels the pointer must travel before a press turns into a drag.
const SIDEBAR_DRAG_THRESHOLD: f32 = 4.0;

/// How long a moved row stays fully highlighted before it starts fading out.
const SIDEBAR_MOVE_FLASH_HOLD: std::time::Duration =
    std::time::Duration::from_millis(1000);
/// Fade-out duration after the hold.
const SIDEBAR_MOVE_FLASH_FADE: std::time::Duration =
    std::time::Duration::from_millis(400);

/// Hold + fade run as one animation: the row stays highlighted, then eases to transparent.
///
/// A single animation with a hold-then-smoothstep curve is used instead of two chained
/// animations, because the chain jumps to the next animation's end value on the frame it
/// switches and would flash transparent for one frame before fading.
fn sidebar_move_flash_animations() -> Vec<Animation> {
    vec![
        Animation::new(SIDEBAR_MOVE_FLASH_HOLD + SIDEBAR_MOVE_FLASH_FADE)
            .with_easing(sidebar_move_flash_easing),
    ]
}

/// `0` through the hold, then a smoothstep ramp to `1` over the fade.
fn sidebar_move_flash_easing(delta: f32) -> f32 {
    let total = SIDEBAR_MOVE_FLASH_HOLD + SIDEBAR_MOVE_FLASH_FADE;
    let hold = SIDEBAR_MOVE_FLASH_HOLD.as_secs_f32() / total.as_secs_f32();
    if delta <= hold {
        0.0
    } else {
        let t = ((delta - hold) / (1.0 - hold)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

/// Whether dropping the dragged entry into `dest_folder` would move anything.
///
/// Dropping on the current parent (for files and folders alike) or into the dragged
/// folder itself/its descendants is a no-op. The tree uses this both to skip the move and
/// to avoid highlighting a target the entry cannot actually move to.
fn sidebar_drop_moves(drag: &SidebarTreeDragState, dest_folder: &Path) -> bool {
    let same_parent = drag
        .source_path
        .parent()
        .map(|parent| parent == dest_folder)
        .unwrap_or(false);
    !same_parent && !(drag.is_folder && dest_folder.starts_with(&drag.source_path))
}

impl AppView {
    /// Renders the scrollable, virtualized file tree.
    ///
    /// Returns the whole viewport: a `relative` wrapper holding the
    /// `#sidebar-tree-scroll` container plus the overlay scrollbar. The caller only
    /// decides where it goes. Everything else — flattening, resolving the scroll offset,
    /// slicing the visible range and sizing the thumb — happens here.
    pub(crate) fn render_sidebar_tree(
        &self,
        active_path: Option<&PathBuf>,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut sidebar_rows: Vec<SidebarRow> = Vec::new();
        let mut selected_row_index: Option<usize> = None;
        self.flatten_sidebar_tree(
            &self.state.file_tree,
            0,
            active_path,
            false,
            &mut sidebar_rows,
            &mut selected_row_index,
        );

        // Uniform row height turns the geometry into a plain stride:
        // row `i` spans [i * SIDEBAR_ROW_H, (i + 1) * SIDEBAR_ROW_H).
        let height_index = VisualRowHeightIndex::new(vec![SIDEBAR_ROW_H; sidebar_rows.len()]);

        // Real viewport height, so the visible slice is exact rather than guessed from the
        // window height. On the very first frame (and right after creation/maximize, when the
        // measured bounds lag one frame behind the live window) the handle is not measured yet:
        // floor the slice at the live window size instead. Overshooting only renders a few
        // extra clipped rows, while undershooting leaves a blank band of missing last rows
        // until the next repaint.
        let sidebar_viewport_h = {
            let h = f32::from(self.sidebar_scroll_handle.bounds().size.height);
            let live = (f32::from(window.bounds().size.height) - 100.0).max(100.0);
            h.max(live)
        };

        // Current scroll position measured from the top of the list (>= 0).
        let mut sidebar_scroll_top = -f32::from(self.sidebar_scroll_handle.offset().y);

        // Cursor-driven ("smart") scrolling, like the tabs popup: while Ctrl
        // (or Cmd on macOS) is held, the absolute cursor position inside the
        // list drives the offset and the wheel is swallowed by the canvas
        // below. Otherwise the list scrolls like a classic wheel list.
        // Off by default (`ui.sidebarSmartScroll`).
        let smart = self.config.ui.sidebar_smart_scroll;

        // Resolve programmatic scroll requests *before* computing the visible range, so the
        // slice we render matches the offset we are about to apply. Virtualization shifts
        // the child indices (two spacers are inserted), so `scroll_to_top_of_item` can no
        // longer be used — the target offset is computed from the height index directly.
        let mut scroll_request = false;
        if !self.initial_sidebar_scroll_done.get() {
            self.initial_sidebar_scroll_done.set(true);
            // A programmatic jump wins over the cursor: drop its pending target.
            self.sidebar_smart_target_y.set(None);
            // At startup the open file is always centered; the saved session
            // offset is only a fallback when there is no active row.
            if let Some(ix) = selected_row_index {
                let half = ((sidebar_viewport_h / SIDEBAR_ROW_H) as usize / 2).max(1);
                sidebar_scroll_top = height_index.top_of(ix.saturating_sub(half)) as f32;
                scroll_request = true;
            } else if self.state.session.sidebar_scroll_top < 0.0 {
                sidebar_scroll_top = -self.state.session.sidebar_scroll_top;
                scroll_request = true;
            }
        } else if self.need_scroll_sidebar_to_active.get() {
            self.need_scroll_sidebar_to_active.set(false);
            // Same as above: revealing the active file wins over the cursor.
            self.sidebar_smart_target_y.set(None);
            if let Some(ix) = selected_row_index {
                let half = ((sidebar_viewport_h / SIDEBAR_ROW_H) as usize / 2).max(1);
                sidebar_scroll_top = height_index.top_of(ix.saturating_sub(half)) as f32;
                scroll_request = true;
            }
        }

        // Clamp the offset before slicing the virtualized range — this is the fix for the
        // end-of-list flicker, and the editor's renderer does the same thing for the same
        // reason. GPUI's wheel handler adds its delta to the shared scroll offset *unclamped*
        // (`Div::add_scroll_wheel_listener`) and only clamps it later, in prepaint
        // (`clamp_scroll_position`). So while the wheel keeps turning past either end, the
        // offset this render reads is briefly out of range: past the bottom it shifts the
        // slice down (dropping the rows at the top of the viewport), past the top it shrinks
        // the slice (dropping the rows at the bottom). One frame later prepaint clamps and
        // the rows come back, which is the flicker. Clamping here makes the slice independent
        // of that stale overshoot; writing the value back keeps the next frame stable too.
        let max_y = f32::from(self.sidebar_scroll_handle.max_offset().height);
        let clamped_top = clamp_sidebar_scroll_top(sidebar_scroll_top, max_y);
        // Only write back when the value actually moved, so a settled list does not issue a
        // redundant scroll write every frame.
        if scroll_request || (clamped_top - sidebar_scroll_top).abs() > 0.5 {
            self.sidebar_scroll_handle
                .set_offset(point(px(0.0), px(-clamped_top)));
        }
        sidebar_scroll_top = clamped_top;

        // Apply a pending cursor-driven target (direct follow, no animation —
        // the tabs popup behaves the same without smoothing). Consumed even
        // when it matches the current offset, so a settled list issues no
        // redundant scroll writes. Runs for Ctrl smart scroll (`smart`) and for
        // middle-button smart scroll (always available while held).
        if smart || self.sidebar_middle_active {
            if let Some(target) = self.sidebar_smart_target_y.take() {
                let max = f32::from(self.sidebar_scroll_handle.max_offset().height);
                let snapped =
                    sidebar_smart_snap(px(clamp_sidebar_smart_target(target, max)), window.scale_factor());
                let snapped_f = f32::from(snapped);
                if (snapped_f - sidebar_scroll_top).abs() > 0.01 {
                    self.sidebar_scroll_handle
                        .set_offset(point(px(0.0), px(-snapped_f)));
                }
                sidebar_scroll_top = snapped_f;
            }
        }

        // One rounded highlight for the whole destination folder block. It is drawn as a
        // single rectangle behind the rows: per-row backgrounds leave visible seams at the
        // row boundaries. The rect is clamped to the visible part of the virtual list.
        let drop_block_range = self
            .sidebar_tree_drag
            .as_ref()
            .filter(|drag| drag.is_dragging)
            .and_then(|drag| {
                drag.target.as_deref().filter(|target| {
                    sidebar_drop_moves(drag, target)
                        && self.state.workspace_path.as_deref() != Some(*target)
                })
            })
            .and_then(|target| {
                let start = sidebar_rows
                    .iter()
                    .position(|row| row.path.starts_with(target))?;
                let len = sidebar_rows[start..]
                    .iter()
                    .take_while(|row| row.path.starts_with(target))
                    .count();
                Some((start, start + len))
            });
        let drop_block_rect = drop_block_range.and_then(|(start, end)| {
            let clip_height = {
                let measured = f32::from(self.sidebar_scroll_handle.bounds().size.height);
                if measured > 0.0 { measured } else { sidebar_viewport_h }
            };
            let top = (height_index.top_of(start) as f32 - sidebar_scroll_top).max(0.0);
            let bottom =
                (height_index.top_of(end) as f32 - sidebar_scroll_top).min(clip_height);
            (bottom > top).then_some((top, bottom - top))
        });

        let visible_rows = height_index.visible_range(
            sidebar_scroll_top as f64,
            sidebar_viewport_h as f64,
            SIDEBAR_OVERSCAN_PX as f64,
        );
        let top_spacer_h = height_index.top_of(visible_rows.start) as f32;
        let bottom_spacer_h =
            (height_index.total() - height_index.top_of(visible_rows.end)).max(0.0) as f32;

        // The two spacers keep the scrollable content as tall as the full list, so the
        // scrollbar stays proportional to the whole tree.
        let mut tree_elements: Vec<AnyElement> = Vec::with_capacity(visible_rows.len() + 2);
        if top_spacer_h > 0.0 {
            tree_elements.push(
                div()
                    .id("sidebar-tree-top-spacer")
                    .w_full()
                    .h(px(top_spacer_h))
                    .flex_shrink_0()
                    .into_any_element(),
            );
        }
        for ix in visible_rows {
            tree_elements.push(self.render_sidebar_row(&sidebar_rows[ix], theme, window, cx));
        }
        if bottom_spacer_h > 0.0 {
            tree_elements.push(
                div()
                    .id("sidebar-tree-bottom-spacer")
                    .w_full()
                    .h(px(bottom_spacer_h))
                    .flex_shrink_0()
                    .into_any_element(),
            );
        }

        // While an entry is dragged, the end of the list becomes a drop zone for the
        // workspace root — the same way the bookmarks list lets a note leave its group.
        let is_dragging_active = self
            .sidebar_tree_drag
            .as_ref()
            .map(|drag| drag.is_dragging)
            .unwrap_or(false);
        if is_dragging_active {
            if let Some(root) = self.state.workspace_path.clone() {
                tree_elements.push(self.render_sidebar_root_drop_zone(root, theme, cx));
            }
        }

        let scroll_container = div()
            .id("sidebar-tree-scroll")
            .track_scroll(&self.sidebar_scroll_handle)
            .flex()
            .flex_col()
            .flex_1()
            .overflow_scroll()
            .children(tree_elements);

        // The scrollbar is an overlay, so it must not scroll with the content: it is a
        // sibling of the scroll container inside a `relative` wrapper, not a child of it.
        let style = sidebar_scrollbar_style();
        let mut viewport = div()
            .id("sidebar-tree-viewport")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0();
        // Painted before the scroll container, so the rows (transparent inside the block)
        // show one continuous rounded highlight with a frame around the destination folder.
        if let Some((top, height)) = drop_block_rect {
            viewport = viewport.child(
                div()
                    .id("sidebar-tree-drop-block")
                    .absolute()
                    .left(px(0.0))
                    .right(px(SIDEBAR_ROW_SCROLLBAR_GAP))
                    .top(px(top))
                    .h(px(height))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.accent)
                    .bg(theme.selection_bg),
            );
        }
        viewport = viewport
            .child(scroll_container)
            // Keeps the drag ghost glued to the pointer even over spacers and gaps
            // between rows, where no row listener runs.
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if let Some(ref mut drag) = this.sidebar_tree_drag {
                    drag.pointer_x = event.position.x.into();
                    drag.pointer_y = event.position.y.into();
                    if drag.is_dragging {
                        cx.notify();
                    }
                }
            }))
            // A drag released over empty space (not a row, not the root zone) has no
            // destination: drop the state instead of leaving the gesture armed. Rows run
            // first in the bubble phase and take the state themselves, so this only sees
            // gestures that ended outside every drop target.
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.sidebar_tree_drag.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.sidebar_tree_drag.take().is_some() {
                    cx.notify();
                }
            }));

        // Cursor-driven ("smart") scrolling: Ctrl/Cmd mode (`smart`) plus
        // middle-button mode (always available while the middle button is
        // held). Both share the same absolute mapping — cursor near the top
        // shows the beginning, near the bottom the end — so the bounds tracker
        // below is always mounted, not only when `smart` is on.
        {
            let bounds = self.sidebar_smart_bounds.clone();
            let smart_enabled = smart;
            viewport = viewport
                .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                    if this.sidebar_middle_active {
                        // Middle-button smart scroll: explicit gesture, works
                        // even when the Ctrl mode is disabled in settings.
                        this.on_sidebar_smart_mouse_move(f32::from(event.position.y), true, cx);
                    } else if smart_enabled {
                        let ctrl = event.modifiers.control || event.modifiers.platform;
                        this.on_sidebar_smart_mouse_move(f32::from(event.position.y), ctrl, cx);
                    }
                }))
                // Leaving the list clears the reference point (see the tabs
                // popup): without this the first move after re-entering jumps.
                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    if !hovered {
                        this.sidebar_smart_last_mouse_y.set(None);
                        cx.notify();
                    }
                }))
                .on_mouse_down(MouseButton::Middle, cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    // One gesture at a time: a border drag already owns the pointer.
                    if this.is_resizing_any() {
                        return;
                    }
                    cx.stop_propagation();
                    this.start_sidebar_middle_scroll(cx);
                }))
                .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                    this.stop_sidebar_middle_scroll(cx);
                }))
                .on_mouse_up_out(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                    this.stop_sidebar_middle_scroll(cx);
                }))
                .child(
                    canvas(
                        move |viewport, _window, _cx| bounds.set(viewport),
                        move |viewport, _state, window, _cx| {
                            // Swallow the wheel and the trackpad in the capture phase,
                            // before it reaches the scroll container (same trick as
                            // the tabs popup: gpui binds the wheel to the scrolled
                            // element itself, so a plain `on_scroll_wheel` above
                            // cannot intercept it) — but only while Ctrl (or Cmd
                            // on macOS) is held and the Ctrl mode is enabled.
                            // Otherwise the wheel scrolls the list as usual.
                            // Middle-button mode keeps the wheel alive: the user
                            // holds the button with one finger and can still
                            // fine-tune with the wheel.
                            if !smart_enabled {
                                return;
                            }
                            window.on_mouse_event(
                                move |event: &ScrollWheelEvent, phase, _window, cx| {
                                    if phase == DispatchPhase::Capture
                                        && viewport.contains(&event.position)
                                        && (event.modifiers.control
                                            || event.modifiers.platform)
                                    {
                                        cx.stop_propagation();
                                    }
                                },
                            );
                        },
                    )
                    .absolute()
                    .size_full(),
                );
        }

        // Middle-button indicator: a small pill so it is obvious the list now
        // follows the cursor; releasing the middle button dismisses it.
        if self.sidebar_middle_active {
            viewport = viewport.child(
                div()
                    .absolute()
                    .top(px(8.0))
                    .left(px(8.0))
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded_full()
                    .bg(theme.bg_hover)
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child("⇅ СКМ-скролл"),
            );
        }

        if let Some(geometry) = ScrollbarGeometry::from_handle(&self.sidebar_scroll_handle, &style) {
            viewport = viewport.child(overlay_scrollbar(
                "sidebar-scrollbar-track",
                &geometry,
                &style,
                theme,
                self.sidebar_scrollbar_drag.active,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    // One gesture at a time: a border drag already owns the pointer, so a
                    // track click must not jump the list out from under it.
                    if this.is_resizing_any() {
                        return;
                    }
                    cx.stop_propagation();
                    let pointer_y: f32 = event.position.y.into();
                    this.set_sidebar_scroll_top(geometry.offset_for_track_click(pointer_y));
                    cx.notify();
                }),
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if this.is_resizing_any() {
                        return;
                    }
                    cx.stop_propagation();
                    this.sidebar_scrollbar_drag.begin(event.position.y.into(), geometry.scroll_y);
                    cx.notify();
                }),
            ));
        }

        viewport.into_any_element()
    }

    /// Scrolls the sidebar tree to `scroll_y` pixels from the top of the list.
    pub(crate) fn set_sidebar_scroll_top(&self, scroll_y: f32) {
        self.sidebar_scroll_handle.set_offset(point(px(0.0), px(-scroll_y)));
    }

    /// Maps the absolute cursor position to a pending scroll target.
    ///
    /// Mirrors the tabs popup (`on_mouse_move` in `tab_list.rs`): the first
    /// event only primes the reference point, then the cursor near the top
    /// edge shows the beginning of the list, near the bottom edge the end,
    /// and in between the offset is proportional to the cursor position.
    /// Engages only while `ctrl` (Ctrl, or Cmd on macOS) is held — otherwise
    /// the list stays classic and the reference point is reset, so pressing
    /// Ctrl later never jumps from a stale anchor.
    /// The repaint at the end is unconditional so a cursor landing on the
    /// list in a single jump still triggers a frame.
    pub(crate) fn on_sidebar_smart_mouse_move(&self, y: f32, ctrl: bool, cx: &mut Context<Self>) {
        if !ctrl {
            self.sidebar_smart_last_mouse_y.set(None);
            self.sidebar_smart_target_y.set(None);
            cx.notify();
            return;
        }
        match self.sidebar_smart_last_mouse_y.get() {
            // The first event only primes the reference point: acting on it
            // would make the list jump from wherever the wheel left it.
            None => self.sidebar_smart_last_mouse_y.set(Some(y)),
            // The distance is measured from the last *accepted* position, so a
            // slow move accumulates instead of being dropped event by event.
            Some(anchor) if (y - anchor).abs() >= SIDEBAR_SMART_MIN_MOVE => {
                self.sidebar_smart_last_mouse_y.set(Some(y));

                let bounds = self.sidebar_smart_bounds.get();
                let max_scroll = self.sidebar_scroll_handle.max_offset().height;
                if bounds.size.height > px(0.0) && max_scroll >= px(SIDEBAR_SMART_MIN_SCROLL_RANGE) {
                    let target = sidebar_smart_cursor_target(
                        px(y),
                        bounds.origin.y,
                        bounds.size.height,
                        max_scroll,
                    );
                    self.sidebar_smart_target_y.set(Some(f32::from(target)));
                }
            }
            Some(_) => {}
        }

        cx.notify();
    }

    /// Follows the pointer while the sidebar scrollbar thumb is being dragged.
    pub(crate) fn handle_sidebar_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.sidebar_scrollbar_drag.active {
            return;
        }
        let style = sidebar_scrollbar_style();
        let Some(geometry) = ScrollbarGeometry::from_handle(&self.sidebar_scroll_handle, &style) else {
            return;
        };
        geometry.apply_drag(&self.sidebar_scroll_handle, &self.sidebar_scrollbar_drag, pointer_y);
        cx.notify();
    }

    /// Flattens the file tree into a list of rows, honouring collapsed folders and the
    /// active search filter, and records the index of the active note's row.
    ///
    /// This replaces the old recursive element builder: instead of constructing a full
    /// `Div` subtree for every node in the vault (thousands of them, all laid out every
    /// frame), we produce one cheap data row per visible node and let the caller lay out
    /// only the slice that is actually on screen.
    fn flatten_sidebar_tree(
        &self,
        nodes: &[FileNode],
        depth: usize,
        active_path: Option<&PathBuf>,
        parent_matched: bool,
        out: &mut Vec<SidebarRow>,
        selected: &mut Option<usize>,
    ) {
        let query = self.state.search_query.trim().to_lowercase();
        let is_filtering = !query.is_empty();

        for node in nodes {
            match node {
                FileNode::Folder { name, path, children } => {
                    let folder_matches_self = is_filtering && name.to_lowercase().contains(&query);
                    let folder_has_matching_descendant = is_filtering && children.iter().any(|c| c.matches_query(&query));

                    // When filtering, hide the folder unless it, its parent, or a descendant matches.
                    if is_filtering && !parent_matched && !folder_matches_self && !folder_has_matching_descendant {
                        continue;
                    }

                    // Auto-expand while filtering when the folder itself or a descendant matches.
                    let is_expanded = self.state.expanded_folders.contains(path)
                        || (is_filtering && (folder_matches_self || folder_has_matching_descendant));

                    let is_selected = active_path.map(|p| p == path).unwrap_or(false);
                    if is_selected {
                        *selected = Some(out.len());
                    }

                    out.push(SidebarRow {
                        is_folder: true,
                        is_expanded,
                        is_selected,
                        name: name.clone(),
                        path: path.clone(),
                        depth,
                    });

                    if is_expanded {
                        let next_parent_matched = parent_matched || folder_matches_self;
                        self.flatten_sidebar_tree(children, depth + 1, active_path, next_parent_matched, out, selected);
                    }
                }
                FileNode::File { name, path } => {
                    if is_filtering && !parent_matched && !name.to_lowercase().contains(&query) {
                        continue;
                    }

                    let is_selected = active_path.map(|p| p == path).unwrap_or(false);
                    if is_selected {
                        *selected = Some(out.len());
                    }

                    out.push(SidebarRow {
                        is_folder: false,
                        is_expanded: false,
                        is_selected,
                        name: name.clone(),
                        path: path.clone(),
                        depth,
                    });
                }
            }
        }
    }

    /// Moves the dragged tree entry into its highlighted drop folder.
    ///
    /// Reuses the same filesystem helpers as the tree's context menu (`move_file` /
    /// `move_folder`) and the same bookkeeping as the "Move to..." modal, so open tabs,
    /// session state and the plugin snapshot follow the file.
    fn execute_sidebar_tree_move(&mut self, drag: SidebarTreeDragState, cx: &mut Context<Self>) {
        if !drag.is_dragging {
            return;
        }
        let Some(dest_folder) = drag.target.clone() else {
            cx.notify();
            return;
        };

        // Dropping on the current parent (or into itself) is a no-op, not an error.
        if !sidebar_drop_moves(&drag, &dest_folder) {
            cx.notify();
            return;
        }
        let source = drag.source_path;

        let result = if drag.is_folder {
            crate::views::sidebar::folder_actions::move_folder(&source, &dest_folder)
        } else {
            crate::views::sidebar::file_actions::move_file(&source, &dest_folder)
        };

        match result {
            Ok(new_path) => {
                // A file that is not open in a tab gets a brief frame where it landed;
                // an open file is already visible in the editor.
                let is_open = self.state.find_document_by_path(&source).is_some()
                    || self.state.open_tabs.contains(&source);
                self.state.handle_file_renamed(&source, &new_path);
                self.state.expanded_folders.remove(&source);
                self.state.expanded_folders.insert(dest_folder.clone());
                self.state.expanded_folders.insert(new_path.clone());
                self.state.reload_file_tree();
                self.state.publish_snapshot();
                self.state.status_text = if drag.is_folder {
                    self.t("sidebar.folder_moved")
                } else {
                    self.t("sidebar.file_moved")
                };

                if drag.is_folder || !is_open {
                    self.sidebar_move_flash = Some(new_path.clone());
                    self.sidebar_move_flash_seq = self.sidebar_move_flash_seq.wrapping_add(1);
                    self.schedule_sidebar_move_flash_clear(new_path, cx);
                }
            }
            Err(e) => {
                let msg = self.t_fmt("sidebar.error_moving", &[&e]);
                self.push_notification(
                    crate::views::notification::NotificationType::Error,
                    None,
                    msg,
                    Some(6000),
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// Starts inline rename for a file or folder row.
    ///
    /// Reuses the bookmarks inline-rename state and input, so both lists edit names
    /// through the same element.
    pub(crate) fn start_sidebar_tree_rename(
        &mut self,
        path: PathBuf,
        is_folder: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.bookmark_rename.is_some() {
            self.commit_bookmark_rename(cx);
        }
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        self.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
        self.sidebar_collapsed = false;
        // An active quick filter could hide the row being renamed, taking the input
        // with it.
        self.quick_filter.clear(&mut self.state.search_query, cx);
        self.state.reveal_path_in_tree(&path);
        if is_folder {
            self.selected_sidebar_folder = Some(path.clone());
            self.need_scroll_sidebar_to_active.set(true);
        } else {
            if self.selected_sidebar_folder.as_deref() == Some(path.as_path()) {
                self.selected_sidebar_folder = None;
            }
            // Only the active note is auto-centered; a file renamed from its context
            // menu is already on screen and must not jump the list elsewhere.
            if self.state.active_document().map(|doc| doc.path == path).unwrap_or(false) {
                self.need_scroll_sidebar_to_active.set(true);
            }
        }

        self.bookmark_rename = Some(crate::views::sidebar::BookmarkRenameState::new(
            crate::views::sidebar::BookmarkRenameTarget::TreePath { path, is_folder },
            name,
            self.bookmark_rename_focus_handle.clone(),
        ));
        self.bookmark_rename_blink
            .enable(crate::blink_manager::BlinkTarget::SidebarRename, cx);
        window.focus(&self.bookmark_rename_focus_handle);
        cx.notify();
    }

    /// Clears the move flash after a second, so the frame is a highlight rather than state.
    fn schedule_sidebar_move_flash_clear(
        &self,
        flash_path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.weak_entity();
        let async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();
        cx.foreground_executor()
            .spawn(async move {
                timer_executor
                    .timer(SIDEBAR_MOVE_FLASH_HOLD + SIDEBAR_MOVE_FLASH_FADE)
                    .await;
                let _ = async_cx.update(|cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            if this.sidebar_move_flash.as_deref()
                                == Some(flash_path.as_path())
                            {
                                this.sidebar_move_flash = None;
                                cx.notify();
                            }
                        });
                    }
                });
            })
            .detach();
    }

    /// Drop zone at the end of the tree that moves the dragged entry to the workspace root.
    fn render_sidebar_root_drop_zone(
        &self,
        root: PathBuf,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_hovered = self
            .sidebar_tree_drag
            .as_ref()
            .filter(|drag| drag.is_dragging)
            .and_then(|drag| {
                drag.target
                    .as_deref()
                    .map(|target| target == root.as_path() && sidebar_drop_moves(drag, target))
            })
            .unwrap_or(false);

        div()
            .id("sidebar-tree-root-drop-zone")
            .h(px(40.0))
            .w_full()
            .flex_shrink_0()
            .rounded_md()
            .border_1()
            .when(is_hovered, |d| {
                d.border_color(theme.accent).bg(theme.bg_hover)
            })
            .when(!is_hovered, |d| d.border_color(rgba(0x00000000)))
            .on_mouse_move(cx.listener({
                let root = root.clone();
                move |this, _, _, cx| {
                    if let Some(ref mut drag) = this.sidebar_tree_drag {
                        if drag.is_dragging {
                            drag.target = Some(root.clone());
                            cx.notify();
                        }
                    }
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if let Some(drag) = this.sidebar_tree_drag.take() {
                    this.execute_sidebar_tree_move(drag, cx);
                }
            }))
            .into_any_element()
    }

    /// Floating preview of the dragged entry, drawn at the pointer so the gesture has a
    /// visible "ghost" even when the source row scrolls out of view.
    pub(crate) fn render_sidebar_drag_ghost(&self, theme: &Theme) -> Option<AnyElement> {
        let drag = self
            .sidebar_tree_drag
            .as_ref()
            .filter(|drag| drag.is_dragging)?;

        let file_name = drag
            .source_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let display_name = if drag.is_folder {
            file_name
        } else {
            file_name
                .strip_suffix(".md")
                .unwrap_or(&file_name)
                .to_string()
        };
        let icon = if drag.is_folder {
            icons::ICON_FOLDER
        } else {
            icons::ICON_FILE_TEXT
        };

        Some(crate::views::sidebar::drag_ghost::sidebar_drag_ghost(
            display_name,
            icon,
            13.0,
            drag.pointer_x,
            drag.pointer_y,
            theme,
        ))
    }

    /// Renders a single flattened sidebar row. Row height is fixed ([`SIDEBAR_ROW_H`]) so
    /// the caller can place it from the row index alone.
    fn render_sidebar_row(
        &self,
        row: &SidebarRow,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_renaming = self
            .bookmark_rename
            .as_ref()
            .map(|rename| match &rename.target {
                crate::views::sidebar::BookmarkRenameTarget::TreePath { path, .. } => {
                    path.as_path() == row.path.as_path()
                }
                _ => false,
            })
            .unwrap_or(false);
        let drag = self
            .sidebar_tree_drag
            .as_ref()
            .filter(|drag| drag.is_dragging);
        let is_drag_active = drag.is_some();
        // Only a destination that would really move the entry gets highlighted, so the
        // row under the pointer does not promise a drop that is a no-op.
        let effective_target = drag
            .filter(|drag| {
                drag.target
                    .as_deref()
                    .map(|target| {
                        sidebar_drop_moves(drag, target)
                            // The root is not a row in the tree; it has its own drop zone
                            // at the end of the list and must not tint the whole tree.
                            && self.state.workspace_path.as_deref() != Some(target)
                    })
                    .unwrap_or(false)
            })
            .and_then(|drag| drag.target.as_deref());
        let is_dragged = drag
            .map(|drag| drag.source_path == row.path)
            .unwrap_or(false);
        // The target folder's whole visible subtree is tinted as one solid block: a folder
        // with subfolders and many files would otherwise look like its single header row
        // is the only place the entry could land.
        let is_drop_block = effective_target
            .map(|target| row.path.starts_with(target))
            .unwrap_or(false);
        let is_move_flash = self
            .sidebar_move_flash
            .as_deref()
            .map(|path| path == row.path.as_path())
            .unwrap_or(false);

        if row.is_folder {
            let is_folder_selected = (self.folder_context_menu.is_open && self.folder_context_menu.folder_path == row.path)
                || row.is_selected;

            let chevron = if row.is_expanded {
                icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted)
            } else {
                icons::icon(icons::ICON_CHEVRON_RIGHT, 12.0, theme.text_muted)
            };
            let folder_svg = if row.is_expanded {
                icons::icon(icons::ICON_FOLDER_OPEN, 14.0, theme.accent)
            } else {
                icons::icon(icons::ICON_FOLDER, 14.0, theme.accent)
            };
            let indent = (row.depth * 14) as f32 + 4.0;
            let label_color = if is_folder_selected || is_dragged || is_move_flash {
                theme.accent
            } else {
                theme.text_primary
            };

            let label = crate::views::sidebar::highlight_matches(
                &row.name,
                &self.state.search_query,
                label_color,
                is_folder_selected,
                theme,
            );

            let folder_path_context = row.path.clone();

            let row_element = div()
                .id(SharedString::from(format!("fld-{}", row.path.to_string_lossy())))
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .h(px(SIDEBAR_ROW_H))
                .flex_shrink_0()
                .pl(px(indent))
                .pr_2()
                .mr(px(SIDEBAR_ROW_SCROLLBAR_GAP))
                .rounded_md()
                .cursor_pointer()
                .when(is_folder_selected, |s| {
                    s.when(!is_drop_block, |s| s.bg(theme.bg_selected))
                        .text_color(theme.accent)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .when(!is_folder_selected, |s| {
                    s.when(
                        !is_drag_active
                            && !self.folder_context_menu.is_open
                            && !self.file_context_menu.is_open
                            && !self.sidebar_scrollbar_drag.active,
                        |d| d.hover(|h| h.bg(theme.bg_hover)),
                    )
                    .text_color(theme.text_primary)
                })
                // The grabbed row keeps an accent frame while the ghost follows the
                // pointer; the destination folder block is the filled highlight.
                .when(is_dragged, |s| {
                    s.border_1()
                        .border_color(theme.accent)
                        .text_color(theme.accent)
                })
                // A freshly moved folder flashes too; the fade is applied by the
                // animation wrapper at the end of this branch.
                .when(is_move_flash, |s| {
                    s.border_1()
                        .border_color(theme.accent)
                        .bg(theme.selection_bg)
                        .text_color(theme.accent)
                })
                .child(chevron)
                .child(folder_svg)
                .child(if is_renaming {
                    self.render_bookmark_rename_input(theme, window, cx)
                } else {
                    div()
                        .text_xs()
                        .text_color(label_color)
                        .truncate()
                        .child(label)
                        .into_any_element()
                })
                // Left mouse down only arms the drag; a plain click is resolved on mouse up,
                // after the drag threshold had a chance to engage (same as bookmarks).
                .on_mouse_down(MouseButton::Left, cx.listener({
                    let path = row.path.clone();
                    move |this, event: &MouseDownEvent, _, cx| {
                        if this.folder_context_menu.is_open || this.file_context_menu.is_open {
                            return;
                        }
                        // Clicking another row finishes an inline rename, like the
                        // bookmarks list does.
                        if this.bookmark_rename.is_some() {
                            this.commit_bookmark_rename(cx);
                        }
                        this.sidebar_tree_drag = Some(SidebarTreeDragState {
                            source_path: path.clone(),
                            is_folder: true,
                            start_y: event.position.y.into(),
                            pointer_x: event.position.x.into(),
                            pointer_y: event.position.y.into(),
                            is_dragging: false,
                            target: None,
                        });
                        cx.notify();
                    }
                }))
                .on_mouse_move(cx.listener({
                    let path = row.path.clone();
                    move |this, event: &MouseMoveEvent, _, cx| {
                        let Some(ref mut drag) = this.sidebar_tree_drag else {
                            return;
                        };
                        let cur_y: f32 = event.position.y.into();
                        if !drag.is_dragging
                            && (cur_y - drag.start_y).abs() > SIDEBAR_DRAG_THRESHOLD
                        {
                            drag.is_dragging = true;
                        }
                        if drag.is_dragging {
                            drag.target = Some(path.clone());
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up(MouseButton::Left, cx.listener({
                    let path = row.path.clone();
                    move |this, _, _, cx| {
                        let Some(drag) = this.sidebar_tree_drag.take() else {
                            return;
                        };
                        if drag.is_dragging {
                            this.execute_sidebar_tree_move(drag, cx);
                        } else if drag.source_path == path {
                            this.selected_sidebar_folder = Some(path.clone());
                            this.state.toggle_folder(path.clone());
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.open_folder_context_menu(folder_path_context.clone(), x, y, cx);
                }));

            if is_move_flash {
                row_element
                    .with_animations(
                        SharedString::from(format!(
                            "sidebar-move-flash-{}-{}",
                            self.sidebar_move_flash_seq,
                            row.path.to_string_lossy()
                        )),
                        sidebar_move_flash_animations(),
                        |el, _phase, delta| el.opacity(1.0 - delta),
                    )
                    .into_any_element()
            } else {
                row_element.into_any_element()
            }
        } else {
            let is_selected = row.is_selected;
            let indent = (row.depth * 14 + 18) as f32 + 4.0;
            let display_name = row.name.strip_suffix(".md").unwrap_or(&row.name).to_string();
            let base_color = if is_selected || is_dragged || is_move_flash {
                theme.accent
            } else {
                theme.text_secondary
            };

            let label = crate::views::sidebar::highlight_matches(
                &display_name,
                &self.state.search_query,
                base_color,
                is_selected,
                theme,
            );

            let file_svg = icons::icon(
                icons::ICON_FILE_TEXT,
                13.0,
                if is_selected || is_dragged || is_move_flash {
                    theme.accent
                } else {
                    theme.text_muted
                },
            );

            let drop_parent = row
                .path
                .parent()
                .map(|parent| parent.to_path_buf());

            let row_element = div()
                .id(SharedString::from(format!("fil-{}", row.path.to_string_lossy())))
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .h(px(SIDEBAR_ROW_H))
                .flex_shrink_0()
                .pl(px(indent))
                .pr_2()
                .mr(px(SIDEBAR_ROW_SCROLLBAR_GAP))
                .rounded_md()
                .cursor_pointer()
                .when(is_selected, |s| {
                    s.when(!is_drop_block, |s| s.bg(theme.bg_selected))
                        .text_color(theme.accent)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .when(!is_selected, |s| {
                    s.when(
                        !is_drag_active
                            && !self.folder_context_menu.is_open
                            && !self.file_context_menu.is_open
                            && !self.sidebar_scrollbar_drag.active,
                        |d| d.hover(|h| h.bg(theme.bg_hover)),
                    )
                    .text_color(theme.text_primary)
                })
                // Transparent inside the destination block: the single highlight drawn
                // behind the rows shows through without per-row stripes. The grabbed row
                // itself is marked with a frame.
                .when(is_dragged, |s| {
                    s.border_1()
                        .border_color(theme.accent)
                        .text_color(theme.accent)
                })
                // A freshly moved file flashes in the accent colour so the user sees
                // exactly where it landed.
                .when(is_move_flash, |s| {
                    s.border_1()
                        .border_color(theme.accent)
                        .bg(theme.selection_bg)
                        .text_color(theme.accent)
                })
                .child(file_svg)
                .child(if is_renaming {
                    self.render_bookmark_rename_input(theme, window, cx)
                } else {
                    div()
                        .text_xs()
                        .truncate()
                        .child(label)
                        .into_any_element()
                })
                .on_mouse_down(MouseButton::Left, cx.listener({
                    let path = row.path.clone();
                    move |this, event: &MouseDownEvent, _, cx| {
                        if this.folder_context_menu.is_open || this.file_context_menu.is_open {
                            return;
                        }
                        // Clicking another row finishes an inline rename, like the
                        // bookmarks list does.
                        if this.bookmark_rename.is_some() {
                            this.commit_bookmark_rename(cx);
                        }
                        this.sidebar_tree_drag = Some(SidebarTreeDragState {
                            source_path: path.clone(),
                            is_folder: false,
                            start_y: event.position.y.into(),
                            pointer_x: event.position.x.into(),
                            pointer_y: event.position.y.into(),
                            is_dragging: false,
                            target: None,
                        });
                        cx.notify();
                    }
                }))
                // Hovering a file while dragging targets its containing folder, so dropping
                // next to an existing note moves the entry there (same as a file manager).
                .on_mouse_move(cx.listener({
                    let drop_parent = drop_parent.clone();
                    move |this, event: &MouseMoveEvent, _, cx| {
                        let Some(ref mut drag) = this.sidebar_tree_drag else {
                            return;
                        };
                        let cur_y: f32 = event.position.y.into();
                        if !drag.is_dragging
                            && (cur_y - drag.start_y).abs() > SIDEBAR_DRAG_THRESHOLD
                        {
                            drag.is_dragging = true;
                        }
                        if drag.is_dragging {
                            // A top-level file's parent is the workspace root, which has
                            // its own drop zone at the end of the list; making it a
                            // per-row target would tint the whole tree.
                            drag.target = drop_parent.clone().filter(|parent| {
                                this.state.workspace_path.as_deref()
                                    != Some(parent.as_path())
                            });
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up(MouseButton::Left, cx.listener({
                    let path = row.path.clone();
                    move |this, _, window, cx| {
                        let Some(drag) = this.sidebar_tree_drag.take() else {
                            return;
                        };
                        if drag.is_dragging {
                            this.execute_sidebar_tree_move(drag, cx);
                        } else if drag.source_path == path {
                            this.select_note(path.clone(), false, window, cx);
                        }
                    }
                }))
                .on_mouse_down(MouseButton::Right, cx.listener({
                    let file_path = row.path.clone();
                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.select_note(file_path.clone(), false, window, cx);
                        this.open_file_context_menu(file_path.clone(), x, y, cx);
                    }
                }));

            if is_move_flash {
                row_element
                    .with_animations(
                        SharedString::from(format!(
                            "sidebar-move-flash-{}-{}",
                            self.sidebar_move_flash_seq,
                            row.path.to_string_lossy()
                        )),
                        sidebar_move_flash_animations(),
                        |el, _phase, delta| el.opacity(1.0 - delta),
                    )
                    .into_any_element()
            } else {
                row_element.into_any_element()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_offset_is_clamped_into_range_before_slicing() {
        // Pushing past the bottom: the wheel leaves an overshoot that must never reach the
        // slice, or the rows at the top of the viewport are dropped for one frame.
        assert_eq!(clamp_sidebar_scroll_top(1200.0, 1000.0), 1000.0);
        // Pushing past the top.
        assert_eq!(clamp_sidebar_scroll_top(-40.0, 1000.0), 0.0);
        // In range: untouched.
        assert_eq!(clamp_sidebar_scroll_top(250.0, 1000.0), 250.0);
    }

    #[test]
    fn scroll_offset_before_the_first_layout_keeps_only_the_lower_bound() {
        // `max_offset` is 0 until the tree has been laid out once, so it must not be used as
        // an upper bound — that would pin the list to the top on the first frame.
        assert_eq!(clamp_sidebar_scroll_top(250.0, 0.0), 250.0);
        assert_eq!(clamp_sidebar_scroll_top(-40.0, 0.0), 0.0);
    }

    #[test]
    fn smart_target_is_top_at_the_top_of_the_band_and_max_at_the_bottom() {
        // list_top = 100, list_height = 300 -> dead zone clamp(300 * 0.10, 29, 56) = 30:
        // the proportional band is 130..370.
        let at = |y: f32| sidebar_smart_cursor_target(px(y), px(100.0), px(300.0), px(1000.0));
        assert!((f32::from(at(130.0)) - 0.0).abs() < 0.01);
        assert!((f32::from(at(370.0)) - 1000.0).abs() < 0.01);
        // Middle lands in the middle.
        assert!((f32::from(at(250.0)) - 500.0).abs() < 0.01);
        // Outside is clamped.
        assert!((f32::from(at(0.0)) - 0.0).abs() < 0.01);
        assert!((f32::from(at(900.0)) - 1000.0).abs() < 0.01);
    }

    #[test]
    fn smart_target_is_clamped_into_range() {
        assert_eq!(clamp_sidebar_smart_target(1200.0, 1000.0), 1000.0);
        assert_eq!(clamp_sidebar_smart_target(-40.0, 1000.0), 0.0);
        assert_eq!(clamp_sidebar_smart_target(250.0, 1000.0), 250.0);
    }
}
