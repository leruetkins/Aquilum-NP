use super::*;

impl AppView {
    pub(super) fn render_leaf_tab_bar_strip(
        &self,
        leaf_id: crate::workspace::LeafId,
        in_titlebar: bool,
        available_strip_w: f32,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let leaf = self.state.leaves.get(&leaf_id);
        let active_tab_index = leaf.map(|l| l.active_tab_index).unwrap_or(0);
        let num_tabs = leaf.map(|l| l.tabs.len()).unwrap_or(0);

        // Keep the panel strip an exact whole number of device pixels so its bottom
        // border coincides with the tabs' bottom and the active-tab baseline mask.
        let scale = window.scale_factor();
        let strip_h = if scale > 0.0 { (38.0 * scale).round() / scale } else { 38.0 };

        let tab_layout = crate::views::tabs_menu::calculate_tab_widths(
            num_tabs,
            Some(active_tab_index),
            available_strip_w,
            160.0,
            120.0,
            6.0,
            32.0,
            6.0,
            8.0,
            32.0,
            6.0,
        );
        let tab_widths = tab_layout.widths;
        let is_tabs_overflowing = tab_layout.is_overflowing;
        let effective_tab_gap = tab_layout.tab_gap;

        let cap_top = 4.0;
        let sep_top = 13.0;
        let tab_body_h = 34.0;

        let is_leaf_focused = self.state.workspace.active_leaf == leaf_id;
        let hovered_tab = self
            .hovered_tab
            .and_then(|(l, i)| if l == leaf_id && i < num_tabs { Some(i) } else { None });

        let mut tabs_strip = div()
            .id(gpui::ElementId::Name(format!("tabs-strip-inner-{}", leaf_id.0).into()))
            .flex()
            .flex_row()
            .items_end()
            .gap(px(effective_tab_gap))
            .h_full()
            .pl(px(8.0))
            .min_w_0()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if !hovered {
                    if let Some((l, _)) = this.hovered_tab {
                        if l == leaf_id {
                            this.hovered_tab = None;
                            cx.notify();
                        }
                    }
                }
            }));

        let snap = |v: f32| if scale > 0.0 { (v * scale).round() / scale } else { v };
        let div_w = snap(1.0);
        let leading_sep_right = snap(-(effective_tab_gap / 2.0).floor());
        let sep_right = snap(-(effective_tab_gap / 2.0).floor());

        let first_tab_active = active_tab_index == 0;
        let show_leading_divider = num_tabs > 0 && !first_tab_active && hovered_tab != Some(0);
        let divider_color = if theme.is_dark {
            gpui::rgba(0xcdd6f440)
        } else {
            gpui::rgba(0x1e203030)
        };

        let swap_buttons = self.config.ui.swap_tab_bar_buttons;

        let leading_button = if swap_buttons {
            div()
                .id(gpui::ElementId::Name(format!("leaf-btn-new-tab-{}", leaf_id.0).into()))
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .h_full()
                .w(px(32.0))
                .flex_shrink_0()
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(leaf) = this.state.leaves.get_mut(&leaf_id) {
                        let num = leaf.tabs.len();
                        leaf.tabs.insert(0, crate::workspace::Tab::new_empty());
                        if num > 0 {
                            leaf.active_tab_index = leaf.active_tab_index.saturating_add(1);
                        } else {
                            leaf.active_tab_index = 1;
                        }
                        this.select_leaf_tab(leaf_id, 0, window, cx);
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
                .when(show_leading_divider, |d| {
                    d.child(
                        div()
                            .id("new-tab-separator")
                            .absolute()
                            .top(px(sep_top))
                            .h(px(16.0))
                            .w(px(div_w))
                            .right(px(leading_sep_right))
                            .bg(divider_color),
                    )
                })
                .child(
                    div()
                        .id("btn-new-tab-inline-inner")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(28.0))
                        .h(px(28.0))
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover))
                        .child(icons::icon(icons::ICON_PLUS, 15.0, theme.text_muted)),
                )
        } else {
            div()
                .id(gpui::ElementId::Name(format!("leaf-btn-new-note-{}", leaf_id.0).into()))
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .h_full()
                .w(px(32.0))
                .flex_shrink_0()
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.state.workspace.active_leaf = leaf_id;
                    this.create_new_note_at_front(window, cx);
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
                .when(show_leading_divider, |d| {
                    d.child(
                        div()
                            .id("new-note-separator")
                            .absolute()
                            .top(px(sep_top))
                            .h(px(16.0))
                            .w(px(div_w))
                            .right(px(leading_sep_right))
                            .bg(divider_color),
                    )
                })
                .child(
                    div()
                        .id("btn-new-note-inline-inner")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(28.0))
                        .h(px(28.0))
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover))
                        .child(icons::icon(icons::ICON_NOTE_CREATE, 15.0, theme.text_muted)),
                )
        };

        tabs_strip = tabs_strip.child(leading_button);

        if let Some(leaf) = leaf {
            for (tab_idx, tab) in leaf.tabs.iter().enumerate() {
                let is_active = tab_idx == active_tab_index;
                let is_next_active = active_tab_index == tab_idx + 1;
                let is_last_tab = tab_idx + 1 == num_tabs;
                let show_divider = if is_last_tab {
                    !is_active && hovered_tab != Some(tab_idx)
                } else {
                    !is_active
                        && !is_next_active
                        && hovered_tab != Some(tab_idx)
                        && hovered_tab != Some(tab_idx + 1)
                };

                let (title, is_dirty, tab_path) = match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        if let Some(doc) = self.state.documents.get(id) {
                            (doc.title.clone(), doc.is_dirty, Some(doc.path.clone()))
                        } else if let Some(meta) = self.state.doc_meta.get(id) {
                            (meta.title.clone(), meta.is_dirty, Some(meta.path.clone()))
                        } else {
                            (self.t("common.untitled"), false, None)
                        }
                    }
                    crate::workspace::TabContent::Empty => {
                        (self.t("tabs.newTab"), false, Some(std::path::PathBuf::from("newtab")))
                    }
                    crate::workspace::TabContent::Plugin { view_id } => {
                        (view_id.clone(), false, Some(std::path::PathBuf::from(format!("plugin:{}", view_id))))
                    }
                };

                let tab_w = tab_widths.get(tab_idx).copied().unwrap_or(160.0);
                let tab_path_context = tab_path.clone();

                let is_focused_tab = is_active && is_leaf_focused;

                let mut tab_item = div()
                    .id(gpui::ElementId::Name(format!("leaf-tab-item-{}-{}", leaf_id.0, tab_idx).into()))
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .w(px(tab_w))
                    .flex_shrink_0()
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered && this.hovered_tab != Some((leaf_id, tab_idx)) {
                            this.hovered_tab = Some((leaf_id, tab_idx));
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_leaf_tab(leaf_id, tab_idx, window, cx);
                    }));

                if let Some(p) = tab_path_context {
                    tab_item = tab_item.on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.open_tab_context_menu(p.clone(), Some(leaf_id), Some(tab_idx), x, y, cx);
                    }));
                }

                let ext_r = (effective_tab_gap / 2.0).ceil();
                let ext_l = (effective_tab_gap - ext_r).max(0.0);

                tab_item = tab_item.when(!is_active, |tab| {
                    let tab = tab
                        .bg(theme.bg_sidebar)
                        .rounded_t_md()
                        .border_b_1()
                        .border_color(theme.border);
                    if hovered_tab == Some(tab_idx) {
                        tab.child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left_0()
                                .right_0()
                                .h(px(tab_body_h))
                                .rounded_t_md()
                                .bg(theme.bg_hover),
                        )
                    } else {
                        tab
                    }
                });

                if show_divider {
                    tab_item = tab_item.child(
                        div()
                            .id(gpui::ElementId::Name(format!("leaf-tab-separator-{}-{}", leaf_id.0, tab_idx).into()))
                            .absolute()
                            .top(px(sep_top))
                            .h(px(16.0))
                            .w(px(div_w))
                            .right(px(sep_right))
                            .bg(divider_color),
                    );
                }

                tab_item = tab_item
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("leaf-tab-hit-l-{}-{}", leaf_id.0, tab_idx).into()))
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(-ext_l))
                            .w(px(ext_l))
                            .cursor_pointer()
                            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                if *hovered && this.hovered_tab != Some((leaf_id, tab_idx)) {
                                    this.hovered_tab = Some((leaf_id, tab_idx));
                                    cx.notify();
                                }
                            }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_leaf_tab(leaf_id, tab_idx, window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("leaf-tab-hit-r-{}-{}", leaf_id.0, tab_idx).into()))
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right(px(-ext_r))
                            .w(px(ext_r))
                            .cursor_pointer()
                            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                if *hovered && this.hovered_tab != Some((leaf_id, tab_idx)) {
                                    this.hovered_tab = Some((leaf_id, tab_idx));
                                    cx.notify();
                                }
                            }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_leaf_tab(leaf_id, tab_idx, window, cx);
                            })),
                    );

                if is_active {
                    // 1. Solid center body fill connecting to active editor
                    tab_item = tab_item.child(
                        div()
                            .id("tab-body-fill")
                            .absolute()
                            .top(px(cap_top))
                            .bottom_0()
                            .left(px(6.0))
                            .right(px(6.0))
                            .bg(theme.bg_editor),
                    );
                    // 2. Solid fill mask at the baseline connecting active tab into the editor
                    tab_item = tab_item.child(
                        div()
                            .id("tab-baseline-mask")
                            .absolute()
                            .bottom(px(-1.0))
                            .left(px(-8.0))
                            .right(px(-8.0))
                            .h(px(2.0))
                            .bg(theme.bg_editor),
                    );
                    // 3. Top border stroke
                    tab_item = tab_item.child(
                        div()
                            .id("tab-top-stroke")
                            .absolute()
                            .top(px(cap_top))
                            .left(px(7.0))
                            .right(px(7.0))
                            .h(px(1.0))
                            .bg(theme.border),
                    );
                    // 4. Left cap
                    tab_item = tab_item.child(
                        div()
                            .id("tab-cap-left")
                            .absolute()
                            .top(px(cap_top))
                            .left(px(-8.0))
                            .w(px(16.0))
                            .h(px(36.0))
                            .child(
                                icons::svg_sized(icons::ICON_TAB_ACTIVE_LEFT_FILL, 16.0, 36.0, theme.bg_editor)
                                    .absolute()
                                    .top_0()
                                    .left_0(),
                            )
                            .child(
                                icons::svg_sized(icons::ICON_TAB_ACTIVE_LEFT_STROKE, 16.0, 36.0, theme.border)
                                    .absolute()
                                    .top_0()
                                    .left_0(),
                            ),
                    );
                    // 5. Right cap
                    tab_item = tab_item.child(
                        div()
                            .id("tab-cap-right")
                            .absolute()
                            .top(px(cap_top))
                            .right(px(-8.0))
                            .w(px(16.0))
                            .h(px(36.0))
                            .child(
                                icons::svg_sized(icons::ICON_TAB_ACTIVE_RIGHT_FILL, 16.0, 36.0, theme.bg_editor)
                                    .absolute()
                                    .top_0()
                                    .left_0(),
                            )
                            .child(
                                icons::svg_sized(icons::ICON_TAB_ACTIVE_RIGHT_STROKE, 16.0, 36.0, theme.border)
                                    .absolute()
                                    .top_0()
                                    .left_0(),
                            ),
                    );
                }

                tabs_strip = tabs_strip.child(
                    tab_item
                        // Tab selection area (icon + title + dirty dot)
                        .child(
                            div()
                                .id(gpui::ElementId::Name(
                                    format!("leaf-tab-select-{}-{}", leaf_id.0, tab_idx).into(),
                                 ))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(if tab_w < 48.0 { px(2.0) } else { px(6.0) })
                                .h_full()
                                .pl(if tab_w < 40.0 { px(2.0) } else { px(10.0) })
                                .pr(if tab_w < 40.0 { px(2.0) } else { px(4.0) })
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .cursor_pointer()
                                .child(icons::icon(
                                    icons::ICON_FILE_TEXT,
                                    12.0,
                                    if is_focused_tab {
                                        theme.accent
                                    } else if is_active {
                                        theme.text_secondary
                                    } else {
                                        theme.text_muted
                                    },
                                ))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(if is_focused_tab {
                                            gpui::FontWeight::SEMIBOLD
                                        } else if is_active {
                                            gpui::FontWeight::MEDIUM
                                        } else {
                                            gpui::FontWeight::NORMAL
                                        })
                                        .text_color(if is_focused_tab {
                                            theme.text_primary
                                        } else if is_active {
                                            theme.text_secondary
                                        } else {
                                            theme.text_muted
                                        })
                                        .truncate()
                                        .flex_1()
                                        .min_w_0()
                                        .child(title),
                                )
                                .when(is_active && is_dirty, |tab| {
                                    tab.child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.accent)
                                            .child("●"),
                                    )
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_leaf_tab(leaf_id, tab_idx, window, cx);
                                }))
                                .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.close_leaf_tab(leaf_id, tab_idx, window, cx);
                                })),
                        )
                        // Tab close button (isolated sibling)
                        .when(is_active || tab_w >= 56.0, |tab| {
                            tab.child(
                                div()
                                    .id(gpui::ElementId::Name(
                                        format!("leaf-tab-close-{}-{}", leaf_id.0, tab_idx).into(),
                                    ))
                                    .p_0p5()
                                    .mr_1p5()
                                    .flex_shrink_0()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_CLOSE, 10.0, theme.text_muted))
                                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.close_leaf_tab(leaf_id, tab_idx, window, cx);
                                    })),
                            )
                        }),
                );
            }
        }

        let trailing_button = if swap_buttons {
            div()
                .id(gpui::ElementId::Name(format!("leaf-btn-new-note-{}", leaf_id.0).into()))
                .flex()
                .items_center()
                .justify_center()
                .h_full()
                .w(px(32.0))
                .flex_shrink_0()
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.state.workspace.active_leaf = leaf_id;
                    this.create_new_note(window, cx);
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
                        .id("btn-new-note-inline-inner")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(28.0))
                        .h(px(28.0))
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover))
                        .child(icons::icon(icons::ICON_NOTE_CREATE, 15.0, theme.text_muted)),
                )
        } else {
            div()
                .id(gpui::ElementId::Name(format!("leaf-btn-new-tab-{}", leaf_id.0).into()))
                .flex()
                .items_center()
                .justify_center()
                .h_full()
                .w(px(32.0))
                .flex_shrink_0()
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(leaf) = this.state.leaves.get_mut(&leaf_id) {
                        leaf.tabs.push(crate::workspace::Tab::new_empty());
                        let new_idx = leaf.tabs.len() - 1;
                        this.select_leaf_tab(leaf_id, new_idx, window, cx);
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
                        .id("btn-new-tab-inline-inner")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(28.0))
                        .h(px(28.0))
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover))
                        .child(icons::icon(icons::ICON_PLUS, 15.0, theme.text_muted)),
                )
        };

        if in_titlebar {
            div()
                .id(gpui::ElementId::Name(format!("leaf-tab-strip-titlebar-{}", leaf_id.0).into()))
                .relative()
                .flex()
                .flex_row()
                .items_end()
                .gap(px(6.0))
                .min_w_0()
                .max_w(px(available_strip_w))
                .overflow_hidden()
                .when(is_tabs_overflowing, |d| d.w(px(available_strip_w)))
                .h_full()
                .child(tabs_strip)
                .child(trailing_button)
        } else {
            div()
                .id(gpui::ElementId::Name(format!("leaf-tab-strip-panel-{}", leaf_id.0).into()))
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .h(px(strip_h))
                .w_full()
                .min_w_0()
                .bg(theme.bg_sidebar)
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("leaf-panel-bottom-border-{}", leaf_id.0).into()))
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(1.0))
                        .bg(theme.border),
                )
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("leaf-tab-strip-panel-inner-{}", leaf_id.0).into()))
                        .relative()
                        .flex()
                        .flex_row()
                        .items_end()
                        .gap(px(6.0))
                        .min_w_0()
                        .max_w(px(available_strip_w))
                        .overflow_hidden()
                        .when(is_tabs_overflowing, |d| d.w(px(available_strip_w)))
                        .h_full()
                        .child(tabs_strip)
                        .child(trailing_button),
                )
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("leaf-panel-spacer-{}", leaf_id.0).into()))
                        .flex_1()
                        .min_w_0()
                        .h_full(),
                )
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("leaf-panel-tabs-btn-wrap-{}", leaf_id.0).into()))
                        .flex()
                        .items_center()
                        .h_full()
                        .flex_shrink_0()
                        .px_1()
                        .child(self.render_leaf_tabs_dropdown_button(leaf_id, theme, cx)),
                )
        }
    }

    pub(super) fn render_titlebar_editor_header(
        &self,
        is_maximized: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut header = div()
            .id("titlebar-editor-header-col")
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .flex_1()
            .h_full()
            .min_w_0()
            .overflow_hidden()
            .bg(theme.bg_sidebar)
            // Continuous bottom border line running across the editor header column
            .child(
                div()
                    .id("titlebar-editor-bottom-border")
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(1.0))
                    .bg(theme.border),
            );

        let top_cols = self.state.workspace.top_columns();
        let num_cols = top_cols.len();

        let viewport_w: f32 = window.viewport_size().width.into();
        let sidebar_area_w = if self.sidebar_collapsed {
            44.0
        } else {
            self.sidebar_width + 1.0
        };
        let right_sidebar_area_w = if self.right_sidebar_collapsed {
            0.0
        } else {
            self.right_sidebar_width + 1.0
        };
        let header_w = (viewport_w - sidebar_area_w - right_sidebar_area_w).max(100.0);
        let right_controls_w = if self.right_sidebar_collapsed { 178.0 } else { 40.0 };

        for (col_idx, col) in top_cols.into_iter().enumerate() {
            let is_last = col_idx + 1 == num_cols;
            let col_w = (header_w * col.ratio).max(60.0);
            let available_strip_w = if is_last {
                (col_w - right_controls_w - 40.0).max(40.0)
            } else {
                (col_w - 40.0).max(40.0)
            };

            let mut col_div = div()
                .id(gpui::ElementId::from(("titlebar-top-col", col_idx)))
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .min_w_0()
                .overflow_hidden();

            if is_last {
                col_div = col_div.flex_1().min_w_0();
            } else {
                col_div = col_div.w(gpui::relative(col.ratio)).flex_shrink_0();
            }

            // Tab bar strip for this top column leaf
            col_div = col_div.child(
                self.render_leaf_tab_bar_strip(col.leaf_id, true, available_strip_w, theme, window, cx)
            );

            // Drag area filling empty space in this top column
            col_div = col_div.child(
                div()
                    .id(gpui::ElementId::from(("titlebar-col-drag", col_idx)))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .window_control_area(WindowControlArea::Drag),
            );

            // Open tabs dropdown button for this leaf on the right side of the window, closer to divider
            col_div = col_div.child(
                div()
                    .id(gpui::ElementId::from(("titlebar-col-tabs-btn-wrap", col_idx)))
                    .flex()
                    .items_center()
                    .h_full()
                    .flex_shrink_0()
                    .px_1()
                    .child(self.render_leaf_tabs_dropdown_button(col.leaf_id, theme, cx)),
            );

            // On the rightmost column: anchor the window controls
            if is_last {
                col_div = col_div.child(
                    div()
                        .id("titlebar-right-controls-group")
                        .flex()
                        .flex_row()
                        .items_center()
                        .h_full()
                        .flex_shrink_0()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .h_full()
                                .flex_shrink_0()
                                .gap_1()
                                .px_1()
                                .child(self.render_right_sidebar_toggle_button(theme, cx)),
                        )
                        .when(self.right_sidebar_collapsed, |d| {
                            d.child(self.render_window_control_buttons(theme, is_maximized, cx))
                        }),
                );
            }

            header = header.child(col_div);

            // Draggable split divider after this column if present
            if let Some(ref div_info) = col.divider_after {
                let div_path = div_info.path.clone();
                let div_idx = div_info.divider_index;
                let init_ratio = div_info.initial_ratio;

                let is_active = self.split_drag_state.as_ref().map(|d| {
                    d.direction == crate::workspace::SplitDirection::Row
                        && d.path == div_path
                        && d.divider_index == div_idx
                }).unwrap_or(false);

                let is_hovered = self.hovered_split_divider.as_ref().map(|(dir, path, idx)| {
                    *dir == crate::workspace::SplitDirection::Row
                        && *path == div_path
                        && *idx == div_idx
                }).unwrap_or(false);

                let is_resizing_or_hovered = is_active || is_hovered;
                let divider_color = if is_resizing_or_hovered { theme.accent } else { theme.border };

                header = header.child(
                    div()
                        .id(gpui::ElementId::from(("titlebar-split-divider", col_idx)))
                        .relative()
                        .w(px(1.0))
                        .h_full()
                        .flex_shrink_0()
                        .bg(divider_color)
                        .child(
                            div()
                                .id(gpui::ElementId::from(("titlebar-split-divider-hitbox", col_idx)))
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(px(-4.0))
                                .w(px(9.0))
                                .cursor(gpui::CursorStyle::ResizeColumn)
                                .on_hover(cx.listener({
                                    let div_path = div_path.clone();
                                    move |this, hovered: &bool, _, cx| {
                                        this.hovered_split_divider = if *hovered && !this.is_dragging_scrollbar {
                                            Some((crate::workspace::SplitDirection::Row, div_path.clone(), div_idx))
                                        } else {
                                            None
                                        };
                                        cx.notify();
                                    }
                                }))
                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    this.is_mouse_selecting = false;
                                    this.is_dragging_scrollbar = false;
                                    this.is_hovering_editor_scrollbar = false;
                                    this.is_hovering_editor_thumb = false;
                                    let top_y = this.current_scroll_top_f64();
                                    if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                                        view.viewport.offset = view.line_at_doc_y(doc.line_index.lines(), top_y);
                                    }
                                    let editor_left_w = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                                    let editor_right_w = if this.right_sidebar_collapsed { 0.0 } else { this.right_sidebar_width + 1.0 };
                                    let win_w: f32 = window.bounds().size.width.into();
                                    let span = (win_w - editor_left_w - editor_right_w).max(100.0);
                                    this.split_drag_state = Some(crate::workspace::SplitDragState {
                                        path: div_path.clone(),
                                        direction: crate::workspace::SplitDirection::Row,
                                        divider_index: div_idx,
                                        start_pos: event.position.x.into(),
                                        span,
                                        initial_ratio: init_ratio,
                                    });
                                    cx.notify();
                                })),
                        ),
                );
            }
        }

        header
    }

    pub(super) fn render_workspace_node(
        &self,
        node: &crate::workspace::WorkspaceNode,
        tree_path: &[usize],
        avail_w: f32,
        avail_h: f32,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        match node {
            crate::workspace::WorkspaceNode::Split { direction, children, ratios } => {
                let is_row = *direction == crate::workspace::SplitDirection::Row;
                let mut container = div()
                    .id(gpui::ElementId::from(("split-container", tree_path.first().copied().unwrap_or(0))))
                    .flex()
                    .flex_1()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden();

                if is_row {
                    container = container.flex_row();
                } else {
                    container = container.flex_col();
                }

                // Snap pane boundaries and the divider to whole physical pixels so the
                // pane's tab strip bottom (and every border on it) lands on an exact
                // device-pixel row at any split ratio / fractional DPI.
                let scale = window.scale_factor();
                let snap = |v: f32| if scale > 0.0 { (v * scale).round() / scale } else { v };
                let div_len = snap(1.0);
                let main_avail = if is_row { avail_w } else { avail_h };
                let cross_avail = if is_row { avail_h } else { avail_w };
                let mut acc: f32 = 0.0;

                for (idx, child) in children.iter().enumerate() {
                    if idx > 0 {
                        acc = snap(acc + div_len);
                        let div_idx = idx - 1;
                        let div_path = tree_path.to_vec();
                        let dir = *direction;
                        let init_ratio = ratios.get(div_idx).copied().unwrap_or(0.5);

                        let is_active = self.split_drag_state.as_ref().map(|d| {
                            d.direction == dir && d.path == div_path && d.divider_index == div_idx
                        }).unwrap_or(false);

                        let is_hovered = self.hovered_split_divider.as_ref().map(|(d, path, idx)| {
                            *d == dir && *path == div_path && *idx == div_idx
                        }).unwrap_or(false);

                        let is_resizing_or_hovered = is_active || is_hovered;
                        let divider_color = if is_resizing_or_hovered { theme.accent } else { theme.border };

                        let mut divider_el = div()
                            .id(gpui::ElementId::from(("split-divider", div_idx)))
                            .relative()
                            .flex_shrink_0()
                            .bg(divider_color);

                        if is_row {
                            divider_el = divider_el
                                .w(px(div_len))
                                .h_full()
                                .child(
                                    div()
                                        .id(gpui::ElementId::from(("split-divider-hitbox-r", div_idx)))
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(px(-4.0))
                                        .w(px(9.0))
                                        .cursor(gpui::CursorStyle::ResizeColumn)
                                        .on_hover(cx.listener({
                                            let div_path = div_path.clone();
                                            move |this, hovered: &bool, _, cx| {
                                                this.hovered_split_divider = if *hovered && !this.is_dragging_scrollbar {
                                                    Some((dir, div_path.clone(), div_idx))
                                                } else {
                                                    None
                                                };
                                                cx.notify();
                                            }
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            this.is_mouse_selecting = false;
                                            this.is_dragging_scrollbar = false;
                                            this.is_hovering_editor_scrollbar = false;
                                            this.is_hovering_editor_thumb = false;
                                            let top_y = this.current_scroll_top_f64();
                                            if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                                                view.viewport.offset = view.line_at_doc_y(doc.line_index.lines(), top_y);
                                            }
                                            let editor_left_w = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                                            let editor_right_w = if this.right_sidebar_collapsed { 0.0 } else { this.right_sidebar_width + 1.0 };
                                            let win_w: f32 = window.bounds().size.width.into();
                                            let span = (win_w - editor_left_w - editor_right_w).max(100.0);
                                            this.split_drag_state = Some(crate::workspace::SplitDragState {
                                                path: div_path.clone(),
                                                direction: dir,
                                                divider_index: div_idx,
                                                start_pos: event.position.x.into(),
                                                span,
                                                initial_ratio: init_ratio,
                                            });
                                            cx.notify();
                                        }))
                                );
                        } else {
                            divider_el = divider_el
                                .h(px(div_len))
                                .w_full()
                                .child(
                                    div()
                                        .id(gpui::ElementId::from(("split-divider-hitbox-c", div_idx)))
                                        .absolute()
                                        .left_0()
                                        .right_0()
                                        .top(px(-4.0))
                                        .h(px(9.0))
                                        .cursor(gpui::CursorStyle::ResizeRow)
                                        .on_hover(cx.listener({
                                            let div_path = div_path.clone();
                                            move |this, hovered: &bool, _, cx| {
                                                this.hovered_split_divider = if *hovered && !this.is_dragging_scrollbar {
                                                    Some((dir, div_path.clone(), div_idx))
                                                } else {
                                                    None
                                                };
                                                cx.notify();
                                            }
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            this.is_mouse_selecting = false;
                                            this.is_dragging_scrollbar = false;
                                            this.is_hovering_editor_scrollbar = false;
                                            this.is_hovering_editor_thumb = false;
                                            let top_y = this.current_scroll_top_f64();
                                            if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                                                view.viewport.offset = view.line_at_doc_y(doc.line_index.lines(), top_y);
                                            }
                                            let win_h: f32 = window.bounds().size.height.into();
                                            let span = (win_h - 66.0).max(100.0);
                                            this.split_drag_state = Some(crate::workspace::SplitDragState {
                                                path: div_path.clone(),
                                                direction: dir,
                                                divider_index: div_idx,
                                                start_pos: event.position.y.into(),
                                                span,
                                                initial_ratio: init_ratio,
                                            });
                                            cx.notify();
                                        }))
                                );
                        }
                        container = container.child(divider_el);
                    }

                    let ratio = ratios.get(idx).copied().unwrap_or(1.0 / children.len().max(1) as f32);
                    let is_last = idx + 1 == children.len();
                    // Non-last panes get an exact device-snapped extent; the last pane
                    // fills the rest, so its (already snapped) top stays pixel-aligned.
                    let main_size = if is_last {
                        (snap(main_avail) - acc).max(0.0)
                    } else {
                        (snap(acc + ratio * main_avail) - acc).max(0.0)
                    };
                    let (cell_w, cell_h) = if is_row {
                        (main_size, cross_avail)
                    } else {
                        (cross_avail, main_size)
                    };

                    let mut child_path = tree_path.to_vec();
                    child_path.push(idx);

                    let child_rendered = self.render_workspace_node(child, &child_path, cell_w, cell_h, theme, window, cx);
                    let mut pane_wrapper = div()
                        .flex()
                        .min_w_0()
                        .min_h_0()
                        .overflow_hidden();

                    if is_row {
                        if is_last {
                            pane_wrapper = pane_wrapper.flex_1().min_w_0().h_full();
                        } else {
                            pane_wrapper = pane_wrapper.w(px(main_size)).flex_shrink_0().h_full();
                        }
                    } else {
                        if is_last {
                            pane_wrapper = pane_wrapper.flex_1().min_h_0().w_full();
                        } else {
                            pane_wrapper = pane_wrapper.h(px(main_size)).flex_shrink_0().w_full();
                        }
                    }

                    container = container.child(pane_wrapper.child(child_rendered));
                    acc += main_size;
                }

                container
            }
            crate::workspace::WorkspaceNode::Leaf { id } => {
                self.render_workspace_leaf(*id, theme, window, cx)
            }
        }
    }

    pub(super) fn render_workspace_leaf(
        &self,
        leaf_id: crate::workspace::LeafId,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let is_active = self.state.workspace.active_leaf == leaf_id;
        let in_titlebar = self.state.workspace.is_leaf_in_titlebar(leaf_id);

        let mut leaf_container = div()
            .id(gpui::ElementId::Name(format!("workspace-leaf-{}", leaf_id.0).into()))
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(theme.bg_editor);

        if !in_titlebar {
            let viewport_w: f32 = window.viewport_size().width.into();
            let sidebar_area_w = if self.sidebar_collapsed {
                44.0
            } else {
                self.sidebar_width + 1.0
            };
            let right_sidebar_area_w = if self.right_sidebar_collapsed {
                0.0
            } else {
                self.right_sidebar_width + 1.0
            };
            let workspace_w = (viewport_w - sidebar_area_w - right_sidebar_area_w).max(100.0);
            let panel_w = (workspace_w * self.state.workspace.horizontal_width_ratio_of_leaf(leaf_id)).max(60.0);
            let available_strip_w = (panel_w - 40.0).max(40.0);
            leaf_container = leaf_container.child(self.render_leaf_tab_bar_strip(leaf_id, false, available_strip_w, theme, window, cx));
        }

        if is_active {
            leaf_container.child(
                self.render_central_editor(None, None, true, Some(leaf_id), theme, window, cx)
            )
        } else {
            let active_tab = self.state.leaves.get(&leaf_id).and_then(|l| l.active_tab());
            if let Some(tab) = active_tab {
                match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        if let Some(doc) = self.state.documents.get(id) {
                            let view = self.state.views.get(&tab.id);
                            leaf_container.child(
                                self.render_central_editor(view, Some(doc), false, Some(leaf_id), theme, window, cx)
                            )
                        } else {
                            leaf_container.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size_full()
                                    .text_color(theme.text_muted)
                                    .child("Document not found")
                            )
                        }
                    }
                    crate::workspace::TabContent::Empty => {
                        leaf_container.child(
                            self.render_central_editor(None, None, false, Some(leaf_id), theme, window, cx)
                        )
                    }
                    crate::workspace::TabContent::Plugin { view_id } => {
                        if let Some(leaf) = self.plugin_leaf_views.get(view_id) {
                            leaf_container.child(
                                div()
                                    .id(gpui::ElementId::Name(format!("leaf-plugin-view-{}", leaf_id.0).into()))
                                    .flex()
                                    .flex_col()
                                    .size_full()
                                    .p(px(16.0))
                                    .overflow_scroll()
                                    .child(crate::plugins::ui::render_ui_node(
                                        &leaf.tree,
                                        &leaf.plugin_id,
                                        0,
                                        theme,
                                        self.active_plugin_input.as_ref(),
                                        &self.plugin_input_focus_handle,
                                        self.active_plugin_select.as_ref(),
                                        self.plugin_select_popup.as_ref(),
                                        self.plugin_input_blink.visible(),
                                        window,
                                        cx,
                                        None,
                                    ))
                            )
                        } else {
                            leaf_container
                        }
                    }
                }
            } else {
                leaf_container.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_full()
                        .text_color(theme.text_muted)
                        .child("No open tabs")
                )
            }
        }
    }
}
