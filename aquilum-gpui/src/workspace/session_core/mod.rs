//! Чистое ядро многооконности: документы, сессии вкладок, раскладка.
//! Без GPUI. Смещения — байтовые и должны лежать на границах char.

pub mod document;
pub mod layout;
pub mod ops;
pub mod session;
pub mod undo;
pub mod workspace;

pub use document::{DocumentBuffer, DocumentRegistry, EditCmd};
pub use layout::{Axis, LayoutNode, LeafState, Rect, WorkspaceLayout};
pub use ops::{transform_offset, transform_through, DocumentId, Edit, EditEvent, LeafId, TabId, TextOp};
pub use session::{CursorState, TabSession, TabSessionStore, TitleInput, ViewState};
pub use undo::{History, Selection, Step, TextChange, HISTORY_CAP};
pub use workspace::{CacheKey, CloseRequest, Effect, ViewCache, Workspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsError {
    UnknownTab,
    UnknownDoc,
    UnknownLeaf,
    BadRange,
    EmptyTitle,
    /// Файл с таким именем уже есть (коллизия при переименовании).
    NameTaken,
    /// finish_close вызван, пока не выполнены эффекты из request_close.
    PendingEffects,
}

#[cfg(test)]
mod tests;
