use super::*;

impl TabSession {
    /// `(anchor_line, first_selected_line, last_selected_line)`.
    pub(super) fn selection_reveal_state(
        &self,
        doc: &DocumentBuffer,
        suppress_selection_reveal: bool,
    ) -> Option<(usize, usize, usize)> {
        if suppress_selection_reveal {
            return None;
        }
        let sel = self.selection_range(doc.len())?;
        let anchor = self.selection_anchor?;
        let mut last = sel.end - 1;
        while last > sel.start && !doc.is_char_boundary(last) {
            last -= 1;
        }
        Some((
            doc.line_of_byte(anchor),
            doc.line_of_byte(sel.start),
            doc.line_of_byte(last),
        ))
    }

    /// Selection reveals a table only when it started outside of it; a drag
    /// that begins inside a cell keeps the grid and its copy semantics.
    pub(super) fn reveals_table(state: Option<(usize, usize, usize)>, tbl: &crate::table::TableBlock) -> bool {
        state.map_or(false, |(anchor, s, e)| {
            !tbl.contains(anchor) && s <= tbl.last_line && e >= tbl.first_line
        })
    }

    /// A table is shown as source while the cursor is inside an explicitly
    /// toggled table, or while an edit-mode selection started outside of it,
    /// or while the ongoing drag already revealed it.
    pub fn table_raw_now(
        &self,
        doc: &DocumentBuffer,
        tbl: &crate::table::TableBlock,
        cursor_line: usize,
        suppress_selection_reveal: bool,
    ) -> bool {
        self.source_mode
            || (self.raw_tables.contains(&tbl.first_line) && tbl.contains(cursor_line))
            || self.drag_revealed_tables.contains(&tbl.first_line)
            || Self::reveals_table(
                self.selection_reveal_state(doc, suppress_selection_reveal),
                tbl,
            )
    }

    /// Remembers every table the ongoing drag reveals as source, so the drag
    /// keeps its layout once it has reached a table. Without this the raw rows
    /// are shorter than the grid and the line under the pointer drops below
    /// the table on the next frame, which flips the reveal back on and off.
    pub fn hold_drag_revealed_tables(
        &mut self,
        doc: &DocumentBuffer,
        suppress_selection_reveal: bool,
    ) {
        if suppress_selection_reveal {
            return;
        }
        if self.window.is_empty() {
            return;
        }
        let Some(state) = self.selection_reveal_state(doc, false) else {
            return;
        };
        let tables = doc.ensure_table_index_for_window(self.window.start_line, self.window.end_line);
        for t in &tables.blocks {
            if Self::reveals_table(Some(state), t) {
                self.drag_revealed_tables.insert(t.first_line);
            }
        }
    }

    /// Image analogue of `hold_drag_revealed_tables`: once the drag reaches a
    /// picture, keep its link visible for the rest of the drag.
    pub fn hold_drag_revealed_images(
        &mut self,
        doc: &DocumentBuffer,
        suppress_selection_reveal: bool,
    ) {
        if suppress_selection_reveal || self.window.is_empty() {
            return;
        }
        let Some((anchor_line, start_line, end_line)) =
            self.selection_reveal_state(doc, false)
        else {
            return;
        };
        let mut changed = false;
        for line in start_line..=end_line {
            if line == anchor_line {
                continue;
            }
            if crate::images::parse_image_reference(&doc.line_text(line)).is_some()
                && self.drag_revealed_images.insert(line)
            {
                changed = true;
            }
        }
        if changed {
            self.visual_cache_dirty = true;
        }
    }

    pub fn toggle_raw_table_at(&mut self, first_line: usize) -> bool {
        let is_now_raw = if self.raw_tables.contains(&first_line) {
            self.raw_tables.remove(&first_line);
            false
        } else {
            self.raw_tables.insert(first_line);
            true
        };
        self.raw_table = is_now_raw;
        self.visual_cache_dirty = true;
        is_now_raw
    }

    /// Publishes the painted column budgets of the window's tables.
    ///
    /// The renderer measures the table layout, so the row geometry cannot know
    /// how wide a cell is on its own; it reads these budgets instead. A change
    /// invalidates the row heights, because the same text wraps differently.
    pub fn set_table_cell_cols(&mut self, cols: std::collections::HashMap<usize, Vec<usize>>) -> bool {
        if self.table_cell_cols == cols {
            return false;
        }
        self.table_cell_cols = cols;
        self.visual_cache_dirty = true;
        true
    }

    pub fn shift_raw_tables(&mut self, edit_line: usize, line_diff: isize) {
        if line_diff == 0 || self.raw_tables.is_empty() {
            return;
        }
        let mut updated = std::collections::HashSet::new();
        for &fl in &self.raw_tables {
            if fl >= edit_line {
                let shifted = (fl as isize + line_diff).max(0) as usize;
                updated.insert(shifted);
            } else {
                updated.insert(fl);
            }
        }
        self.raw_tables = updated;
    }

    /// Line-number shift for the published table budgets, so an edit above the
    /// table does not leave the budgets under the wrong first line.
    pub fn shift_table_cell_cols(&mut self, edit_line: usize, line_diff: isize) {
        if line_diff == 0 || self.table_cell_cols.is_empty() {
            return;
        }
        let mut updated = std::collections::HashMap::with_capacity(self.table_cell_cols.len());
        for (first_line, cols) in self.table_cell_cols.drain() {
            let first_line = if first_line >= edit_line {
                (first_line as isize + line_diff).max(0) as usize
            } else {
                first_line
            };
            updated.insert(first_line, cols);
        }
        self.table_cell_cols = updated;
    }

    /// Line-number shift for the stored wrap exceptions, so an external edit
    /// above the window does not leave a painted line wrapped for the wrong
    /// row. A full rebuild clears the map; this covers the in-place shift.
    pub(crate) fn shift_line_paints(&mut self, edit_line: usize, line_diff: isize) {
        if line_diff == 0 || self.line_paints.is_empty() {
            return;
        }
        let mut updated = std::collections::HashMap::with_capacity(self.line_paints.len());
        for (line, spec) in self.line_paints.drain() {
            let line = if line >= edit_line {
                (line as isize + line_diff).max(0) as usize
            } else {
                line
            };
            updated.insert(line, spec);
        }
        self.line_paints = updated;
    }

    /// Bring the window's per-line wrap exceptions in line with the caret and
    /// the selection.
    ///
    /// The base layout is canonical (markup hidden). Only the caret line and
    /// the selected lines are re-wrapped for their revealed markup, so the
    /// caret cannot reflow the whole note and preview/Ctrl-selection keep a
    /// layout that never moves. `suppress_selection_reveal` (preview mode and
    /// Ctrl-selection) drops every exception, which is exactly the canonical
    /// layout.
    pub(super) fn sync_line_paints(
        &mut self,
        doc: &DocumentBuffer,
        typography: Option<&crate::settings::EditorTypography>,
        suppress_selection_reveal: bool,
    ) {
        let headings = typography.map(|typography| &typography.headings);
        if self.window.is_empty() || self.source_mode {
            self.line_paints.clear();
            return;
        }
        let mut desired: std::collections::HashMap<usize, crate::markdown::PaintedLineSpec> =
            std::collections::HashMap::new();
        let selection = self.selection_range(doc.len());
        let reveal_state = self.selection_reveal_state(doc, suppress_selection_reveal);
        let (cursor_line, cursor_col) = self.line_col(doc);
        if !suppress_selection_reveal {
            if self.window.covers_line(cursor_line) {
                if let Some(spec) = self.paint_spec_for_line(
                    doc,
                    cursor_line,
                    Some(cursor_col),
                    selection.as_ref(),
                    self.reveal_all_line == Some(cursor_line),
                ) {
                    desired.insert(cursor_line, spec);
                }
            }
            if let (Some(_), Some((_, first, last))) = (selection.as_ref(), reveal_state) {
                let lo = first.max(self.window.start_line);
                let hi = last.min(self.window.end_line.saturating_sub(1));
                for line in lo..=hi {
                    if line == cursor_line || desired.contains_key(&line) {
                        continue;
                    }
                    if let Some(spec) = self.paint_spec_for_line(
                        doc,
                        line,
                        None,
                        selection.as_ref(),
                        false,
                    ) {
                        desired.insert(line, spec);
                    }
                }
            }
        }

        // Lines whose paint did not change keep their rows as they are.
        let mut changed: Vec<(usize, Option<crate::markdown::PaintedLineSpec>)> = Vec::new();
        for (&line, spec) in &desired {
            if self.line_paints.get(&line) != Some(spec) {
                changed.push((line, Some(spec.clone())));
            }
        }
        for &line in self.line_paints.keys() {
            if !desired.contains_key(&line) {
                changed.push((line, None));
            }
        }
        if changed.is_empty() {
            return;
        }

        let tables = doc.ensure_table_index_for_window(self.window.start_line, self.window.end_line);
        let kinds: Vec<Option<crate::markdown::TableRowVisualKind>> = changed
            .iter()
            .map(|(line, _)| {
                let text = self.window.line(*line).unwrap_or("");
                Self::table_row_kind_at(
                    &tables,
                    &self.raw_tables,
                    &self.drag_revealed_tables,
                    &self.table_cell_cols,
                    reveal_state,
                    cursor_line,
                    cursor_col,
                    !suppress_selection_reveal,
                    *line,
                    text,
                    headings,
                )
            })
            .collect();

        // Reflowing a line above the viewport anchor moves every row below it,
        // so the scroll has to follow — the same rule the edit paths use.
        let anchor = self.viewport.offset;
        let lines_total = doc.line_index.lines();
        let anchor_before = self.doc_y_of_line(lines_total, anchor);
        let canonical = crate::markdown::PaintedLineSpec::default();

        for ((line, spec), kind) in changed.into_iter().zip(kinds) {
            let paint = spec.as_ref().unwrap_or(&canonical);
            self.window.rewrap_line(line, paint, typography, |_, _| kind);
        }
        let anchor_after = self.doc_y_of_line(lines_total, anchor);
        let shift = anchor_after - anchor_before;
        if shift.abs() > 0.5 {
            self.scroll_top = (self.scroll_top + shift).max(0.0);
        }
        self.line_paints = desired;
    }

    /// Paint spec for one line, or `None` when its painted state cannot change
    /// how it wraps (plain prose, code, rules, table rows).
    pub(super) fn paint_spec_for_line(
        &self,
        doc: &DocumentBuffer,
        line: usize,
        cursor_char: Option<usize>,
        selection: Option<&std::ops::Range<usize>>,
        reveal_all: bool,
    ) -> Option<crate::markdown::PaintedLineSpec> {
        let text = self.window.line(line)?;
        if text.is_empty() {
            return None;
        }
        let state = self.window.code_state(line);
        let classified =
            crate::markdown::classify_line(text, state.map(|s| (s.kind, s.lang.as_deref())));
        if crate::markdown::line_never_wraps(&classified)
            || matches!(classified, crate::markdown::LineKind::CodeLine(_))
        {
            return None;
        }
        // A line with no heading marker and no inline marker paints exactly
        // like the canonical layout; parsing it would only cost time.
        let may_change = matches!(
            classified,
            crate::markdown::LineKind::Heading1(_)
                | crate::markdown::LineKind::Heading2(_)
                | crate::markdown::LineKind::Heading3(_)
                | crate::markdown::LineKind::Heading4(_)
                | crate::markdown::LineKind::Heading5(_)
                | crate::markdown::LineKind::Heading6(_)
        ) || text.contains(['*', '_', '`', '~', '[', ']', '\\', '<', '=', '#']);
        if !may_change {
            return None;
        }
        let line_start = doc.line_start_byte(line);
        let selection = crate::markdown::selection_chars_in_line(text, line_start, selection);
        Some(crate::markdown::line_paint_spec(
            text,
            &classified,
            cursor_char,
            selection,
            reveal_all,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn table_row_kind_at(
        tables: &crate::table::TableIndex,
        raw_tables: &std::collections::HashSet<usize>,
        drag_revealed_tables: &std::collections::HashSet<usize>,
        cell_cols: &std::collections::HashMap<usize, Vec<usize>>,
        reveal_state: Option<(usize, usize, usize)>,
        cursor_line: usize,
        cursor_col: usize,
        // Whether the caret may reveal its cell (edit mode, not preview).
        allow_active_cell: bool,
        line: usize,
        line_text: &str,
        headings: Option<&crate::settings::HeadingsSettings>,
    ) -> Option<crate::markdown::TableRowVisualKind> {
        let t = tables.at(line)?;
        if (raw_tables.contains(&t.first_line) && t.contains(cursor_line))
            || drag_revealed_tables.contains(&t.first_line)
            || Self::reveals_table(reveal_state, t)
        {
            return None;
        }
        if line == t.sep_line() {
            return Some(crate::markdown::TableRowVisualKind::Delimiter);
        }
        // The renderer publishes the painted column budget of every column; the
        // row must wrap its cells with those exact widths or the height it
        // claims would not match the grid that is painted over it.
        let lines = match cell_cols.get(&t.first_line) {
            Some(cols) => {
                let cells = crate::table::split_row(line_text);
                let ranges: Vec<std::ops::Range<usize>> =
                    cells.iter().map(|c| c.content.clone()).collect();
                let active_cell = (allow_active_cell && line == cursor_line)
                    .then(|| crate::table::cell_index_at(&cells, cursor_col));
                let default_headings = crate::settings::HeadingsSettings::default();
                let h = headings.unwrap_or(&default_headings);
                crate::markdown::table_row_wrap_lines(
                    line_text,
                    &ranges,
                    cols,
                    active_cell,
                    &crate::markdown::InlineReveal::None,
                    h,
                )
            }
            None => 1,
        };
        Some(if line == t.first_line {
            crate::markdown::TableRowVisualKind::Header { lines }
        } else if line == t.last_line {
            crate::markdown::TableRowVisualKind::LastRow { lines }
        } else {
            crate::markdown::TableRowVisualKind::Body { lines }
        })
    }

    pub fn grid_table_at<'a>(
        &self,
        doc: &DocumentBuffer,
        tables: &'a crate::table::TableIndex,
        line: usize,
        cursor_line: usize,
        suppress_selection_reveal: bool,
    ) -> Option<&'a crate::table::TableBlock> {
        let t = tables.at(line)?;
        if self.table_raw_now(doc, t, cursor_line, suppress_selection_reveal) {
            None
        } else {
            Some(t)
        }
    }

    pub fn selection_range(&self, doc_len: usize) -> Option<std::ops::Range<usize>> {
        let anchor = self.selection_anchor?;
        if anchor != self.cursor_offset {
            let start = anchor.min(self.cursor_offset).min(doc_len);
            let end = anchor.max(self.cursor_offset).min(doc_len);
            if start != end {
                return Some(start..end);
            }
        }
        None
    }

    pub fn selected_text(&self, doc: &DocumentBuffer) -> Option<String> {
        let range = self.selection_range(doc.len())?;
        if doc.is_char_boundary(range.start) && doc.is_char_boundary(range.end) {
            Some(doc.slice_to_string(range))
        } else {
            None
        }
    }

    pub fn select_all(&mut self, doc: &DocumentBuffer) {
        self.selection_anchor = Some(0);
        self.cursor_offset = doc.len();
    }

    pub fn move_cursor_left(&mut self, doc: &DocumentBuffer, shift: bool) {
        if !shift && self.selection_anchor.is_some() {
            if let Some(range) = self.selection_range(doc.len()) {
                self.cursor_offset = range.start;
                self.selection_anchor = None;
                return;
            }
        }
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        if self.cursor_offset == 0 {
            return;
        }
        let mut prev = self.cursor_offset - 1;
        while prev > 0 && !doc.is_char_boundary(prev) {
            prev -= 1;
        }
        self.cursor_offset = prev;
        if !shift {
            self.selection_anchor = None;
        }
    }

    pub fn move_cursor_right(&mut self, doc: &DocumentBuffer, shift: bool) {
        if !shift && self.selection_anchor.is_some() {
            if let Some(range) = self.selection_range(doc.len()) {
                self.cursor_offset = range.end;
                self.selection_anchor = None;
                return;
            }
        }
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        if self.cursor_offset >= doc.len() {
            return;
        }
        let mut next = self.cursor_offset + 1;
        while next <= doc.len() && !doc.is_char_boundary(next) {
            next += 1;
        }
        if next <= doc.len() {
            self.cursor_offset = next;
        }
        if !shift {
            self.selection_anchor = None;
        }
    }

    /// Current text selection as an undo-step selection.
    pub(crate) fn selection(&self) -> Selection {
        Selection { cursor: self.cursor_offset, anchor: self.selection_anchor }
    }

    pub fn delete_selection(&mut self, doc: &mut DocumentBuffer) -> Option<(BufferEvent, NoteEditEvent, EditEvent)> {
        let range = self.selection_range(doc.len())?;
        let sel_before = self.selection();
        let removed = doc.slice_to_string(range.clone());
        let at = range.start;
        self.cursor_offset = at;
        self.selection_anchor = None;
        self.visual_cache_dirty = true;
        Some(doc.record_change_with_selection(at, removed, String::new(), sel_before, Selection::cursor_only(at), Some(self.tab_id)))
    }

    pub fn replace_selection(&mut self, doc: &mut DocumentBuffer, text: &str) -> (BufferEvent, NoteEditEvent, EditEvent) {
        let text = crate::document_buffer::normalize_newlines(text);
        if let Some(range) = self.selection_range(doc.len()) {
            let sel_before = self.selection();
            let removed = doc.slice_to_string(range.clone());
            let at = range.start;
            let new_cursor = at + text.len();
            self.cursor_offset = new_cursor;
            self.selection_anchor = None;
            self.visual_cache_dirty = true;
            doc.record_change_with_selection(at, removed, text.into_owned(), sel_before, Selection::cursor_only(new_cursor), Some(self.tab_id))
        } else {
            let mut offset = self.cursor_offset.min(doc.len());
            while offset > 0 && !doc.is_char_boundary(offset) {
                offset -= 1;
            }
            let sel_before = self.selection();
            let at = offset;
            let new_cursor = at + text.len();
            self.cursor_offset = new_cursor;
            self.selection_anchor = None;
            let res = doc.record_change_with_selection(at, String::new(), text.into_owned(), sel_before, Selection::cursor_only(new_cursor), Some(self.tab_id));
            let (_, ref note_edit, _) = res;
            if note_edit.line_diff == 0 && self.window.covers_line(note_edit.edit_line) {
                let delta = note_edit.new_len as isize - note_edit.old_len as isize;
                if self.update_window_line_after_edit(doc, note_edit.edit_line, delta) {
                    return res;
                }
            }
            self.visual_cache_dirty = true;
            res
        }
    }

    /// Refreshes a window line and its geometry after a same-line edit.
    ///
    /// Returns `true` when the line was updated in place. The caller must not
    /// mark the visual cache dirty in that case: the full rebuild is exactly
    /// what makes holding a key repaint 6000 window lines per character.
    ///
    /// The in-place path is only taken for pipe-free lines: table kinds are
    /// derived from pipe-bearing runs, so a pipe in the old or new text may
    /// change the visual kind of *neighbouring* lines, which only a full
    /// rebuild can recompute. Code fence lines are refused inside
    /// `update_single_line` (a fence line re-states the code state of every
    /// following line). Everything else — cursor moves into raw tables, etc. —
    /// goes through the conservative dirty path.
    pub(super) fn update_window_line_after_edit(&mut self, doc: &DocumentBuffer, edit_line: usize, delta: isize) -> bool {
        let (cursor_line, _) = self.line_col(doc);
        let tables = doc.ensure_table_index_for_window(self.window.start_line, self.window.end_line);
        let in_table_block = tables.at(edit_line).is_some_and(|t| t.contains(edit_line));

        let old_has_pipe = self
            .window
            .lines
            .get(edit_line)
            .is_some_and(|t| t.contains('|'));
        let new_has_pipe = doc.line_text(edit_line).contains('|');

        if in_table_block || old_has_pipe || new_has_pipe {
            return false;
        }

        // An image block's height is the picture height, which the in-place
        // update cannot measure (it has no note path or window width). A line
        // that is or becomes an image reference takes the full rebuild.
        let old_is_image = self
            .window
            .lines
            .get(edit_line)
            .is_some_and(|text| crate::images::parse_image_reference(text).is_some());
        let new_is_image = crate::images::parse_image_reference(&doc.line_text(edit_line)).is_some();
        if old_is_image || new_is_image {
            return false;
        }

        // The guard above proved the line is in no table block, so its kind is
        // `None`; `update_single_line` only asks for the edited line. The
        // closure must not capture `self` (it is passed into a
        // `&mut self.window` call).
        let typography = self.typography_cache.clone();
        let updated = self.window.update_single_line(
            edit_line,
            doc.line_text(edit_line),
            delta,
            Some(&typography),
            |_, _| None,
        );
        if !updated {
            return false;
        }
        // The in-place path wrote canonical rows. If this is the caret or a
        // selected line, the next ensure re-wraps it for the revealed markup;
        // dropping the stale spec is what makes the diff see it.
        self.line_paints.remove(&edit_line);
        if cursor_line != self.cursor_line {
            self.cursor_line = cursor_line;
        }
        true
    }

    pub fn insert_text(&mut self, doc: &mut DocumentBuffer, text: &str) -> (BufferEvent, NoteEditEvent, EditEvent) {
        self.replace_selection(doc, text)
    }

    pub fn replace_range(
        &mut self,
        doc: &mut DocumentBuffer,
        byte_range: std::ops::Range<usize>,
        new_text: String,
        new_cursor: usize,
    ) -> (BufferEvent, NoteEditEvent, EditEvent) {
        let sel_before = self.selection();
        let removed = doc.slice_to_string(byte_range.clone());
        let at = byte_range.start;
        self.cursor_offset = new_cursor;
        self.selection_anchor = None;
        self.visual_cache_dirty = true;
        doc.record_change_with_selection(at, removed, new_text, sel_before, Selection::cursor_only(new_cursor), Some(self.tab_id))
    }

    pub fn delete_backwards(&mut self, doc: &mut DocumentBuffer) -> Option<(BufferEvent, NoteEditEvent, EditEvent)> {
        if self.selection_range(doc.len()).is_some() {
            return self.delete_selection(doc);
        }
        if self.cursor_offset == 0 || doc.is_empty() {
            return None;
        }
        let mut prev = self.cursor_offset - 1;
        while prev > 0 && !doc.is_char_boundary(prev) {
            prev -= 1;
        }
        let sel_before = self.selection();
        let removed = doc.slice_to_string(prev..self.cursor_offset);
        self.cursor_offset = prev;
        self.selection_anchor = None;
        let res = doc.record_change_with_selection(prev, removed, String::new(), sel_before, Selection::cursor_only(prev), Some(self.tab_id));
        let (_, ref note_edit, _) = res;
        if note_edit.line_diff == 0 && self.window.covers_line(note_edit.edit_line) {
            let delta = note_edit.new_len as isize - note_edit.old_len as isize;
            if self.update_window_line_after_edit(doc, note_edit.edit_line, delta) {
                return Some(res);
            }
        }
        self.visual_cache_dirty = true;
        Some(res)
    }

    pub fn delete_forwards(&mut self, doc: &mut DocumentBuffer) -> Option<(BufferEvent, NoteEditEvent, EditEvent)> {
        if self.selection_range(doc.len()).is_some() {
            return self.delete_selection(doc);
        }
        let offset = self.cursor_offset.min(doc.len());
        if offset >= doc.len() {
            return None;
        }
        let mut next = offset + 1;
        while next <= doc.len() && !doc.is_char_boundary(next) {
            next += 1;
        }
        let sel_before = self.selection();
        let removed = doc.slice_to_string(offset..next);
        self.cursor_offset = offset;
        self.selection_anchor = None;
        let res = doc.record_change_with_selection(offset, removed, String::new(), sel_before, Selection::cursor_only(offset), Some(self.tab_id));
        let (_, ref note_edit, _) = res;
        if note_edit.line_diff == 0 && self.window.covers_line(note_edit.edit_line) {
            let delta = note_edit.new_len as isize - note_edit.old_len as isize;
            if self.update_window_line_after_edit(doc, note_edit.edit_line, delta) {
                return Some(res);
            }
        }
        self.visual_cache_dirty = true;
        Some(res)
    }

    pub fn insert_newline_auto_indent(&mut self, doc: &mut DocumentBuffer) -> (BufferEvent, NoteEditEvent, EditEvent) {
        let (line_idx, _) = self.line_col(doc);
        let line_text = doc.line_text(line_idx);
        let indent: String = line_text.chars().take_while(|c| c.is_whitespace() && *c != '\n').collect();
        let text = format!("\n{}", indent);
        self.replace_selection(doc, &text)
    }

    pub fn undo(&mut self, doc: &mut DocumentBuffer) -> Option<(BufferEvent, Vec<NoteEditEvent>, EditEvent)> {
        let (sel_before, note_edits, edit_event) = doc.undo_step(Some(self.tab_id))?;
        for edit in &note_edits {
            if edit.line_diff != 0 {
                self.shift_raw_tables(edit.edit_line, edit.line_diff);
            }
        }
        self.cursor_offset = sel_before.cursor.min(doc.len());
        self.selection_anchor = sel_before.anchor;
        self.visual_cache_dirty = true;
        let buf_event = BufferEvent::Edited {
            edit: Edit {
                start: edit_event.ops.first().map(|op| match op {
                    TextOp::Insert { at, .. } | TextOp::Delete { at, .. } => *at,
                }).unwrap_or(0),
                old_len: 0,
                new_len: 0,
            },
            source: Some(self.tab_id),
            version: doc.version,
        };
        Some((buf_event, note_edits, edit_event))
    }

    pub fn redo(&mut self, doc: &mut DocumentBuffer) -> Option<(BufferEvent, Vec<NoteEditEvent>, EditEvent)> {
        let (sel_after, note_edits, edit_event) = doc.redo_step(Some(self.tab_id))?;
        for edit in &note_edits {
            if edit.line_diff != 0 {
                self.shift_raw_tables(edit.edit_line, edit.line_diff);
            }
        }
        self.cursor_offset = sel_after.cursor.min(doc.len());
        self.selection_anchor = sel_after.anchor;
        self.visual_cache_dirty = true;
        let buf_event = BufferEvent::Edited {
            edit: Edit {
                start: edit_event.ops.first().map(|op| match op {
                    TextOp::Insert { at, .. } | TextOp::Delete { at, .. } => *at,
                }).unwrap_or(0),
                old_len: 0,
                new_len: 0,
            },
            source: Some(self.tab_id),
            version: doc.version,
        };
        Some((buf_event, note_edits, edit_event))
    }
}
