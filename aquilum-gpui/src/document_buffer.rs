use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::path::PathBuf;
use std::time::Instant;
use uuid::Uuid;

use crate::code_scan::CodeCheckpoints;
use crate::line_index::LineIndex;
use crate::state::NoteEditEvent;
use crate::workspace::session_core::ops::{EditEvent, TextOp};
use crate::workspace::session_core::undo::{History, Selection, Step, TextChange};
use crate::workspace::TabId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DocumentId(pub Uuid);

impl DocumentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_u64(val: u64) -> Self {
        Self(Uuid::from_u128(val as u128))
    }
}

impl Default for DocumentId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bias {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    /// Byte offset of the start of the replaced range (in coordinates BEFORE edit)
    pub start: usize,
    /// Byte length of the replaced text
    pub old_len: usize,
    /// Byte length of the inserted text
    pub new_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewportAnchor {
    /// Byte offset of the start of the first (partially) visible logical line
    pub offset: usize,
    /// Bias when edits occur exactly at anchor boundary (Right ensures inserted text moves above viewport)
    pub bias: Bias,
    /// Pixel distance this line is scrolled above the top edge
    pub px_in_line: f32,
}

impl Default for ViewportAnchor {
    fn default() -> Self {
        Self {
            offset: 0,
            bias: Bias::Right,
            px_in_line: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum BufferEvent {
    Edited {
        edit: Edit,
        source: Option<TabId>,
        version: u64,
    },
    Reloaded,
    PathChanged {
        old: PathBuf,
        new: PathBuf,
    },
    Deleted,
    DirtyChanged(bool),
}

/// Pure function to transform a byte offset after an edit.
/// Used for cursor, anchor, selection ranges, and viewport anchors.
pub fn transform_offset(offset: usize, e: Edit, bias: Bias) -> usize {
    let old_end = e.start + e.old_len;
    if offset < e.start || (offset == e.start && matches!(bias, Bias::Left)) {
        return offset; // Before edit
    }
    if offset >= old_end {
        return offset + e.new_len - e.old_len; // After edit: shifted by delta
    }
    // Inside replaced range: collapse
    e.start + if matches!(bias, Bias::Right) { e.new_len } else { 0 }
}

/// Pure function to transform a selection range [start, end].
/// Start is mapped with Bias::Left, end with Bias::Right.
pub fn transform_selection_range(range: (usize, usize), e: Edit) -> (usize, usize) {
    let (sel_start, sel_end) = if range.0 <= range.1 {
        (range.0, range.1)
    } else {
        (range.1, range.0)
    };

    let new_start = transform_offset(sel_start, e, Bias::Left);
    let new_end = transform_offset(sel_end, e, Bias::Right);

    if range.0 <= range.1 {
        (new_start, new_end)
    } else {
        (new_end, new_start)
    }
}

pub struct DocumentBuffer {
    pub id: DocumentId,
    pub path: PathBuf,
    pub title: String,
    pub rope: crop::Rope,
    pub version: u64,
    pub is_dirty: bool,
    pub first_dirty_time: Option<Instant>,
    pub line_index: LineIndex,
    pub code_checkpoints: CodeCheckpoints,
    pub history: History,
    pub stats_cache: std::cell::Cell<Option<(u64, crate::state::TextStats)>>,
    pub table_index: std::cell::RefCell<crate::table::TableIndex>,
}

impl DocumentBuffer {
    pub fn new(id: DocumentId, path: PathBuf, title: String, content: String, version: u64) -> Self {
        let rope = crop::Rope::from(content);
        Self::from_rope(id, path, title, rope, version)
    }

    pub fn from_rope(id: DocumentId, path: PathBuf, title: String, rope: crop::Rope, version: u64) -> Self {
        let line_index = LineIndex::new_from_rope(&rope);
        Self {
            id,
            path,
            title,
            rope,
            version,
            is_dirty: false,
            first_dirty_time: None,
            line_index,
            code_checkpoints: CodeCheckpoints::new(),
            history: History::new(),
            stats_cache: std::cell::Cell::new(None),
            table_index: std::cell::RefCell::new(crate::table::TableIndex::new()),
        }
    }

    pub fn ensure_table_index_for_window(&self, start_line: usize, end_line: usize) -> std::cell::Ref<'_, crate::table::TableIndex> {
        let mut idx = self.table_index.borrow_mut();
        idx.scan_window(self.version, &self.rope, start_line, end_line);
        drop(idx);
        self.table_index.borrow()
    }

    pub fn ensure_table_index(&self) -> std::cell::Ref<'_, crate::table::TableIndex> {
        let mut idx = self.table_index.borrow_mut();
        if idx.rev != self.version || !idx.fully_scanned {
            idx.scan_rope(self.version, &self.rope);
        }
        drop(idx);
        self.table_index.borrow()
    }

    pub fn from_file(id: DocumentId, path: PathBuf, title: String, version: u64) -> std::io::Result<Self> {
        use std::io::Read;
        let file = std::fs::File::open(&path)?;
        let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if file_len < 2 * 1024 * 1024 {
            let mut s = String::with_capacity(file_len as usize);
            let mut reader = std::io::BufReader::new(file);
            reader.read_to_string(&mut s)?;
            return Ok(Self::new(id, path, title, s, version));
        }

        let mut reader = std::io::BufReader::with_capacity(256 * 1024, file);
        let mut builder = crop::RopeBuilder::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut leftover = 0;
        loop {
            let n = reader.read(&mut buf[leftover..])?;
            if n == 0 {
                if leftover > 0 {
                    let s = String::from_utf8_lossy(&buf[..leftover]);
                    builder.append(&s);
                }
                break;
            }
            let total = leftover + n;
            match std::str::from_utf8(&buf[..total]) {
                Ok(s) => {
                    builder.append(s);
                    leftover = 0;
                }
                Err(e) => {
                    let valid_up_to = e.valid_up_to();
                    if valid_up_to > 0 {
                        // Safety: validated by std::str::from_utf8
                        let s = unsafe { std::str::from_utf8_unchecked(&buf[..valid_up_to]) };
                        builder.append(s);
                    }
                    if let Some(err_len) = e.error_len() {
                        // Invalid byte sequence encountered: replace with replacement char and skip
                        builder.append("\u{FFFD}");
                        let next_start = valid_up_to + err_len;
                        let remaining = total - next_start;
                        buf.copy_within(next_start..total, 0);
                        leftover = remaining;
                    } else {
                        // Incomplete UTF-8 sequence at end of buffer: wait for next read
                        let remaining = total - valid_up_to;
                        buf.copy_within(valid_up_to..total, 0);
                        leftover = remaining;
                    }
                }
            }
        }
        let rope = builder.build();
        Ok(Self::from_rope(id, path, title, rope, version))
    }

    #[inline]
    pub fn rope(&self) -> &crop::Rope {
        &self.rope
    }

    #[inline]
    pub fn content(&self) -> String {
        self.rope.to_string()
    }

    #[inline]
    pub fn to_string(&self) -> String {
        self.rope.to_string()
    }

    pub fn text_stats(&self) -> crate::state::TextStats {
        if let Some((v, s)) = self.stats_cache.get() {
            if v == self.version {
                return s;
            }
        }
        let len = self.rope.byte_len();
        if len > crate::state::TEXT_STATS_EXACT_LIMIT {
            let s = crate::state::TextStats::Large { bytes: len };
            self.stats_cache.set(Some((self.version, s)));
            return s;
        }
        // Count words and chars by iterating chunks of the rope without allocating a full String
        let mut words = 0usize;
        let mut chars = 0usize;
        let mut in_word = false;
        for chunk in self.rope.chunks() {
            chars += chunk.chars().count();
            for c in chunk.chars() {
                if c.is_whitespace() {
                    in_word = false;
                } else if !in_word {
                    in_word = true;
                    words += 1;
                }
            }
        }
        let s = crate::state::TextStats::Exact { words, chars };
        self.stats_cache.set(Some((self.version, s)));
        s
    }

    pub fn ensure_checkpoints(&mut self, up_to_line: usize) {
        self.code_checkpoints.ensure_rope(&self.rope, up_to_line);
    }

    pub fn checkpoint_state_at(&self, line: usize) -> Option<crate::markdown::CodeScanState> {
        self.code_checkpoints.state_at_rope(&self.rope, line)
    }

    pub fn scan_code_ahead(&mut self, max_lines: usize) -> usize {
        if self.code_checkpoints.is_complete(self.line_index.lines()) {
            return 0;
        }
        self.code_checkpoints.extend_slice_rope(&self.rope, max_lines)
    }

    #[inline]
    pub fn line_start_byte(&self, line: usize) -> usize {
        let lines = self.line_index.lines();
        if line >= lines {
            self.rope.byte_len()
        } else if line < self.rope.line_len() {
            self.rope.byte_of_line(line)
        } else {
            self.rope.byte_len()
        }
    }

    #[inline]
    pub fn line_end_byte(&self, line: usize) -> usize {
        let lines = self.line_index.lines();
        if line + 1 >= lines {
            self.rope.byte_len()
        } else {
            let next_start = self.line_start_byte(line + 1);
            let cur_start = self.line_start_byte(line);
            next_start.saturating_sub(1).max(cur_start)
        }
    }

    #[inline]
    pub fn line_of_byte(&self, byte: usize) -> usize {
        let byte = byte.min(self.rope.byte_len());
        self.rope.line_of_byte(byte)
    }

    #[inline]
    pub fn is_char_boundary(&self, byte: usize) -> bool {
        self.rope.is_char_boundary(byte)
    }

    #[inline]
    pub fn slice_to_string(&self, range: std::ops::Range<usize>) -> String {
        let start = range.start.min(self.rope.byte_len());
        let end = range.end.min(self.rope.byte_len());
        if start >= end {
            String::new()
        } else {
            self.rope.byte_slice(start..end).to_string()
        }
    }

    #[inline]
    pub fn line_text(&self, line: usize) -> String {
        let start = self.line_start_byte(line);
        let end = self.line_end_byte(line);
        self.slice_to_string(start..end)
    }

    pub fn word_range_at(&self, offset: usize) -> (usize, usize) {
        let len = self.rope.byte_len();
        if len == 0 {
            return (0, 0);
        }
        let mut offset = offset.min(len);
        while offset > 0 && !self.is_char_boundary(offset) {
            offset -= 1;
        }

        // Bounded window around offset: 1024 bytes before and after
        let win_start = offset.saturating_sub(1024);
        let mut win_start = win_start;
        while win_start > 0 && !self.is_char_boundary(win_start) {
            win_start -= 1;
        }
        let win_end = (offset + 1024).min(len);
        let mut win_end = win_end;
        while win_end < len && !self.is_char_boundary(win_end) {
            win_end += 1;
        }

        let slice = self.slice_to_string(win_start..win_end);
        let local_offset = offset - win_start;

        let chars: Vec<(usize, char)> = slice.char_indices().collect();
        if chars.is_empty() {
            return (offset, offset);
        }
        let char_idx = chars.iter().position(|(idx, _)| *idx >= local_offset).unwrap_or(chars.len() - 1);
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

        let mut start_idx = char_idx;
        while start_idx > 0 && is_word_char(chars[start_idx - 1].1) {
            start_idx -= 1;
        }
        let mut end_idx = char_idx;
        while end_idx < chars.len() && is_word_char(chars[end_idx].1) {
            end_idx += 1;
        }
        if start_idx == end_idx {
            return (offset, offset);
        }
        let start_byte = win_start + chars[start_idx].0;
        let end_byte = if end_idx < chars.len() {
            win_start + chars[end_idx].0
        } else {
            win_start + slice.len()
        };
        (start_byte, end_byte)
    }

    pub fn line_range_at(&self, offset: usize) -> (usize, usize) {
        let len = self.rope.byte_len();
        let mut offset = offset.min(len);
        while offset > 0 && !self.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.line_of_byte(offset);
        let start = self.line_start_byte(line);
        let end = if line + 1 < self.line_index.lines() {
            self.line_start_byte(line + 1)
        } else {
            len
        };
        (start, end)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.rope.byte_len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rope.byte_len() == 0
    }

    pub fn mark_dirty(&mut self) -> Option<BufferEvent> {
        if !self.is_dirty {
            self.is_dirty = true;
            if self.first_dirty_time.is_none() {
                self.first_dirty_time = Some(Instant::now());
            }
            Some(BufferEvent::DirtyChanged(true))
        } else {
            None
        }
    }

    pub fn mark_clean(&mut self) -> Option<BufferEvent> {
        if self.is_dirty {
            self.is_dirty = false;
            self.first_dirty_time = None;
            Some(BufferEvent::DirtyChanged(false))
        } else {
            None
        }
    }

    pub fn set_path(&mut self, new_path: PathBuf) -> BufferEvent {
        let old = std::mem::replace(&mut self.path, new_path.clone());
        BufferEvent::PathChanged { old, new: new_path }
    }

    pub fn reload(&mut self, new_content: String) -> BufferEvent {
        self.rope = crop::Rope::from(new_content);
        self.version += 1;
        self.is_dirty = false;
        self.first_dirty_time = None;
        self.line_index = LineIndex::new_from_rope(&self.rope);
        self.code_checkpoints = CodeCheckpoints::new();
        self.history = History::new();
        self.table_index.borrow_mut().scan_rope(self.version, &self.rope);
        BufferEvent::Reloaded
    }

    pub fn record_change(
        &mut self,
        at: usize,
        removed: String,
        inserted: String,
        cursor_before: usize,
        cursor_after: usize,
        origin: Option<TabId>,
    ) -> (BufferEvent, NoteEditEvent, EditEvent) {
        self.record_change_with_selection(at, removed, inserted, Selection::cursor_only(cursor_before), Selection::cursor_only(cursor_after), origin)
    }

    /// Single point of truth for text mutation from the editing view.
    /// Every call is one undo step; no coalescing (AkelPad AECO_DETAILEDUNDO).
    pub fn record_change_with_selection(
        &mut self,
        at: usize,
        removed: String,
        inserted: String,
        sel_before: Selection,
        sel_after: Selection,
        origin: Option<TabId>,
    ) -> (BufferEvent, NoteEditEvent, EditEvent) {
        if removed.is_empty() && inserted.is_empty() {
            let edit_line = self.line_of_byte(at);
            return (
                BufferEvent::Edited {
                    edit: Edit { start: at, old_len: 0, new_len: 0 },
                    source: origin,
                    version: self.version,
                },
                NoteEditEvent { edit_line, at, old_len: 0, new_len: 0, line_diff: 0 },
                EditEvent { doc: self.id, version: self.version, ops: SmallVec::new(), origin, cursor_after: Some(sel_after.cursor) },
            );
        }

        let edit_line = self.line_of_byte(at);
        let line_diff = (inserted.matches('\n').count() as isize) - (removed.matches('\n').count() as isize);
        let note_edit = NoteEditEvent {
            edit_line,
            at,
            old_len: removed.len(),
            new_len: inserted.len(),
            line_diff,
        };

        let mut ops = SmallVec::new();
        if !removed.is_empty() {
            ops.push(TextOp::Delete { at, len: removed.len() });
        }
        if !inserted.is_empty() {
            ops.push(TextOp::Insert { at, len: inserted.len() });
        }

        self.rope.replace(at..at + removed.len(), &inserted);
        self.version += 1;
        self.line_index.splice_rope(&self.rope, at);
        self.code_checkpoints.truncate_to(edit_line);
        self.table_index.borrow_mut().apply_edit(self.version, edit_line, line_diff, &self.rope);
        self.is_dirty = true;
        if self.first_dirty_time.is_none() {
            self.first_dirty_time = Some(Instant::now());
        }
        self.stats_cache.set(None);

        self.history.record(Step {
            changes: vec![TextChange { at, removed, inserted }],
            sel_before,
            sel_after,
        });

        let edit_event = EditEvent {
            doc: self.id,
            version: self.version,
            ops,
            origin,
            cursor_after: Some(sel_after.cursor),
        };

        let buffer_event = BufferEvent::Edited {
            edit: Edit {
                start: at,
                old_len: note_edit.old_len,
                new_len: note_edit.new_len,
            },
            source: origin,
            version: self.version,
        };

        (buffer_event, note_edit, edit_event)
    }

    pub fn undo_step(
        &mut self,
        origin: Option<TabId>,
    ) -> Option<(Selection, Vec<NoteEditEvent>, EditEvent)> {
        let step = self.history.take(false)?;
        let mut note_edits = Vec::new();
        let mut ops = SmallVec::new();

        self.version += 1;
        for change in step.changes.iter().rev() {
            let at = change.at;
            let old_len = change.inserted.len();
            let new_len = change.removed.len();

            self.rope.replace(at..at + old_len, &change.removed);
            self.line_index.splice_rope(&self.rope, at);
            let edit_line = self.line_of_byte(at);
            self.code_checkpoints.truncate_to(edit_line);

            let line_diff = (change.removed.matches('\n').count() as isize)
                - (change.inserted.matches('\n').count() as isize);

            note_edits.push(NoteEditEvent {
                edit_line,
                at,
                old_len,
                new_len,
                line_diff,
            });

            if !change.inserted.is_empty() {
                ops.push(TextOp::Delete { at, len: change.inserted.len() });
            }
            if !change.removed.is_empty() {
                ops.push(TextOp::Insert { at, len: change.removed.len() });
            }

            self.table_index
                .borrow_mut()
                .apply_edit(self.version, edit_line, line_diff, &self.rope);
        }

        self.is_dirty = true;
        if self.first_dirty_time.is_none() {
            self.first_dirty_time = Some(Instant::now());
        }
        self.stats_cache.set(None);

        let edit_event = EditEvent {
            doc: self.id,
            version: self.version,
            ops,
            origin,
            cursor_after: Some(step.sel_before.cursor),
        };

        Some((step.sel_before, note_edits, edit_event))
    }

    pub fn redo_step(
        &mut self,
        origin: Option<TabId>,
    ) -> Option<(Selection, Vec<NoteEditEvent>, EditEvent)> {
        let step = self.history.take(true)?;
        let mut note_edits = Vec::new();
        let mut ops = SmallVec::new();

        self.version += 1;
        for change in step.changes.iter() {
            let at = change.at;
            let old_len = change.removed.len();
            let new_len = change.inserted.len();

            self.rope.replace(at..at + old_len, &change.inserted);
            self.line_index.splice_rope(&self.rope, at);
            let edit_line = self.line_of_byte(at);
            self.code_checkpoints.truncate_to(edit_line);

            let line_diff = (change.inserted.matches('\n').count() as isize)
                - (change.removed.matches('\n').count() as isize);

            note_edits.push(NoteEditEvent {
                edit_line,
                at,
                old_len,
                new_len,
                line_diff,
            });

            if !change.removed.is_empty() {
                ops.push(TextOp::Delete { at, len: change.removed.len() });
            }
            if !change.inserted.is_empty() {
                ops.push(TextOp::Insert { at, len: change.inserted.len() });
            }

            self.table_index
                .borrow_mut()
                .apply_edit(self.version, edit_line, line_diff, &self.rope);
        }

        self.is_dirty = true;
        if self.first_dirty_time.is_none() {
            self.first_dirty_time = Some(Instant::now());
        }
        self.stats_cache.set(None);

        let edit_event = EditEvent {
            doc: self.id,
            version: self.version,
            ops,
            origin,
            cursor_after: Some(step.sel_after.cursor),
        };

        Some((step.sel_after, note_edits, edit_event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_offset_rules() {
        // Insertion: "Hello World" -> insert "Beautiful " at 6
        let e_ins = Edit {
            start: 6,
            old_len: 0,
            new_len: 10,
        };

        // Before edit stays unchanged
        assert_eq!(transform_offset(0, e_ins, Bias::Left), 0);
        assert_eq!(transform_offset(5, e_ins, Bias::Left), 5);

        // Exactly at insertion point: Bias::Left stays at 6, Bias::Right moves to 16
        assert_eq!(transform_offset(6, e_ins, Bias::Left), 6);
        assert_eq!(transform_offset(6, e_ins, Bias::Right), 16);

        // After edit is shifted by +10
        assert_eq!(transform_offset(11, e_ins, Bias::Left), 21);
        assert_eq!(transform_offset(11, e_ins, Bias::Right), 21);

        // Deletion: delete 5 bytes starting at 6
        let e_del = Edit {
            start: 6,
            old_len: 5,
            new_len: 0,
        };

        // Inside deleted range collapses to start
        assert_eq!(transform_offset(7, e_del, Bias::Left), 6);
        assert_eq!(transform_offset(9, e_del, Bias::Right), 6);
        // After deleted range shifts by -5
        assert_eq!(transform_offset(15, e_del, Bias::Left), 10);
    }

    #[test]
    fn test_transform_selection_range() {
        // Range 5..10, insert 4 chars at 7
        let edit = Edit {
            start: 7,
            old_len: 0,
            new_len: 4,
        };
        let res = transform_selection_range((5, 10), edit);
        assert_eq!(res, (5, 14));

        // Range 5..10, delete 7..9 (2 chars)
        let edit_del = Edit {
            start: 7,
            old_len: 2,
            new_len: 0,
        };
        let res_del = transform_selection_range((5, 10), edit_del);
        assert_eq!(res_del, (5, 8));
    }

    #[test]
    fn test_document_buffer_mutations_and_undo() {
        let doc_id = DocumentId::new();
        let mut buf = DocumentBuffer::new(
            doc_id,
            PathBuf::from("test.md"),
            "test".to_string(),
            "Hello World".to_string(),
            1,
        );

        assert_eq!(buf.content(), "Hello World");
        assert!(!buf.is_dirty);

        // Apply edit
        let (ev, _, _) = buf.record_change(6, "World".to_string(), "Rust".to_string(), 6, 10, None);
        match ev {
            BufferEvent::Edited { edit, version, .. } => {
                assert_eq!(edit.start, 6);
                assert_eq!(edit.old_len, 5);
                assert_eq!(edit.new_len, 4);
                assert_eq!(version, 2);
            }
            _ => panic!("Expected Edited event"),
        }

        assert_eq!(buf.content(), "Hello Rust");
        assert!(buf.is_dirty);

        // Undo
        let (sel, _, _) = buf.undo_step(None).expect("Undo should succeed");
        assert_eq!(sel.cursor, 6);
        assert_eq!(buf.content(), "Hello World");

        // Redo
        let (sel_redo, _, _) = buf.redo_step(None).expect("Redo should succeed");
        assert_eq!(sel_redo.cursor, 10);
        assert_eq!(buf.content(), "Hello Rust");
    }

    #[test]
    fn test_document_buffer_insert_delete_replace_and_line_index() {
        let doc_id = DocumentId::new();
        let mut buf = DocumentBuffer::new(
            doc_id,
            PathBuf::from("doc.md"),
            "doc".to_string(),
            "line 1\nline 2\nline 3".to_string(),
            1,
        );
        assert_eq!(buf.line_index.lines(), 3);
        assert_eq!(buf.version, 1);
        assert!(!buf.is_dirty);

        // 1. Insertion with newline
        buf.record_change(7, String::new(), "inserted\n".to_string(), 7, 16, None);
        assert_eq!(buf.content(), "line 1\ninserted\nline 2\nline 3");
        assert_eq!(buf.line_index.lines(), 4);
        assert_eq!(buf.version, 2);
        assert!(buf.is_dirty);

        // 2. Deletion across lines
        // Delete "inserted\n" (9 bytes at index 7)
        buf.record_change(7, "inserted\n".to_string(), String::new(), 7, 7, None);
        assert_eq!(buf.content(), "line 1\nline 2\nline 3");
        assert_eq!(buf.line_index.lines(), 3);
        assert_eq!(buf.version, 3);

        // 3. Replacement
        // Replace "line 1" with "header 1"
        buf.record_change(0, "line 1".to_string(), "header 1".to_string(), 0, 8, None);
        assert_eq!(buf.content(), "header 1\nline 2\nline 3");
        assert_eq!(buf.line_index.lines(), 3);
        assert_eq!(buf.version, 4);
    }

    #[test]
    fn test_document_buffer_multi_step_undo_redo_and_redo_clearing() {
        let doc_id = DocumentId::new();
        let mut buf = DocumentBuffer::new(
            doc_id,
            PathBuf::from("undo.md"),
            "undo".to_string(),
            "A".to_string(),
            1,
        );

        // Edit 1: A -> AB
        buf.record_change(1, String::new(), "B".to_string(), 1, 2, None);
        // Edit 2: AB -> ABC
        buf.record_change(2, String::new(), "C".to_string(), 2, 3, None);
        assert_eq!(buf.content(), "ABC");
        assert_eq!(buf.history.undo_len(), 2);
        assert_eq!(buf.history.redo_len(), 0);

        // Undo edit 2 -> AB
        let (sel1, _, _) = buf.undo_step(None).unwrap();
        assert_eq!(buf.content(), "AB");
        assert_eq!(sel1.cursor, 2);
        assert_eq!(buf.history.redo_len(), 1);

        // Undo edit 1 -> A
        let (sel0, _, _) = buf.undo_step(None).unwrap();
        assert_eq!(buf.content(), "A");
        assert_eq!(sel0.cursor, 1);
        assert_eq!(buf.history.redo_len(), 2);

        // Redo edit 1 -> AB
        let (sel_r1, _, _) = buf.redo_step(None).unwrap();
        assert_eq!(buf.content(), "AB");
        assert_eq!(sel_r1.cursor, 2);
        assert_eq!(buf.history.redo_len(), 1);

        // New edit while redo stack has items: AB -> AB!
        buf.record_change(2, String::new(), "!".to_string(), 2, 3, None);
        assert_eq!(buf.content(), "AB!");
        // Redo stack must be cleared
        assert_eq!(buf.history.redo_len(), 0);
        assert!(buf.redo_step(None).is_none());
    }

    #[test]
    fn test_document_buffer_stats_cache_invalidation() {
        let doc_id = DocumentId::new();
        let mut buf = DocumentBuffer::new(
            doc_id,
            PathBuf::from("stats.md"),
            "stats".to_string(),
            "hello world".to_string(),
            1,
        );

        let s1 = buf.text_stats();
        assert_eq!(s1, crate::state::TextStats::Exact { words: 2, chars: 11 });

        // Stats cache is now populated
        assert!(buf.stats_cache.get().is_some());

        // Edit changes content and increments version
        buf.record_change(11, String::new(), " test".to_string(), 11, 16, None);
        assert_eq!(buf.version, 2);

        let s2 = buf.text_stats();
        assert_eq!(s2, crate::state::TextStats::Exact { words: 3, chars: 16 });
    }

    #[test]
    fn test_document_buffer_set_path() {
        let doc_id = DocumentId::new();
        let mut buf = DocumentBuffer::new(
            doc_id,
            PathBuf::from("old_name.md"),
            "old_name".to_string(),
            "text".to_string(),
            1,
        );

        let ev = buf.set_path(PathBuf::from("new_name.md"));
        match ev {
            BufferEvent::PathChanged { old, new } => {
                assert_eq!(old, PathBuf::from("old_name.md"));
                assert_eq!(new, PathBuf::from("new_name.md"));
            }
            _ => panic!("Expected PathChanged event"),
        }
        assert_eq!(buf.path, PathBuf::from("new_name.md"));
    }

    #[test]
    fn test_forward_deletes_are_separate_undo_steps() {
        let mut buf = DocumentBuffer::new(
            DocumentId::new(),
            PathBuf::from("delete.md"),
            "delete".to_string(),
            "abcdef".to_string(),
            1,
        );

        buf.record_change(2, "c".to_string(), String::new(), 3, 2, None);
        buf.record_change(2, "d".to_string(), String::new(), 2, 2, None);

        assert_eq!(buf.history.undo_len(), 2, "no coalescing: each delete is a step");
        assert_eq!(buf.content(), "abef");

        buf.undo_step(None).expect("undo must succeed");
        assert_eq!(buf.content(), "abdef");
        buf.undo_step(None).expect("undo must succeed");
        assert_eq!(buf.content(), "abcdef");
    }

    #[test]
    fn test_selection_delete_undo_restores_text() {
        let mut buf = DocumentBuffer::new(
            DocumentId::new(),
            PathBuf::from("selection.md"),
            "selection".to_string(),
            "one **two** three".to_string(),
            1,
        );

        let removed = "**two** ".to_string();
        let at = 4;
        buf.record_change(at, removed.clone(), String::new(), at + removed.len(), at, None);
        assert_eq!(buf.content(), "one three");

        buf.undo_step(None).expect("undo must succeed");
        assert_eq!(buf.content(), "one **two** three");
    }

    #[test]
    fn test_undo_redo_updates_table_index_incrementally() {
        let text = "# T\n\n| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n\ntail\n";
        let mut buf = DocumentBuffer::new(
            DocumentId::new(),
            PathBuf::from("table.md"),
            "table".to_string(),
            text.to_string(),
            1,
        );

        let start = buf.line_start_byte(4);
        let old_len = buf.line_start_byte(5) - start;
        let removed = buf.slice_to_string(start..start + old_len);
        buf.record_change(start, removed, String::new(), start, start, None);

        {
            let idx = buf.ensure_table_index();
            assert_eq!(
                (idx.blocks[0].first_line, idx.blocks[0].last_line),
                (2, 4)
            );
        }

        buf.undo_step(None).expect("undo must succeed");
        {
            let idx = buf.ensure_table_index();
            assert_eq!(
                (idx.blocks[0].first_line, idx.blocks[0].last_line),
                (2, 5)
            );
        }

        buf.redo_step(None).expect("redo must succeed");
        {
            let idx = buf.ensure_table_index();
            assert_eq!(
                (idx.blocks[0].first_line, idx.blocks[0].last_line),
                (2, 4)
            );
        }
    }

    #[test]
    fn test_from_file_large() {
        let temp_dir = std::env::temp_dir().join(format!("test_large_from_file_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let p = temp_dir.join("test.txt");
        let text = "Привет мир! Hello world!\n".repeat(500_000); // ~18.5 MB
        std::fs::write(&p, &text).unwrap();
        let doc = DocumentBuffer::from_file(
            DocumentId::new(),
            p.clone(),
            "test".into(),
            1,
        ).unwrap();
        println!("Expected len: {}, actual doc len: {}", text.len(), doc.len());
        assert_eq!(doc.len(), text.len());

        let mut state = crate::state::AppState::new();
        state.open_note(p.clone());
        let active_doc = state.active_document().unwrap();
        assert_eq!(active_doc.len(), text.len());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
