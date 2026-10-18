use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::path::PathBuf;
use std::time::Instant;
use uuid::Uuid;

use crate::code_scan::CodeCheckpoints;
use crate::line_index::LineIndex;
use crate::state::{NoteEditEvent, TextChange, UndoStep};
use crate::workspace::session_core::ops::{EditEvent, TextOp};
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
    content: String,
    pub version: u64,
    pub is_dirty: bool,
    pub first_dirty_time: Option<Instant>,
    pub line_index: LineIndex,
    pub code_checkpoints: CodeCheckpoints,
    pub undo_stack: Vec<UndoStep>,
    pub redo_stack: Vec<UndoStep>,
    pub stats_cache: std::cell::Cell<Option<(u64, crate::state::TextStats)>>,
}

impl DocumentBuffer {
    pub fn new(id: DocumentId, path: PathBuf, title: String, content: String, version: u64) -> Self {
        let line_index = LineIndex::new(&content);
        Self {
            id,
            path,
            title,
            content,
            version,
            is_dirty: false,
            first_dirty_time: None,
            line_index,
            code_checkpoints: CodeCheckpoints::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            stats_cache: std::cell::Cell::new(None),
        }
    }

    #[inline]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[inline]
    pub fn take_content(&mut self) -> String {
        std::mem::take(&mut self.content)
    }

    pub fn update_from_active_note(
        &mut self,
        content: String,
        version: u64,
        line_index: LineIndex,
        code_checkpoints: CodeCheckpoints,
        undo_stack: Vec<UndoStep>,
        redo_stack: Vec<UndoStep>,
        is_dirty: bool,
        first_dirty_time: Option<Instant>,
    ) {
        self.content = content;
        self.version = version;
        self.line_index = line_index;
        self.code_checkpoints = code_checkpoints;
        self.undo_stack = undo_stack;
        self.redo_stack = redo_stack;
        self.is_dirty = is_dirty;
        self.first_dirty_time = first_dirty_time;
        self.stats_cache.set(None);
    }

    pub fn text_stats(&self) -> crate::state::TextStats {
        if let Some((v, s)) = self.stats_cache.get() {
            if v == self.version {
                return s;
            }
        }
        let s = crate::state::compute_text_stats(&self.content);
        self.stats_cache.set(Some((self.version, s)));
        s
    }

    pub fn ensure_checkpoints(&mut self, up_to_line: usize) {
        self.code_checkpoints.ensure(&self.content, &self.line_index, up_to_line);
    }

    pub fn checkpoint_state_at(&self, line: usize) -> Option<crate::markdown::CodeScanState> {
        self.code_checkpoints.state_at(&self.content, &self.line_index, line)
    }

    pub fn scan_code_ahead(&mut self, max_lines: usize) -> usize {
        if self.code_checkpoints.is_complete(self.line_index.lines()) {
            return 0;
        }
        self.code_checkpoints
            .extend_slice(&self.content, &self.line_index, max_lines)
    }

    #[inline]
    pub fn line_start_byte(&self, line: usize) -> usize {
        self.line_index.start(&self.content, line)
    }

    #[inline]
    pub fn line_of_byte(&self, byte: usize) -> usize {
        self.line_index.line_of(&self.content, byte)
    }

    pub fn word_range_at(&self, offset: usize) -> (usize, usize) {
        if self.content.is_empty() {
            return (0, 0);
        }
        let mut offset = offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        let chars: Vec<(usize, char)> = self.content.char_indices().collect();
        if chars.is_empty() {
            return (0, 0);
        }
        let char_idx = chars.iter().position(|(idx, _)| *idx >= offset).unwrap_or(chars.len() - 1);
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
        let start_byte = chars[start_idx].0;
        let end_byte = if end_idx < chars.len() {
            chars[end_idx].0
        } else {
            self.content.len()
        };
        (start_byte, end_byte)
    }

    pub fn line_range_at(&self, offset: usize) -> (usize, usize) {
        let mut offset = offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        let start = self.content[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let end = self.content[offset..].find('\n').map(|i| offset + i + 1).unwrap_or(self.content.len());
        (start, end)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.content.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
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
        self.content = new_content;
        self.version += 1;
        self.is_dirty = false;
        self.first_dirty_time = None;
        self.line_index = LineIndex::new(&self.content);
        self.code_checkpoints = CodeCheckpoints::new();
        self.undo_stack.clear();
        self.redo_stack.clear();
        BufferEvent::Reloaded
    }

    /// Single point of truth for mutating document content.
    /// Emits `BufferEvent::Edited` with exact deltas for views.
    pub fn apply_edit(
        &mut self,
        start: usize,
        old_len: usize,
        new_text: &str,
        source: Option<TabId>,
        cursor_before: usize,
        cursor_after: usize,
    ) -> BufferEvent {
        assert!(start <= self.content.len(), "start offset exceeds content len");
        assert!(start + old_len <= self.content.len(), "replaced range exceeds content len");

        let removed = self.content[start..start + old_len].to_string();
        self.content.replace_range(start..start + old_len, new_text);

        let edit = Edit {
            start,
            old_len,
            new_len: new_text.len(),
        };

        self.version += 1;
        self.line_index = LineIndex::new(&self.content);

        // Record undo step
        let undo_step = UndoStep {
            changes: vec![TextChange {
                at: start,
                removed,
                inserted: new_text.to_string(),
            }],
            cursor_before,
            cursor_after,
            time: Instant::now(),
        };
        self.undo_stack.push(undo_step);
        self.redo_stack.clear();

        self.is_dirty = true;
        if self.first_dirty_time.is_none() {
            self.first_dirty_time = Some(Instant::now());
        }

        BufferEvent::Edited {
            edit,
            source,
            version: self.version,
        }
    }

    pub fn undo(&mut self, source: Option<TabId>) -> Option<(BufferEvent, usize)> {
        let step = self.undo_stack.pop()?;
        let mut composite_edit = None;

        for change in step.changes.iter().rev() {
            let start = change.at;
            let old_len = change.inserted.len();
            let new_len = change.removed.len();

            self.content.replace_range(start..start + old_len, &change.removed);
            composite_edit = Some(Edit {
                start,
                old_len,
                new_len,
            });
        }

        self.version += 1;
        self.line_index = LineIndex::new(&self.content);
        self.is_dirty = true;

        let edit = composite_edit?;
        self.redo_stack.push(step.clone());

        Some((
            BufferEvent::Edited {
                edit,
                source,
                version: self.version,
            },
            step.cursor_before,
        ))
    }

    pub fn redo(&mut self, source: Option<TabId>) -> Option<(BufferEvent, usize)> {
        let step = self.redo_stack.pop()?;
        let mut composite_edit = None;

        for change in step.changes.iter() {
            let start = change.at;
            let old_len = change.removed.len();
            let new_len = change.inserted.len();

            self.content.replace_range(start..start + old_len, &change.inserted);
            composite_edit = Some(Edit {
                start,
                old_len,
                new_len,
            });
        }

        self.version += 1;
        self.line_index = LineIndex::new(&self.content);
        self.is_dirty = true;

        let edit = composite_edit?;
        let cursor_after = step.cursor_after;
        self.undo_stack.push(step);

        Some((
            BufferEvent::Edited {
                edit,
                source,
                version: self.version,
            },
            cursor_after,
        ))
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
        if removed.is_empty() && inserted.is_empty() {
            let edit_line = self.line_index.line_of(&self.content, at);
            return (
                BufferEvent::Edited {
                    edit: Edit { start: at, old_len: 0, new_len: 0 },
                    source: origin,
                    version: self.version,
                },
                NoteEditEvent { edit_line, at, old_len: 0, new_len: 0, line_diff: 0 },
                EditEvent { doc: self.id, version: self.version, ops: SmallVec::new(), origin, cursor_after: Some(cursor_after) },
            );
        }

        let edit_line = self.line_index.line_of(&self.content, at);
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

        self.content.replace_range(at..at + removed.len(), &inserted);
        self.version += 1;
        self.line_index.splice(&self.content, at, &removed, &inserted);
        self.code_checkpoints.truncate_to(edit_line);
        self.is_dirty = true;
        if self.first_dirty_time.is_none() {
            self.first_dirty_time = Some(Instant::now());
        }
        self.stats_cache.set(None);

        let now = Instant::now();
        let change = TextChange { at, removed, inserted };
        let mut coalesced = false;
        if let Some(last_step) = self.undo_stack.last_mut() {
            if now.duration_since(last_step.time) < std::time::Duration::from_millis(1000)
                && last_step.changes.len() == 1
            {
                let last = &mut last_step.changes[0];
                if last.removed.is_empty()
                    && change.removed.is_empty()
                    && !change.inserted.contains('\n')
                    && last.at + last.inserted.len() == change.at
                {
                    last.inserted.push_str(&change.inserted);
                    last_step.cursor_after = cursor_after;
                    last_step.time = now;
                    coalesced = true;
                } else if last.inserted.is_empty()
                    && change.inserted.is_empty()
                    && !change.removed.contains('\n')
                    && change.at + change.removed.len() == last.at
                {
                    last.removed.insert_str(0, &change.removed);
                    last.at = change.at;
                    last_step.cursor_after = cursor_after;
                    last_step.time = now;
                    coalesced = true;
                }
            }
        }

        if !coalesced {
            if self.undo_stack.len() >= 1000 {
                self.undo_stack.remove(0);
            }
            self.undo_stack.push(UndoStep {
                changes: vec![change],
                cursor_before,
                cursor_after,
                time: now,
            });
        }
        self.redo_stack.clear();

        let edit_event = EditEvent {
            doc: self.id,
            version: self.version,
            ops,
            origin,
            cursor_after: Some(cursor_after),
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
    ) -> Option<(usize, Vec<NoteEditEvent>, EditEvent)> {
        let step = self.undo_stack.pop()?;
        let mut note_edits = Vec::new();
        let mut ops = SmallVec::new();

        for change in step.changes.iter().rev() {
            let at = change.at;
            let old_len = change.inserted.len();
            let new_len = change.removed.len();

            self.content.replace_range(at..at + old_len, &change.removed);
            self.line_index.splice(&self.content, at, &change.inserted, &change.removed);
            let edit_line = self.line_index.line_of(&self.content, at);
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
        }

        self.version += 1;
        self.is_dirty = true;
        self.stats_cache.set(None);
        self.redo_stack.push(step.clone());

        let edit_event = EditEvent {
            doc: self.id,
            version: self.version,
            ops,
            origin,
            cursor_after: Some(step.cursor_before),
        };

        Some((step.cursor_before, note_edits, edit_event))
    }

    pub fn redo_step(
        &mut self,
        origin: Option<TabId>,
    ) -> Option<(usize, Vec<NoteEditEvent>, EditEvent)> {
        let step = self.redo_stack.pop()?;
        let mut note_edits = Vec::new();
        let mut ops = SmallVec::new();

        for change in step.changes.iter() {
            let at = change.at;
            let old_len = change.removed.len();
            let new_len = change.inserted.len();

            self.content.replace_range(at..at + old_len, &change.inserted);
            self.line_index.splice(&self.content, at, &change.removed, &change.inserted);
            let edit_line = self.line_index.line_of(&self.content, at);
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
        }

        self.version += 1;
        self.is_dirty = true;
        self.stats_cache.set(None);
        self.undo_stack.push(step.clone());

        let edit_event = EditEvent {
            doc: self.id,
            version: self.version,
            ops,
            origin,
            cursor_after: Some(step.cursor_after),
        };

        Some((step.cursor_after, note_edits, edit_event))
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
        let ev = buf.apply_edit(6, 5, "Rust", None, 6, 10);
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
        let (undo_ev, cursor) = buf.undo(None).expect("Undo should succeed");
        assert_eq!(cursor, 6);
        match undo_ev {
            BufferEvent::Edited { edit, .. } => {
                assert_eq!(edit.start, 6);
                assert_eq!(edit.old_len, 4);
                assert_eq!(edit.new_len, 5);
            }
            _ => panic!("Expected Edited event on undo"),
        }
        assert_eq!(buf.content(), "Hello World");

        // Redo
        let (_redo_ev, cursor_redo) = buf.redo(None).expect("Redo should succeed");
        assert_eq!(cursor_redo, 10);
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
        buf.apply_edit(7, 0, "inserted\n", None, 7, 16);
        assert_eq!(buf.content(), "line 1\ninserted\nline 2\nline 3");
        assert_eq!(buf.line_index.lines(), 4);
        assert_eq!(buf.version, 2);
        assert!(buf.is_dirty);

        // 2. Deletion across lines
        // Delete "inserted\n" (9 bytes at index 7)
        buf.apply_edit(7, 9, "", None, 7, 7);
        assert_eq!(buf.content(), "line 1\nline 2\nline 3");
        assert_eq!(buf.line_index.lines(), 3);
        assert_eq!(buf.version, 3);

        // 3. Replacement
        // Replace "line 1" with "header 1"
        buf.apply_edit(0, 6, "header 1", None, 0, 8);
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
        buf.apply_edit(1, 0, "B", None, 1, 2);
        // Edit 2: AB -> ABC
        buf.apply_edit(2, 0, "C", None, 2, 3);
        assert_eq!(buf.content(), "ABC");
        assert_eq!(buf.undo_stack.len(), 2);
        assert_eq!(buf.redo_stack.len(), 0);

        // Undo edit 2 -> AB
        let (_, c1) = buf.undo(None).unwrap();
        assert_eq!(buf.content(), "AB");
        assert_eq!(c1, 2);
        assert_eq!(buf.redo_stack.len(), 1);

        // Undo edit 1 -> A
        let (_, c0) = buf.undo(None).unwrap();
        assert_eq!(buf.content(), "A");
        assert_eq!(c0, 1);
        assert_eq!(buf.redo_stack.len(), 2);

        // Redo edit 1 -> AB
        let (_, c_r1) = buf.redo(None).unwrap();
        assert_eq!(buf.content(), "AB");
        assert_eq!(c_r1, 2);
        assert_eq!(buf.redo_stack.len(), 1);

        // New edit while redo stack has items: AB -> AB!
        buf.apply_edit(2, 0, "!", None, 2, 3);
        assert_eq!(buf.content(), "AB!");
        // Redo stack must be cleared
        assert_eq!(buf.redo_stack.len(), 0);
        assert!(buf.redo(None).is_none());
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
        buf.apply_edit(11, 0, " test", None, 11, 16);
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
}
