use super::*;

impl AppView {
    pub fn sync_views_for_active_note(&mut self) {}

    pub fn ensure_inactive_views_built(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_leaf_id = self.state.workspace.active_leaf;
        let viewport_w: f32 = window.viewport_size().width.into();
        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(editor_font.clone()));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);
        // Image blocks are measured while a visual window is built, where the
        // text system is not reachable. Publishing the advance keeps the height
        // index and the painted picture the same width.
        crate::images::set_char_width(line_char_width);

        let leaf_info: Vec<(crate::workspace::LeafId, crate::workspace::TabId, crate::document_buffer::DocumentId, usize)> = self
            .state
            .leaves
            .iter()
            .filter(|(id, _)| **id != active_leaf_id)
            .filter_map(|(id, leaf)| {
                let tab = leaf.active_tab()?;
                match tab.content {
                    crate::workspace::TabContent::Document { id: doc_id } => {
                        let wrap_cols = self.compute_wrap_cols_for_leaf(Some(*id), viewport_w, line_char_width);
                        Some((*id, tab.id, doc_id, wrap_cols))
                    }
                    _ => None,
                }
            })
            .collect();

        // Table cell budgets for every background pane, measured before the
        // state is borrowed mutably. Their rows are wrapped exactly like the
        // active pane's, so a split view does not paint a taller table than the
        // height its geometry claims. A pane that needs no rebuild keeps its
        // published budgets: measuring them again would only hash its tables.
        let word_wrap = self.word_wrap;
        let text_system = window.text_system().clone();
        let leaf_info: Vec<_> = leaf_info
            .into_iter()
            .map(|(leaf_id, tab_id, doc_id, wrap_cols)| {
                let cols = (|| {
                    let doc = self.state.documents.get(&doc_id)?;
                    let view = self.state.views.get(&tab_id)?;
                    let needs_build = view.window.is_empty()
                        || view.visual_cache_dirty
                        || view.window.should_move_to(
                            view.viewport.offset,
                            doc.line_index.lines(),
                            word_wrap,
                            wrap_cols,
                        )
                        || !view.line_paints.is_empty();
                    if !needs_build {
                        return None;
                    }
                    let (start, end) = if !view.window.is_empty()
                        && view.window.start_line < view.window.end_line
                    {
                        (view.window.start_line, view.window.end_line)
                    } else {
                        crate::editor_window::EditorWindow::bounds_for(
                            view.viewport.offset,
                            doc.line_index.lines(),
                        )
                    };
                    Some(Self::collect_table_cell_cols_for_range(
                        &self.table_layouts,
                        &text_system,
                        doc,
                        start,
                        end,
                        &editor_font,
                        &self.manual_table_col_widths,
                        line_char_width,
                        (wrap_cols as f32 * line_char_width).max(160.0),
                    ))
                })();
                (leaf_id, tab_id, doc_id, wrap_cols, cols)
            })
            .collect();

        let headings = &self.config.editor.headings;

        let crate::state::AppState {
            ref mut documents,
            ref mut views,
            ..
        } = self.state;

        for (_leaf_id, tab_id, doc_id, wrap_cols, table_cols) in leaf_info {
            let doc = match documents.get_mut(&doc_id) {
                Some(d) => d,
                None => continue,
            };

            let line_index = &doc.line_index;
            let mut checkpoints = doc.code_checkpoints.clone();

            let lines_total = line_index.lines();
            let view = views.entry(tab_id).or_insert_with(|| crate::workspace::EditorViewState::new(tab_id, doc_id));
            if let Some(cols) = table_cols {
                view.set_table_cell_cols(cols);
            }

            if view.window.is_empty()
                || view.visual_cache_dirty
                || view.window.should_move_to(view.viewport.offset, lines_total, word_wrap, wrap_cols)
            {
                let anchor_line = view.viewport.offset;
                let (start, end) = crate::editor_window::EditorWindow::bounds_for(anchor_line, lines_total);
                checkpoints.ensure_rope(doc.rope(), start + 1);
                let initial = checkpoints.state_at_rope(doc.rope(), start).unwrap_or_default();
                let start_byte = doc.line_start_byte(start);
                let end_byte = doc.line_end_byte(end.saturating_sub(1)).max(start_byte);
                let slice = doc.slice_to_string(start_byte..end_byte);

                let tables = doc.ensure_table_index_for_window(start, end);
                let new_window = crate::editor_window::EditorWindow::build_from_slice_with_tables(
                    &slice,
                    start,
                    end,
                    start_byte,
                    end_byte,
                    word_wrap,
                    wrap_cols,
                    Some(headings),
                    false,
                    initial,
                    |line, line_text| {
                        // A background pane never reveals a cell: its grid is
                        // painted with markup hidden, so no cell is active.
                        crate::workspace::EditorViewState::table_row_kind_at(
                            &tables,
                            &view.raw_tables,
                            &view.drag_revealed_tables,
                            &view.table_cell_cols,
                            None,
                            usize::MAX,
                            0,
                            false,
                            line,
                            line_text,
                            Some(headings),
                        )
                    },
                    |line, line_text| {
                        let show_original = crate::images::parse_image_reference(line_text)
                            .is_some_and(|reference| view.shows_original_image(line, &reference.target));
                        crate::images::block_height(&doc.path, line_text, wrap_cols, show_original)
                    },
                );
                view.above_lines = new_window.start_line;
                view.below_lines = lines_total.saturating_sub(new_window.end_line);
                let rows = new_window.visual_rows.len();
                let lines = new_window.line_count();
                if rows > 0 && lines > 0 {
                    view.est_rows_per_line = rows as f64 / lines as f64;
                    view.est_row_h = new_window.total_height() / rows as f64;
                }
                let rows_per_line = view.est_rows_per_line.max(f64::MIN_POSITIVE);
                let row_h = view.est_row_h;
                // Same formula as `recompute_estimate`, without `.round()`:
                // both paths must produce identical above_h/below_h, or every
                // rebuild here shifts the estimates and the caret jitters.
                view.above_h = view.above_lines as f64 * rows_per_line * row_h;
                view.below_h = view.below_lines as f64 * rows_per_line * row_h;
                let wrap_changed = view.window.wrap_cols != wrap_cols;
                view.window = new_window;
                view.headings_cache = headings.clone();
                view.visual_cache_dirty = false;
                if wrap_changed {
                    if let Some(first_v_idx) = view.window.first_row_of_line(view.viewport.offset) {
                        let line_local_y = view.window.height_index.top_of(first_v_idx);
                        view.scroll_top = view.above_h + line_local_y + view.viewport.px_in_line as f64;
                    }
                }
            } else if !view.line_paints.is_empty() {
                // A background pane is painted with markup hidden, so the
                // caret/selection reflow it still carries (it was focused when
                // the rows were wrapped) has to go: its rows must be the
                // canonical ones the renderer draws.
                view.ensure_visual_cache_with_headings(
                    doc,
                    word_wrap,
                    wrap_cols,
                    Some(headings),
                    view.viewport.offset,
                    true,
                );
            }
        }
    }

    pub fn switch_to_leaf(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_idx = self.state.leaves.get(&leaf_id).map(|l| l.active_tab_index).unwrap_or(0);
        self.select_leaf_tab(leaf_id, active_idx, window, cx);
    }

    pub fn select_leaf_tab(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_leaf_tab_internal(leaf_id, tab_index, false, window, cx);
    }

    pub fn select_leaf_tab_forced(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_leaf_tab_internal(leaf_id, tab_index, true, window, cx);
    }

    pub(super) fn select_leaf_tab_internal(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        tab_index: usize,
        forced: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_leaf_id = self.state.workspace.active_leaf;
        let is_same_leaf = current_leaf_id == leaf_id;
        let same_tab = is_same_leaf && self.state.leaves.get(&leaf_id).map(|l| l.active_tab_index) == Some(tab_index);

        let active_doc_missing = if let Some(leaf) = self.state.leaves.get(&leaf_id) {
            if let Some(tab) = leaf.tabs.get(tab_index) {
                match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        !self.state.documents.contains_key(id) || !self.state.views.contains_key(&tab.id)
                    }
                    crate::workspace::TabContent::Empty => false,
                    crate::workspace::TabContent::Plugin { .. } => false,
                }
            } else {
                false
            }
        } else {
            false
        };

        if same_tab && !forced && !active_doc_missing {
            return;
        }

        self.selected_sidebar_folder = None;

        // The match list belongs to the outgoing note.
        if self.find_bar.is_open {
            self.close_find_bar(window, cx);
        }

        self.commit_title_rename_internal();
        self.flush_active_note_now(cx);

        // 1. Snapshot outgoing note settings & view state ONLY if switching away from a different tab
        if !same_tab {
            let current_scroll = self.current_scroll_top();
            self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap, self.source_mode);
            let current_scroll_f64 = self.current_scroll_top_f64();
            if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
                let lines_total = doc.line_index.lines();
                view.scroll_top = current_scroll_f64;
                view.viewport.offset = view.line_at_doc_y(lines_total, current_scroll_f64);
                let line_top = view.doc_y_of_line(lines_total, view.viewport.offset);
                view.viewport.px_in_line = ((current_scroll_f64 - line_top) as f32).max(0.0);
                // The pane turns into a background surface: return it to the
                // canonical layout so its rows match what a non-editing render
                // paints, instead of keeping the caret/selection reflow.
                view.ensure_visual_cache_with_headings(
                    doc,
                    view.word_wrap,
                    view.window.wrap_cols,
                    Some(&self.config.editor.headings),
                    view.viewport.offset,
                    true,
                );
            }
        }

        // 2. Set new active tab index in target leaf and set active leaf
        if let Some(leaf) = self.state.leaves.get_mut(&leaf_id) {
            if tab_index < leaf.tabs.len() {
                leaf.active_tab_index = tab_index;
            }
        }
        self.state.workspace.active_leaf = leaf_id;

        // Ensure incoming leaf's active document is loaded into memory
        if let Some(doc_id) = self.state.leaves.get(&leaf_id).and_then(|l| l.active_tab()).and_then(|t| match &t.content {
            crate::workspace::TabContent::Document { id } => Some(*id),
            _ => None,
        }) {
            self.state.ensure_document_loaded(doc_id);
        }

        // 3. Load incoming leaf's active tab
        if let Some(leaf) = self.state.leaves.get(&leaf_id) {
            if let Some(tab) = leaf.active_tab() {
                match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        let doc_id = *id;
                        let viewport_w: f32 = window.viewport_size().width.into();
                        let editor_font = self.editor_font_family();
                        let font_id = cx.text_system().resolve_font(&gpui::font(editor_font));
                        let line_char_width = cx
                            .text_system()
                            .advance(font_id, px(14.0), 'x')
                            .map(|s| f32::from(s.width))
                            .unwrap_or(7.7);
                        let target_wrap_cols = self.compute_wrap_cols_for_leaf(Some(leaf_id), viewport_w, line_char_width);
                        self.last_wrap_cols = target_wrap_cols;

                        if let Some(doc) = self.state.documents.get_mut(&doc_id) {
                            let doc_path = doc.path.clone();
                            let lines_total = doc.line_index.lines();

                            let restore_y = {
                                let view = self.state.views.entry(tab.id).or_insert_with(|| crate::workspace::EditorViewState::new(tab.id, doc_id));

                                view.ensure_visual_cache_with_headings(doc, self.word_wrap, target_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
                                if view.window.wrap_cols == target_wrap_cols {
                                    view.scroll_top
                                } else {
                                    let line_top = view.doc_y_of_line(lines_total, view.viewport.offset);
                                    let calculated = line_top + view.viewport.px_in_line as f64;
                                    if (calculated - view.scroll_top).abs() < 1.0 {
                                        view.scroll_top
                                    } else {
                                        calculated
                                    }
                                }
                            };

                            self.set_editor_scroll_top_f64(restore_y);
                            self.pending_editor_restore = Some(restore_y);
                            self.need_reveal_cursor = false;
                            self.layout_settle_frames.set(SCROLL_SETTLE_FRAMES);

                            self.state.active_empty_tab = None;
                            self.state.session.active_note = Some(doc_path);
                        }
                    }
                    crate::workspace::TabContent::Empty => {
                        self.state.active_empty_tab = Some(std::path::PathBuf::from("newtab"));
                        self.active_center_leaf = None;
                        self.state.session.active_note = Some(crate::state::empty_tab_path());
                    }
                    crate::workspace::TabContent::Plugin { view_id } => {
                        self.state.active_empty_tab = None;
                        self.active_center_leaf = Some(view_id.clone());
                    }
                }
            } else {
                self.state.active_empty_tab = None;
                self.active_center_leaf = None;
            }

            self.state.open_tabs = leaf.tabs.iter().filter_map(|t| match &t.content {
                crate::workspace::TabContent::Document { id } => {
                    self.state.document_path(*id).map(|p| p.to_path_buf())
                }
                crate::workspace::TabContent::Empty => Some(std::path::PathBuf::from("newtab")),
                _ => None,
            }).collect();
            self.state.session.open_tabs = self.state.open_tabs.clone();
        }

        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// Selects a tab within a leaf by its TabId.
    pub fn select_tab(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        tab_id: crate::workspace::TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(leaf) = self.state.leaves.get(&leaf_id) {
            if let Some(idx) = leaf.tabs.iter().position(|t| t.id == tab_id) {
                self.select_leaf_tab(leaf_id, idx, window, cx);
            }
        }
    }

    pub fn close_leaf_tab(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let req = match self.state.request_close_leaf_tab(leaf_id, tab_index) {
            Ok((_tab_id, req)) => req,
            Err(_) => return,
        };

        if let crate::workspace::CloseRequest::NeedsEffects(effects) = req {
            for eff in effects {
                if let Err(err) = self.state.run_close_effect(&eff) {
                    self.state.status_text = format!("Close cancelled: {}", err);
                    cx.notify();
                    return;
                }
            }
        }

        let was_active_tab = self.state.leaves.get(&leaf_id).map(|l| l.active_tab_index == tab_index).unwrap_or(false);

        if self.state.finish_close_leaf_tab(leaf_id, tab_index).is_err() {
            return;
        }

        let active = self.state.workspace.active_leaf;
        let active_idx = self.state.leaves.get(&active).map(|l| l.active_tab_index).unwrap_or(0);
        if was_active_tab || leaf_id != active {
            self.select_leaf_tab_forced(active, active_idx, window, cx);
        } else {
            self.select_leaf_tab(active, active_idx, window, cx);
        }
        self.layout_settle_frames.set(SCROLL_SETTLE_FRAMES);
        cx.notify();
    }

    pub fn close_active_leaf_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.state.workspace.active_leaf;
        let active_idx = self.state.leaves.get(&active).map(|l| l.active_tab_index).unwrap_or(0);
        self.close_leaf_tab(active, active_idx, window, cx);
    }

    /// Closes all tabs in the specified leaf without affecting other leaves/windows.
    pub fn close_leaf_all_tabs(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        while let Some(count) = self.state.leaves.get(&leaf_id).map(|l| l.tabs.len()) {
            if count == 0 {
                break;
            }
            let prev_count = count;
            self.close_leaf_tab(leaf_id, count - 1, window, cx);
            if self.state.leaves.get(&leaf_id).map(|l| l.tabs.len()) == Some(prev_count) {
                break;
            }
        }
        self.layout_settle_frames.set(SCROLL_SETTLE_FRAMES);
        cx.notify();
    }
}
