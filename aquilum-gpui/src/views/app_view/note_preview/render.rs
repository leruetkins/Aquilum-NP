use super::*;

impl AppView {
    /// Selection rectangles of one visual row, in row-local pixels. Tables
    /// keep their cell selection in the editor; the card paints the plain
    /// text flow.
    fn selection_rects(
        selection: &Range<usize>,
        line: &str,
        line_start: usize,
        line_end: usize,
        v: &VisualRow,
        row_layout: &super::super::RowShapedLayout,
        pixel_scale: f32,
    ) -> Vec<(f32, f32, f32)> {
        if selection.start >= selection.end {
            return Vec::new();
        }
        if line.is_empty() {
            return if selection.start <= line_start && selection.end > line_start {
                row_layout
                    .span_rects(0, 1, pixel_scale)
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            };
        }

        let mut byte_s = selection.start.saturating_sub(line_start).min(line.len());
        let mut byte_e = selection.end.saturating_sub(line_start).min(line.len());
        while byte_s > 0 && !line.is_char_boundary(byte_s) {
            byte_s -= 1;
        }
        while byte_e > 0 && !line.is_char_boundary(byte_e) {
            byte_e -= 1;
        }
        let char_s = line[..byte_s].chars().count();
        let char_e = char_s + line[byte_s..byte_e].chars().count();
        let includes_newline =
            v.is_last_sub && selection.end > line_end && selection.start <= line_end;

        if byte_s < byte_e {
            let intersect_s = char_s.max(v.char_start);
            let intersect_e = char_e.min(v.char_end);
            if intersect_s > intersect_e {
                return Vec::new();
            }
            let vis_s = row_layout.column_for_line_char(intersect_s);
            let mut vis_e = row_layout.column_for_line_char(intersect_e);
            if includes_newline {
                vis_e += 1;
            }
            if vis_s < vis_e {
                row_layout
                    .span_rects(vis_s, vis_e, pixel_scale)
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            }
        } else if includes_newline && char_s == v.char_end {
            let vis_s = row_layout.column_for_line_char(v.char_end);
            row_layout
                .span_rects(vis_s, vis_s + 1, pixel_scale)
                .into_iter()
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn render_note_preview(
        &self,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let Some(state) = self.note_preview.as_ref() else {
            return div().into_any_element();
        };
        let header_title = match state.section.as_deref() {
            Some(section) => format!("{} · {}", state.title, section),
            None => state.title.clone(),
        };
        let layout = preview_layout(window);
        let typography = self.editor_typography();
        let editor_font = self.editor_font_family();
        let line_char_width = self.editor_char_width(cx);
        let pixel_scale = window.scale_factor();
        let selection = state.view.selection_range(state.doc.len());
        let lines_arc = &state.view.window.lines;
        let visual_rows = &state.view.window.visual_rows;
        let code_states = &state.view.window.code_states;
        let height_index = &state.view.window.height_index;

        let guide_columns = crate::markdown::list_block_guide_columns(
            lines_arc
                .as_slice()
                .iter()
                .enumerate()
                .map(|(idx, line)| {
                    code_states
                        .get(lines_arc.start() + idx)
                        .map_or(true, |state| state.is_none())
                        .then_some(line.as_str())
                }),
        );
        let callout_blocks = crate::markdown::callout_blocks(
            lines_arc.as_slice(),
            code_states.as_slice(),
        );
        let heading_widths = Self::resolve_heading_char_widths(
            cx.text_system(),
            &editor_font,
            &typography,
            line_char_width,
        );
        let tables = state
            .doc
            .ensure_table_index_for_window(state.view.window.start_line, state.view.window.end_line);

        let scroll = state.scroll_top;
        let local_scroll = scroll - state.view.above_h;
        let visible = height_index.visible_range(
            local_scroll,
            f64::from(layout.body_h),
            f64::from(EDITOR_OVERSCAN_PX),
        );
        let shift = height_index.top_of(visible.start) - local_scroll;
        let shift = crate::snap::snap_x(shift as f32, pixel_scale) as f32;

        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        for v_idx in visible {
            let Some(v) = visual_rows.get(v_idx) else {
                continue;
            };
            let Some(line) = lines_arc.get(v.line_idx).map(|s| s.as_str()) else {
                continue;
            };
            let line_start = state.doc.line_start_byte(v.line_idx);
            let line_end = line_start + line.len();
            let row_h = height_index.height_of(v_idx).max(1.0);
            let code_state = code_states
                .get(v.line_idx)
                .and_then(|s| s.as_ref())
                .map(|s| (s.kind, s.lang.as_deref()));
            let classified = crate::markdown::classify_line(line, code_state);
            let (_, row_char_w, _, _, _) = Self::heading_typography_metrics_with_widths(
                &classified,
                &typography,
                line_char_width,
                Some(&heading_widths),
            );
            let grid_tbl =
                state
                    .view
                    .grid_table_at(&state.doc, &tables, v.line_idx, usize::MAX, true);
            let table_layout = grid_tbl.map(|tbl| {
                Self::get_or_compute_table_layout(
                    &state.table_layouts,
                    window.text_system(),
                    &|idx| lines_arc.get(idx).map(|s| s.as_str()),
                    tbl,
                    &editor_font,
                    None,
                    line_char_width,
                    Self::table_room_px(&state.view, line_char_width),
                    &typography,
                )
            });
            let edge_top = grid_tbl
                .filter(|tbl| v.line_idx == tbl.first_line)
                .map(|_| crate::markdown::TABLE_EDGE_MARGIN)
                .unwrap_or(0.0);
            let row_layout = if let Some(tbl) = grid_tbl {
                shape_table_row(
                    window.text_system(),
                    line,
                    tbl,
                    v.line_idx,
                    None,
                    table_layout.as_ref().unwrap().as_ref(),
                    &typography.headings,
                    &editor_font,
                    row_char_w,
                )
            } else {
                shape_row(
                    window.text_system(),
                    line,
                    &classified,
                    v,
                    false,
                    &InlineReveal::None,
                    &typography,
                    &editor_font,
                    row_char_w,
                )
            };

            let callout = callout_blocks
                .get(v.line_idx.wrapping_sub(lines_arc.start()))
                .copied()
                .flatten();
            let guide = v
                .line_idx
                .checked_sub(lines_arc.start())
                .and_then(|offset| guide_columns.get(offset))
                .map(|columns| columns.as_slice())
                .unwrap_or(&[]);
            let is_opening_fence =
                matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: true, .. });
            let is_closing_fence =
                matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: false, .. });
            let is_code_line = matches!(classified, crate::markdown::LineKind::CodeLine(_));
            let callout_first = callout.map_or(false, |block| block.first) && v.sub_idx == 0;
            let callout_last = callout.map_or(false, |block| block.last) && v.is_last_sub;

            let reference = crate::images::parse_image_reference(line);
            let image_block = if reference.is_some()
                && v.sub_idx == 0
                && v.is_last_sub
                && grid_tbl.is_none()
            {
                crate::images::resolve_for_note(
                    &state.doc.path,
                    line,
                    state.view.window.wrap_cols,
                    false,
                )
            } else {
                None
            };
            let table_render_arg = grid_tbl.map(|tbl| {
                (
                    tbl,
                    table_layout.as_ref().unwrap().as_ref(),
                    None,
                    v.line_idx,
                    row_h,
                )
            });

            let chip_cell_h = if grid_tbl.is_some() {
                typography.table_line_height
            } else {
                row_h
            };
            let chip_inset = super::super::paint::code_chip_inset(chip_cell_h);
            let mut backdrops: Vec<gpui::AnyElement> = Vec::new();
            for &(code_start, code_end) in row_layout.code_ranges().iter() {
                for (chip_x, chip_w, line_top) in
                    row_layout.span_rects(code_start, code_end, pixel_scale)
                {
                    backdrops.push(
                        div()
                            .absolute()
                            .left(px(chip_x))
                            .top(px(edge_top + line_top + chip_inset))
                            .h(px((chip_cell_h - 2.0 * chip_inset).max(1.0)))
                            .w(px(chip_w))
                            .bg(theme.code_bg)
                            .rounded_sm()
                            .into_any_element(),
                    );
                }
            }
            if grid_tbl.is_none() && image_block.is_none() {
                if let Some(sel) = selection.as_ref() {
                    for (x, w, top) in Self::selection_rects(
                        sel,
                        line,
                        line_start,
                        line_end,
                        v,
                        &row_layout,
                        pixel_scale,
                    ) {
                        backdrops.push(
                            div()
                                .absolute()
                                .left(px(x))
                                .top(px(top))
                                .h(px(row_h))
                                .w(px(w))
                                .bg(theme.preview_selection_bg())
                                .into_any_element(),
                        );
                    }
                }
            }

            let content = if let Some(image) = image_block.as_ref() {
                self.render_image_block(
                    image,
                    v.line_idx,
                    false,
                    false,
                    false,
                    None,
                    theme,
                    window,
                    cx,
                )
            } else if let Some(reference) = reference.as_ref() {
                self.render_broken_image(&reference.target, theme)
            } else {
                render_live_styled_row(
                    line,
                    &classified,
                    v.char_start,
                    v.char_end,
                    v.line_idx,
                    v.sub_idx,
                    v.is_last_sub,
                    false,
                    &InlineReveal::None,
                    false,
                    &typography.headings,
                    theme,
                    &editor_font,
                    None,
                    None,
                    false,
                    None,
                    &self.plugin_input_focus_handle,
                    None,
                    None,
                    false,
                    table_render_arg,
                    false,
                    false,
                    None,
                    false,
                    window,
                    Some(cx),
                    None,
                    line_char_width,
                    guide,
                    callout,
                    self.config.ui.language.as_str(),
                    false,
                    "note-preview-row",
                )
                .into_any_element()
            };

            let mut row = div()
                .id(gpui::ElementId::Name(
                    format!("note-preview-line-{}", v_idx).into(),
                ))
                .relative()
                .w_full()
                .h(px(row_h))
                .line_height(px(row_h))
                .font_family(editor_font.clone())
                .text_size(px(typography.base_font_size))
                .whitespace_nowrap();
            if is_opening_fence {
                row = row.bg(theme.code_bg).rounded_t_md();
            } else if is_closing_fence {
                row = row.bg(theme.code_bg).rounded_b_md();
            } else if is_code_line {
                row = row.bg(theme.code_bg);
            } else if let Some(block) = callout {
                row = row
                    .bg(crate::markdown::callout_bg(block.kind, theme.is_dark))
                    .when(callout_first, |d| d.rounded_t_md())
                    .when(callout_last, |d| d.rounded_b_md());
            }
            row = row.children(backdrops).child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_row()
                    .when(grid_tbl.is_none(), |d| d.items_center())
                    .when(grid_tbl.is_some(), |d| d.items_start())
                    .whitespace_nowrap()
                    .child(content),
            );
            rows.push(row.into_any_element());
        }

        let max_scroll = Self::editor_max_scroll(state.view.total_height(), f64::from(layout.body_h));
        let scroll_style = crate::views::scrollbar::ScrollbarStyle::default();
        let mut body = div()
            .id("note-preview-body")
            .relative()
            .w_full()
            .h(px(layout.body_h))
            .overflow_hidden()
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.handle_note_preview_mouse_down(event, window, cx);
                }),
            )
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.end_note_preview_selection(cx);
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.end_note_preview_selection(cx);
            }))
            .on_scroll_wheel(cx.listener(
                |this, event: &ScrollWheelEvent, window, cx| {
                    this.handle_note_preview_wheel(event, window, cx);
                },
            ))
            .child(
                div()
                    .absolute()
                    .left(px(PREVIEW_PAD))
                    .top(px(shift))
                    .w(px(layout.content_w))
                    .flex()
                    .flex_col()
                    .children(rows),
            );

        if let Some(geometry) = crate::views::scrollbar::ScrollbarGeometry::new(
            layout.body_top,
            layout.body_h,
            max_scroll as f32,
            scroll as f32,
            &scroll_style,
        ) {
            let dragging = self.note_preview_scrollbar_drag.active;
            body = body.child(crate::views::scrollbar::overlay_scrollbar(
                "note-preview-scrollbar",
                &geometry,
                &scroll_style,
                theme,
                dragging,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let pointer_y: f32 = event.position.y.into();
                    this.set_note_preview_scroll(
                        f64::from(geometry.offset_for_track_click(pointer_y)),
                        window,
                        cx,
                    );
                }),
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.note_preview_scrollbar_drag
                        .begin(event.position.y.into(), geometry.scroll_y);
                    cx.notify();
                }),
            ));
        }

        let panel = div()
            .id("note-preview-panel")
            .w(px(layout.panel_w))
            .h(px(layout.panel_h))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                this.update_note_preview_selection(event, window, cx);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.end_note_preview_selection(cx);
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.end_note_preview_selection(cx);
            }))
            .child(
                div()
                    .h(px(PREVIEW_HEADER_H))
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .px(px(PREVIEW_PAD))
                    .border_b_1()
                    .border_color(theme.border_subtle)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_base()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(header_title),
                    )
                    .child(self.image_close_button(
                        "note-preview-close",
                        theme,
                        cx.listener(|this, _, _, cx| this.close_note_preview(cx)),
                    )),
            )
            .child(body);

        div()
            .id("note-preview-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000059))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.close_note_preview(cx);
            }))
            .child(panel)
            .into_any_element()
    }
}
