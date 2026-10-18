//! Идентификаторы, элементарные операции и OT-преобразование смещений.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LeafId(pub u64);

/// Позиционная операция для трансформации чужих курсоров.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOp {
    Insert { at: usize, len: usize },
    Delete { at: usize, len: usize },
}

/// ЗАГЛУШКА: в проекте замените вызовом существующих
/// `transform_offset` / `transform_selection_range`.
/// Вставка ровно в позиции курсора сдвигает его вперёд.
pub fn transform_offset(off: usize, op: TextOp) -> usize {
    match op {
        TextOp::Insert { at, len } => if at <= off { off + len } else { off },
        TextOp::Delete { at, len } => {
            if off <= at { off } else if off >= at + len { off - len } else { at }
        }
    }
}

/// Операции применяются последовательно: каждая позиция относится
/// к тексту после предыдущей.
pub fn transform_through(off: usize, ops: &[TextOp]) -> usize {
    ops.iter().fold(off, |o, &op| transform_offset(o, op))
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
/// (В проекте `ops` можно заменить на SmallVec<[TextOp; 2]>.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditEvent {
    pub doc: DocumentId,
    /// Версия документа ПОСЛЕ правки.
    pub version: u64,
    pub ops: Vec<TextOp>,
    /// Кто правил (undo/redo тоже). None — внешнее изменение (файл на диске).
    pub origin: Option<TabId>,
    /// Позиция курсора инициатора; остальные вкладки трансформируются.
    pub cursor_after: Option<usize>,
}
