//! Identifiers, elementary operations, and OT transformation of offsets.

use smallvec::SmallVec;

pub use crate::document_buffer::DocumentId;
pub use crate::workspace::tree::{LeafId, TabId};

/// Positional operation for transforming other cursors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOp {
    Insert { at: usize, len: usize },
    Delete { at: usize, len: usize },
}

/// Inserting exactly at the cursor position shifts it forward (Bias::Right).
/// Deleting text around the cursor snaps the cursor to the start of the deletion zone (at).
pub fn transform_offset(off: usize, op: TextOp) -> usize {
    match op {
        TextOp::Insert { at, len } => {
            if at <= off {
                off + len
            } else {
                off
            }
        }
        TextOp::Delete { at, len } => {
            if off <= at {
                off
            } else if off >= at + len {
                off - len
            } else {
                at
            }
        }
    }
}

/// Transform the selection range [offset, anchor] through a TextOp.
pub fn transform_selection(range: (usize, usize), op: TextOp) -> (usize, usize) {
    let (head, anchor) = range;
    let (start, end) = if head <= anchor { (head, anchor) } else { (anchor, head) };
    let new_start = match op {
        TextOp::Insert { at, len } => if at < start { start + len } else { start },
        TextOp::Delete { at, len } => {
            if start <= at { start } else if start >= at + len { start - len } else { at }
        }
    };
    let new_end = transform_offset(end, op);
    if head <= anchor {
        (new_start, new_end)
    } else {
        (new_end, new_start)
    }
}

/// Operations are applied sequentially: each position refers to the text after the previous one.
pub fn transform_through(off: usize, ops: &[TextOp]) -> usize {
    ops.iter().fold(off, |o, &op| transform_offset(o, op))
}

pub fn transform_selection_through(range: (usize, usize), ops: &[TextOp]) -> (usize, usize) {
    ops.iter().fold(range, |r, &op| transform_selection(r, op))
}

/// Concrete edit with text (needed for undo: Delete stores the deleted text).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Insert { at: usize, text: String },
    Delete { at: usize, text: String },
}

impl Edit {
    pub fn op(&self) -> TextOp {
        match self {
            Edit::Insert { at, text } => TextOp::Insert { at: *at, len: text.len() },
            Edit::Delete { at, text } => TextOp::Delete { at: *at, len: text.len() },
        }
    }
    pub fn inverse(&self) -> Edit {
        match self {
            Edit::Insert { at, text } => Edit::Delete { at: *at, text: text.clone() },
            Edit::Delete { at, text } => Edit::Insert { at: *at, text: text.clone() },
        }
    }
    /// Where the initiator's cursor should end up after the edit.
    pub fn cursor_after(&self) -> usize {
        match self {
            Edit::Insert { at, text } => at + text.len(),
            Edit::Delete { at, .. } => *at,
        }
    }
    pub fn apply(&self, content: &mut String) {
        match self {
            Edit::Insert { at, text } => content.insert_str(*at, text),
            Edit::Delete { at, text } => content.replace_range(*at..*at + text.len(), ""),
        }
    }
}

/// Edit result; broadcast to all sessions of the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditEvent {
    pub doc: DocumentId,
    /// Document version AFTER the edit.
    pub version: u64,
    pub ops: SmallVec<[TextOp; 2]>,
    /// Who made the edit (undo/redo too). None — external change (file on disk).
    pub origin: Option<TabId>,
    /// Initiator's cursor position; other tabs are transformed.
    pub cursor_after: Option<usize>,
}
