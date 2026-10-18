//! Идентификаторы, элементарные операции и OT-преобразование смещений.

use smallvec::SmallVec;

pub use crate::document_buffer::DocumentId;
pub use crate::workspace::tree::{LeafId, TabId};

/// Позиционная операция для трансформации чужих курсоров.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOp {
    Insert { at: usize, len: usize },
    Delete { at: usize, len: usize },
}

/// Вставка ровно в позиции курсора сдвигает его вперёд (Bias::Right).
/// Удаление текста вокруг курсора прижимает курсор к началу зоны удаления (at).
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

/// Трансформация диапазона выделения [offset, anchor] через TextOp.
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

/// Операции применяются последовательно: каждая позиция относится к тексту после предыдущей.
pub fn transform_through(off: usize, ops: &[TextOp]) -> usize {
    ops.iter().fold(off, |o, &op| transform_offset(o, op))
}

pub fn transform_selection_through(range: (usize, usize), ops: &[TextOp]) -> (usize, usize) {
    ops.iter().fold(range, |r, &op| transform_selection(r, op))
}

/// Конкретная правка с текстом (нужна для undo: у Delete хранится удалённое).
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
    /// Где должен оказаться курсор инициатора после правки.
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

/// Результат правки; рассылается всем сессиям документа.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditEvent {
    pub doc: DocumentId,
    /// Версия документа ПОСЛЕ правки.
    pub version: u64,
    pub ops: SmallVec<[TextOp; 2]>,
    /// Кто правил (undo/redo тоже). None — внешнее изменение (файл на диске).
    pub origin: Option<TabId>,
    /// Позиция курсора инициатора; остальные вкладки трансформируются.
    pub cursor_after: Option<usize>,
}
