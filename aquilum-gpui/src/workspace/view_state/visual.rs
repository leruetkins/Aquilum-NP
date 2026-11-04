use super::*;

impl TabSession {
    pub fn doc_y_of_local_v_idx(&self, local_v_idx: usize) -> f64 {
        self.above_h + self.window.height_index.top_of(local_v_idx)
    }

    pub fn local_v_idx_at_doc_y(&self, doc_y: f64) -> usize {
        if self.window.visual_rows.is_empty() {
            return 0;
        }
        self.window.height_index.v_idx_at_y(doc_y - self.above_h)
    }

    pub fn doc_y_of_line(&self, lines_total: usize, line: usize) -> f64 {
        if lines_total == 0 {
            return 0.0;
        }
        let line = line.min(lines_total.saturating_sub(1));

        if self.window.covers_line(line) {
            if let Some(v_idx) = self.window.first_row_of_line(line) {
                return self.above_h + self.window.height_index.top_of(v_idx);
            }
        }

        if line < self.window.start_line {
            if self.window.start_line == 0 || self.above_lines == 0 {
                return 0.0;
            }
            return (line as f64 / self.above_lines as f64) * self.above_h;
        }

        let window_bottom_y = self.above_h + self.window.total_height();
        if self.below_lines == 0 {
            return window_bottom_y;
        }
        let lines_below = line.saturating_sub(self.window.end_line);
        let fraction = (lines_below as f64 / self.below_lines as f64).clamp(0.0, 1.0);
        window_bottom_y + fraction * self.below_h
    }

    pub fn line_col(&self, doc: &DocumentBuffer) -> (usize, usize) {
        let offset = doc.clamp_offset(self.cursor_offset);
        let line_idx = doc.line_of_byte(offset);
        let line_start = doc.line_start_byte(line_idx).min(offset);
        let prefix = doc.slice_to_string(line_start..offset);
        let col_idx = prefix.chars().count();
        (line_idx, col_idx)
    }

    pub fn offset_for_line_col(&self, doc: &DocumentBuffer, target_line: usize, target_col: usize) -> usize {
        if target_line >= doc.line_index.lines() {
            return doc.len();
        }
        let line_start = doc.line_start_byte(target_line);
        let line_end = doc.line_end_byte(target_line);
        let line = doc.slice_to_string(line_start..line_end);
        let actual_col = target_col.min(line.chars().count());
        let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
        line_start + byte_col
    }

    pub fn rebuild_visual_cache_with_headings(
        &mut self,
        doc: &mut DocumentBuffer,
        word_wrap: bool,
        wrap_cols: usize,
        typography: Option<&crate::settings::EditorTypography>,
        anchor_line: usize,
        suppress_selection_reveal: bool,
    ) {
        let headings = typography.map(|typography| &typography.headings);
        let basis_stale = self.window.is_empty()
            || self.window.word_wrap != word_wrap
            || self.window.wrap_cols != wrap_cols;

        let lines_total = doc.line_index.lines();
        // What a pure window move must not disturb, captured before anything
        // is replaced: the document's total height and the anchor line's
        // document Y. Both are built from the *measured* window plus
        // estimate-based margins, and a jump swaps real window content for
        // estimate-based ones — without anchoring, the total wobbles by the
        // real-vs-estimate difference of the swapped span, the render-side
        // re-anchor rewrites the scroll by the residue, and the scrollbar
        // thumb ticks (visible whenever scrolling is continuous, e.g. a wheel
        // glide). Same idea as the original's align-on-jump: derive `above_h`
        // from the preserved anchor Y and pin `below_h` to the preserved
        // total, so a scroll that does not change the document never changes
        // its height. Only a pure move pins — a dirty rebuild (edit, reveal,
        // wrap change) may legitimately change heights.
        let pure_move = !basis_stale
            && !self.visual_cache_dirty
            && !self.window.is_empty()
            && lines_total > 2 * crate::editor_window::MARGIN_LINES + 1;
        let old_total = self.total_height();
        // The row the pin hangs on, read off the OLD window before it is
        // replaced. It must genuinely represent the anchor line: its first
        // own row when the line has one (a wrapped line's sub-rows share its
        // line_idx), or — for a line with no row of its own, like a collapsed
        // fence's body — the row that contains it. An anchor the old window
        // does not cover (a jump) has no such row: pinning to the clamped
        // last row would anchor the wrong line, so those moves fall back to
        // the plain estimate.
        let row_of = |window: &crate::editor_window::EditorWindow| -> Option<usize> {
            if !window.covers_line(anchor_line) || window.visual_rows.is_empty() {
                return None;
            }
            Some(
                window
                    .first_row_of_line(anchor_line)
                    .unwrap_or_else(|| {
                        window
                            .visual_rows
                            .partition_point(|r| r.line_idx <= anchor_line)
                            .saturating_sub(1)
                    }),
            )
        };
        let old_row_top = pure_move.then(|| row_of(&self.window)).flatten()
            .map(|v| self.window.height_index.top_of(v));
        // Keep the current start when the anchor sits comfortably below it and
        // the window still reaches the document end (or is within a follow
        // margin of it). Growth at the end — typing a new last line — is then
        // absorbed by extending the window's end. Re-placing the window on the
        // caret instead moves the estimated `above_h` under the rows being
        // read, which shows up as the content above the caret drifting on
        // every keystroke.
        let window = &self.window;
        let keep_start = !window.is_empty()
            && window.word_wrap == word_wrap
            && window.wrap_cols == wrap_cols
            && window.covers_line(anchor_line)
            && anchor_line >= window.start_line.saturating_add(crate::editor_window::FOLLOW_SLACK)
            && (anchor_line.saturating_add(crate::editor_window::FOLLOW_SLACK) < window.end_line
                || window.end_line.saturating_add(crate::editor_window::FOLLOW_SLACK)
                    >= lines_total);
        let (start, end) = if lines_total <= 2 * crate::editor_window::MARGIN_LINES + 1 {
            (0, lines_total)
        } else if keep_start {
            let target_len = self
                .window
                .line_count()
                .max(2 * crate::editor_window::MARGIN_LINES + 1);
            let start = self.window.start_line.min(lines_total.saturating_sub(1));
            let end = (start + target_len)
                .max(anchor_line.saturating_add(crate::editor_window::FOLLOW_SLACK + 1))
                .min(lines_total);
            (start, end)
        } else {
            crate::editor_window::EditorWindow::bounds_for(anchor_line, lines_total)
        };

        doc.ensure_checkpoints(start + 1);
        let initial = doc.checkpoint_state_at(start).unwrap_or_default();
        let start_byte = doc.line_start_byte(start);
        let end_byte = doc.line_end_byte(end.saturating_sub(1)).max(start_byte);
        let slice = doc.slice_to_string(start_byte..end_byte);

        let tables = doc.ensure_table_index_for_window(start, end);
        let (cursor_line, cursor_col) = self.line_col(doc);
        self.cursor_line = cursor_line;
        let reveal_state = self.selection_reveal_state(doc, suppress_selection_reveal);
        let window = crate::editor_window::EditorWindow::build_from_slice_with_tables(
            &slice,
            start,
            end,
            start_byte,
            end_byte,
            word_wrap,
            wrap_cols,
            typography,
            self.source_mode,
            initial,
            |line, line_text| {
                if self.source_mode {
                    return None;
                }
                Self::table_row_kind_at(
                    &tables,
                    &self.raw_tables,
                    &self.drag_revealed_tables,
                    &self.table_cell_cols,
                    reveal_state,
                    cursor_line,
                    cursor_col,
                    !suppress_selection_reveal,
                    line,
                    line_text,
                    headings,
                )
            },
            |line, line_text| {
                if self.source_mode {
                    return None;
                }
                let show_original = crate::images::parse_image_reference(line_text)
                    .is_some_and(|reference| self.shows_original_image(line, &reference.target));
                if show_original {
                    return crate::images::block_height(
                        &doc.path,
                        line_text,
                        wrap_cols,
                        true,
                    );
                }
                // Reserve the picture block even when its markup is painted
                // instead of the picture (caret on the line, selection, click
                // to either side). Collapsing it moved every following line
                // and made selection jump.
                crate::images::block_height(&doc.path, line_text, wrap_cols, false)
            },
        );

        self.above_lines = window.start_line;
        self.below_lines = doc
            .line_index
            .lines()
            .saturating_sub(window.end_line);
        self.window = window;
        self.revealed_selection = reveal_state;
        // The rebuild wrote canonical rows for every line; the exceptions are
        // re-applied by `sync_line_paints` on the same call.
        self.line_paints.clear();
        if let Some(typography) = typography {
            self.typography_cache = typography.clone();
        }
        if basis_stale {
            self.refresh_estimate_basis();
        }
        if pure_move {
            let new_row_top = row_of(&self.window).map(|v| self.window.height_index.top_of(v));
            match (old_row_top, new_row_top) {
                // Content: the anchor row keeps its exact document Y, so the
                // frame that moved the window paints it where the previous
                // frame did and the render-side re-anchor writes back the
                // same scroll value — a no-op instead of a residue.
                // Scrollbar: below absorbs whatever the est-vs-real
                // difference of the swapped span is, so `total_height` —
                // and with it `max_scroll` and the thumb geometry — stays
                // constant across moves. When the window reaches a document
                // edge the corresponding margin is exactly zero and the
                // accumulated estimate error resolves into the other side
                // (one small correction, like the original).
                (Some(old_top), Some(inner_top))
                    if self.above_lines > 0 || self.below_lines > 0 =>
                {
                    let old_y = self.above_h + old_top;
                    let above = if self.above_lines == 0 {
                        0.0
                    } else {
                        old_y - inner_top
                    };
                    let below = if self.below_lines == 0 {
                        0.0
                    } else {
                        old_total - above - self.window.total_height()
                    };
                    // A negative margin means the real heights accumulated
                    // by the anchor chain have outgrown the estimate-based
                    // total the chain inherited from the first window (the
                    // frozen per-line average is only as good as the window
                    // it was frozen from). Clamping to zero would leave the
                    // state broken — below stays 0 while lines remain below,
                    // so every following move clamps again and the total
                    // creeps. Falling back to the plain estimate rebuilds
                    // both margins consistently: one small correction
                    // towards the true height, then the chain pins again
                    // from there.
                    if above >= 0.0 && below >= 0.0 {
                        self.above_h = above;
                        self.below_h = below;
                    } else {
                        self.recompute_estimate();
                    }
                }
                _ => self.recompute_estimate(),
            }
        } else {
            self.recompute_estimate();
        }
        self.visual_cache_dirty = false;
    }

    pub fn refresh_estimate_basis(&mut self) {
        let rows = self.window.visual_rows.len();
        let lines = self.window.line_count();
        if rows == 0 || lines == 0 {
            return;
        }
        self.est_rows_per_line = rows as f64 / lines as f64;
        self.est_row_h = self.window.total_height() / rows as f64;
    }

    pub fn recompute_estimate(&mut self) {
        let rows_per_line = self.est_rows_per_line.max(f64::MIN_POSITIVE);
        let row_h = self.est_row_h;
        // No `.round()` on the line count: the incremental paths
        // (`apply_external_edit`, `apply_external_edit_detailed`) grow
        // above_h/below_h by exact `lines * rows_per_line * row_h` steps, so
        // rounding here makes every full rebuild jump the estimates by a few
        // pixels — and the render-side scroll clamp turns that into visible
        // caret jitter while typing. f64 keeps ~15 significant digits; a
        // million-line note introduces no visible error.
        self.above_h = self.above_lines as f64 * rows_per_line * row_h;
        self.below_h = self.below_lines as f64 * rows_per_line * row_h;
    }

    pub fn ensure_visual_cache_with_headings(
        &mut self,
        doc: &mut DocumentBuffer,
        word_wrap: bool,
        wrap_cols: usize,
        typography: Option<&crate::settings::EditorTypography>,
        anchor_line: usize,
        suppress_selection_reveal: bool,
    ) {
        let reveal_state = self.selection_reveal_state(doc, suppress_selection_reveal);
        // Entering or leaving an image line swaps between the picture height
        // and a plain text row. The height index has to be rebuilt for that,
        // otherwise the old picture-sized row leaves a blank strip.
        let cursor_line_now = self.line_col(doc).0;
        if cursor_line_now != self.cursor_line {
            let is_image = |line: usize| {
                crate::images::parse_image_reference(&doc.line_text(line)).is_some()
            };
            if is_image(cursor_line_now) || is_image(self.cursor_line) {
                self.visual_cache_dirty = true;
            }
        }
        if suppress_selection_reveal
            && (!self.drag_revealed_tables.is_empty() || !self.drag_revealed_images.is_empty())
        {
            self.drag_revealed_tables.clear();
            self.drag_revealed_images.clear();
            self.visual_cache_dirty = true;
        }
        if reveal_state != self.revealed_selection {
            let prev_state = self.revealed_selection;
            self.revealed_selection = reveal_state;
            let tables =
                doc.ensure_table_index_for_window(self.window.start_line, self.window.end_line);
            let hits_table = |state: Option<(usize, usize, usize)>| {
                tables
                    .blocks
                    .iter()
                    .any(|t| Self::reveals_table(state, t))
            };
            if hits_table(prev_state) || hits_table(reveal_state) {
                self.visual_cache_dirty = true;
            }
        }

        if !self.raw_tables.is_empty() {
            let (cursor_line, _) = self.line_col(doc);
            if cursor_line != self.cursor_line {
                let prev_line = self.cursor_line;
                self.cursor_line = cursor_line;
                let tables = doc.ensure_table_index_for_window(self.window.start_line, self.window.end_line);
                let prev_in_raw = tables.at(prev_line).map_or(false, |t| self.raw_tables.contains(&t.first_line) && t.contains(prev_line));
                let curr_in_raw = tables.at(cursor_line).map_or(false, |t| self.raw_tables.contains(&t.first_line) && t.contains(cursor_line));
                if prev_in_raw != curr_in_raw || (prev_in_raw && curr_in_raw && tables.at(prev_line).map(|t| t.first_line) != tables.at(cursor_line).map(|t| t.first_line)) {
                    self.visual_cache_dirty = true;
                }
            }
        }

        if self.visual_cache_dirty
            || self.window.should_move_to(
                anchor_line,
                doc.line_index.lines(),
                word_wrap,
                wrap_cols,
            )
        {
            self.rebuild_visual_cache_with_headings(
                doc,
                word_wrap,
                wrap_cols,
                typography,
                anchor_line,
                suppress_selection_reveal,
            );
        }
        // Runs on every ensure: the caret may have moved without rebuilding the
        // window, and only the lines it left or entered need a new wrap.
        self.sync_line_paints(doc, typography, suppress_selection_reveal);
    }

    pub fn rebuild_visual_cache(
        &mut self,
        doc: &mut DocumentBuffer,
        word_wrap: bool,
        wrap_cols: usize,
    ) {
        self.rebuild_visual_cache_with_headings(doc, word_wrap, wrap_cols, None, 0, false);
    }

    pub fn ensure_visual_cache(
        &mut self,
        doc: &mut DocumentBuffer,
        word_wrap: bool,
        wrap_cols: usize,
        anchor_line: usize,
    ) {
        self.ensure_visual_cache_with_headings(doc, word_wrap, wrap_cols, None, anchor_line, false);
    }

    /// Handles `BufferEvent::Edited` from another view/panel according to Section 5.
    /// Invariant R5: Visible text does not move when edits occur outside or above the viewport.
    pub fn apply_external_edit(
        &mut self,
        edit: Edit,
        inserted_newlines: usize,
        removed_newlines: usize,
    ) {
        // 1. Transform viewport anchor
        let old_anchor_offset = self.viewport.offset;
        self.viewport.offset = transform_offset(self.viewport.offset, edit, Bias::Right);

        // 2. Transform cursor and selection
        if let Some(anchor) = self.selection_anchor {
            let (new_c, new_a) = transform_selection_range((self.cursor_offset, anchor), edit);
            self.cursor_offset = new_c;
            self.selection_anchor = Some(new_a);
        } else {
            self.cursor_offset = transform_offset(self.cursor_offset, edit, Bias::Left);
        }

        // 3. Invalidate visual cache for re-layout
        self.visual_cache_dirty = true;

        // 4. Update scroll_top and above/below estimates based on whether the edit was above the viewport
        if edit.start < old_anchor_offset {
            // Edit occurred above current viewport anchor.
            let line_diff = (inserted_newlines as isize) - (removed_newlines as isize);
            let h_diff = (line_diff as f64) * self.est_rows_per_line * self.est_row_h;

            self.scroll_top = (self.scroll_top + h_diff).max(0.0);
            self.above_h = (self.above_h + h_diff).max(0.0);
            if line_diff >= 0 {
                self.above_lines += line_diff as usize;
            } else {
                self.above_lines = self.above_lines.saturating_sub((-line_diff) as usize);
            }
        } else {
            // Edit occurred inside or below visible viewport: scroll_top stays EXACTLY the same!
            // Visible text does not shift by even 1px (R5).
        }
    }

    /// Handles a concrete `NoteEditEvent` from another active view/panel.
    /// Invariant R5: Visible text does not move when edits occur outside or above the viewport.
    /// Recalculates gutter line numbers, shifts viewport anchor and scroll_top without visible drift.
    ///
    /// A same-line-count edit of a pipe-free line inside this window updates
    /// the cached line in place — without this, every keystroke in one panel
    /// fully rebuilt the window of every other panel showing the same note.
    pub fn apply_external_edit_detailed(&mut self, doc: &DocumentBuffer, edit: &crate::state::NoteEditEvent) {
        let edit_obj = Edit {
            start: edit.at,
            old_len: edit.old_len,
            new_len: edit.new_len,
        };
        let line_diff = edit.line_diff;
        let byte_diff = (edit.new_len as isize) - (edit.old_len as isize);
        let line_h = self.est_rows_per_line * self.est_row_h;
        let h_diff = (line_diff as f64) * line_h;
        if line_diff != 0 {
            self.shift_raw_tables(edit.edit_line, line_diff);
            self.shift_line_paints(edit.edit_line, line_diff);
            if let Some(line) = self.reveal_all_line {
                self.reveal_all_line = Some(if line >= edit.edit_line {
                    (line as isize + line_diff).max(0) as usize
                } else {
                    line
                });
            }
        }

        // 1. Transform cursor and selection
        if let Some(anchor) = self.selection_anchor {
            let (new_c, new_a) = transform_selection_range((self.cursor_offset, anchor), edit_obj);
            self.cursor_offset = new_c;
            self.selection_anchor = Some(new_a);
        } else {
            self.cursor_offset = transform_offset(self.cursor_offset, edit_obj, Bias::Left);
        }

        // 2. Viewport and window adjustments based on edit position
        if self.window.is_empty() {
            if edit.edit_line < self.viewport.offset {
                self.viewport.offset = (self.viewport.offset as isize + line_diff).max(0) as usize;
                self.scroll_top = (self.scroll_top + h_diff).max(0.0);
                self.above_h = (self.above_h + h_diff).max(0.0);
                if line_diff >= 0 {
                    self.above_lines += line_diff as usize;
                } else {
                    self.above_lines = self.above_lines.saturating_sub((-line_diff) as usize);
                }
            }
            self.visual_cache_dirty = true;
            return;
        }

        if edit.edit_line < self.window.start_line {
            // Edit occurred strictly above the window:
            // Text inside window does not change; only line numbers, byte bounds, anchor line,
            // above_lines, above_h and scroll_top shift by exactly line_diff / h_diff.
            self.viewport.offset = (self.viewport.offset as isize + line_diff).max(0) as usize;
            self.above_lines = (self.above_lines as isize + line_diff).max(0) as usize;
            self.above_h = (self.above_h + h_diff).max(0.0);
            self.scroll_top = (self.scroll_top + h_diff).max(0.0);
            self.window.shift_lines_and_bytes(line_diff, byte_diff);
            self.visual_cache_dirty = false;
        } else if edit.edit_line < self.window.end_line {
            // Edit occurred inside the window:
            if edit.edit_line < self.viewport.offset {
                self.viewport.offset = (self.viewport.offset as isize + line_diff).max(0) as usize;
                self.scroll_top = (self.scroll_top + h_diff).max(0.0);
                self.above_h = (self.above_h + h_diff).max(0.0);
                if line_diff >= 0 {
                    self.above_lines += line_diff as usize;
                } else {
                    self.above_lines = self.above_lines.saturating_sub((-line_diff) as usize);
                }
            }
            let byte_diff = edit.new_len as isize - edit.old_len as isize;
            if edit.line_diff == 0 && self.update_window_line_after_edit(doc, edit.edit_line, byte_diff) {
                // Line updated in place; the window stays valid.
            } else {
                self.window = EditorWindow::default();
                self.visual_cache_dirty = true;
            }
        } else {
            // Edit occurred strictly below the window:
            if line_diff >= 0 {
                self.below_lines += line_diff as usize;
            } else {
                self.below_lines = self.below_lines.saturating_sub((-line_diff) as usize);
            }
            self.below_h = (self.below_lines as f64) * line_h;
            self.visual_cache_dirty = false;
        }
    }

    #[inline]
    pub fn total_height(&self) -> f64 {
        self.above_h + self.window.total_height() + self.below_h
    }

    pub fn line_at_doc_y(&self, lines_count: usize, doc_y: f64) -> usize {
        let last_line = lines_count.saturating_sub(1);
        if self.window.visual_rows.is_empty() {
            let row_h = self.est_row_h.max(1.0);
            return ((doc_y.max(0.0) / row_h) as usize).min(last_line);
        }
        let doc_y = doc_y.max(0.0);
        let local = doc_y - self.above_h;
        if local < 0.0 {
            // Exact inverse of `doc_y_of_line`'s above-window branch (the
            // above_h / above_lines ratio). With anchored margins the frozen
            // per-row estimate no longer describes the region outside the
            // window, and an inconsistent inverse would resolve the same
            // scroll Y to a different line every frame — the window would
            // chase its own anchor.
            if self.above_lines == 0 || self.above_h <= 0.0 {
                return 0;
            }
            let line = (doc_y / self.above_h * self.above_lines as f64).floor() as usize;
            return line.min(last_line);
        }
        let window_h = self.window.total_height();
        if local >= window_h {
            // Mirror of the below-window branch: window_bottom plus
            // (lines_below / below_lines) × below_h.
            if self.below_lines == 0 {
                return last_line;
            }
            let window_bottom = self.above_h + window_h;
            let line = self.window.end_line
                + ((doc_y - window_bottom) / self.below_h * self.below_lines as f64).floor() as usize;
            return line.min(last_line);
        }
        let local_v = self.window.height_index.v_idx_at_y(local);
        self.window
            .visual_rows
            .get(local_v)
            .map(|v| v.line_idx)
            .unwrap_or(self.window.start_line)
    }
}
