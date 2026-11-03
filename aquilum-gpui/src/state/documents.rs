use super::*;

impl AppState {
    pub fn find_document_by_path(&self, path: &Path) -> Option<DocumentId> {
        let norm = Self::normalize_path(path);
        self.by_path.get(&norm).copied().or_else(|| {
            self.documents.iter().find_map(|(id, doc)| {
                if doc.path == path {
                    Some(*id)
                } else {
                    None
                }
            })
        }).or_else(|| {
            self.doc_meta.iter().find_map(|(id, meta)| {
                if meta.path == path {
                    Some(*id)
                } else {
                    None
                }
            })
        })
    }

    pub fn active_tab_id(&self) -> Option<TabId> {
        self.leaves.get(&self.workspace.active_leaf).and_then(|l| l.active_tab()).map(|t| t.id)
    }

    pub fn active_tab(&self) -> Option<&crate::workspace::Tab> {
        self.leaves.get(&self.workspace.active_leaf).and_then(|l| l.active_tab())
    }

    pub fn active_doc_id(&self) -> Option<DocumentId> {
        self.active_tab().and_then(|t| match &t.content {
            crate::workspace::TabContent::Document { id } => Some(*id),
            _ => None,
        })
    }

    pub fn active_document(&self) -> Option<&DocumentBuffer> {
        let doc_id = self.active_doc_id()?;
        self.documents.get(&doc_id)
    }

    pub fn active_document_mut(&mut self) -> Option<&mut DocumentBuffer> {
        let doc_id = self.active_doc_id()?;
        self.documents.get_mut(&doc_id)
    }

    pub fn active_view(&self) -> Option<&EditorViewState> {
        let tab_id = self.active_tab_id()?;
        self.views.get(&tab_id)
    }

    pub fn active_view_mut(&mut self) -> Option<&mut EditorViewState> {
        let tab_id = self.active_tab_id()?;
        self.views.get_mut(&tab_id)
    }

    pub fn active_view_and_doc(&self) -> Option<(&EditorViewState, &DocumentBuffer)> {
        let tab_id = self.active_tab_id()?;
        let doc_id = self.active_doc_id()?;
        let view = self.views.get(&tab_id)?;
        let doc = self.documents.get(&doc_id)?;
        Some((view, doc))
    }

    pub fn active_view_and_doc_mut(&mut self) -> Option<(&mut EditorViewState, &mut DocumentBuffer)> {
        let tab_id = self.active_tab_id()?;
        let doc_id = self.active_doc_id()?;
        let view = self.views.get_mut(&tab_id)?;
        let doc = self.documents.get_mut(&doc_id)?;
        Some((view, doc))
    }

    pub fn dispatch_edit_event(&mut self, ev: &EditEvent, note_edit: &NoteEditEvent) {
        for (tab_id, view) in self.views.iter_mut() {
            if view.doc_id == ev.doc {
                if Some(*tab_id) == ev.origin {
                    if let Some(c) = ev.cursor_after {
                        view.cursor_offset = c;
                        view.selection_anchor = None;
                    }
                    // The view method that produced this event already manages
                    // its own cache invalidation; forcing a full rebuild here
                    // made every keystroke repaint the whole window.
                } else if let Some(doc) = self.documents.get(&ev.doc) {
                    view.apply_external_edit_detailed(doc, note_edit);
                } else {
                    view.visual_cache_dirty = true;
                }
            }
        }
        // The plugin snapshot carries the buffer text: an edit (keyboard, undo,
        // another plugin) invalidates every synchronous editor getter.
        self.publish_snapshot();
    }

    pub fn active_insert_text(&mut self, text: &str) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        let (_, note_edit, edit_event) = view.insert_text(doc, text);
        self.dispatch_edit_event(&edit_event, &note_edit);
    }

    pub fn active_apply_formatting(&mut self, op: &crate::formatting::FormattingOp) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };

        let doc_len = doc.len();
        let cur = view.cursor_offset.min(doc_len);
        let (sel_min, sel_max) = match view.selection_anchor {
            Some(a) => {
                let a = a.min(doc_len);
                (a.min(cur), a.max(cur))
            }
            None => (cur, cur),
        };

        let result = match op {
            crate::formatting::FormattingOp::InsertTable => {
                let template = "\n| Column 1 | Column 2 | Column 3 |\n| -------- | -------- | -------- |\n| Item 1   | Item 2   | Item 3   |\n| Item 4   | Item 5   | Item 6   |\n";
                crate::formatting::FormattingResult {
                    byte_range: sel_min..sel_max,
                    new_text: template.to_string(),
                    new_cursor: sel_min + template.len(),
                    new_anchor: None,
                }
            }
            crate::formatting::FormattingOp::CodeBlock => {
                let (replacement, new_cursor) = if sel_min == sel_max {
                    ("```\n\n```\n".to_string(), sel_min + 4)
                } else {
                    let selected_text = doc.slice_to_string(sel_min..sel_max);
                    (format!("```\n{}\n```\n", selected_text), sel_min + 4 + selected_text.len())
                };
                crate::formatting::FormattingResult {
                    byte_range: sel_min..sel_max,
                    new_text: replacement,
                    new_cursor,
                    new_anchor: None,
                }
            }
            crate::formatting::FormattingOp::InsertCallout(kind) => {
                let c_type = if kind.trim().is_empty() { "note" } else { kind.trim() };
                let (replacement, new_cursor) = if sel_min == sel_max {
                    (format!("> [!{}]\n> \n", c_type), sel_min + format!("> [!{}]\n> ", c_type).len())
                } else {
                    let selected_text = doc.slice_to_string(sel_min..sel_max);
                    let lines: Vec<String> = selected_text.lines().map(|l| format!("> {}", l)).collect();
                    let content = if lines.is_empty() { "> ".to_string() } else { lines.join("\n") };
                    let rep = format!("> [!{}]\n{}\n", c_type, content);
                    let cur = sel_min + rep.len();
                    (rep, cur)
                };
                crate::formatting::FormattingResult {
                    byte_range: sel_min..sel_max,
                    new_text: replacement,
                    new_cursor,
                    new_anchor: None,
                }
            }
            crate::formatting::FormattingOp::AttachFile(target) => {
                let selected_text = if sel_min != sel_max { doc.slice_to_string(sel_min..sel_max) } else { String::new() };
                let (replacement, new_cursor) = match target {
                    Some(path) => {
                        let alt = if selected_text.is_empty() { "" } else { &selected_text };
                        (format!("![{}]({})", alt, path), sel_min + format!("![{}]({})", alt, path).len())
                    }
                    None => {
                        if selected_text.is_empty() {
                            ("![[]]".to_string(), sel_min + 3)
                        } else {
                            (format!("![[{}]]", selected_text), sel_min + format!("![[{}]]", selected_text).len())
                        }
                    }
                };
                crate::formatting::FormattingResult {
                    byte_range: sel_min..sel_max,
                    new_text: replacement,
                    new_cursor,
                    new_anchor: None,
                }
            }
            crate::formatting::FormattingOp::ToggleInline { open, close } => {
                let open_chars = open.chars().count();
                let close_chars = close.chars().count();
                let char_sel_start = doc.byte_to_char(sel_min);
                let char_sel_end = doc.byte_to_char(sel_max);
                let slice_char_start = char_sel_start.saturating_sub(open_chars);
                let slice_char_end = (char_sel_end + close_chars).min(doc.char_len());
                let slice_byte_start = doc.char_to_byte(slice_char_start);
                let slice_byte_end = doc.char_to_byte(slice_char_end);
                let slice_text = doc.slice_to_string(slice_byte_start..slice_byte_end);
                let rel_sel_start = char_sel_start - slice_char_start;
                let rel_sel_end = char_sel_end - slice_char_start;
                let res = crate::formatting::apply_toggle_inline(&slice_text, rel_sel_start, rel_sel_end, open, close);
                crate::formatting::FormattingResult {
                    byte_range: slice_byte_start + res.byte_range.start .. slice_byte_start + res.byte_range.end,
                    new_text: res.new_text,
                    new_cursor: slice_byte_start + res.new_cursor,
                    new_anchor: res.new_anchor.map(|a| slice_byte_start + a),
                }
            }
            line_op => {
                let start_line = doc.line_of_byte(sel_min);
                let end_line = doc.line_of_byte(sel_max);
                let lines_byte_start = doc.line_start_byte(start_line);
                let lines_byte_end = doc.line_end_byte(end_line);
                let lines_text = doc.slice_to_string(lines_byte_start..lines_byte_end);
                let char_lines_start = doc.byte_to_char(lines_byte_start);
                let char_sel_start = doc.byte_to_char(sel_min).saturating_sub(char_lines_start);
                let char_sel_end = doc.byte_to_char(sel_max).saturating_sub(char_lines_start);

                let res = match line_op {
                    crate::formatting::FormattingOp::ToggleHeading(level) => {
                        crate::formatting::apply_toggle_heading(&lines_text, char_sel_start, char_sel_end, *level)
                    }
                    crate::formatting::FormattingOp::ClearFormatting => {
                        crate::formatting::apply_clear_formatting(&lines_text, char_sel_start, char_sel_end)
                    }
                    crate::formatting::FormattingOp::ToggleTask | crate::formatting::FormattingOp::ListTask => {
                        crate::formatting::apply_toggle_task(&lines_text, char_sel_start, char_sel_end)
                    }
                    crate::formatting::FormattingOp::ListBullet => {
                        crate::formatting::apply_list_bullet(&lines_text, char_sel_start, char_sel_end)
                    }
                    crate::formatting::FormattingOp::ListNumbered => {
                        crate::formatting::apply_list_numbered(&lines_text, char_sel_start, char_sel_end)
                    }
                    crate::formatting::FormattingOp::Indent => {
                        crate::formatting::apply_indent(&lines_text, char_sel_start, char_sel_end)
                    }
                    crate::formatting::FormattingOp::Outdent => {
                        crate::formatting::apply_outdent(&lines_text, char_sel_start, char_sel_end)
                    }
                    crate::formatting::FormattingOp::Align(align) => {
                        crate::formatting::apply_align(&lines_text, char_sel_start, char_sel_end, *align)
                    }
                    _ => unreachable!(),
                };

                crate::formatting::FormattingResult {
                    byte_range: lines_byte_start + res.byte_range.start .. lines_byte_start + res.byte_range.end,
                    new_text: res.new_text,
                    new_cursor: lines_byte_start + res.new_cursor,
                    new_anchor: res.new_anchor.map(|a| lines_byte_start + a),
                }
            }
        };

        let range = result.byte_range;
        if range.start > range.end || range.end > doc.len() || !doc.is_char_boundary(range.start) || !doc.is_char_boundary(range.end) {
            return false;
        }

        let removed = doc.slice_to_string(range.clone());
        let inserted = result.new_text;
        let sel_before = view.selection();
        let sel_after = match result.new_anchor {
            Some(anchor) => crate::workspace::session_core::undo::Selection {
                cursor: result.new_cursor,
                anchor: Some(anchor),
            },
            None => crate::workspace::session_core::undo::Selection::cursor_only(result.new_cursor),
        };

        let (_, note_edit, edit_event) = doc.record_change_with_selection(
            range.start,
            removed,
            inserted,
            sel_before,
            sel_after,
            Some(tab_id),
        );

        let mut cursor = result.new_cursor.min(doc.len());
        while cursor > 0 && !doc.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let anchor = result.new_anchor.map(|a| {
            let mut anchor = a.min(doc.len());
            while anchor > 0 && !doc.is_char_boundary(anchor) {
                anchor -= 1;
            }
            anchor
        });

        self.dispatch_edit_event(&edit_event, &note_edit);

        if let Some(v) = self.views.get_mut(&tab_id) {
            v.cursor_offset = cursor;
            v.selection_anchor = anchor;
            v.visual_cache_dirty = true;
            if note_edit.line_diff != 0 {
                v.shift_raw_tables(note_edit.edit_line, note_edit.line_diff);
                v.shift_table_cell_cols(note_edit.edit_line, note_edit.line_diff);
                v.shift_line_paints(note_edit.edit_line, note_edit.line_diff);
                if note_edit.edit_line < v.viewport.offset {
                    let line_h = v.est_rows_per_line * v.est_row_h;
                    let h_diff = (note_edit.line_diff as f64) * line_h;
                    v.viewport.offset = (v.viewport.offset as isize + note_edit.line_diff).max(0) as usize;
                    v.above_lines = (v.above_lines as isize + note_edit.line_diff).max(0) as usize;
                    v.above_h = (v.above_h + h_diff).max(0.0);
                    v.scroll_top = (v.scroll_top + h_diff).max(0.0);
                }
            }
        }
        true
    }

    pub fn active_formatting_state(&self) -> crate::formatting::ActiveFormatting {
        let Some((_view, doc)) = self.active_view_and_doc() else {
            return crate::formatting::ActiveFormatting::default();
        };

        crate::formatting::ActiveFormatting {
            can_undo: doc.can_undo(),
            can_redo: doc.can_redo(),
            ..Default::default()
        }
    }

    pub fn active_replace_range(
        &mut self,
        byte_range: std::ops::Range<usize>,
        new_text: String,
        new_cursor: usize,
    ) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        let (_, note_edit, edit_event) = view.replace_range(doc, byte_range, new_text, new_cursor);
        self.dispatch_edit_event(&edit_event, &note_edit);
    }

    /// Flips the mark of a task checkbox on `line`, without moving the caret or
    /// dropping the selection.
    ///
    /// `mark` is the byte range of the single character between the brackets,
    /// relative to the line. Replacing one character with one character keeps
    /// every offset valid, so the click only has to repaint the edited row. The
    /// step lands in the document history like any other edit.
    pub fn active_toggle_task_mark(&mut self, line: usize, mark: std::ops::Range<usize>) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };

        let line_start = doc.line_start_byte(line);
        let range = line_start + mark.start..line_start + mark.end;
        if range.end > doc.len() {
            return false;
        }
        let removed = doc.slice_to_string(range.clone());
        let inserted = if removed.trim().is_empty() { "x" } else { " " };
        let selection = view.selection();
        let (_, note_edit, edit_event) = doc.record_change_with_selection(
            range.start,
            removed,
            inserted.to_string(),
            selection,
            selection,
            // An external edit (`None`) is transformed by every view instead of
            // overwriting its caret: the click must not move the cursor.
            None,
        );
        self.dispatch_edit_event(&edit_event, &note_edit);
        true
    }

    pub fn active_toggle_raw_table(&mut self) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };

        let (cursor_line, _) = view.line_col(doc);
        let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
        if let Some(t) = tables.at(cursor_line) {
            view.cursor_line = cursor_line;
            view.toggle_raw_table_at(t.first_line);
            true
        } else {
            false
        }
    }

    pub fn active_toggle_raw_table_at(&mut self, first_line: usize) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };

        let (cursor_line, _) = view.line_col(doc);
        view.cursor_line = cursor_line;
        view.toggle_raw_table_at(first_line);
        true
    }

    pub fn active_apply_table_line_edit(
        &mut self,
        edit: &crate::table::LineEdit,
        new_cursor_line_col: Option<(usize, usize)>,
    ) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };

        if edit.new_lines.is_empty() {
            view.raw_tables.remove(&edit.first_line);
        }
        let line_diff = edit.new_lines.len() as isize - edit.old_line_count as isize;
        if line_diff != 0 {
            view.shift_raw_tables(edit.first_line + 1, line_diff);
        }

        let total_lines = doc.line_index.lines();
        let start_byte = doc.line_start_byte(edit.first_line);
        let end_byte = doc.line_start_byte(edit.first_line + edit.old_line_count);
        let mut new_text = edit.new_lines.join("\n");
        if !edit.new_lines.is_empty() && edit.first_line + edit.old_line_count < total_lines {
            new_text.push('\n');
        }
        let target_cursor = if let Some((target_line, target_col)) = new_cursor_line_col {
            if target_line >= edit.first_line && target_line < edit.first_line + edit.new_lines.len() {
                let rel_line = target_line - edit.first_line;
                let mut offset_in_new = 0;
                for i in 0..rel_line {
                    offset_in_new += edit.new_lines[i].len() + 1;
                }
                let byte_col: usize = edit.new_lines[rel_line]
                    .chars()
                    .take(target_col)
                    .map(|c| c.len_utf8())
                    .sum();
                start_byte + offset_in_new + byte_col
            } else {
                start_byte
            }
        } else {
            // Preserve existing cursor location
            let old_cursor = view.cursor_offset;
            let (cur_line, cur_col) = view.line_col(doc);
            if cur_line < edit.first_line {
                // Cursor was before the table: position unchanged
                old_cursor
            } else if cur_line >= edit.first_line + edit.old_line_count {
                // Cursor was after the table: shift by byte delta of replacement
                let old_table_bytes = end_byte.saturating_sub(start_byte);
                let new_table_bytes = new_text.len();
                let delta = new_table_bytes as isize - old_table_bytes as isize;
                (old_cursor as isize + delta).max(0) as usize
            } else {
                // Cursor was inside the table: keep it on the same line and column if possible
                let rel_line = cur_line - edit.first_line;
                if rel_line < edit.new_lines.len() {
                    let mut offset_in_new = 0;
                    for i in 0..rel_line {
                        offset_in_new += edit.new_lines[i].len() + 1;
                    }
                    let line_char_count = edit.new_lines[rel_line].chars().count();
                    let target_col = cur_col.min(line_char_count);
                    let byte_col: usize = edit.new_lines[rel_line]
                        .chars()
                        .take(target_col)
                        .map(|c| c.len_utf8())
                        .sum();
                    start_byte + offset_in_new + byte_col
                } else {
                    start_byte + new_text.len()
                }
            }
        };

        let (_, note_edit, edit_event) = view.replace_range(doc, start_byte..end_byte, new_text, target_cursor);
        self.dispatch_edit_event(&edit_event, &note_edit);
    }

    pub fn active_cursor_line_col(&self) -> Option<(usize, usize)> {
        let (view, doc) = self.active_view_and_doc()?;
        Some(view.line_col(doc))
    }

    pub fn active_set_cursor_line_col(&mut self, line: usize, col: usize) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        let offset = view.offset_for_line_col(doc, line, col);
        view.cursor_offset = offset;
        view.selection_anchor = None;
    }

    pub fn active_delete_backwards(&mut self) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        if let Some((_, note_edit, edit_event)) = view.delete_backwards(doc) {
            self.dispatch_edit_event(&edit_event, &note_edit);
        }
    }

    pub fn active_delete_forwards(&mut self) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        if let Some((_, note_edit, edit_event)) = view.delete_forwards(doc) {
            self.dispatch_edit_event(&edit_event, &note_edit);
        }
    }

    pub fn active_insert_newline_auto_indent(&mut self) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        let (_, note_edit, edit_event) = view.insert_newline_auto_indent(doc);
        self.dispatch_edit_event(&edit_event, &note_edit);
    }

    pub fn active_delete_selection(&mut self) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        if let Some((_, note_edit, edit_event)) = view.delete_selection(doc) {
            self.dispatch_edit_event(&edit_event, &note_edit);
            true
        } else {
            false
        }
    }

    pub fn active_replace_selection(&mut self, text: &str) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        let (_, note_edit, edit_event) = view.replace_selection(doc, text);
        self.dispatch_edit_event(&edit_event, &note_edit);
    }

    /// Full editor buffer state for the plugin protocol: text, cursor and
    /// selection as (line, ch) pairs with `ch` counted in characters, which is
    /// what Obsidian's `EditorPosition` uses.
    pub fn active_editor_buffer(&self) -> Option<(String, (usize, usize), Option<(usize, usize)>)> {
        let (view, doc) = self.active_view_and_doc()?;
        let head = view.line_col(doc);
        let anchor = view.selection_anchor.map(|a| {
            let anchor_line = doc.line_of_byte(a.min(doc.len()));
            let anchor_start = doc.line_start_byte(anchor_line).min(a);
            let anchor_ch = doc.slice_to_string(anchor_start..a).chars().count();
            (anchor_line, anchor_ch)
        });
        Some((doc.content(), head, anchor))
    }

    /// Replaces the whole document text, keeping the cursor clamped to the new end.
    pub fn active_set_buffer(&mut self, text: &str) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        let new_cursor = text.len();
        let (_, note_edit, edit_event) = view.replace_range(doc, 0..doc.len(), text.to_string(), new_cursor);
        self.dispatch_edit_event(&edit_event, &note_edit);
        true
    }

    /// Moves the cursor to a document byte offset, dropping the selection.
    pub fn active_set_cursor_offset(&mut self, offset: usize) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        let offset = offset.min(doc.len());
        view.cursor_offset = offset;
        view.selection_anchor = None;
        view.visual_cache_dirty = true;
        true
    }

    /// Selects the byte range `anchor..head`; the cursor sits at `head`.
    pub fn active_set_selection_range(&mut self, anchor: usize, head: usize) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        view.cursor_offset = head.min(doc.len());
        view.selection_anchor = Some(anchor.min(doc.len()));
        view.visual_cache_dirty = true;
        true
    }

    pub fn active_undo(&mut self) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        if let Some((_, note_edits, edit_event)) = view.undo(doc) {
            for note_edit in &note_edits {
                self.dispatch_edit_event(&edit_event, note_edit);
            }
            true
        } else {
            false
        }
    }

    pub fn active_redo(&mut self) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        if let Some((_, note_edits, edit_event)) = view.redo(doc) {
            for note_edit in &note_edits {
                self.dispatch_edit_event(&edit_event, note_edit);
            }
            true
        } else {
            false
        }
    }

    pub fn active_select_all(&mut self) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        view.select_all(doc);
    }

    /// Read-only access to the active view and document for callers that need
    /// to compute coordinates (byte offsets from line/col) before mutating.
    pub fn with_active_doc_and_view<R>(&self, f: impl FnOnce(&DocumentBuffer, &EditorViewState) -> R) -> Option<R> {
        let (view, doc) = self.active_view_and_doc()?;
        Some(f(doc, view))
    }

    pub fn active_selected_text(&self) -> Option<String> {
        let (view, doc) = self.active_view_and_doc()?;
        view.selected_text(doc)
    }

    pub fn active_move_cursor_left(&mut self, shift: bool) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        view.move_cursor_left(doc, shift);
    }

    pub fn active_move_cursor_right(&mut self, shift: bool) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        view.move_cursor_right(doc, shift);
    }
}
