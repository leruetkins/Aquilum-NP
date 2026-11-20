use super::*;

impl AppView {
    pub(super) fn handle_note_preview_mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let x: f32 = event.position.x.into();
        let y: f32 = event.position.y.into();
        let Some(offset) = self.note_preview_hit_test(x, y, window, cx) else {
            return;
        };
        let Some(state) = self.note_preview.as_mut() else {
            return;
        };
        match event.click_count {
            2 => {
                let (start, end) = state.doc.word_range_at(offset);
                state.view.selection_anchor = Some(start);
                state.view.cursor_offset = end;
                state.is_mouse_selecting = false;
            }
            count if count >= 3 => {
                let (start, end) = state.doc.line_range_at(offset);
                state.view.selection_anchor = Some(start);
                state.view.cursor_offset = end;
                state.is_mouse_selecting = false;
            }
            _ if event.modifiers.shift => {
                if state.view.selection_anchor.is_none() {
                    state.view.selection_anchor = Some(state.view.cursor_offset);
                }
                state.view.cursor_offset = offset;
                state.is_mouse_selecting = true;
            }
            _ => {
                state.view.cursor_offset = offset;
                state.view.selection_anchor = Some(offset);
                state.is_mouse_selecting = true;
            }
        }
        state.mouse_down = Some((x, y));
        state.drag_started = false;
        cx.notify();
    }

    pub(super) fn update_note_preview_selection(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let x: f32 = event.position.x.into();
        let y: f32 = event.position.y.into();
        let layout = preview_layout(window);
        let (selecting, drag_started, mouse_down, scroll_start, max) = {
            let Some(state) = self.note_preview.as_ref() else {
                return;
            };
            let max =
                Self::editor_max_scroll(state.view.total_height(), f64::from(layout.body_h));
            (
                state.is_mouse_selecting,
                state.drag_started,
                state.mouse_down,
                state.scroll_top,
                max,
            )
        };
        if !selecting {
            return;
        }
        let mut drag_started = drag_started;
        if !drag_started {
            if let Some((down_x, down_y)) = mouse_down {
                let dx = x - down_x;
                let dy = y - down_y;
                if dx * dx + dy * dy < 16.0 {
                    return;
                }
            }
            drag_started = true;
        }

        let body_bottom = layout.body_top + layout.body_h;
        let mut scroll = scroll_start;
        if y < layout.body_top {
            let step = f64::from(((layout.body_top - y).max(1.0) * 0.8).min(30.0));
            scroll = (scroll - step).max(0.0);
        } else if y > body_bottom - 16.0 {
            let step = f64::from(((y - (body_bottom - 16.0)).max(1.0) * 0.8).min(30.0));
            scroll = (scroll + step).min(max);
        }
        let scrolled = scroll != scroll_start;
        if let Some(state) = self.note_preview.as_mut() {
            state.drag_started = drag_started;
            state.scroll_top = scroll;
        }
        let offset = self.note_preview_hit_test(x, y, window, cx);

        let Some(state) = self.note_preview.as_mut() else {
            return;
        };
        let moved = offset.map_or(false, |target| target != state.view.cursor_offset);
        if let Some(target) = offset {
            state.view.cursor_offset = target;
        }
        if moved || scrolled {
            cx.notify();
        }
    }

    pub(super) fn end_note_preview_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.note_preview.as_mut() {
            state.is_mouse_selecting = false;
            state.drag_started = false;
            state.mouse_down = None;
            cx.notify();
        }
    }

    /// Byte offset in the card's document under the pointer.
    fn note_preview_hit_test(
        &self,
        x: f32,
        y: f32,
        window: &Window,
        cx: &gpui::App,
    ) -> Option<usize> {
        let layout = preview_layout(window);
        if y < layout.body_top || y > layout.body_top + layout.body_h {
            return None;
        }
        let state = self.note_preview.as_ref()?;
        let rel_y = f64::from(y - layout.body_top) + state.scroll_top;
        let visual_rows = &state.view.window.visual_rows;
        if visual_rows.is_empty() {
            return None;
        }
        let v_idx = state
            .view
            .local_v_idx_at_doc_y(rel_y)
            .min(visual_rows.len().saturating_sub(1));
        let v = &visual_rows[v_idx];
        let line = state.view.window.lines.get(v.line_idx)?.as_str();
        let code_state = state
            .view
            .window
            .code_states
            .get(v.line_idx)
            .and_then(|s| s.as_ref())
            .map(|s| (s.kind, s.lang.as_deref()));
        let classified = crate::markdown::classify_line(line, code_state);
        let typography = self.editor_typography();
        let editor_font = self.editor_font_family();
        let line_char_width = self.editor_char_width(cx);
        let heading_widths = Self::resolve_heading_char_widths(
            cx.text_system(),
            &editor_font,
            &typography,
            line_char_width,
        );
        let (_, row_char_w, _, _, _) = Self::heading_typography_metrics_with_widths(
            &classified,
            &typography,
            line_char_width,
            Some(&heading_widths),
        );

        let tables = state
            .doc
            .ensure_table_index_for_window(state.view.window.start_line, state.view.window.end_line);
        let grid_tbl =
            state
                .view
                .grid_table_at(&state.doc, &tables, v.line_idx, usize::MAX, true);
        let edge_top = grid_tbl
            .filter(|tbl| v.line_idx == tbl.first_line)
            .map(|_| crate::markdown::TABLE_EDGE_MARGIN)
            .unwrap_or(0.0);
        let row_layout = if let Some(tbl) = grid_tbl {
            let table_layout = Self::get_or_compute_table_layout(
                &state.table_layouts,
                window.text_system(),
                &|idx| state.view.window.lines.get(idx).map(|s| s.as_str()),
                tbl,
                &editor_font,
                None,
                line_char_width,
                Self::table_room_px(&state.view, line_char_width),
                &typography,
            );
            shape_table_row(
                window.text_system(),
                line,
                tbl,
                v.line_idx,
                None,
                table_layout.as_ref(),
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

        let row_content_top = state.view.above_h
            + state.view.window.height_index.top_of(v_idx)
            + f64::from(edge_top);
        let column = if x > layout.text_left {
            row_layout.column_for_xy(
                x - layout.text_left,
                (rel_y - row_content_top) as f32,
            )
        } else {
            0
        };
        let v_len = v.char_end.saturating_sub(v.char_start);
        let col_in_row = row_layout
            .line_char_for_column(column)
            .saturating_sub(v.char_start)
            .min(v_len);
        let target_char = v.char_start + col_in_row;
        Some(state.view.offset_for_line_col(&state.doc, v.line_idx, target_char))
    }
}
