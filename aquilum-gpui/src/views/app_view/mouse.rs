use super::*;

impl AppView {
    pub(super) fn handle_editor_mouse_down(&mut self, click_x: f32, click_y: f32, shift: bool, alt: bool, click_count: usize, window: &Window, cx: &mut Context<Self>) {
        self.mouse_down_pos = Some((click_x, click_y));
        self.mouse_drag_started = false;
        if self.state.active_doc_id().is_none() {
            return;
        }

        let editor_rows_top_y = self.editor_rows_top_y();
        if click_y < editor_rows_top_y {
            return;
        }
        // f64: at the tail of a huge note an f32 scroll position is only good to
        // 16 px, which is enough to pick the neighbouring row.
        let current_scroll_y = self.current_scroll_top_f64();
        let rel_y = f64::from(click_y) - f64::from(editor_rows_top_y) + current_scroll_y;

        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(&editor_font));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);
        let text_start_x = self.pane_text_start_x(None, window);
        let word_wrap = self.word_wrap;
        let wrap_cols = self.last_wrap_cols;

        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };

        view.ensure_visual_cache_with_headings(doc, word_wrap, wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let target_v_idx = if rel_y <= 0.0 {
            0
        } else {
            view.local_v_idx_at_doc_y(rel_y)
        };

        let v = &visual_rows[target_v_idx];
        let v_len = v.char_end.saturating_sub(v.char_start);

        // An image block does not take the caret on a single click: it becomes
        // selected (the toolbar stays) and the picture stays visible. A second
        // click falls through to the markup-revealing path.
        if !self.source_mode && self.edit_mode && !self.ctrl_preview_selection && click_count == 1 {
            let is_image = view
                .window
                .lines
                .get(v.line_idx)
                .is_some_and(|text| crate::images::parse_image_reference(text).is_some());
            if is_image {
                self.selected_image_line = Some(v.line_idx);
                self.selected_image_doc = Some(doc.id);
                self.hovered_image_line = Some(v.line_idx);
                self.hovered_image_doc = Some(doc.id);
                self.is_mouse_selecting = false;
                self.mouse_down_pos = None;
                return;
            }
        }
        self.selected_image_line = None;
        self.selected_image_doc = None;
        self.hovered_image_line = None;
        self.hovered_image_doc = None;

        let (cursor_line, cursor_col) = view.line_col(doc);
        let selection_range = view.selection_range(doc.len());
        let line = &view.window.lines[v.line_idx];
        let line_start = doc.line_start_byte(v.line_idx);
        let line_end = line_start + line.len();
        let is_line_in_selection = selection_range.as_ref().map_or(false, |sel| {
            if sel.start == sel.end {
                false
            } else if line_start == line_end {
                sel.start <= line_start && sel.end >= line_end
            } else {
                sel.end > line_start && sel.start < line_end
            }
        });
        let is_in_active_block = view.window.code_states.get(v.line_idx).and_then(|s| s.as_ref()).map_or(false, |s| {
            let cursor_in_block = view.window.code_states.get(cursor_line).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id);
            let sel_in_block = selection_range.as_ref().map_or(false, |sel| {
                let sel_start_l = doc.line_of_byte(sel.start).max(view.window.code_states.start());
                let sel_end_l = doc.line_of_byte(sel.end).min(view.window.code_states.end().saturating_sub(1));
                if sel_start_l <= sel_end_l {
                    (sel_start_l..=sel_end_l).any(|l_idx| {
                        view.window.code_states.get(l_idx).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id)
                    })
                } else {
                    false
                }
            });
            cursor_in_block || sel_in_block
        });
        let is_active_line = Self::is_active_editor_line(self.edit_mode && !self.ctrl_preview_selection, v.line_idx, cursor_line, is_line_in_selection, is_in_active_block);
        let code_state = view.window.code_states.get(v.line_idx).and_then(|s| s.as_ref()).map(|s| (s.kind, s.lang.as_deref()));
        let classified = crate::markdown::classify_line(line, code_state);
        let heading_widths = Self::resolve_heading_char_widths(cx.text_system(), &editor_font, &self.config.editor.headings, line_char_width);
        let (_, row_char_w, _, _, _) = Self::heading_typography_metrics_with_widths(&classified, &self.config.editor.headings, 24.0, line_char_width, Some(&heading_widths));

        // Hit-test against the layout that is on screen right now: this row's reveal is
        // the one the previous frame painted, so the click cannot shift the geometry it
        // is resolved against.
        let reveal = if self.edit_mode && !self.ctrl_preview_selection {
            crate::markdown::inline_reveal(
                line,
                &classified,
                (cursor_line == v.line_idx).then_some(cursor_col),
                selection_chars_in_line(line, line_start, selection_range.as_ref()),
                view.reveal_all_line == Some(v.line_idx) && cursor_line == v.line_idx,
            )
        } else {
            crate::markdown::InlineReveal::None
        };

        // Hit-test against the shaped layout of the row rather than a
        // `column * char_width` cell model: wide glyphs (emoji, CJK) do not advance
        // by exactly one cell.
        let col_in_row = if click_x > text_start_x {
            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            let row_layout = if let Some(tbl) = view.grid_table_at(doc, &tables, v.line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                let table_layout = Self::get_or_compute_table_layout(
                    &self.table_layouts,
                    window.text_system(),
                    &|idx| view.window.lines.get(idx).map(|s| s.as_str()),
                    tbl,
                    &editor_font,
                    self.manual_table_col_widths.get(&tbl.first_line).map(|w| w.as_slice()),
                    line_char_width,
                    Self::table_room_px(view, line_char_width),
                );
                let cursor_col = if !self.ctrl_preview_selection && v.line_idx == cursor_line {
                    let (_, col) = view.line_col(doc);
                    Some(col)
                } else {
                    None
                };
                shape_table_row(
                    window.text_system(),
                    line,
                    tbl,
                    v.line_idx,
                    cursor_col,
                    &table_layout,
                    &self.config.editor.headings,
                    &editor_font,
                    row_char_w,
                )
            } else {
                let raw_row = tables
                    .at(v.line_idx)
                    .map_or(false, |t| view.table_raw_now(doc, t, cursor_line, self.ctrl_preview_selection || !self.edit_mode));
                shape_row(
                    window.text_system(),
                    line,
                    &classified,
                    v,
                    is_active_line || raw_row,
                    &reveal,
                    &self.config.editor.headings,
                    &editor_font,
                    row_char_w,
                )
            };
            let edge_top = tables
                .at(v.line_idx)
                .filter(|t| v.line_idx == t.first_line)
                .map(|_| crate::markdown::TABLE_EDGE_MARGIN)
                .unwrap_or(0.0);
            let row_content_top =
                view.above_h + view.window.height_index.top_of(target_v_idx) + f64::from(edge_top);
            let column = row_layout.column_for_xy(
                click_x - text_start_x,
                (rel_y - row_content_top) as f32,
            );
            row_layout.line_char_for_column(column).saturating_sub(v.char_start).min(v_len)
        } else {
            0
        };

        let mut target_char = v.char_start + col_in_row;
        if self.config.editor.exact_reveal_cursor && self.edit_mode && !self.ctrl_preview_selection && click_count == 1 {
            let new_reveal = crate::markdown::inline_reveal(
                line,
                &classified,
                Some(target_char),
                None,
                alt || view.reveal_all_line == Some(v.line_idx),
            );
            let reveal_changed = new_reveal != reveal && !matches!(new_reveal, crate::markdown::InlineReveal::None);
            let heading_activated = !is_active_line && v.sub_idx == 0 && matches!(
                classified,
                crate::markdown::LineKind::Heading1(_)
                    | crate::markdown::LineKind::Heading2(_)
                    | crate::markdown::LineKind::Heading3(_)
                    | crate::markdown::LineKind::Heading4(_)
                    | crate::markdown::LineKind::Heading5(_)
                    | crate::markdown::LineKind::Heading6(_)
            );
            if reveal_changed || heading_activated {
                let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
                if view.grid_table_at(doc, &tables, v.line_idx, cursor_line, false).is_none() {
                    let rev_layout = shape_row(
                        window.text_system(),
                        line,
                        &classified,
                        v,
                        true,
                        &new_reveal,
                        &self.config.editor.headings,
                        &editor_font,
                        row_char_w,
                    );
                    if click_x > text_start_x {
                        let rev_column = rev_layout.column_for_x(click_x - text_start_x);
                        let rev_col_in_row = rev_layout.line_char_for_column(rev_column).saturating_sub(v.char_start).min(v_len);
                        target_char = v.char_start + rev_col_in_row;
                    }
                }
            }
        }
        let offset = view.offset_for_line_col(doc, v.line_idx, target_char);

        if click_count == 2 {
            let (w_start, w_end) = doc.word_range_at(offset);
            view.selection_anchor = Some(w_start);
            view.cursor_offset = w_end;
            self.is_mouse_selecting = false;
            self.mouse_drag_started = false;
            self.mouse_down_pos = None;
        } else if click_count >= 3 {
            let (l_start, l_end) = doc.line_range_at(offset);
            view.selection_anchor = Some(l_start);
            view.cursor_offset = l_end;
            self.is_mouse_selecting = false;
            self.mouse_drag_started = false;
            self.mouse_down_pos = None;
        } else if shift {
            if view.selection_anchor.is_none() {
                view.selection_anchor = Some(view.cursor_offset);
            }
            view.cursor_offset = offset;
        } else {
            view.cursor_offset = offset;
            view.selection_anchor = Some(offset);
        }
        // Alt+click is the escape hatch that shows the whole markup of the clicked line.
        // A plain click on that same line keeps it revealed, so placing the caret does
        // not fold the markup back; a click on any other line drops it.
        view.reveal_all_line = if alt {
            Some(v.line_idx)
        } else if view.reveal_all_line == Some(v.line_idx) {
            view.reveal_all_line
        } else {
            None
        };
        self.reset_editor_cursor_blink(cx);
        self.state.publish_snapshot();
        cx.notify();
    }

    pub(super) fn update_mouse_selection(&mut self, pos_x: f32, pos_y: f32, viewport_h: f32, window: &Window, cx: &mut Context<Self>) {
        if !self.is_mouse_selecting {
            return;
        }
        if !self.mouse_drag_started {
            if let Some((down_x, down_y)) = self.mouse_down_pos {
                let dx = pos_x - down_x;
                let dy = pos_y - down_y;
                if dx * dx + dy * dy >= 16.0 {
                    self.mouse_drag_started = true;
                } else {
                    return;
                }
            } else {
                self.mouse_drag_started = true;
            }
        }
        if self.state.active_doc_id().is_none() {
            return;
        }

        // Same top the click mapping uses: measured, so an open find bar does
        // not make the drag start two or three rows off.
        let editor_rows_top_y = self.editor_rows_top_y();
        let editor_bottom_y = (viewport_h - 28.0).max(editor_rows_top_y + 100.0);
        let scroll_before = self.current_scroll_top_f64();

        // Edge autoscroll when dragging outside or near the top/bottom bounds.
        // The scroll write must be followed by a repaint even when the cursor
        // ends up on the same character: the content under the fixed pointer
        // changed, so the painted selection is stale until a frame runs.
        if pos_y < editor_rows_top_y && scroll_before > 0.0 {
            let step = f64::from(((editor_rows_top_y - pos_y).max(1.0) * 0.8).min(30.0));
            self.set_editor_scroll_top_f64((scroll_before - step).max(0.0));
        } else if pos_y > editor_bottom_y - 16.0 {
            let step = f64::from(((pos_y - (editor_bottom_y - 16.0)).max(1.0) * 0.8).min(30.0));
            self.set_editor_scroll_top_f64(scroll_before + step);
        }
        let scroll_moved = self.current_scroll_top_f64() != scroll_before;

        let current_scroll_y = self.current_scroll_top_f64();
        let rel_y = f64::from(pos_y) - f64::from(editor_rows_top_y) + current_scroll_y;

        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(&editor_font));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);
        let text_start_x = self.pane_text_start_x(None, window);
        let word_wrap = self.word_wrap;
        let wrap_cols = self.last_wrap_cols;

        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };

        view.ensure_visual_cache_with_headings(doc, word_wrap, wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let target_v_idx = if rel_y <= 0.0 {
            0
        } else {
            view.local_v_idx_at_doc_y(rel_y)
        };

        let v = &visual_rows[target_v_idx];
        let v_len = v.char_end.saturating_sub(v.char_start);

        let (cursor_line, cursor_col) = view.line_col(doc);
        let selection_range = view.selection_range(doc.len());
        let line = &view.window.lines[v.line_idx];
        let line_start = doc.line_start_byte(v.line_idx);
        let line_end = line_start + line.len();
        let is_line_in_selection = selection_range.as_ref().map_or(false, |sel| {
            if sel.start == sel.end {
                false
            } else if line_start == line_end {
                sel.start <= line_start && sel.end >= line_end
            } else {
                sel.end > line_start && sel.start < line_end
            }
        });
        let is_in_active_block = view.window.code_states.get(v.line_idx).and_then(|s| s.as_ref()).map_or(false, |s| {
            let cursor_in_block = view.window.code_states.get(cursor_line).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id);
            let sel_in_block = selection_range.as_ref().map_or(false, |sel| {
                let sel_start_l = doc.line_of_byte(sel.start).max(view.window.code_states.start());
                let sel_end_l = doc.line_of_byte(sel.end).min(view.window.code_states.end().saturating_sub(1));
                if sel_start_l <= sel_end_l {
                    (sel_start_l..=sel_end_l).any(|l_idx| {
                        view.window.code_states.get(l_idx).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id)
                    })
                } else {
                    false
                }
            });
            cursor_in_block || sel_in_block
        });
        let is_active_line = Self::is_active_editor_line(self.edit_mode && !self.ctrl_preview_selection, v.line_idx, cursor_line, is_line_in_selection, is_in_active_block);
        let code_state = view.window.code_states.get(v.line_idx).and_then(|s| s.as_ref()).map(|s| (s.kind, s.lang.as_deref()));
        let classified = crate::markdown::classify_line(line, code_state);
        let heading_widths = Self::resolve_heading_char_widths(cx.text_system(), &editor_font, &self.config.editor.headings, line_char_width);
        let (_, row_char_w, _, _, _) = Self::heading_typography_metrics_with_widths(&classified, &self.config.editor.headings, 24.0, line_char_width, Some(&heading_widths));

        let reveal = if self.edit_mode && !self.ctrl_preview_selection {
            crate::markdown::inline_reveal(
                line,
                &classified,
                (cursor_line == v.line_idx).then_some(cursor_col),
                selection_chars_in_line(line, line_start, selection_range.as_ref()),
                view.reveal_all_line == Some(v.line_idx) && cursor_line == v.line_idx,
            )
        } else {
            crate::markdown::InlineReveal::None
        };

        let col_in_row = if pos_x > text_start_x {
            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            let row_layout = if let Some(tbl) = view.grid_table_at(doc, &tables, v.line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                let table_layout = Self::get_or_compute_table_layout(
                    &self.table_layouts,
                    window.text_system(),
                    &|idx| view.window.lines.get(idx).map(|s| s.as_str()),
                    tbl,
                    &editor_font,
                    self.manual_table_col_widths.get(&tbl.first_line).map(|w| w.as_slice()),
                    line_char_width,
                    Self::table_room_px(view, line_char_width),
                );
                let cursor_col = if !self.ctrl_preview_selection && v.line_idx == cursor_line {
                    let (_, col) = view.line_col(doc);
                    Some(col)
                } else {
                    None
                };
                shape_table_row(
                    window.text_system(),
                    line,
                    tbl,
                    v.line_idx,
                    cursor_col,
                    &table_layout,
                    &self.config.editor.headings,
                    &editor_font,
                    row_char_w,
                )
            } else {
                let raw_row = tables
                    .at(v.line_idx)
                    .map_or(false, |t| view.table_raw_now(doc, t, cursor_line, self.ctrl_preview_selection || !self.edit_mode));
                shape_row(
                    window.text_system(),
                    line,
                    &classified,
                    v,
                    is_active_line || raw_row,
                    &reveal,
                    &self.config.editor.headings,
                    &editor_font,
                    row_char_w,
                )
            };
            let edge_top = tables
                .at(v.line_idx)
                .filter(|t| v.line_idx == t.first_line)
                .map(|_| crate::markdown::TABLE_EDGE_MARGIN)
                .unwrap_or(0.0);
            let row_content_top =
                view.above_h + view.window.height_index.top_of(target_v_idx) + f64::from(edge_top);
            let column = row_layout.column_for_xy(
                pos_x - text_start_x,
                (rel_y - row_content_top) as f32,
            );
            row_layout.line_char_for_column(column).saturating_sub(v.char_start).min(v_len)
        } else {
            0
        };

        let target_char = v.char_start + col_in_row;
        let offset = view.offset_for_line_col(doc, v.line_idx, target_char);
        let moved = view.cursor_offset != offset;
        if moved {
            view.cursor_offset = offset;
        }
        view.hold_drag_revealed_tables(doc, self.ctrl_preview_selection || !self.edit_mode);
        view.hold_drag_revealed_images(doc, self.ctrl_preview_selection || !self.edit_mode);
        if moved {
            cx.notify();
        } else if scroll_moved {
            // Same character under the pointer, but the content scrolled:
            // without this notify the painted selection waits for the next
            // mousemove, which is exactly the visible drag lag.
            cx.notify();
        }
    }

    /// Whether the render path paints line `line_idx` as the active line, i.e. with its
    /// markdown markers (`# `, `> `, task/bullet/numbered prefixes) visible.
    ///
    /// `shape_row` reproduces exactly the text the render paints, so every hit-test must
    /// answer this the same way the render does (`is_editing && (...)`). Dropping the
    /// `edit_mode` guard makes the hit-test count markers that read mode hides, and the
    /// clicked column then lands one marker length away from the glyph under the pointer.
    pub(super) fn is_active_editor_line(
        edit_mode: bool,
        line_idx: usize,
        cursor_line: usize,
        is_line_in_selection: bool,
        is_in_active_block: bool,
    ) -> bool {
        edit_mode && (line_idx == cursor_line || is_line_in_selection || is_in_active_block)
    }
}
