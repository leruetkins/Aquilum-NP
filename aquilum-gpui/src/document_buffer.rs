use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;
use uuid::Uuid;

use crate::line_index::LineIndex;
use crate::code_scan::CodeCheckpoints;
use crate::state::{TextChange, UndoStep};
use crate::workspace::TabId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(pub Uuid);

impl DocumentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
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

    pub fn stash_from_note(
        &mut self,
        content: String,
        line_index: LineIndex,
        code_checkpoints: CodeCheckpoints,
        undo_stack: Vec<UndoStep>,
        redo_stack: Vec<UndoStep>,
        is_dirty: bool,
        first_dirty_time: Option<Instant>,
    ) {
        self.content = content;
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

    pub fn take_for_active_note(
        &mut self,
    ) -> (String, LineIndex, CodeCheckpoints, Vec<UndoStep>, Vec<UndoStep>, Option<Instant>) {
        (
            std::mem::take(&mut self.content),
            std::mem::take(&mut self.line_index),
            std::mem::take(&mut self.code_checkpoints),
            std::mem::take(&mut self.undo_stack),
            std::mem::take(&mut self.redo_stack),
            self.first_dirty_time,
        )
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
}
