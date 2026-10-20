use std::time::Instant;

use crate::document_buffer::{
    transform_offset, transform_selection_range, Bias, BufferEvent, DocumentBuffer, DocumentId,
    Edit, ViewportAnchor,
};
use crate::editor_window::EditorWindow;
use crate::state::NoteEditEvent;
use crate::workspace::session_core::ops::{EditEvent, TextOp};
use crate::workspace::TabId;

#[derive(Clone, Copy, Debug)]
pub struct WheelEaseState {
    pub start: f64,
    pub target: f64,
    pub t0: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleInput {
    pub text: String,
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub is_focused: bool,
    pub is_mouse_selecting: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseEffect {
    CommitTitle {
        tab_id: TabId,
        doc_id: DocumentId,
        new_title: String,
    },
    FlushDoc(DocumentId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseRequest {
    Ready,
    NeedsEffects(Vec<CloseEffect>),
}

impl TitleInput {
    pub fn new(text: String) -> Self {
        let cursor_pos = text.chars().count();
        Self {
            text,
            cursor_pos,
            selection_anchor: None,
            is_focused: true,
            is_mouse_selecting: false,
        }
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        if anchor != self.cursor_pos {
            let start = anchor.min(self.cursor_pos);
            let end = anchor.max(self.cursor_pos);
            Some((start, end))
        } else {
            None
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        let (s_start, s_end) = self.selection_range()?;
        Some(self.text.chars().skip(s_start).take(s_end - s_start).collect())
    }

    pub fn delete_selection(&mut self) -> bool {
        if let Some((s_start, s_end)) = self.selection_range() {
            let mut chars: Vec<char> = self.text.chars().collect();
            if s_start < chars.len() {
                chars.drain(s_start..s_end.min(chars.len()));
                self.text = chars.into_iter().collect();
            }
            self.cursor_pos = s_start;
            self.selection_anchor = None;
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor_pos = self.text.chars().count();
    }

    pub fn insert_text(&mut self, text: &str) {
        self.delete_selection();
        let mut chars: Vec<char> = self.text.chars().collect();
        let pos = self.cursor_pos.min(chars.len());
        for (i, c) in text.chars().enumerate() {
            chars.insert(pos + i, c);
        }
        self.cursor_pos = pos + text.chars().count();
        self.text = chars.into_iter().collect();
    }

    pub fn move_left(&mut self, select: bool) {
        if select {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_pos);
            }
            if self.cursor_pos > 0 {
                self.cursor_pos -= 1;
            }
        } else {
            if let Some((s_start, _)) = self.selection_range() {
                self.cursor_pos = s_start;
                self.selection_anchor = None;
            } else if self.cursor_pos > 0 {
                self.cursor_pos -= 1;
            }
        }
    }

    pub fn move_right(&mut self, select: bool) {
        let total_chars = self.text.chars().count();
        if select {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_pos);
            }
            if self.cursor_pos < total_chars {
                self.cursor_pos += 1;
            }
        } else {
            if let Some((_, s_end)) = self.selection_range() {
                self.cursor_pos = s_end;
                self.selection_anchor = None;
            } else if self.cursor_pos < total_chars {
                self.cursor_pos += 1;
            }
        }
    }

    pub fn move_home(&mut self, select: bool) {
        if select {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_pos);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor_pos = 0;
    }

    pub fn move_end(&mut self, select: bool) {
        let total_chars = self.text.chars().count();
        if select {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_pos);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor_pos = total_chars;
    }

    pub fn backspace(&mut self) {
        if !self.delete_selection() {
            let mut chars: Vec<char> = self.text.chars().collect();
            let pos = self.cursor_pos.min(chars.len());
            if pos > 0 {
                chars.remove(pos - 1);
                self.cursor_pos = pos - 1;
                self.text = chars.into_iter().collect();
            }
        }
    }

    pub fn delete(&mut self) {
        if !self.delete_selection() {
            let mut chars: Vec<char> = self.text.chars().collect();
            let pos = self.cursor_pos.min(chars.len());
            if pos < chars.len() {
                chars.remove(pos);
                self.text = chars.into_iter().collect();
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ViewCache {
    pub window: EditorWindow,
    pub above_lines: usize,
    pub below_lines: usize,
    pub above_h: f64,
    pub below_h: f64,
    pub est_rows_per_line: f64,
    pub est_row_h: f64,
    pub visual_cache_dirty: bool,
    pub raw_table: bool,
    pub raw_tables: std::collections::HashSet<usize>,
    pub revealed_selection: Option<(usize, usize, usize)>,
    pub cursor_line: usize,
}

impl Default for ViewCache {
    fn default() -> Self {
        Self {
            window: EditorWindow::default(),
            above_lines: 0,
            below_lines: 0,
            above_h: 0.0,
            below_h: 0.0,
            est_rows_per_line: 1.0,
            est_row_h: 24.0,
            visual_cache_dirty: true,
            raw_table: false,
            raw_tables: std::collections::HashSet::new(),
            revealed_selection: None,
            cursor_line: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableCellSelection {
    pub table_first_line: usize,
    pub anchor_row: usize,
    pub anchor_col: usize,
    pub head_row: usize,
    pub head_col: usize,
}

impl TableCellSelection {
    pub fn new(table_first_line: usize, row: usize, col: usize) -> Self {
        Self {
            table_first_line,
            anchor_row: row,
            anchor_col: col,
            head_row: row,
            head_col: col,
        }
    }

    pub fn rect(&self) -> crate::table::CellRect {
        crate::table::CellRect::new(self.anchor_row, self.anchor_col, self.head_row, self.head_col)
    }

    pub fn is_multi_cell(&self) -> bool {
        self.anchor_row != self.head_row || self.anchor_col != self.head_col
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        self.rect().contains_cell(row, col)
    }
}

#[derive(Clone, Debug)]
pub struct TabSession {
    pub tab_id: TabId,
    pub doc_id: DocumentId,
    pub cursor_offset: usize,
    pub selection_anchor: Option<usize>,
    pub table_cell_selection: Option<TableCellSelection>,
    pub viewport: ViewportAnchor,
    pub scroll_top: f64,
    pub wheel_ease: Option<WheelEaseState>,
    pub is_mouse_selecting: bool,
    pub word_wrap: bool,
    pub edit_mode: bool,
    pub title_input: Option<TitleInput>,
    pub measured_viewport_h: Option<f32>,
    pub cache: ViewCache,
}

pub type EditorViewState = TabSession;

impl std::ops::Deref for TabSession {
    type Target = ViewCache;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.cache
    }
}

impl std::ops::DerefMut for TabSession {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cache
    }
}

impl TabSession {
    pub fn new(tab_id: TabId, doc_id: DocumentId) -> Self {
        Self {
            tab_id,
            doc_id,
            cursor_offset: 0,
            selection_anchor: None,
            table_cell_selection: None,
            viewport: ViewportAnchor::default(),
            scroll_top: 0.0,
            wheel_ease: None,
            is_mouse_selecting: false,
            word_wrap: true,
            edit_mode: false,
            title_input: None,
            measured_viewport_h: None,
            cache: ViewCache::default(),
        }
    }

    #[inline]
    pub fn title_text<'a>(&'a self, doc: &'a DocumentBuffer) -> &'a str {
        if let Some(ref ti) = self.title_input {
            &ti.text
        } else {
            &doc.title
        }
    }

    pub fn start_title_edit(&mut self, initial_text: String) {
        self.title_input = Some(TitleInput::new(initial_text));
    }

    pub fn cancel_title_edit(&mut self) {
        self.title_input = None;
    }

    #[inline]
    pub fn is_table_raw(&self, first_line: usize) -> bool {
        self.raw_tables.contains(&first_line)
    }

    /// `(anchor_line, first_selected_line, last_selected_line)`.
    fn selection_reveal_state(
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
    fn reveals_table(state: Option<(usize, usize, usize)>, tbl: &crate::table::TableBlock) -> bool {
        state.map_or(false, |(anchor, s, e)| {
            !tbl.contains(anchor) && s <= tbl.last_line && e >= tbl.first_line
        })
    }

    /// A table is shown as source while the cursor is inside an explicitly
    /// toggled table, or while an edit-mode selection started outside of it.
    pub fn table_raw_now(
        &self,
        doc: &DocumentBuffer,
        tbl: &crate::table::TableBlock,
        cursor_line: usize,
        suppress_selection_reveal: bool,
    ) -> bool {
        (self.raw_tables.contains(&tbl.first_line) && tbl.contains(cursor_line))
            || Self::reveals_table(
                self.selection_reveal_state(doc, suppress_selection_reveal),
                tbl,
            )
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

    pub fn delete_selection(&mut self, doc: &mut DocumentBuffer) -> Option<(BufferEvent, NoteEditEvent, EditEvent)> {
        let range = self.selection_range(doc.len())?;
        let cursor_before = self.cursor_offset;
        let removed = doc.slice_to_string(range.clone());
        let at = range.start;
        self.cursor_offset = at;
        self.selection_anchor = None;
        self.visual_cache_dirty = true;
        Some(doc.record_change(at, removed, String::new(), cursor_before, at, Some(self.tab_id)))
    }

    pub fn replace_selection(&mut self, doc: &mut DocumentBuffer, text: &str) -> (BufferEvent, NoteEditEvent, EditEvent) {
        if let Some(range) = self.selection_range(doc.len()) {
            let cursor_before = self.cursor_offset;
            let removed = doc.slice_to_string(range.clone());
            let at = range.start;
            let new_cursor = at + text.len();
            self.cursor_offset = new_cursor;
            self.selection_anchor = None;
            self.visual_cache_dirty = true;
            doc.record_change(at, removed, text.to_string(), cursor_before, new_cursor, Some(self.tab_id))
        } else {
            let mut offset = self.cursor_offset.min(doc.len());
            while offset > 0 && !doc.is_char_boundary(offset) {
                offset -= 1;
            }
            let cursor_before = self.cursor_offset;
            let at = offset;
            let new_cursor = at + text.len();
            self.cursor_offset = new_cursor;
            self.selection_anchor = None;
            let res = doc.record_change(at, String::new(), text.to_string(), cursor_before, new_cursor, Some(self.tab_id));
            let (_, ref note_edit, _) = res;
            if note_edit.line_diff == 0 && self.window.covers_line(note_edit.edit_line) {
                let delta = note_edit.new_len as isize - note_edit.old_len as isize;
                self.update_window_line_after_edit(doc, note_edit.edit_line, delta);
            }
            self.visual_cache_dirty = true;
            res
        }
    }

    /// Refreshes a window line and its geometry after a same-line edit.
    ///
    /// Table rows are never wrapped and keep their fixed heights. Heights of the
    /// rest of the window are recomputed with the table kinds, so editing one
    /// cell cannot collapse the whole table geometry to ordinary 24 px rows.
    fn update_window_line_after_edit(&mut self, doc: &DocumentBuffer, edit_line: usize, delta: isize) {
        let tables = doc.ensure_table_index_for_window(self.window.start_line, self.window.end_line);
        let (cursor_line, _) = self.line_col(doc);
        let raw_first = tables
            .at(cursor_line)
            .filter(|t| self.table_raw_now(doc, t, cursor_line, false))
            .map(|t| t.first_line);
        let table_info = |line: usize| {
            let t = tables.at(line)?;
            if Some(t.first_line) == raw_first {
                return None;
            }
            Some(if line == t.sep_line() {
                crate::markdown::TableRowVisualKind::Delimiter
            } else if line == t.first_line {
                crate::markdown::TableRowVisualKind::Header
            } else if line == t.last_line {
                crate::markdown::TableRowVisualKind::LastRow
            } else {
                crate::markdown::TableRowVisualKind::Body
            })
        };
        let new_line_text = doc.line_text(edit_line);
        let _ = self.window.update_single_line(edit_line, new_line_text, delta, None, table_info);
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
        let cursor_before = self.cursor_offset;
        let removed = doc.slice_to_string(byte_range.clone());
        let at = byte_range.start;
        self.cursor_offset = new_cursor;
        self.selection_anchor = None;
        self.visual_cache_dirty = true;
        doc.record_change(at, removed, new_text, cursor_before, new_cursor, Some(self.tab_id))
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
        let cursor_before = self.cursor_offset;
        let removed = doc.slice_to_string(prev..self.cursor_offset);
        self.cursor_offset = prev;
        self.selection_anchor = None;
        let res = doc.record_change(prev, removed, String::new(), cursor_before, prev, Some(self.tab_id));
        let (_, ref note_edit, _) = res;
        if note_edit.line_diff == 0 && self.window.covers_line(note_edit.edit_line) {
            let delta = note_edit.new_len as isize - note_edit.old_len as isize;
            self.update_window_line_after_edit(doc, note_edit.edit_line, delta);
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
        let cursor_before = self.cursor_offset;
        let removed = doc.slice_to_string(offset..next);
        self.cursor_offset = offset;
        self.selection_anchor = None;
        let res = doc.record_change(offset, removed, String::new(), cursor_before, offset, Some(self.tab_id));
        let (_, ref note_edit, _) = res;
        if note_edit.line_diff == 0 && self.window.covers_line(note_edit.edit_line) {
            let delta = note_edit.new_len as isize - note_edit.old_len as isize;
            self.update_window_line_after_edit(doc, note_edit.edit_line, delta);
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
        let (cursor_before, note_edits, edit_event) = doc.undo_step(Some(self.tab_id))?;
        self.cursor_offset = cursor_before.min(doc.len());
        self.selection_anchor = None;
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
        let (cursor_after, note_edits, edit_event) = doc.redo_step(Some(self.tab_id))?;
        self.cursor_offset = cursor_after.min(doc.len());
        self.selection_anchor = None;
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
        let offset = self.cursor_offset.min(doc.len());
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
        headings: Option<&crate::settings::HeadingsSettings>,
        anchor_line: usize,
        suppress_selection_reveal: bool,
    ) {
        let basis_stale = self.window.is_empty()
            || self.window.word_wrap != word_wrap
            || self.window.wrap_cols != wrap_cols;

        let lines_total = doc.line_index.lines();
        let (start, end) = if lines_total <= 2 * crate::editor_window::MARGIN_LINES + 1 {
            (0, lines_total)
        } else if !self.window.is_empty()
            && !self.window.should_move_to(anchor_line, lines_total, word_wrap, wrap_cols)
        {
            let target_len = self
                .window
                .line_count()
                .max(2 * crate::editor_window::MARGIN_LINES + 1);
            let start = self.window.start_line.min(lines_total.saturating_sub(1));
            let end = (start + target_len).min(lines_total);
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
        let (cursor_line, _) = self.line_col(doc);
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
            headings,
            initial,
            |line| {
                if let Some(t) = tables.at(line) {
                    if (self.raw_tables.contains(&t.first_line) && t.contains(cursor_line))
                        || Self::reveals_table(reveal_state, t)
                    {
                        None
                    } else if line == t.sep_line() {
                        Some(crate::markdown::TableRowVisualKind::Delimiter)
                    } else if line == t.first_line {
                        Some(crate::markdown::TableRowVisualKind::Header)
                    } else if line == t.last_line {
                        Some(crate::markdown::TableRowVisualKind::LastRow)
                    } else {
                        Some(crate::markdown::TableRowVisualKind::Body)
                    }
                } else {
                    None
                }
            },
        );

        self.above_lines = window.start_line;
        self.below_lines = doc
            .line_index
            .lines()
            .saturating_sub(window.end_line);
        self.window = window;
        self.revealed_selection = reveal_state;
        if basis_stale {
            self.refresh_estimate_basis();
        }
        self.recompute_estimate();
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
        self.above_h = (self.above_lines as f64 * rows_per_line).round() * row_h;
        self.below_h = (self.below_lines as f64 * rows_per_line).round() * row_h;
    }

    pub fn ensure_visual_cache_with_headings(
        &mut self,
        doc: &mut DocumentBuffer,
        word_wrap: bool,
        wrap_cols: usize,
        headings: Option<&crate::settings::HeadingsSettings>,
        anchor_line: usize,
        suppress_selection_reveal: bool,
    ) {
        let reveal_state = self.selection_reveal_state(doc, suppress_selection_reveal);
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
                headings,
                anchor_line,
                suppress_selection_reveal,
            );
        }
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
    pub fn apply_external_edit_detailed(&mut self, edit: &crate::state::NoteEditEvent) {
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
            self.window = EditorWindow::default();
            self.visual_cache_dirty = true;
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
        let rows_per_line = self.est_rows_per_line.max(f64::MIN_POSITIVE);
        let row_h = self.est_row_h.max(1.0);
        let local = doc_y - self.above_h;
        if local < 0.0 {
            let rows_above = (-local / row_h).ceil();
            let lines_above = (rows_above / rows_per_line).round() as usize;
            return self.window.start_line.saturating_sub(lines_above);
        }
        let window_h = self.window.total_height();
        if local >= window_h {
            let rows_below = (local - window_h) / row_h;
            let lines_below = (rows_below / rows_per_line).round() as usize;
            return self.window.end_line.saturating_add(lines_below).min(last_line);
        }
        let local_v = self.window.height_index.v_idx_at_y(local);
        self.window
            .visual_rows
            .get(local_v)
            .map(|v| v.line_idx)
            .unwrap_or(self.window.start_line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_stability_when_edit_is_above() {
        let text_initial = "line 0\nline 1\nline 2\nline 3\nline 4\nline 5\n";

        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        // Viewport anchored at "line 3" (offset: 7*3 = 21)
        let line_3_offset = 21;
        view.viewport.offset = line_3_offset;
        view.scroll_top = 3.0 * 24.0;
        view.above_lines = 3;
        view.above_h = 3.0 * 24.0;

        // Another view inserts 100 new lines at line 0 (offset 0)
        let inserted = "inserted\n".repeat(100);
        let edit = Edit {
            start: 0,
            old_len: 0,
            new_len: inserted.len(),
        };

        view.apply_external_edit(edit, 100, 0);

        // The viewport anchor offset shifted by new_len
        assert_eq!(view.viewport.offset, line_3_offset + inserted.len());

        let text_after = format!("{}{}", inserted, text_initial);
        let line_slice = &text_after[view.viewport.offset..view.viewport.offset + 6];
        assert_eq!(line_slice, "line 3");

        // scroll_top increased by 100 lines * 24px
        assert_eq!(view.scroll_top, (3.0 + 100.0) * 24.0);
    }

    #[test]
    fn test_viewport_stability_when_edit_is_below() {
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        // Viewport anchored at "line 1" (offset 7)
        let line_1_offset = 7;
        view.viewport.offset = line_1_offset;
        view.scroll_top = 24.0;

        // Edit happens at line 4 (below viewport, offset 28)
        let line_4_offset = 28;
        let inserted = "hello\nworld\n";
        let edit = Edit {
            start: line_4_offset,
            old_len: 0,
            new_len: inserted.len(),
        };

        view.apply_external_edit(edit, 2, 0);

        // Viewport anchor and scroll_top did not change at all!
        assert_eq!(view.viewport.offset, line_1_offset);
        assert_eq!(view.scroll_top, 24.0);
    }

    #[test]
    fn test_multi_window_viewport_stability_with_window_shift() {
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        let start_line = 997_000;
        let end_line = 1_000_000;
        let anchor_line = 1_000_000;
        view.viewport.offset = anchor_line;
        view.cursor_offset = 50_000_000;

        let lines_vec: Vec<String> = (start_line..end_line).map(|i| format!("line {i}")).collect();
        let visual_rows: Vec<crate::markdown::VisualRow> = (start_line..end_line)
            .map(|i| crate::markdown::VisualRow {
                line_idx: i,
                sub_idx: 0,
                is_last_sub: true,
                char_start: 0,
                char_end: 10,
            })
            .collect();
        let height_index = crate::markdown::VisualRowHeightIndex::new(vec![24.0; visual_rows.len()]);

        view.window = EditorWindow {
            start_line,
            end_line,
            start_byte: 40_000_000,
            end_byte: 50_000_000,
            lines: std::sync::Arc::new(crate::editor_window::WindowLines::new(start_line, lines_vec)),
            visual_rows: std::sync::Arc::new(visual_rows),
            code_states: std::sync::Arc::new(crate::editor_window::WindowCodeStates::default()),
            height_index: std::sync::Arc::new(height_index),
            word_wrap: false,
            wrap_cols: 80,
        };

        view.above_lines = start_line;
        view.est_rows_per_line = 1.0;
        view.est_row_h = 24.0;
        view.above_h = start_line as f64 * 24.0;
        view.scroll_top = view.above_h + 100.0;

        let initial_local_scroll = view.scroll_top - view.above_h;
        assert_eq!(initial_local_scroll, 100.0);

        // View 1 at line 10 inserts 10 lines
        let edit_event = crate::state::NoteEditEvent {
            edit_line: 10,
            at: 100,
            old_len: 0,
            new_len: 10,
            line_diff: 10,
        };

        view.apply_external_edit_detailed(&edit_event);

        // 1. Invariant: local scroll (screen position of visible text) must be 100% UNCHANGED
        let new_local_scroll = view.scroll_top - view.above_h;
        assert_eq!(new_local_scroll, initial_local_scroll);

        // 2. Window lines shifted: 997,000 -> 997,010; 1,000,000 -> 1,000,010
        assert_eq!(view.window.start_line, 997_010);
        assert_eq!(view.window.end_line, 1_000_010);
        assert_eq!(view.window.lines.start(), 997_010);

        // 3. Line numbers in the visual rows incremented by 10 (gutter line numbers update!)
        assert_eq!(view.window.visual_rows.last().unwrap().line_idx, 1_000_009);

        // 4. Cursor shifted by new_len
        assert_eq!(view.cursor_offset, 50_000_010);

        // 5. Anchor line shifted by 10
        assert_eq!(view.viewport.offset, 1_000_010);

        // 6. The text inside the window is preserved exactly
        assert_eq!(view.window.line(997_010).unwrap(), "line 997000");

        // 7. Window remains non-empty and cached (zero rebuild cost on render/typing)
        assert!(!view.window.is_empty());
        assert!(!view.visual_cache_dirty);
    }

    #[test]
    fn test_view_state_resize_and_wrap_cols_anchoring() {
        let tab_id = TabId::new();
        let doc_id = DocumentId::new();
        let mut view = EditorViewState::new(tab_id, doc_id);

        let anchor_line = 500;
        view.viewport.offset = anchor_line;
        view.viewport.px_in_line = 5.0;

        let lines_vec: Vec<String> = (0..1000).map(|i| format!("line {i} with some content that can wrap when resized")).collect();
        let mut heights = Vec::new();
        let mut visual_rows = Vec::new();
        for i in 0..1000 {
            heights.push(24.0);
            visual_rows.push(crate::markdown::VisualRow {
                line_idx: i,
                sub_idx: 0,
                is_last_sub: true,
                char_start: 0,
                char_end: lines_vec[i].len(),
            });
        }
        let height_index = crate::markdown::VisualRowHeightIndex::new(heights);

        view.window = crate::editor_window::EditorWindow {
            start_line: 0,
            end_line: 1000,
            start_byte: 0,
            end_byte: 50_000,
            lines: std::sync::Arc::new(crate::editor_window::WindowLines::new(0, lines_vec.clone())),
            visual_rows: std::sync::Arc::new(visual_rows),
            code_states: std::sync::Arc::new(crate::editor_window::WindowCodeStates::default()),
            height_index: std::sync::Arc::new(height_index),
            word_wrap: true,
            wrap_cols: 80,
        };
        view.above_lines = 0;
        view.above_h = 0.0;
        let first_v = view.window.first_row_of_line(anchor_line).unwrap();
        view.scroll_top = view.above_h + view.window.height_index.top_of(first_v) + view.viewport.px_in_line as f64;

        // When resizing (dragging split border), wrap_cols changes:
        // Say wrap_cols changes to 40, increasing rows for wrapped lines
        let new_wrap_cols = 40;
        assert!(view.window.should_move_to(view.viewport.offset, 1000, true, new_wrap_cols));

        // Rebuilt window with new_wrap_cols
        let mut new_heights = Vec::new();
        let mut new_visual_rows = Vec::new();
        for i in 0..1000 {
            // Some lines wrap into 2 rows at wrap_cols 40
            let rows_count = if i % 2 == 0 { 2 } else { 1 };
            for sub in 0..rows_count {
                new_heights.push(24.0);
                new_visual_rows.push(crate::markdown::VisualRow {
                    line_idx: i,
                    sub_idx: sub,
                    is_last_sub: sub + 1 == rows_count,
                    char_start: 0,
                    char_end: lines_vec[i].len(),
                });
            }
        }
        let new_height_index = crate::markdown::VisualRowHeightIndex::new(new_heights);
        let new_window = crate::editor_window::EditorWindow {
            start_line: 0,
            end_line: 1000,
            start_byte: 0,
            end_byte: 50_000,
            lines: std::sync::Arc::new(crate::editor_window::WindowLines::new(0, lines_vec)),
            visual_rows: std::sync::Arc::new(new_visual_rows),
            code_states: std::sync::Arc::new(crate::editor_window::WindowCodeStates::default()),
            height_index: std::sync::Arc::new(new_height_index),
            word_wrap: true,
            wrap_cols: new_wrap_cols,
        };

        // Anchor recalculation
        let new_first_v = new_window.first_row_of_line(view.viewport.offset).unwrap();
        let new_line_local_y = new_window.height_index.top_of(new_first_v);
        let new_scroll_top = view.above_h + new_line_local_y + view.viewport.px_in_line as f64;
        view.scroll_top = new_scroll_top;
        view.window = new_window;

        // Invariant: local_scroll = scroll_top - above_h lands exactly on anchor line + px_in_line
        let local_scroll = view.scroll_top - view.above_h;
        assert_eq!(local_scroll, new_line_local_y + 5.0);
        let vr = view.window.height_index.visible_range(local_scroll, 600.0, 300.0);
        assert!(vr.start <= new_first_v);
        assert!(vr.end > new_first_v);
    }

    #[test]
    fn test_apply_external_edit_detailed_cursor_and_selection_cases() {
        let tab_id = TabId::new();
        let doc_id = DocumentId::new();

        // 1. Edit strictly above selection: [20..30], edit at 5 (insert 10 bytes)
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 30;
        view.selection_anchor = Some(20);
        let edit = crate::state::NoteEditEvent {
            edit_line: 0,
            at: 5,
            old_len: 0,
            new_len: 10,
            line_diff: 0,
        };
        view.apply_external_edit_detailed(&edit);
        assert_eq!(view.cursor_offset, 40);
        assert_eq!(view.selection_anchor, Some(30));

        // 2. Edit strictly below selection: [20..30], edit at 50 (insert 10 bytes)
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 30;
        view.selection_anchor = Some(20);
        let edit = crate::state::NoteEditEvent {
            edit_line: 5,
            at: 50,
            old_len: 0,
            new_len: 10,
            line_diff: 0,
        };
        view.apply_external_edit_detailed(&edit);
        assert_eq!(view.cursor_offset, 30);
        assert_eq!(view.selection_anchor, Some(20));

        // 3. Edit inside selection: [20..30], edit at 25 (insert 5 bytes)
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 30;
        view.selection_anchor = Some(20);
        let edit = crate::state::NoteEditEvent {
            edit_line: 2,
            at: 25,
            old_len: 0,
            new_len: 5,
            line_diff: 0,
        };
        view.apply_external_edit_detailed(&edit);
        // Start (20) unchanged, end (30) expanded to 35
        assert_eq!(view.selection_anchor, Some(20));
        assert_eq!(view.cursor_offset, 35);

        // 4. Edit at start boundary: [20..30], insert at 20
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 30;
        view.selection_anchor = Some(20);
        let edit = crate::state::NoteEditEvent {
            edit_line: 2,
            at: 20,
            old_len: 0,
            new_len: 5,
            line_diff: 0,
        };
        view.apply_external_edit_detailed(&edit);
        // Start mapped with Bias::Left stays at 20, end mapped with Bias::Right shifts to 35
        assert_eq!(view.selection_anchor, Some(20));
        assert_eq!(view.cursor_offset, 35);

        // 5. Cursor without selection at boundary of insert: cursor=20, insert at 20
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 20;
        view.selection_anchor = None;
        let edit = crate::state::NoteEditEvent {
            edit_line: 2,
            at: 20,
            old_len: 0,
            new_len: 5,
            line_diff: 0,
        };
        view.apply_external_edit_detailed(&edit);
        // Single cursor with Bias::Left stays at 20
        assert_eq!(view.cursor_offset, 20);
    }

    #[test]
    fn test_multi_window_switch_stability_no_drift() {
        let text = (0..500)
            .map(|i| format!("Line {:03}: some meaningful content here for testing.\n", i))
            .collect::<String>();
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text,
            1,
        );
        let lines_total = doc.line_index.lines();

        let tab1 = TabId::new();
        let tab2 = TabId::new();
        let mut view1 = EditorViewState::new(tab1, doc.id);
        let mut view2 = EditorViewState::new(tab2, doc.id);

        let wrap_cols = 60;
        view1.rebuild_visual_cache_with_headings(&mut doc, true, wrap_cols, None, 100, false);
        view2.rebuild_visual_cache_with_headings(&mut doc, true, wrap_cols, None, 50, false);

        let initial_scroll_1 = view1.doc_y_of_line(lines_total, 100);
        let initial_scroll_2 = view2.doc_y_of_line(lines_total, 50);

        view1.scroll_top = initial_scroll_1;
        view1.viewport.offset = view1.line_at_doc_y(lines_total, initial_scroll_1);
        let line_top1 = view1.doc_y_of_line(lines_total, view1.viewport.offset);
        view1.viewport.px_in_line = ((initial_scroll_1 - line_top1) as f32).max(0.0);

        view2.scroll_top = initial_scroll_2;
        view2.viewport.offset = view2.line_at_doc_y(lines_total, initial_scroll_2);
        let line_top2 = view2.doc_y_of_line(lines_total, view2.viewport.offset);
        view2.viewport.px_in_line = ((initial_scroll_2 - line_top2) as f32).max(0.0);

        // Simulate 20 alternating switch cycles between Split 1 and Split 2
        for _ in 0..20 {
            // Switch 1 -> 2:
            // 1) Snapshot outgoing (view1)
            view1.viewport.offset = view1.line_at_doc_y(lines_total, view1.scroll_top);
            let top1 = view1.doc_y_of_line(lines_total, view1.viewport.offset);
            view1.viewport.px_in_line = ((view1.scroll_top - top1) as f32).max(0.0);

            // 2) Restore incoming (view2)
            view2.ensure_visual_cache_with_headings(&mut doc, true, wrap_cols, None, view2.viewport.offset, false);
            let restore_y2 = if view2.window.wrap_cols == wrap_cols {
                view2.scroll_top
            } else {
                let top2 = view2.doc_y_of_line(lines_total, view2.viewport.offset);
                top2 + view2.viewport.px_in_line as f64
            };
            view2.scroll_top = restore_y2;

            // Switch 2 -> 1:
            // 1) Snapshot outgoing (view2)
            view2.viewport.offset = view2.line_at_doc_y(lines_total, view2.scroll_top);
            let top2 = view2.doc_y_of_line(lines_total, view2.viewport.offset);
            view2.viewport.px_in_line = ((view2.scroll_top - top2) as f32).max(0.0);

            // 2) Restore incoming (view1)
            view1.ensure_visual_cache_with_headings(&mut doc, true, wrap_cols, None, view1.viewport.offset, false);
            let restore_y1 = if view1.window.wrap_cols == wrap_cols {
                view1.scroll_top
            } else {
                let top1 = view1.doc_y_of_line(lines_total, view1.viewport.offset);
                top1 + view1.viewport.px_in_line as f64
            };
            view1.scroll_top = restore_y1;
        }

        assert_eq!(view1.scroll_top, initial_scroll_1, "Split 1 scroll_top must have zero drift after repeated switches");
        assert_eq!(view1.viewport.offset, 100, "Split 1 anchor line must remain line 100");
        assert_eq!(view2.scroll_top, initial_scroll_2, "Split 2 scroll_top must have zero drift after repeated switches");
        assert_eq!(view2.viewport.offset, 50, "Split 2 anchor line must remain line 50");
    }

    #[test]
    fn test_grid_table_at_and_raw_table_reset() {
        let text = "Intro\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nMiddle\n\n| C | D |\n|---|---|\n| 3 | 4 |\n\nOutro";
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text.to_string(),
            1,
        );
        let tab_id = TabId::new();
        let mut view = TabSession::new(tab_id, doc.id);

        let (tbl1_fl, tbl2_fl) = {
            let tables = doc.ensure_table_index();
            assert_eq!(tables.blocks.len(), 2);
            assert_eq!(tables.blocks[0].first_line, 2);
            assert_eq!(tables.blocks[0].last_line, 4);
            assert_eq!(tables.blocks[1].first_line, 8);
            assert_eq!(tables.blocks[1].last_line, 10);
            (tables.blocks[0].first_line, tables.blocks[1].first_line)
        };

        // Initially both tables return Some (grid mode)
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 2, 2, false).is_some());
            assert!(view.grid_table_at(&doc, &tables, 8, 8, false).is_some());
        }

        // Toggle Table 1 into raw mode
        view.toggle_raw_table_at(tbl1_fl);
        assert!(view.is_table_raw(tbl1_fl));
        assert!(!view.is_table_raw(tbl2_fl));

        // Table 1 is in raw mode (None), Table 2 remains in grid mode (Some)
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 2, 2, false).is_none());
            assert!(view.grid_table_at(&doc, &tables, 4, 3, false).is_none());
            assert!(view.grid_table_at(&doc, &tables, 8, 8, false).is_some());
            assert!(view.grid_table_at(&doc, &tables, 10, 8, false).is_some());
        }

        // When cursor moves outside, both tables display as grid tables, but Table 1 remembers it is raw
        view.cursor_offset = 0; // outside tables
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        assert!(view.is_table_raw(tbl1_fl), "Table 1 raw setting persists");
        {
            let tables2 = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables2, 2, 0, false).is_some(), "Table 1 displays as grid mode when cursor outside");
            assert!(view.grid_table_at(&doc, &tables2, 8, 0, false).is_some(), "Table 2 displays as grid mode");
        }

        // When cursor moves back to Table 1, Table 1 is in raw code mode
        view.cursor_offset = doc.line_start_byte(2);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables3 = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables3, 2, 2, false).is_none(), "Table 1 enters raw mode when cursor returns");
            assert!(view.grid_table_at(&doc, &tables3, 8, 2, false).is_some(), "Table 2 remains in grid mode");
        }

        // When cursor moves to Table 2 (which is in inline mode), Table 2 stays in inline mode
        view.cursor_offset = doc.line_start_byte(8);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables4 = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables4, 2, 8, false).is_some(), "Table 1 displays as grid mode");
            assert!(view.grid_table_at(&doc, &tables4, 8, 8, false).is_some(), "Table 2 displays as inline grid mode");
        }
    }

    #[test]
    fn test_selection_reveals_table_as_source() {
        let text = "Intro\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nMiddle\n\n| C | D |\n|---|---|\n| 3 | 4 |\n\nOutro";
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text.to_string(),
            1,
        );
        let mut view = TabSession::new(TabId::new(), doc.id);

        // Selection from "Middle" through the second table reveals its source.
        view.selection_anchor = Some(doc.line_start_byte(6));
        view.cursor_offset = doc.line_start_byte(10) + 3;
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 8, 10, false).is_none());
            assert!(view.grid_table_at(&doc, &tables, 9, 10, false).is_none());
            assert!(view.grid_table_at(&doc, &tables, 10, 10, false).is_none());
            assert!(view.grid_table_at(&doc, &tables, 2, 10, false).is_some());
            assert_eq!(view.revealed_selection, Some((6, 6, 10)));

            let header_row = view.window.first_row_of_line(8).unwrap();
            assert_eq!(view.window.height_index.height_of(header_row), 24.0);
            assert!(!view.is_table_raw(8), "reveal must not persist as raw mode");
        }

        // A drag that starts inside a cell keeps the grid and table copy mode,
        // even when it is extended above the table.
        view.selection_anchor = Some(doc.line_start_byte(10) + 3);
        view.cursor_offset = doc.line_start_byte(6);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 8, 6, false).is_some());
            assert_eq!(view.revealed_selection, Some((10, 6, 10)));
        }
        view.selection_anchor = Some(doc.line_start_byte(6));
        view.cursor_offset = doc.line_start_byte(10) + 3;
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);

        // Ctrl/preview selection keeps the grid layout.
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, true);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 8, 10, true).is_some());
            assert_eq!(view.revealed_selection, None);
            let header_row = view.window.first_row_of_line(8).unwrap();
            assert_eq!(view.window.height_index.height_of(header_row), 42.0);
        }

        // A selection that ends above the table keeps the grid.
        view.selection_anchor = Some(doc.line_start_byte(0));
        view.cursor_offset = doc.line_start_byte(6) + 2;
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 8, 6, false).is_some());
            let header_row = view.window.first_row_of_line(8).unwrap();
            assert_eq!(view.window.height_index.height_of(header_row), 42.0);
        }

        // Collapsing the selection returns the grid.
        view.selection_anchor = Some(view.cursor_offset);
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);
        {
            let tables = doc.ensure_table_index();
            assert!(view.grid_table_at(&doc, &tables, 8, 6, false).is_some());
        }
    }

    #[test]
    fn test_selection_span_handles_multibyte_boundaries() {
        let text = "Начало\n\n| Колонка |\n|---|\n| значение";
        let mut doc = DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::from("test.md"),
            "test".to_string(),
            text.to_string(),
            1,
        );
        let mut view = TabSession::new(TabId::new(), doc.id);

        view.selection_anchor = Some(0);
        view.cursor_offset = doc.len();
        view.ensure_visual_cache_with_headings(&mut doc, false, 80, None, 0, false);

        assert_eq!(view.revealed_selection, Some((0, 0, 4)));
        let tables = doc.ensure_table_index();
        assert!(view.grid_table_at(&doc, &tables, 2, 4, false).is_none());
    }

    #[test]
    fn test_table_cell_selection_column_drag() {
        let mut sel = TableCellSelection::new(2, 6, 2); // anchor: row 6, col 2
        assert!(!sel.is_multi_cell());
        assert!(sel.contains(6, 2));
        assert!(!sel.contains(5, 2));

        // Dragging upwards in the same column to row 1
        sel.head_row = 1;
        sel.head_col = 2;
        assert!(sel.is_multi_cell());
        let rect = sel.rect();
        assert_eq!(rect.row_start, 1);
        assert_eq!(rect.row_end, 7);
        assert_eq!(rect.col_start, 2);
        assert_eq!(rect.col_end, 3);

        // All rows 1..=6 in col 2 are in the selection
        for r in 1..=6 {
            assert!(sel.contains(r, 2), "row {} in col 2 must be selected", r);
            assert!(!sel.contains(r, 0), "col 0 must not be selected");
            assert!(!sel.contains(r, 1), "col 1 must not be selected");
            assert!(!sel.contains(r, 3), "col 3 must not be selected");
        }
        assert!(!sel.contains(0, 2), "row 0 not selected");
        assert!(!sel.contains(7, 2), "row 7 not selected");
    }
}
