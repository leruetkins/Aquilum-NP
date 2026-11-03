//! A pure multi-window core: documents, tab sessions, layout.
//! No GPUI. Offsets are byte-based and must lie on char boundaries.

pub mod document;
pub mod layout;
pub mod ops;
pub mod session;
pub mod workspace;

pub use document::{DocumentBuffer, DocumentRegistry, EditCmd};
pub use layout::{Axis, LayoutNode, LeafState, Rect, WorkspaceLayout};
pub use ops::{transform_offset, transform_through, DocumentId, Edit, EditEvent, LeafId, TabId, TextOp};
pub use session::{CursorState, TabSession, TabSessionStore, TitleInput, ViewState};
pub use workspace::{CacheKey, CloseRequest, Effect, ViewCache, Workspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsError {
    UnknownTab,
    UnknownDoc,
    UnknownLeaf,
    BadRange,
    EmptyTitle,
    /// A file with this name already exists (collision on rename).
    NameTaken,
    /// finish_close was called while effects from request_close are not yet executed.
    PendingEffects,
}

#[cfg(test)]
mod tests;
