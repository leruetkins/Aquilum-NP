use gpui::{
    div, prelude::*, px, rgb, Context, IntoElement, MouseButton, MouseDownEvent, Styled,
};
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;

impl AppView {
    /// Closes all open tabs and resets active note state.
    pub fn close_all_tabs(&mut self, cx: &mut Context<Self>) {
        if let Some(ref note) = self.state.active_note {
            if note.is_dirty {
                self.state.save_active_note();
            }
        }
        self.commit_title_rename_internal();
        for leaf in self.state.leaves.values_mut() {
            leaf.tabs.clear();
            leaf.tabs.push(crate::workspace::Tab::new_empty());
            leaf.active_tab_index = 0;
        }
        self.state.cleanup_orphaned_documents_and_views();
        self.state.open_tabs.clear();
        self.state.active_note = None;
        self.state.active_empty_tab = None;
        self.state.session.open_tabs.clear();
        self.state.session.active_note = None;
        self.state.save_session_state();
        self.title_editing_path = None;
        self.title_edit_text.clear();
        self.title_cursor_pos = 0;
        self.title_selection_anchor = None;
        self.title_is_mouse_selecting = false;
        cx.notify();
    }

    /// Renders the chevron-down button for opening the tabs dropdown menu.
    pub fn render_tabs_dropdown_button(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let is_open = self.tabs_popup_is_open();
        div()
            .id("btn-tabs-dropdown")
            .flex()
            .items_center()
            .justify_center()
            .w(px(32.0))
            .h(px(32.0))
            .rounded_md()
            .cursor_pointer()
            .when(is_open, |d| d.bg(theme.bg_hover))
            .hover(|h| h.bg(theme.bg_hover))
            .child(icons::icon(
                icons::ICON_CHEVRON_DOWN,
                14.0,
                if is_open { theme.text_primary } else { theme.text_secondary },
            ))
            .on_click(cx.listener(|this, _, window, cx| {
                if this.tabs_popup_is_open() {
                    this.close_tabs_popup(cx);
                } else {
                    this.open_tabs_popup(window, cx);
                }
            }))
    }

    /// Renders the chevron-down button for opening the tabs dropdown menu for a specific leaf/window.
    pub fn render_leaf_tabs_dropdown_button(
        &self,
        leaf_id: crate::workspace::LeafId,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_tabs_popup_open_for_leaf = self.tabs_popup_leaf == Some(leaf_id);
        div()
            .id(gpui::ElementId::Name(format!("leaf-btn-tabs-dropdown-{}", leaf_id.0).into()))
            .flex()
            .items_center()
            .justify_center()
            .h_full()
            .w(px(32.0))
            .flex_shrink_0()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                if this.tabs_popup_leaf == Some(leaf_id) {
                    this.close_tabs_popup(cx);
                } else {
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.open_leaf_tabs_popup(leaf_id, x, y, window, cx);
                }
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    if let Some((l, _)) = this.hovered_tab {
                        if l == leaf_id {
                            this.hovered_tab = None;
                            cx.notify();
                        }
                    }
                }
            }))
            .child(
                div()
                    .id(gpui::ElementId::Name(format!("leaf-btn-tabs-dropdown-inner-{}", leaf_id.0).into()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(28.0))
                    .h(px(28.0))
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_tabs_popup_open_for_leaf, |d| d.bg(theme.bg_hover))
                    .hover(|h| h.bg(theme.bg_hover))
                    .child(icons::icon(
                        icons::ICON_CHEVRON_DOWN,
                        13.0,
                        if is_tabs_popup_open_for_leaf { theme.text_primary } else { theme.text_muted },
                    )),
            )
    }

    /// Renders the right sidebar toggle button.
    pub fn render_right_sidebar_toggle_button(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let is_open = !self.right_sidebar_collapsed;
        div()
            .id("btn-toggle-right-sidebar")
            .flex()
            .items_center()
            .justify_center()
            .w(px(32.0))
            .h(px(32.0))
            .rounded_md()
            .cursor_pointer()
            .hover(|h| h.bg(theme.bg_hover))
            .child(icons::icon(
                icons::ICON_PANEL_RIGHT,
                15.0,
                if is_open { theme.accent } else { theme.text_secondary },
            ))
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_right_sidebar(cx);
            }))
    }

    /// Renders the native Windows titlebar control buttons (Minimize,
    /// Maximize/Restore, Close). Shared between the editor header (right
    /// sidebar closed) and the right sidebar header column (right sidebar
    /// open), so the buttons always stay glued to the right window edge.
    pub fn render_window_control_buttons(
        &self,
        theme: &Theme,
        is_maximized: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        // Each button claims its own mouse-down. Marking it as a platform
        // control area makes the OS repeat the action on mouse-up
        // (`nc_button_pressed` is recorded only when the non-client
        // mouse-down is NOT handled), so without this the action would run
        // twice: minimize survives it, maximize toggles back, close is
        // delivered twice. Same guard as in the settings window.
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
                    .window_control_area(gpui::WindowControlArea::Min)
                    .hover(|h| h.bg(theme.bg_hover))
                    .child(icons::icon(icons::ICON_MINIMIZE, 12.0, theme.text_secondary))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
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
                    .window_control_area(gpui::WindowControlArea::Max)
                    .hover(|h| h.bg(theme.bg_hover))
                    .child(icons::icon(
                        if is_maximized { icons::ICON_RESTORE } else { icons::ICON_MAXIMIZE },
                        12.0,
                        theme.text_secondary,
                    ))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
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
                    .window_control_area(gpui::WindowControlArea::Close)
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
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.prepare_shutdown(window, cx);
                        cx.quit();
                    }))
            })
    }
}

/// Layout metrics and overflow state for the tab strip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabsLayout {
    pub tab_width: f32,
    pub is_overflowing: bool,
    pub total_tabs_width: f32,
    pub max_scroll: f32,
}

/// Calculates dynamic tab width and whether tabs overflow.
pub fn calculate_tabs_layout(
    num_tabs: usize,
    available_width: f32,
    max_tab_w: f32,
    min_tab_w: f32,
    tab_gap: f32,
    new_tab_btn_w: f32,
) -> TabsLayout {
    if num_tabs == 0 {
        return TabsLayout {
            tab_width: max_tab_w,
            is_overflowing: false,
            total_tabs_width: 0.0,
            max_scroll: 0.0,
        };
    }

    let gaps_total = (num_tabs.saturating_sub(1)) as f32 * tab_gap;
    let total_at_max = num_tabs as f32 * max_tab_w + gaps_total + new_tab_btn_w;

    if total_at_max <= available_width {
        // Case 1: All tabs fit comfortably at max width with the inline + button
        let total_tabs_width = num_tabs as f32 * max_tab_w + gaps_total;
        return TabsLayout {
            tab_width: max_tab_w,
            is_overflowing: false,
            total_tabs_width,
            max_scroll: 0.0,
        };
    }

    // Shrink tabs proportionally without artificial cutoff so the + button never disappears
    let space_for_tabs = (available_width - new_tab_btn_w - gaps_total).max(0.0);
    let candidate_w = space_for_tabs / (num_tabs as f32);
    let tab_w = candidate_w.clamp(min_tab_w, max_tab_w);
    let total_tabs_width = num_tabs as f32 * tab_w + gaps_total;

    TabsLayout {
        tab_width: tab_w,
        is_overflowing: true,
        total_tabs_width,
        max_scroll: 0.0,
    }
}

/// Result of tab widths and gap calculation.
#[derive(Debug, Clone, PartialEq)]
pub struct TabWidthsResult {
    pub widths: Vec<f32>,
    pub is_overflowing: bool,
    pub tab_gap: f32,
}

/// Calculates individual tab widths, gap, and overflow state.
///
/// Features:
/// 1. When overflowing, the last tab's right boundary is locked at a fixed position right before the + button.
/// 2. Active tab maintains a minimum readable size (`min_active_tab_w`), while inactive tabs shrink proportionally.
/// 3. Tab widths and gaps are computed with integer precision so the right boundary never jitters.
///
/// `leading_w` / `leading_gap` reserve room for a button at the very start of the
/// strip (e.g. the new-note button): the tabs region starts after it. Pass 0/0
/// when there is no leading button.
pub fn calculate_tab_widths(
    num_tabs: usize,
    active_tab_idx: Option<usize>,
    available_width: f32,
    max_tab_w: f32,
    min_active_tab_w: f32,
    nominal_tab_gap: f32,
    btn_w: f32,
    btn_gap: f32,
    left_pad: f32,
    leading_w: f32,
    leading_gap: f32,
) -> TabWidthsResult {
    if num_tabs == 0 {
        return TabWidthsResult {
            widths: Vec::new(),
            is_overflowing: false,
            tab_gap: nominal_tab_gap,
        };
    }

    let tabs_start_x = left_pad + leading_w + leading_gap;
    let gaps_total = (num_tabs.saturating_sub(1)) as f32 * nominal_tab_gap;
    let total_at_max = tabs_start_x + num_tabs as f32 * max_tab_w + gaps_total + btn_gap + btn_w;

    if total_at_max <= available_width {
        // Case 1: All tabs fit comfortably at max width
        return TabWidthsResult {
            widths: vec![max_tab_w; num_tabs],
            is_overflowing: false,
            tab_gap: nominal_tab_gap,
        };
    }

    // Case 2: Overflowing. Last tab boundary is fixed right before the + button.
    let btn_x = available_width - btn_w;
    let tabs_end_x = btn_x - btn_gap;
    let w_tabs = (tabs_end_x - tabs_start_x).max(0.0);

    if num_tabs == 1 {
        let single_w = w_tabs.min(max_tab_w);
        return TabWidthsResult {
            widths: vec![single_w],
            is_overflowing: true,
            tab_gap: nominal_tab_gap,
        };
    }

    // Compute effective tab gap. If tab count is huge (e.g. 50-100 tabs),
    // reduce tab gap smoothly down to 1px or 0px so inactive tabs still get room.
    let mut effective_tab_gap = nominal_tab_gap;
    let target_active_w = min_active_tab_w.min(w_tabs);
    let remaining_for_inactive_and_gaps = (w_tabs - target_active_w).max(0.0);
    let min_inactive_tab_reserve = (num_tabs - 1) as f32 * 2.0;
    let max_allowed_gaps = (remaining_for_inactive_and_gaps - min_inactive_tab_reserve).max(0.0);
    let max_gap = (max_allowed_gaps / (num_tabs - 1) as f32).floor().max(0.0);
    if max_gap < effective_tab_gap {
        effective_tab_gap = max_gap;
    }

    let actual_gaps_total = (num_tabs - 1) as f32 * effective_tab_gap;
    let space_for_tab_bodies = (w_tabs - actual_gaps_total).max(0.0);
    let total_tab_pixels = space_for_tab_bodies.round() as i32;

    let equal_tab_w = space_for_tab_bodies / num_tabs as f32;

    let valid_active_idx = active_tab_idx.filter(|&idx| idx < num_tabs);

    if equal_tab_w >= min_active_tab_w || valid_active_idx.is_none() {
        // Equal distribution across all tabs
        let base_tab_w = total_tab_pixels / num_tabs as i32;
        let remainder = (total_tab_pixels - base_tab_w * num_tabs as i32) as usize;

        let mut widths = Vec::with_capacity(num_tabs);
        for idx in 0..num_tabs {
            let w = if idx < remainder {
                base_tab_w + 1
            } else {
                base_tab_w
            } as f32;
            widths.push(w);
        }

        TabWidthsResult {
            widths,
            is_overflowing: true,
            tab_gap: effective_tab_gap,
        }
    } else {
        // Active tab is guaranteed min_active_tab_w; inactive tabs shrink proportionally.
        let act_idx = valid_active_idx.unwrap();
        let active_pixels = (min_active_tab_w as i32).min(total_tab_pixels);
        let inactive_pixels = (total_tab_pixels - active_pixels).max(0);
        let num_inactive = (num_tabs - 1) as i32;

        let base_inactive_w = inactive_pixels / num_inactive;
        let rem_inactive = (inactive_pixels - base_inactive_w * num_inactive) as usize;

        let mut widths = Vec::with_capacity(num_tabs);
        let mut rem_distributed = 0;
        for idx in 0..num_tabs {
            if idx == act_idx {
                widths.push(active_pixels as f32);
            } else {
                let w = if rem_distributed < rem_inactive {
                    rem_distributed += 1;
                    base_inactive_w + 1
                } else {
                    base_inactive_w
                };
                widths.push(w as f32);
            }
        }

        TabWidthsResult {
            widths,
            is_overflowing: true,
            tab_gap: effective_tab_gap,
        }
    }
}

/// Formats the bookmark label with localized pluralization.
pub fn format_bookmark_label(count: usize, lang: &str) -> String {
    let norm = crate::i18n::LocaleInfo::normalize(lang);
    if norm == "ru" || norm == "uk" {
        let suffix = match count % 10 {
            1 if count % 100 != 11 => crate::i18n::t("tabs.add_one_tab", norm),
            2..=4 if !(12..=14).contains(&(count % 100)) => crate::i18n::t("tabs.add_few_tabs", norm),
            _ => crate::i18n::t("tabs.add_many_tabs", norm),
        };
        if count > 0 {
            crate::i18n::t_fmt("tabs.add_n_tabs", norm, &[&count.to_string(), &suffix])
        } else {
            crate::i18n::t("tabs.add_tabs_to_bookmarks", norm)
        }
    } else if count == 1 {
        let unit = crate::i18n::t("tabs.add_one_tab", norm);
        crate::i18n::t_fmt("tabs.add_n_tabs", norm, &["1", &unit])
    } else if count > 1 {
        let unit = crate::i18n::t("tabs.add_many_tabs", norm);
        crate::i18n::t_fmt("tabs.add_n_tabs", norm, &[&count.to_string(), &unit])
    } else {
        crate::i18n::t("tabs.add_tabs_to_bookmarks", norm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bookmark_label() {
        assert_eq!(format_bookmark_label(0, "ru"), "Добавить в закладки N вкладок...");
        assert_eq!(format_bookmark_label(1, "ru"), "Добавить в закладки 1 вкладку...");
        assert_eq!(format_bookmark_label(2, "ru"), "Добавить в закладки 2 вкладки...");
        assert_eq!(format_bookmark_label(4, "ru"), "Добавить в закладки 4 вкладки...");
        assert_eq!(format_bookmark_label(5, "ru"), "Добавить в закладки 5 вкладок...");
        assert_eq!(format_bookmark_label(11, "ru"), "Добавить в закладки 11 вкладок...");
        assert_eq!(format_bookmark_label(12, "ru"), "Добавить в закладки 12 вкладок...");
        assert_eq!(format_bookmark_label(21, "ru"), "Добавить в закладки 21 вкладку...");
        assert_eq!(format_bookmark_label(22, "ru"), "Добавить в закладки 22 вкладки...");
    }

    #[test]
    fn test_calculate_tabs_layout_few_tabs() {
        // 1 tab, wide area -> max width, not overflowing
        let layout = calculate_tabs_layout(1, 800.0, 160.0, 0.0, 6.0, 48.0);
        assert_eq!(layout.tab_width, 160.0);
        assert!(!layout.is_overflowing);
        assert_eq!(layout.max_scroll, 0.0);
    }

    #[test]
    fn test_calculate_tabs_layout_shrunk_tabs() {
        // 4 tabs, 500px -> shrunk between 0 and 160, is_overflowing is true
        let layout = calculate_tabs_layout(4, 500.0, 160.0, 0.0, 6.0, 48.0);
        assert!(layout.tab_width > 0.0 && layout.tab_width <= 160.0);
        assert!(layout.is_overflowing);
        assert_eq!(layout.max_scroll, 0.0);
    }

    #[test]
    fn test_calculate_tabs_layout_overflowing_tabs() {
        // 15 tabs, 500px -> shrinks down without cutoff so + button fits, is_overflowing is true
        let layout = calculate_tabs_layout(15, 500.0, 160.0, 0.0, 6.0, 48.0);
        assert!(layout.tab_width > 0.0 && layout.tab_width < 64.0);
        assert!(layout.is_overflowing);
        assert_eq!(layout.max_scroll, 0.0);
    }

    #[test]
    fn test_calculate_tab_widths_locked_boundary() {
        // Test that for any number of tabs when overflowing, the right boundary is locked at exactly tabs_end_x
        let avail_w = 600.0;
        let btn_w = 28.0;
        let btn_gap = 6.0;
        let left_pad = 8.0;
        let nominal_gap = 6.0;
        let min_active_tab_w = 120.0;
        let expected_tabs_end_x = avail_w - btn_w - btn_gap; // 566.0

        for num_tabs in 4..=100 {
            // Test with active tab in various positions: None, first, middle, last
            let active_cases = vec![None, Some(0), Some(num_tabs / 2), Some(num_tabs - 1)];
            for active_idx in active_cases {
                let res = calculate_tab_widths(
                    num_tabs,
                    active_idx,
                    avail_w,
                    160.0,
                    min_active_tab_w,
                    nominal_gap,
                    btn_w,
                    btn_gap,
                    left_pad,
                    0.0,
                    0.0,
                );
                assert!(res.is_overflowing, "Should be overflowing for {} tabs", num_tabs);
                assert_eq!(res.widths.len(), num_tabs);

                let gaps_total = (num_tabs - 1) as f32 * res.tab_gap;
                let sum_widths: f32 = res.widths.iter().sum();
                let actual_end_x = left_pad + sum_widths + gaps_total;
                assert_eq!(
                    actual_end_x, expected_tabs_end_x,
                    "End boundary must match exactly for {} tabs with active {:?}",
                    num_tabs, active_idx
                );

                                // Verify active tab minimum readable size constraint
                if let Some(act) = active_idx {
                    let w_tabs = expected_tabs_end_x - left_pad;
                    let target_active = min_active_tab_w.min(w_tabs);

                    assert!(
                        res.widths[act] >= target_active,
                        "Active tab {} width {} was less than minimum readable size {} for {} tabs",
                        act, res.widths[act], target_active, num_tabs
                    );
                    if num_tabs > 5 {
                        // Inactive tabs should be smaller than active tab
                        for (i, &w) in res.widths.iter().enumerate() {
                            if i != act {
                                assert!(
                                    w <= res.widths[act],
                                    "Inactive tab {} width {} should be <= active tab width {}",
                                    i, w, res.widths[act]
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_calculate_tab_widths_leading_button() {
        // With a leading button (new-note, 32px + 6px gap) the tabs region starts
        // after it, while the right boundary stays locked before the + button.
        let avail_w = 600.0;
        let (btn_w, btn_gap, left_pad) = (28.0, 6.0, 8.0);
        let (leading_w, leading_gap) = (32.0, 6.0);
        let tabs_start_x = left_pad + leading_w + leading_gap; // 46.0
        let expected_end_x = avail_w - btn_w - btn_gap; // 566.0

        for num_tabs in [1, 2, 4, 10, 40] {
            let res = calculate_tab_widths(
                num_tabs,
                Some(0),
                avail_w,
                160.0,
                120.0,
                6.0,
                btn_w,
                btn_gap,
                left_pad,
                leading_w,
                leading_gap,
            );
            assert_eq!(res.widths.len(), num_tabs);
            let gaps_total = (num_tabs.saturating_sub(1)) as f32 * res.tab_gap;
            let sum_widths: f32 = res.widths.iter().sum();
            if res.is_overflowing {
                // Tabs exactly fill [tabs_start_x, expected_end_x].
                let actual_end_x = tabs_start_x + sum_widths + gaps_total;
                assert_eq!(actual_end_x, expected_end_x, "num_tabs={}", num_tabs);
            } else {
                // Everything at max width incl. the leading button must fit.
                let total = tabs_start_x + sum_widths + gaps_total + btn_gap + btn_w;
                assert!(total <= avail_w, "num_tabs={}", num_tabs);
            }
        }
    }
}
