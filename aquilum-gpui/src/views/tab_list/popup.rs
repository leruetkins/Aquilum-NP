use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::time::Instant;
use gpui::{
    canvas, div, point, prelude::*, px, uniform_list, AnyElement, Bounds, ClipboardItem, Context,
    DispatchPhase, EventEmitter, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, ScrollHandle, ScrollWheelEvent, SharedString,
    UniformListScrollHandle, Window,
};
use crate::blink_manager::BlinkManager;
use crate::icons;
use crate::snap::{snap_x, snapped_span};
use crate::theme::Theme;
use super::model::*;

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
pub(crate) fn search_word_range_at(text: &str, pos: usize) -> (usize, usize) {
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
        let trailing_icon = tab.trailing_icon;
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
            .when(hovered, move |row| row.bg(bg_hover));

        let row = if self.subtitle_below {
            let has_leading = leading.is_some();
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
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .when_some(leading, |r, icon_el| r.child(icon_el))
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .font_family(font_family)
                                    .text_color(text_primary)
                                    .child(title),
                            ),
                    )
                    .when_some(subtitle, |col, subtitle| {
                        col.child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(text_muted)
                                .when(has_leading, |d| d.pl(px(22.0)))
                                .child(subtitle),
                        )
                    }),
            )
        } else {
            row.when_some(leading, |row, icon_el| row.child(icon_el))
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
                            .ml_1p5()
                            .text_xs()
                            .text_color(text_muted)
                            .child(subtitle),
                    )
                })
        };

        let row = row.when_some(trailing_icon, |row, tr_icon| {
            row.child(
                div()
                    .flex_shrink_0()
                    .child(icons::icon(tr_icon, 13.0, text_muted)),
            )
        });

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
impl EventEmitter<TabListEvent> for TabListPopup {}
