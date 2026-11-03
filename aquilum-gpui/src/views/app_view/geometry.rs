use super::*;

impl AppView {
    pub(crate) fn refresh_monitor_width(&mut self, window: &Window, cx: &gpui::App) {
        if let Some(display) = window.display(cx) {
            let width = f32::from(display.bounds().size.width);
            if width > 0.0 {
                self.monitor_width_px = width;
            }
        }
    }

    fn editor_chrome_width(&self) -> f32 {
        64.0 + 48.0 + if self.config.editor.wide_scrollbar { 16.0 } else { 14.0 }
    }

    pub(crate) fn window_content_width(&self, window_width: f32) -> f32 {
        (window_width - self.editor_chrome_width()).max(40.0)
    }

    pub(crate) fn note_width_target_px(&self, slider: u32, window_width: f32) -> u32 {
        // Use the whole editor area between the sidebars, not the raw window:
        // keeping the sidebars open must not make 100 px wider than the space
        // that is actually left for text.
        let editor_area_w = self.pane_width_for_leaf(None, window_width);
        crate::settings::note_width_px(
            slider,
            self.monitor_width_px,
            self.window_content_width(editor_area_w),
        )
    }

    /// Width the active leaf can actually use after pane divider, padding,
    /// gutter and scrollbar. Sidebars and the split ratio are already part of
    /// the pane width.
    pub(crate) fn active_note_width_px(&self, slider: u32, window_width: f32) -> u32 {
        let target = self.note_width_target_px(slider, window_width);
        let max_content_w = self.max_content_width_for_leaf(
            Some(self.state.workspace.active_leaf),
            window_width,
        );
        (target as f32).min(max_content_w).round() as u32
    }

    pub(crate) fn max_content_width_for_leaf(
        &self,
        leaf_id: Option<crate::workspace::LeafId>,
        window_width: f32,
    ) -> f32 {
        let pane_w = self.pane_width_for_leaf(leaf_id, window_width);
        let width_ratio = leaf_id
            .map(|lid| self.state.workspace.horizontal_width_ratio_of_leaf(lid))
            .unwrap_or(1.0);
        let divider_w = if width_ratio < 0.999 { 4.0 } else { 0.0 };
        (pane_w - divider_w - self.editor_chrome_width()).max(40.0)
    }

    pub(crate) fn note_content_width(&self, max_content_w: f32, window_width: f32) -> f32 {
        let target = self.note_width_target_px(self.config.editor.editor_width_pct, window_width) as f32;
        target.min(max_content_w)
    }

    pub(crate) fn pane_width_for_leaf(&self, leaf_id: Option<crate::workspace::LeafId>, window_width: f32) -> f32 {
        let editor_left_w = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
        let editor_right_w = if self.right_sidebar_collapsed {
            0.0
        } else {
            self.right_sidebar_width + 1.0
        };
        let workspace_w = (window_width - editor_left_w - editor_right_w).max(100.0);
        let width_ratio = leaf_id
            .map(|lid| self.state.workspace.horizontal_width_ratio_of_leaf(lid))
            .unwrap_or(1.0);
        (workspace_w * width_ratio).max(100.0)
    }

    pub(crate) fn note_margins_for_leaf(&self, leaf_id: Option<crate::workspace::LeafId>, window_width: f32) -> (f32, f32) {
        let max_content_w = self.max_content_width_for_leaf(leaf_id, window_width);
        let content_w = self.note_content_width(max_content_w, window_width);
        let extra_space = (max_content_w - content_w).max(0.0);

        match self.config.editor.note_align {
            crate::settings::NoteAlignment::Left => (0.0, extra_space.round()),
            crate::settings::NoteAlignment::Center => {
                let half = (extra_space / 2.0).round();
                (half, half)
            }
        }
    }

    pub(crate) fn note_margin_left_for_leaf(&self, leaf_id: Option<crate::workspace::LeafId>, window_width: f32) -> f32 {
        self.note_margins_for_leaf(leaf_id, window_width).0
    }

    pub fn set_note_align(&mut self, align: crate::settings::NoteAlignment, cx: &mut Context<Self>) {
        if self.config.editor.note_align == align {
            return;
        }
        self.config.editor.note_align = align;
        self.save_settings(cx);
        for view in self.state.views.values_mut() {
            view.visual_cache_dirty = true;
        }
        cx.notify();
    }

    pub(crate) fn note_title_offset_for_leaf(&self, leaf_id: Option<crate::workspace::LeafId>, window_width: f32, title_text_w: f32) -> f32 {
        let pane_w = self.pane_width_for_leaf(leaf_id, window_width);
        let max_content_w = self.max_content_width_for_leaf(leaf_id, window_width);
        let content_w = self.note_content_width(max_content_w, window_width);
        let extra_space = (max_content_w - content_w).max(0.0);

        let note_margin_left = match self.config.editor.note_align {
            crate::settings::NoteAlignment::Left => 0.0,
            crate::settings::NoteAlignment::Center => (extra_space / 2.0).round(),
        };

        match self.config.editor.title_align {
            crate::settings::TitleAlignment::WindowLeft => 48.0,
            crate::settings::TitleAlignment::WindowCenter => {
                ((pane_w - title_text_w) / 2.0).max(48.0).round()
            }
            crate::settings::TitleAlignment::EditorLeft => {
                48.0 + note_margin_left
            }
            crate::settings::TitleAlignment::EditorCenter => {
                let note_center_offset = note_margin_left + 48.0 + ((content_w - title_text_w) / 2.0).round();
                note_center_offset.max(48.0 + note_margin_left)
            }
        }
    }

    pub fn set_title_align(&mut self, align: crate::settings::TitleAlignment, cx: &mut Context<Self>) {
        if self.config.editor.title_align == align {
            return;
        }
        self.config.editor.title_align = align;
        self.save_settings(cx);
        cx.notify();
    }

    pub(crate) fn find_bar_left_offset_for_leaf(&self, leaf_id: Option<crate::workspace::LeafId>, window_width: f32) -> f32 {
        let pane_w = self.pane_width_for_leaf(leaf_id, window_width);
        let (note_margin_left, note_margin_right) = self.note_margins_for_leaf(leaf_id, window_width);
        let note_center_x = (pane_w + note_margin_left - note_margin_right) / 2.0;
        let bar_w = 620.0_f32.min(pane_w - 32.0).max(100.0);
        let max_left = (pane_w - 16.0 - bar_w).max(16.0);
        (note_center_x - bar_w / 2.0).clamp(16.0, max_left).round()
    }

    pub(super) fn compute_wrap_cols_for_leaf(&self, leaf_id: Option<crate::workspace::LeafId>, window_width: f32, char_width: f32) -> usize {
        let char_w = if char_width > 1.0 { char_width } else { 7.7 };
        let max_content_w = self.max_content_width_for_leaf(leaf_id, window_width);
        let content_w = self.note_content_width(max_content_w, window_width);
        ((content_w / char_w).floor() as usize).max(20)
    }

    pub(super) fn compute_wrap_cols(&self, window_width: f32, char_width: f32) -> usize {
        self.compute_wrap_cols_for_leaf(Some(self.state.workspace.active_leaf), window_width, char_width)
    }

    /// Width in pixels the editor lets a table occupy. A table wider than this
    /// is fitted to it and its cells wrap instead of widening the note.
    pub(super) fn table_room_px(view: &crate::workspace::EditorViewState, char_width: f32) -> f32 {
        (view.window.wrap_cols as f32 * char_width).max(160.0)
    }

    /// Painted column budgets of every table intersecting the window.
    ///
    /// The row geometry cannot measure text, so the renderer — the only side
    /// that can — hands it these numbers. They are the same `TableLayout`
    /// values the visible rows are painted from, which is what makes a wrapped
    /// cell and the height the row claims the same thing.
    pub(super) fn collect_table_cell_cols(
        table_layouts: &std::cell::RefCell<std::collections::HashMap<usize, (u64, Arc<TableLayout>)>>,
        text_system: &gpui::WindowTextSystem,
        doc: &crate::document_buffer::DocumentBuffer,
        view: &crate::workspace::EditorViewState,
        font: &SharedString,
        manual: &HashMap<usize, Vec<f32>>,
        char_width: f32,
    ) -> HashMap<usize, Vec<usize>> {
        if view.window.is_empty() {
            return HashMap::new();
        }
        Self::collect_table_cell_cols_for_range(
            table_layouts,
            text_system,
            doc,
            view.window.start_line,
            view.window.end_line,
            font,
            manual,
            char_width,
            Self::table_room_px(view, char_width),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn collect_table_cell_cols_for_range(
        table_layouts: &std::cell::RefCell<std::collections::HashMap<usize, (u64, Arc<TableLayout>)>>,
        text_system: &gpui::WindowTextSystem,
        doc: &crate::document_buffer::DocumentBuffer,
        start: usize,
        end: usize,
        font: &SharedString,
        manual: &HashMap<usize, Vec<f32>>,
        char_width: f32,
        room: f32,
    ) -> HashMap<usize, Vec<usize>> {
        let mut out = HashMap::new();
        let tables = doc.ensure_table_index_for_window(start, end);
        for t in &tables.blocks {
            if t.last_line < start || t.first_line >= end {
                continue;
            }
            let lines: Vec<String> = (t.first_line..=t.last_line)
                .map(|l| doc.line_text(l))
                .collect();
            let get_line = |idx: usize| -> Option<&str> {
                idx.checked_sub(t.first_line).and_then(|i| lines.get(i)).map(|s| s.as_str())
            };
            let layout = Self::get_or_compute_table_layout(
                table_layouts,
                text_system,
                &get_line,
                t,
                font,
                manual.get(&t.first_line).map(|w| w.as_slice()),
                char_width,
                room,
            );
            out.insert(t.first_line, layout.col_chars.clone());
        }
        out
    }

    /// Screen rectangle of a note image block. Used to place the size/radius
    /// cards next to the picture, the same way the crop editor panel is placed.
    pub(crate) fn image_block_screen_rect(
        &self,
        line: usize,
        window: &Window,
    ) -> Option<(f32, f32, f32, f32)> {
        let (view, doc) = self.state.active_view_and_doc()?;
        let text = doc.line_text(line);
        let image = crate::images::resolve_for_note(
            &doc.path,
            &text,
            view.window.wrap_cols,
            false,
        )?;
        let v_idx = view.window.first_row_of_line(line)?;
        let row_doc_top = view.above_h + view.window.height_index.top_of(v_idx);
        let screen_y = self.editor_rows_top_y() - self.current_scroll_top_f64() as f32
            + row_doc_top as f32
            + crate::images::BLOCK_PADDING
            + crate::images::TOOLBAR_RESERVE;
        let text_start_x = self.pane_text_start_x(None, window);
        let center_margin = ((image.max_width - image.width) / 2.0).max(0.0);
        Some((
            text_start_x + center_margin,
            screen_y,
            image.width,
            image.height,
        ))
    }

    pub(crate) fn pane_text_start_x(&self, leaf_id: Option<crate::workspace::LeafId>, window: &Window) -> f32 {
        let editor_left_x = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
        let editor_right_x = if self.right_sidebar_collapsed { 0.0 } else { self.right_sidebar_width + 1.0 };
        let window_w: f32 = window.bounds().size.width.into();
        let workspace_w = (window_w - editor_left_x - editor_right_x).max(0.0);
        let target_lid = leaf_id.unwrap_or(self.state.workspace.active_leaf);
        let (start_ratio, _) = self.state.workspace.horizontal_span_of_leaf(target_lid);
        let margin_left = self.note_margin_left_for_leaf(Some(target_lid), window_w);
        editor_left_x + start_ratio * workspace_w + margin_left + 32.0 + 8.0 + 40.0
    }

    pub fn set_editor_width_pct(&mut self, pct: u32, window: &Window, cx: &mut Context<Self>) {
        let clamped = pct.clamp(0, 100);
        if self.config.editor.editor_width_pct == clamped {
            return;
        }
        self.refresh_monitor_width(window, cx);
        self.config.editor.editor_width_pct = clamped;
        let viewport_w: f32 = window.viewport_size().width.into();
        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(editor_font));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);

        for (leaf_id, leaf) in self.state.leaves.iter() {
            if let Some(tab) = leaf.active_tab() {
                if let crate::workspace::TabContent::Document { id: doc_id } = tab.content {
                    let wrap_cols = self.compute_wrap_cols_for_leaf(Some(*leaf_id), viewport_w, line_char_width);
                    if let Some(doc) = self.state.documents.get_mut(&doc_id) {
                        if let Some(view) = self.state.views.get_mut(&tab.id) {
                            view.visual_cache_dirty = true;
                            view.ensure_visual_cache_with_headings(
                                doc,
                                self.word_wrap,
                                wrap_cols,
                                Some(&self.config.editor.headings),
                                view.viewport.offset,
                                self.ctrl_preview_selection || !self.edit_mode,
                            );
                        }
                    }
                }
            }
        }
        self.last_wrap_cols = self.compute_wrap_cols(viewport_w, line_char_width);
        let width_px = self.active_note_width_px(clamped, viewport_w);
        self.state.status_text = self.t_fmt("editor.note_width", &[&width_px.to_string()]);
        cx.notify();
    }

    pub fn render_note_width_slider(&self, theme: &Theme, cx: &Context<Self>) -> gpui::Stateful<Div> {
        const TRACK_W: f32 = 120.0;
        const THUMB_W: f32 = 16.0;

        let pct = self.config.editor.editor_width_pct.clamp(0, 100);
        let thumb_x = (TRACK_W - THUMB_W) * (pct as f32 / 100.0);
        let cell = self.note_width_slider_origin_x.clone();

        let track_bg = if theme.is_dark {
            gpui::rgba(0xffffff28)
        } else {
            gpui::rgba(0x00000022)
        };

        div()
            .id("status-note-width-container")
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            // Track area
            .child(
                div()
                    .id("status-note-width-track-wrapper")
                    .relative()
                    .w(px(TRACK_W))
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .child(
                        gpui::canvas(
                            move |bounds, _, _| {
                                cell.set(f32::from(bounds.origin.x));
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    // The track line
                    .child(
                        div()
                            .id("status-note-width-track")
                            .w_full()
                            .h(px(6.0))
                            .rounded_full()
                            .bg(track_bg),
                    )
                    // The circular thumb
                    .child(
                        div()
                            .id("status-note-width-thumb")
                            .absolute()
                            .left(px(thumb_x))
                            .top(px(2.0))
                            .w(px(THUMB_W))
                            .h(px(THUMB_W))
                            .rounded_full()
                            .bg(gpui::white())
                            .border_1()
                            .border_color(if theme.is_dark {
                                gpui::rgba(0x00000040)
                            } else {
                                gpui::rgba(0x00000020)
                            })
                            .shadow_sm(),
                    )
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let click_x: f32 = event.position.x.into();
                        let track_x = this.note_width_slider_origin_x.get();
                        let rel_x = click_x - track_x;
                        let new_pct = ((rel_x / TRACK_W) * 100.0).round().clamp(0.0, 100.0) as u32;
                        this.is_dragging_note_width_slider = true;
                        this.set_editor_width_pct(new_pct, window, cx);
                    }))
                    .on_scroll_wheel(cx.listener(move |this, event: &gpui::ScrollWheelEvent, window, cx| {
                        cx.stop_propagation();
                        let delta = match event.delta {
                            gpui::ScrollDelta::Lines(l) => l.y * 3.0,
                            gpui::ScrollDelta::Pixels(p) => f32::from(p.y) / 8.0,
                        };
                        let curr = this.config.editor.editor_width_pct as f32;
                        let next = (curr + delta).round().clamp(0.0, 100.0) as u32;
                        this.set_editor_width_pct(next, window, cx);
                        this.save_settings(cx);
                    })),
            )
            // Value badge (purple rectangle with white number, clicking cycles 30 → 45 → 80 → 100)
            .child(
                div()
                    .id("status-note-width-badge")
                    .h(px(20.0))
                    .min_w(px(28.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.0))
                    .bg(gpui::rgb(0x9873f7))
                    .hover(|h| h.bg(gpui::rgb(0x8b5cf6)))
                    .cursor_pointer()
                    .font_family("Segoe UI")
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_size(px(11.0))
                    .text_color(gpui::rgb(0xffffff))
                    .child(format!("{}", pct))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let next = crate::settings::next_note_width_preset(this.config.editor.editor_width_pct);
                        this.set_editor_width_pct(next, window, cx);
                        this.save_settings(cx);
                    })),
            )
    }

    /// The painted layout of one visual row, in the state its rows were wrapped
    /// for. Hidden markup occupies no column, so motion that must keep the
    /// visual x has to work in painted columns, not raw char offsets.
    pub(super) fn painted_row_for_visual_row(
        view: &crate::workspace::EditorViewState,
        line: &str,
        v: &crate::markdown::VisualRow,
        headings: &crate::settings::HeadingsSettings,
    ) -> crate::markdown::PaintedRow {
        let code_state = view
            .window
            .code_states
            .get(v.line_idx)
            .and_then(|s| s.as_ref())
            .map(|s| (s.kind, s.lang.as_deref()));
        let classified = crate::markdown::classify_line(line, code_state);
        let spec = view.line_paints.get(&v.line_idx).cloned().unwrap_or_default();
        crate::markdown::row_painted_text(line, &classified, v, spec.active, &spec.reveal, headings)
    }

    pub(super) fn move_cursor_up_visual(&mut self, shift: bool) {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let (cursor_line, cursor_col) = view.line_col(doc);
        let (curr_v_idx, col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));
        let headings = &self.config.editor.headings;
        let painted_col = {
            let curr_v = &visual_rows[curr_v_idx];
            let line = &view.window.lines[curr_v.line_idx];
            let painted = Self::painted_row_for_visual_row(view, line, curr_v, headings);
            crate::markdown::painted_column_of_line_char(&painted, curr_v.char_start + col_in_row)
        };

        if shift && view.selection_anchor.is_none() {
            view.selection_anchor = Some(view.cursor_offset);
        }

        if curr_v_idx == 0 {
            view.cursor_offset = 0;
            if !shift {
                view.selection_anchor = None;
            }
            return;
        }

        let mut target_idx = curr_v_idx - 1;
        if !shift {
            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            if let Some(tbl) = view.grid_table_at(doc, &tables, visual_rows[target_idx].line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                if visual_rows[target_idx].line_idx == tbl.sep_line() && target_idx > 0 {
                    target_idx -= 1;
                }
            }
        }
        let target_v = &visual_rows[target_idx];
        let target_line = &view.window.lines[target_v.line_idx];
        let target_painted = Self::painted_row_for_visual_row(view, target_line, target_v, headings);
        let target_columns = target_painted.columns();
        let max_col = if target_v.is_last_sub {
            target_columns
        } else {
            target_columns.saturating_sub(1)
        };
        let target_char = crate::markdown::line_char_at_painted_column(
            &target_painted,
            painted_col.min(max_col),
        );
        let offset = view.offset_for_line_col(doc, target_v.line_idx, target_char);
        view.cursor_offset = offset;
        if !shift {
            view.selection_anchor = None;
        }
    }

    pub(super) fn move_cursor_down_visual(&mut self, shift: bool) {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let (cursor_line, cursor_col) = view.line_col(doc);
        let (curr_v_idx, col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));
        let headings = &self.config.editor.headings;
        let painted_col = {
            let curr_v = &visual_rows[curr_v_idx];
            let line = &view.window.lines[curr_v.line_idx];
            let painted = Self::painted_row_for_visual_row(view, line, curr_v, headings);
            crate::markdown::painted_column_of_line_char(&painted, curr_v.char_start + col_in_row)
        };

        if shift && view.selection_anchor.is_none() {
            view.selection_anchor = Some(view.cursor_offset);
        }

        if curr_v_idx + 1 >= visual_rows.len() {
            view.cursor_offset = doc.len();
            if !shift {
                view.selection_anchor = None;
            }
            return;
        }

        let mut target_idx = curr_v_idx + 1;
        if !shift {
            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            if let Some(tbl) = view.grid_table_at(doc, &tables, visual_rows[target_idx].line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                if visual_rows[target_idx].line_idx == tbl.sep_line() && target_idx + 1 < visual_rows.len() {
                    target_idx += 1;
                }
            }
        }
        let target_v = &visual_rows[target_idx];
        let target_line = &view.window.lines[target_v.line_idx];
        let target_painted = Self::painted_row_for_visual_row(view, target_line, target_v, headings);
        let target_columns = target_painted.columns();
        let max_col = if target_v.is_last_sub {
            target_columns
        } else {
            target_columns.saturating_sub(1)
        };
        let target_char = crate::markdown::line_char_at_painted_column(
            &target_painted,
            painted_col.min(max_col),
        );
        let offset = view.offset_for_line_col(doc, target_v.line_idx, target_char);
        view.cursor_offset = offset;
        if !shift {
            view.selection_anchor = None;
        }
    }

    pub(super) fn move_cursor_home_visual(&mut self, shift: bool) {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }
        let (cursor_line, cursor_col) = view.line_col(doc);
        let (curr_v_idx, _) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));
        let v = &visual_rows[curr_v_idx];

        if shift && view.selection_anchor.is_none() {
            view.selection_anchor = Some(view.cursor_offset);
        }

        let target_char = if cursor_col == v.char_start && v.sub_idx > 0 {
            0
        } else {
            v.char_start
        };
        view.cursor_offset = view.offset_for_line_col(doc, v.line_idx, target_char);
        if !shift {
            view.selection_anchor = None;
        }
    }

    pub(super) fn move_cursor_end_visual(&mut self, shift: bool) {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }
        let (cursor_line, cursor_col) = view.line_col(doc);
        let (curr_v_idx, _) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));
        let v = &visual_rows[curr_v_idx];

        if shift && view.selection_anchor.is_none() {
            view.selection_anchor = Some(view.cursor_offset);
        }

        let line_len = view.window.lines.get(v.line_idx).map(|l| l.chars().count()).unwrap_or(0);
        let sub_end = if v.is_last_sub {
            v.char_end
        } else {
            v.char_end.saturating_sub(1)
        };
        let target_char = if cursor_col >= sub_end && !v.is_last_sub {
            line_len
        } else {
            sub_end
        };
        view.cursor_offset = view.offset_for_line_col(doc, v.line_idx, target_char);
        if !shift {
            view.selection_anchor = None;
        }
    }

    pub fn heading_typography_metrics(
        classified: &crate::markdown::LineKind<'_>,
        headings: &crate::settings::HeadingsSettings,
        base_line_h: f32,
        base_char_w: f32,
    ) -> (f32, f32, f32, f32, f32) {
        Self::heading_typography_metrics_with_widths(classified, headings, base_line_h, base_char_w, None)
    }

    pub fn heading_typography_metrics_with_widths(
        classified: &crate::markdown::LineKind<'_>,
        headings: &crate::settings::HeadingsSettings,
        base_line_h: f32,
        base_char_w: f32,
        heading_char_widths: Option<&[f32; 6]>,
    ) -> (f32, f32, f32, f32, f32) {
        let (h_size, is_heading, h_idx) = match classified {
            crate::markdown::LineKind::Heading1(_) => (headings.h1.font_size as f32, true, 0),
            crate::markdown::LineKind::Heading2(_) => (headings.h2.font_size as f32, true, 1),
            crate::markdown::LineKind::Heading3(_) => (headings.h3.font_size as f32, true, 2),
            crate::markdown::LineKind::Heading4(_) => (headings.h4.font_size as f32, true, 3),
            crate::markdown::LineKind::Heading5(_) => (headings.h5.font_size as f32, true, 4),
            crate::markdown::LineKind::Heading6(_) => (headings.h6.font_size as f32, true, 5),
            _ => (14.0, false, 0),
        };
        if is_heading {
            let row_h = crate::markdown::heading_row_height(h_size, base_line_h);
            let char_w = heading_char_widths
                .and_then(|w| w.get(h_idx).copied())
                .unwrap_or_else(|| base_char_w * (h_size / 14.0));
            let cursor_h = (h_size * 0.88).round().max(16.0);
            let cursor_top = ((row_h - cursor_h) / 2.0).round().max(2.0);
            (row_h, char_w, h_size, cursor_h, cursor_top)
        } else {
            (base_line_h, base_char_w, 14.0, 16.0, 4.0)
        }
    }

    pub fn resolve_heading_char_widths(
        text_system: &gpui::TextSystem,
        editor_font: &gpui::SharedString,
        headings: &crate::settings::HeadingsSettings,
        base_char_w: f32,
    ) -> [f32; 6] {
        let heading_configs = [
            (&headings.h1, 0),
            (&headings.h2, 1),
            (&headings.h3, 2),
            (&headings.h4, 3),
            (&headings.h5, 4),
            (&headings.h6, 5),
        ];
        let mut widths = [base_char_w; 6];
        for (h_cfg, idx) in heading_configs {
            let mut font = gpui::font(editor_font.clone());
            if h_cfg.bold {
                font = font.bold();
            }
            if h_cfg.italic {
                font = font.italic();
            }
            let font_id = text_system.resolve_font(&font);
            let h_size = h_cfg.font_size as f32;
            let char_w = text_system
                .advance(font_id, gpui::px(h_size), 'x')
                .map(|s| f32::from(s.width))
                .unwrap_or(base_char_w * (h_size / 14.0));
            widths[idx] = char_w;
        }
        widths
    }

    
/// Scrolls the viewport the least amount that brings the caret into view.
pub fn scroll_cursor_into_view_with_strategy(&mut self, _strategy: ScrollStrategy) {
    // handle_key updates the visual cache after text changes
    // and cursor movement.
    let target = self
        .state
        .active_view_and_doc()
        .map(|(view, doc)| view.line_col(doc));
    if let Some((line, col)) = target {
        self.scroll_visual_row_into_view(line, col);
    }
}

/// Scrolls the viewport the least amount that brings the row of `line` at
/// `col` into view.
///
/// The row can be anywhere — the caret after `Ctrl+End`, a search hit, a note
/// restored mid-file — while the window follows the viewport. Revealing it
/// needs its row, so the window is placed on `line` first; after the reveal
/// the viewport shows it, so that anchor is the right one for the next frame
/// too. Find uses it to reveal a match without moving the document caret.
pub(super) fn scroll_visual_row_into_view(&mut self, line: usize, col: usize) {
    let measured_h = f32::from(self.editor_scroll_handle.bounds().size.height);

    let viewport_h = if measured_h > 50.0 {
        measured_h
    } else {
        self.last_editor_h.get().max(100.0)
    };

    // Row geometry of the target row, in document coordinates. Scoped so the
    // note borrow ends before the scroll position is read and written through
    // `self`.
    let geometry = {
        let word_wrap = self.word_wrap;
        let wrap_cols = self.last_wrap_cols;
        let (view, doc) = match self.state.active_view_and_doc_mut() {
            Some(p) => p,
            None => return,
        };
        let anchor = if view.window.covers_line(line) && view.window.covers_line(view.viewport.offset) {
            view.viewport.offset
        } else {
            view.viewport.offset = line;
            line
        };
        view.ensure_visual_cache_with_headings(
            doc,
            word_wrap,
            wrap_cols,
            Some(&self.config.editor.headings),
            anchor,
            self.ctrl_preview_selection || !self.edit_mode,
        );
        crate::markdown::find_visual_row_for_cursor(&view.window.visual_rows, line, col)
        .map(|(v_idx, _)| {
            let top = view.doc_y_of_local_v_idx(v_idx);
            (
                top,
                top + f64::from(view.window.height_index.height_of(v_idx)),
                view.total_height(),
            )
        })
    };

    let Some((row_top, row_bottom, total_content_h)) = geometry else {
        return;
    };

    // Content height without extra bottom padding.
    let max_scroll = (total_content_h - f64::from(viewport_h)).max(0.0);

    let raw_scroll = self.current_scroll_top_f64();
    let current_scroll = raw_scroll.clamp(0.0, max_scroll);
    let viewport_bottom = current_scroll + f64::from(viewport_h);

    // No extra padding at the top or bottom.
    // Scroll only when the row goes beyond the boundaries.
    let target_scroll = if row_top < current_scroll {
        row_top
    } else if row_bottom > viewport_bottom {
        row_bottom - f64::from(viewport_h)
    } else {
        current_scroll
    }
    .clamp(0.0, max_scroll);

    // Correct a stale position after the content height changes.
    // Ignore subpixel changes to avoid provoking jitter.
    if (target_scroll - raw_scroll).abs() > 0.5 {
        self.set_editor_scroll_top_f64(target_scroll);
    }
}




    pub(super) fn scroll_cursor_into_view(&mut self) {
        self.scroll_cursor_into_view_with_strategy(ScrollStrategy::Top);
    }

    /// Keeps the visual cache and the scroll position in sync after the cursor
    /// moved without an edit (e.g. inside a table), so table navigation follows
    /// the same path as ordinary cursor movement.
    pub(super) fn refresh_view_after_cursor_move(&mut self) {
        if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
            let (cursor_line, _) = view.line_col(doc);
            let anchor = if view.window.covers_line(cursor_line) && view.window.covers_line(view.viewport.offset) {
                view.viewport.offset
            } else {
                view.viewport.offset = cursor_line;
                cursor_line
            };
            let word_wrap = self.word_wrap;
            let wrap_cols = self.last_wrap_cols;
            view.ensure_visual_cache_with_headings(doc, word_wrap, wrap_cols, Some(&self.config.editor.headings), anchor, self.ctrl_preview_selection || !self.edit_mode);
        }
        self.scroll_cursor_into_view();
    }

    /// Moves the editor to `scroll_y`.
    ///
    /// Any write through here also cancels an in-flight wheel ease: the
    /// position came from somewhere else (a restore, a thumb drag, a cursor
    /// reveal, the keyboard), so there is nothing left to ease towards.
    pub(crate) fn set_editor_scroll_top(&self, scroll_y: f32) {
        self.set_editor_scroll_top_f64(scroll_y as f64);
    }

    pub(crate) fn set_editor_scroll_top_f64(&self, scroll_y: f64) {
        self.wheel_ease.set(None);
        self.editor_scroll.set(scroll_y.max(0.0));
    }

    /// Owns the editor wheel.
    ///
    /// With `editor.wheelSmoothScroll` off (the default) every notch applies
    /// instantly: when the wheel stops, the text stops — no settle tail. With
    /// the flag on, a notch starts a ~90 ms ease-out with a hard landing
    /// instead, so the two behaviours can be compared live.
    ///
    /// The editor's rows are painted as a slice anchored near y = 0 and shifted
    /// by the leftover of the scroll position, so nothing else may move them:
    /// the list no longer uses gpui's own scrolling (`overflow_hidden`), and the
    /// built-in wheel listener ignores an element that is not `Overflow::Scroll`,
    /// so this listener is the only writer.
    pub fn handle_editor_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        max_scroll: f64,
        cx: &mut Context<Self>,
    ) {
        if max_scroll <= 1.0 {
            return;
        }
        let current = self.current_scroll_top_f64().clamp(0.0, max_scroll);
        match event.delta {
            gpui::ScrollDelta::Pixels(p) => {
                if f32::from(p.y) == 0.0 {
                    return;
                }
                // A trackpad delivers its own smooth stream: it always applies
                // directly and takes over from any running ease.
                let next = (current - f64::from(f32::from(p.y))).clamp(0.0, max_scroll);
                self.set_editor_scroll_top_f64(next);
            }
            gpui::ScrollDelta::Lines(l) => {
                if l.y == 0.0 {
                    return;
                }
                let delta = -f64::from(l.y) * f64::from(WHEEL_LINE_PX);
                if !self.config.editor.wheel_smooth_scroll {
                    let next = (current + delta).clamp(0.0, max_scroll);
                    self.set_editor_scroll_top_f64(next);
                } else {
                    // Glide chain: a notch arriving mid-glide inherits its
                    // speed and extends the accumulated target when it
                    // scrolls on in the same direction, so a notch stream
                    // reads as one continuous motion; a reversal re-anchors
                    // at the live position and the old motion dies out
                    // gently instead of jumping. The duration adapts to the
                    // notch cadence (2 × mean of the last three intervals,
                    // clamped).
                    let now = std::time::Instant::now();
                    let (duration_secs, history) =
                        wheel_glide_duration_secs(self.wheel_notch_history.get(), now);
                    self.wheel_notch_history.set(history);
                    let (base, velocity) = match self.wheel_ease.get() {
                        Some(glide) => {
                            let v = glide.velocity_at(now);
                            if (glide.target - current) * delta > 0.0 {
                                (glide.target, v)
                            } else {
                                (current, v)
                            }
                        }
                        None => (current, 0.0),
                    };
                    let target = (base + delta).clamp(0.0, max_scroll);
                    if (target - current).abs() > 0.01 {
                        self.wheel_ease
                            .set(Some(WheelEase::new(current, target, velocity, duration_secs)));
                    } else {
                        self.wheel_ease.set(None);
                    }
                }
            }
        }
        cx.notify();
    }

    /// Steps the optional wheel glide one frame towards its landing. Called from
    /// `render` when `editor.wheelSmoothScroll` is on.
    ///
    /// The glide has a hard deadline: past `duration` the position lands
    /// exactly on `target` and the animation is over — no sub-pixel creep, no
    /// final re-settle.
    pub(super) fn step_wheel_ease(&self, window: &mut Window) {
        let Some(anim) = self.wheel_ease.get() else {
            return;
        };
        let now = std::time::Instant::now();
        if now.duration_since(anim.t0).as_secs_f32() >= anim.duration_secs {
            self.wheel_ease.set(None);
            // Direct write, not through `set_editor_scroll_top_f64`: that
            // would cancel this very landing.
            self.editor_scroll.set(anim.target.max(0.0));
            return;
        }
        window.request_animation_frame();
        // Direct write for the same reason as above.
        self.editor_scroll.set(anim.position_at(now).max(0.0));
    }

    /// Starts middle-button smart scroll for the editor.
    ///
    /// While active, the absolute cursor Y inside the editor viewport drives
    /// `editor_scroll` (same idea as the sidebar's Ctrl smart scroll): cursor
    /// near the top shows the beginning, near the bottom the end. The first
    /// mouse-move only primes the reference point so the content never jumps
    /// on entry.
    pub(crate) fn start_editor_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if self.editor_middle_active {
            return;
        }
        self.editor_middle_active = true;
        self.editor_middle_last_y.set(None);
        cx.notify();
    }

    /// Stops editor middle-button smart scroll (idempotent).
    pub(crate) fn stop_editor_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if !self.editor_middle_active {
            return;
        }
        self.editor_middle_active = false;
        self.editor_middle_last_y.set(None);
        cx.notify();
    }

    /// Follows the cursor while editor middle-button smart scroll is active.
    ///
    /// Mirrors `on_sidebar_smart_mouse_move`: absolute mapping of the cursor Y
    /// inside the editor viewport to `[0, max_scroll]`, with an edge dead zone
    /// so the first/last row is fully visible before the cursor hits the
    /// border. Small jitter is ignored. Writes directly through
    /// `set_editor_scroll_top_f64`, which also ends any wheel ease.
    pub(crate) fn on_editor_middle_mouse_move(
        &self,
        cursor_y: f32,
        viewport_top: f32,
        viewport_h: f32,
        max_scroll: f64,
        cx: &mut Context<Self>,
    ) {
        if !self.editor_middle_active {
            return;
        }
        if max_scroll <= 1.0 || viewport_h <= 50.0 {
            cx.notify();
            return;
        }
        match self.editor_middle_last_y.get() {
            // First event only primes the reference point.
            None => self.editor_middle_last_y.set(Some(cursor_y)),
            Some(anchor) if (cursor_y - anchor).abs() >= 0.5 => {
                self.editor_middle_last_y.set(Some(cursor_y));
                let target = editor_middle_cursor_target(cursor_y, viewport_top, viewport_h, max_scroll);
                let current = self.current_scroll_top_f64();
                if (target - current).abs() > 0.5 {
                    self.set_editor_scroll_top_f64(target);
                }
            }
            Some(_) => {}
        }
        cx.notify();
    }

    /// Starts middle-button smart scroll for the sidebar tree.
    pub(crate) fn start_sidebar_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_middle_active {
            return;
        }
        self.sidebar_middle_active = true;
        // Prime the shared reference point so the list never jumps on entry.
        self.sidebar_smart_last_mouse_y.set(None);
        self.sidebar_smart_target_y.set(None);
        cx.notify();
    }

    /// Stops sidebar middle-button smart scroll (idempotent).
    pub(crate) fn stop_sidebar_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if !self.sidebar_middle_active {
            return;
        }
        self.sidebar_middle_active = false;
        self.sidebar_smart_last_mouse_y.set(None);
        self.sidebar_smart_target_y.set(None);
        cx.notify();
    }

    /// Starts middle-button smart scroll for an active plugin modal list.
    pub(crate) fn start_plugin_modal_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if self.plugin_modal_middle_active {
            return;
        }
        self.plugin_modal_middle_active = true;
        self.plugin_modal_smart_last_y.set(None);
        cx.notify();
    }

    /// Stops plugin modal middle-button smart scroll (idempotent).
    pub(crate) fn stop_plugin_modal_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if !self.plugin_modal_middle_active {
            return;
        }
        self.plugin_modal_middle_active = false;
        self.plugin_modal_smart_last_y.set(None);
        cx.notify();
    }

    /// Cursor-driven smart scroll for plugin modal lists while middle mouse button is held.
    pub(crate) fn on_plugin_modal_middle_mouse_move(&self, y: f32, cx: &mut Context<Self>) {
        if !self.plugin_modal_middle_active {
            self.plugin_modal_smart_last_y.set(None);
            cx.notify();
            return;
        }
        match self.plugin_modal_smart_last_y.get() {
            None => self.plugin_modal_smart_last_y.set(Some(y)),
            Some(anchor) if (y - anchor).abs() >= 0.5 => {
                self.plugin_modal_smart_last_y.set(Some(y));
                let mut bounds = self.plugin_modal_smart_bounds.get();
                if bounds.size.height <= px(20.0) {
                    bounds = self.plugin_modal_scroll_handle.bounds();
                }
                let viewport_h = f32::from(bounds.size.height);
                let viewport_top = f32::from(bounds.origin.y);
                let max_scroll = f32::from(self.plugin_modal_scroll_handle.max_offset().height);
                if viewport_h > 20.0 && max_scroll >= 1.0 {
                    let pad = (viewport_h * 0.10).clamp(16.0, 48.0).min(viewport_h / 3.0);
                    let usable = (viewport_h - pad * 2.0).max(1.0);
                    let t = ((y - viewport_top - pad) / usable).clamp(0.0, 1.0);
                    let target = (max_scroll * t).clamp(0.0, max_scroll);
                    self.plugin_modal_scroll_handle.set_offset(gpui::point(px(0.0), px(-target)));
                }
            }
            Some(_) => {}
        }
        cx.notify();
    }

    /// Follows pointer Y while dragging the plugin modal scrollbar thumb.
    pub(crate) fn handle_plugin_modal_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.plugin_modal_scrollbar_drag.active {
            return;
        }
        let style = crate::views::scrollbar::ScrollbarStyle {
            track_w: 16.0,
            thumb_w: 6.0,
            thumb_w_active: 8.0,
            thumb_inset: 3.0,
            min_thumb_h: 24.0,
            track_right_offset: 2.0,
        };
        let Some(geometry) = crate::views::scrollbar::ScrollbarGeometry::from_handle(&self.plugin_modal_scroll_handle, &style) else {
            return;
        };
        geometry.apply_drag(&self.plugin_modal_scroll_handle, &self.plugin_modal_scrollbar_drag, pointer_y);
        cx.notify();
    }

    /// Top of the editor's text rows in window coordinates.
    ///
    /// Measured rather than assumed: the note title panel sits above the rows
    /// and the find bar can be inserted between them, so a fixed header height
    /// resolves a click to a row two or three lines below the pointer whenever
    /// the bar is open. The handle is tracked by the active leaf's row list
    /// only, which is also the only leaf whose rows accept clicks. Before the
    /// first frame the handle has no bounds yet, so the old header heights are
    /// the fallback.
    pub(super) fn editor_rows_top_y(&self) -> f32 {
        let measured = f32::from(self.editor_scroll_handle.bounds().origin.y);
        if measured > 0.0 {
            return measured;
        }
        if self.state.workspace.is_leaf_in_titlebar(self.state.workspace.active_leaf) {
            90.0
        } else {
            126.0
        }
    }

    /// Viewport geometry for editor middle-button smart scroll.
    ///
    /// Returns `(viewport_top, viewport_h, max_scroll)` in window coordinates.
    /// `None` when there is no active note or nothing to scroll. Ensures the
    /// visual cache so `max_scroll` matches the painted content.
    pub(crate) fn editor_middle_metrics(&mut self) -> Option<(f32, f32, f64)> {
        let bounds = self.editor_scroll_handle.bounds();
        let bounds_h = f32::from(bounds.size.height);
        let viewport_h = if bounds_h > 50.0 {
            bounds_h
        } else {
            self.last_editor_h.get().max(100.0)
        };
        if viewport_h <= 50.0 {
            return None;
        }
        let viewport_top = if bounds.origin.y > px(0.0) {
            f32::from(bounds.origin.y)
        } else {
            132.0
        };
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return None;
        };
        let wrap_cols = if self.last_wrap_cols > 0 { self.last_wrap_cols } else { 80 };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let total = view.total_height();
        let max_scroll = (total - f64::from(viewport_h)).max(0.0);
        if max_scroll <= 1.0 {
            return None;
        }
        Some((viewport_top, viewport_h, max_scroll))
    }

    /// Thumb height for the editor scrollbar lane.
    ///
    /// Proportional to the viewport share, floored at 28 px so it stays
    /// grabbable. The Zed-style wide lane instead uses a fixed slim thumb —
    /// it marks position rather than viewport share, so it never resizes
    /// with the document length.
    pub(crate) fn editor_thumb_h(viewport_h: f32, total_content_h: f64, wide: bool) -> f64 {
        if wide {
            return WIDE_THUMB_H;
        }
        let ratio = (f64::from(viewport_h) / total_content_h.max(f64::from(viewport_h))).clamp(0.04, 1.0);
        (f64::from(viewport_h) * ratio).max(28.0)
    }

    /// Clamps a saved scroll target into the true scrollable range.
    ///
    /// The real max is `total - viewport`: the list paints a slice anchored near
    /// y = 0, so gpui's own `max_offset` describes the slice, not the document.
    pub(crate) fn clamp_restore_target(target: f64, total_h: f64, viewport_h: f64) -> f64 {
        let max = (total_h - viewport_h).max(0.0);
        target.clamp(0.0, max)
    }

    /// Document Y (px from the top of the content) of the visual row holding
    /// byte `offset`.
    ///
    /// Maps through line → char column → visual row, so an offset on a wrapped
    /// line resolves to its exact sub-row instead of the line top. Drives the
    /// overview-ruler marks on the editor scrollbar (cursor line, selection
    /// span). Out-of-range offsets clamp to the document, empty caches sit at
    /// zero.
    #[cfg(test)]
    pub(crate) fn editor_doc_y_for_offset(
        offset: usize,
        content: &str,
        line_index: &crate::line_index::LineIndex,
        above_h: f64,
        lines: &crate::editor_window::WindowLines,
        visual_rows: &[crate::markdown::VisualRow],
        height_index: &crate::markdown::VisualRowHeightIndex,
    ) -> f64 {
        if lines.is_empty() || visual_rows.is_empty() || line_index.is_empty() {
            return 0.0;
        }
        let offset = offset.min(content.len());
        let line_idx = line_index.line_of(content, offset);
        let line = match lines.get(line_idx) {
            Some(line) => line,
            // Outside the window — a selection endpoint can be anywhere. The
            // ruler mark only has to land in the right region, so clamp it to
            // the window's edge rather than guessing a row.
            None => {
                return if line_idx < lines.start() {
                    above_h
                } else {
                    above_h + height_index.total()
                }
            }
        };
        let mut byte_in_line = offset
            .saturating_sub(line_index.start(content, line_idx))
            .min(line.len());
        while byte_in_line > 0 && !line.is_char_boundary(byte_in_line) {
            byte_in_line -= 1;
        }
        let col = line[..byte_in_line].chars().count();
        let (v_idx, _) =
            crate::markdown::find_visual_row_for_cursor(visual_rows, line_idx, col).unwrap_or((0, 0));
        // The row index is window-local, the caller wants a document Y.
        above_h + height_index.top_of(v_idx)
    }

    /// Returns `(slice_h, viewport_h, max_scroll_content, track_top_y)`.
    ///
    /// `viewport_h` is the true viewport height — measured bounds, or the last
    /// known height before the first layout. It is never inflated: scroll math
    /// (max, thumb travel, cursor reveal) must match the real track, otherwise
    /// the thumb slides under the status bar and the cursor hides below the
    /// fold. `slice_h` is additionally floored at the live window size (which
    /// is current from frame one, like in the sidebar tree), so the first
    /// frames already cover the whole viewport instead of popping the last
    /// rows in later. Overshooting the slice only renders a few extra clipped
    /// rows.
    ///
    /// `max_scroll_content` is `f64`: it is an absolute offset into the
    /// document, and at 2×10^8 px an f32 cannot express it more finely than
    /// every 16 px.
    pub fn editor_scroll_metrics_for_leaf(
        &self,
        leaf_id: Option<crate::workspace::LeafId>,
        is_leaf_active: bool,
        total_content_h: f64,
        window_h: f32,
    ) -> (f32, f32, f64, f32) {
        let (leaf_v_ratio, in_titlebar) = if let Some(lid) = leaf_id {
            (
                self.state.workspace.vertical_height_ratio_of_leaf(lid),
                self.state.workspace.is_leaf_in_titlebar(lid),
            )
        } else {
            (1.0, true)
        };
        let workspace_h = (window_h - 66.0).max(100.0);
        let leaf_total_h = workspace_h * leaf_v_ratio;
        let header_h = if in_titlebar { 90.0 } else { 126.0 };
        let estimated_viewport_h = (leaf_total_h - header_h).max(50.0);

        let leaf_measured_h = leaf_id
            .and_then(|lid| self.state.leaves.get(&lid))
            .and_then(|leaf| leaf.active_tab())
            .and_then(|tab| self.state.views.get(&tab.id))
            .and_then(|view| view.measured_viewport_h)
            .filter(|&mh| (mh - estimated_viewport_h).abs() < (estimated_viewport_h * 0.35).max(40.0));

        let viewport_h = if is_leaf_active {
            let bounds = self.editor_scroll_handle.bounds();
            let bounds_h = f32::from(bounds.size.height);
            if bounds_h > 50.0
                && self.split_drag_state.is_none()
                && (bounds_h - estimated_viewport_h).abs() < (estimated_viewport_h * 0.35).max(40.0)
            {
                bounds_h
            } else if let Some(mh) = leaf_measured_h {
                mh
            } else {
                estimated_viewport_h
            }
        } else if let Some(mh) = leaf_measured_h {
            mh
        } else {
            estimated_viewport_h
        };

        let slice_h = viewport_h;
        let max_scroll_content = (total_content_h - viewport_h as f64).max(0.0);
        let track_top_y = if is_leaf_active && self.split_drag_state.is_none() {
            let bounds = self.editor_scroll_handle.bounds();
            let estimated_top_y = 38.0 + (workspace_h * leaf_id.map(|lid| self.state.workspace.vertical_ratio_of_leaf(lid)).unwrap_or(0.0)) + (if in_titlebar { 0.0 } else { 36.0 }) + 90.0;
            if bounds.origin.y > px(0.0) && (f32::from(bounds.origin.y) - estimated_top_y).abs() < 50.0 {
                f32::from(bounds.origin.y)
            } else {
                estimated_top_y
            }
        } else {
            let v_ratio_top = leaf_id
                .map(|lid| self.state.workspace.vertical_ratio_of_leaf(lid))
                .unwrap_or(0.0);
            38.0 + (workspace_h * v_ratio_top) + (if in_titlebar { 0.0 } else { 36.0 }) + 90.0
        };
        (slice_h, viewport_h, max_scroll_content, track_top_y)
    }

    pub fn editor_scroll_metrics(&self, total_content_h: f64, window_h: f32) -> (f32, f32, f64, f32) {
        let active_leaf = Some(self.state.workspace.active_leaf);
        self.editor_scroll_metrics_for_leaf(active_leaf, true, total_content_h, window_h)
    }

    pub(super) fn handle_scrollbar_drag(&mut self, current_y: f32, window: &Window, cx: &mut Context<Self>) {
        if !self.is_dragging_scrollbar {
            return;
        }
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let total_content_h = view.total_height();
        let window_h = f32::from(window.bounds().size.height);
        let (_, viewport_h, max_scroll_content, _) = self.editor_scroll_metrics(total_content_h, window_h);
        let thumb_h = Self::editor_thumb_h(viewport_h, total_content_h, self.config.editor.wide_scrollbar);
        let track_scrollable = (f64::from(viewport_h) - thumb_h).max(1.0);

        let delta_y = f64::from(current_y - self.scrollbar_drag_start_y);
        let delta_fraction = delta_y / track_scrollable;
        let new_scroll_y = (self.scrollbar_drag_start_scroll_y + delta_fraction * max_scroll_content)
            .clamp(0.0, max_scroll_content);
        self.set_editor_scroll_top_f64(new_scroll_y);
        cx.notify();
    }
}
