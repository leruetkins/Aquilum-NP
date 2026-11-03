//! Identifiers, primitive operations, and OT transformation of offsets.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LeafId(pub u64);

/// Positional operation for transforming foreign cursors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOp {
    Insert { at: usize, len: usize },
    Delete { at: usize, len: usize },
}

/// STUB: replace this in the project with calls to the existing
/// `transform_offset` / `transform_selection_range`.
/// An insert exactly at the cursor position moves it forward.
pub fn transform_offset(off: usize, op: TextOp) -> usize {
    match op {
        TextOp::Insert { at, len } => if at <= off { off + len } else { off },
        TextOp::Delete { at, len } => {
            if off <= at { off } else if off >= at + len { off - len } else { at }
        }
    }
}

/// Operations are applied sequentially: each position refers
/// to the text after the previous one.
pub fn transform_through(off: usize, ops: &[TextOp]) -> usize {
    ops.iter().fold(off, |o, &op| transform_offset(o, op))
}

/// Concrete edit with text (needed for undo: Delete stores the removed text).
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

/// Result of an edit; broadcast to all sessions of the document.
/// (In the project, `ops` can be replaced with SmallVec<[TextOp; 2]>.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditEvent {
    pub doc: DocumentId,
    /// Document version AFTER the edit.
    pub version: u64,
    pub ops: Vec<TextOp>,
    /// Who made the edit (undo/redo too). None — external change (file on disk).
    pub origin: Option<TabId>,
    /// Initiator's cursor position; other tabs are transformed.
    pub cursor_after: Option<usize>,
}
