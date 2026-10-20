//! Реестр документов: единственный владелец текста, undo/redo, пути и заголовка.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use smallvec::smallvec;

use super::ops::{DocumentId, Edit, EditEvent, TabId, TextOp};
use super::undo::{History, Selection, Step, TextChange};
use super::WsError;

#[derive(Clone, Debug)]
pub enum EditCmd {
    Insert { at: usize, text: String },
    Delete { at: usize, len: usize },
}

pub struct DocumentBuffer {
    pub id: DocumentId,
    pub path: PathBuf,
    pub title: String,
    content: String,
    pub version: u64,
    pub is_dirty: bool,
    pub history: History,
}

impl DocumentBuffer {
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// Ключ пути: разделители и регистр нормализуются (Windows).
/// Для регистрозависимых ФС это сознательно консервативно.
pub fn path_key(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

#[derive(Default)]
pub struct DocumentRegistry {
    docs: HashMap<DocumentId, DocumentBuffer>,
    by_path: HashMap<String, DocumentId>,
    next: u64,
}

impl DocumentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }
    pub fn get(&self, id: DocumentId) -> Option<&DocumentBuffer> {
        self.docs.get(&id)
    }
    pub fn id_by_path(&self, path: &Path) -> Option<DocumentId> {
        self.by_path.get(&path_key(path)).copied()
    }

    /// Один файл = один документ: повторное открытие возвращает тот же id
    /// (переданный content игнорируется).
    pub fn open(&mut self, path: &Path, content: &str) -> DocumentId {
        let key = path_key(path);
        if let Some(&id) = self.by_path.get(&key) {
            return id;
        }
        let id = DocumentId::from_u64(self.next);
        self.next += 1;
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.docs.insert(
            id,
            DocumentBuffer {
                id,
                path: path.to_path_buf(),
                title,
                content: content.to_string(),
                version: 0,
                is_dirty: false,
                history: History::new(),
            },
        );
        self.by_path.insert(key, id);
        id
    }

    pub fn release(&mut self, id: DocumentId) {
        if let Some(d) = self.docs.remove(&id) {
            self.by_path.remove(&path_key(&d.path));
        }
    }

    pub fn rename(&mut self, id: DocumentId, new_title: &str) -> Result<(), WsError> {
        let new_title = new_title.trim();
        if new_title.is_empty() {
            return Err(WsError::EmptyTitle);
        }
        let d = self.docs.get(&id).ok_or(WsError::UnknownDoc)?;
        let ext = d.path.extension().and_then(|e| e.to_str()).unwrap_or("md");
        let base = new_title.strip_suffix(&format!(".{}", ext)).unwrap_or(new_title);
        if d.title == base {
            return Ok(());
        }
        let file = format!("{}.{}", base, ext);
        let new_path = d.path.with_file_name(file);
        let (old_key, new_key) = (path_key(&d.path), path_key(&new_path));
        if new_key != old_key && self.by_path.contains_key(&new_key) {
            return Err(WsError::NameTaken);
        }
        self.by_path.remove(&old_key);
        self.by_path.insert(new_key, id);
        let d = self.docs.get_mut(&id).unwrap();
        d.path = new_path;
        d.title = base.to_string();
        Ok(())
    }

    /// Единственный путь изменения текста (в т.ч. внешние изменения: origin = None).
    pub fn apply_edit(
        &mut self,
        id: DocumentId,
        cmd: EditCmd,
        origin: Option<TabId>,
    ) -> Result<EditEvent, WsError> {
        let d = self.docs.get_mut(&id).ok_or(WsError::UnknownDoc)?;
        let edit = match cmd {
            EditCmd::Insert { at, text } => {
                if at > d.content.len() || !d.content.is_char_boundary(at) {
                    return Err(WsError::BadRange);
                }
                Edit::Insert { at, text }
            }
            EditCmd::Delete { at, len } => {
                let end = at.checked_add(len).ok_or(WsError::BadRange)?;
                if end > d.content.len()
                    || !d.content.is_char_boundary(at)
                    || !d.content.is_char_boundary(end)
                {
                    return Err(WsError::BadRange);
                }
                Edit::Delete { at, text: d.content[at..end].to_string() }
            }
        };
        let ev = Self::run(d, &edit, origin);
        let (at, text, inserted) = match &edit {
            Edit::Insert { at, text } => (*at, text.clone(), true),
            Edit::Delete { at, text } => (*at, text.clone(), false),
        };
        // Undo puts the caret at the start of the previously inserted text
        // (or at the end of the reinserted deletion); redo mirrors Edit::cursor_after.
        let (sel_before, sel_after) = if inserted {
            (Selection::cursor_only(at), Selection::cursor_only(at + text.len()))
        } else {
            (Selection::cursor_only(at + text.len()), Selection::cursor_only(at))
        };
        d.history.record(Step {
            changes: vec![TextChange { at, removed: if inserted { String::new() } else { text.clone() }, inserted: if inserted { text } else { String::new() } }],
            sel_before,
            sel_after,
        });
        Ok(ev)
    }

    /// Undo общий на документ: откатывает последнюю правку независимо от вкладки.
    pub fn undo(&mut self, id: DocumentId, origin: TabId) -> Option<EditEvent> {
        let d = self.docs.get_mut(&id)?;
        let step = d.history.take(false)?;
        Some(Self::apply_step(d, &step, Some(origin), false))
    }

    pub fn redo(&mut self, id: DocumentId, origin: TabId) -> Option<EditEvent> {
        let d = self.docs.get_mut(&id)?;
        let step = d.history.take(true)?;
        Some(Self::apply_step(d, &step, Some(origin), true))
    }

    /// Applies an edit to content and emits its event; no history involvement.
    fn run(d: &mut DocumentBuffer, edit: &Edit, origin: Option<TabId>) -> EditEvent {
        edit.apply(&mut d.content);
        d.version += 1;
        d.is_dirty = true;
        EditEvent {
            doc: d.id,
            version: d.version,
            ops: smallvec![edit.op()],
            origin,
            cursor_after: Some(edit.cursor_after()),
        }
    }

    fn apply_step(d: &mut DocumentBuffer, step: &Step, origin: Option<TabId>, redo: bool) -> EditEvent {
        let mut ops = smallvec![];
        // Undo applies changes right-to-left so stored positions stay valid.
        let ordered: Vec<&TextChange> = if redo {
            step.changes.iter().collect()
        } else {
            step.changes.iter().rev().collect()
        };
        for change in &ordered {
            let (removed, inserted) = if redo {
                (change.removed.as_str(), change.inserted.as_str())
            } else {
                (change.inserted.as_str(), change.removed.as_str())
            };
            d.content.replace_range(change.at..change.at + removed.len(), inserted);
            if !removed.is_empty() {
                ops.push(TextOp::Delete { at: change.at, len: removed.len() });
            }
            if !inserted.is_empty() {
                ops.push(TextOp::Insert { at: change.at, len: inserted.len() });
            }
        }
        let cursor = if redo { step.sel_after.cursor } else { step.sel_before.cursor };
        d.version += 1;
        d.is_dirty = true;
        EditEvent {
            doc: d.id,
            version: d.version,
            ops,
            origin,
            cursor_after: Some(cursor),
        }
    }

    pub fn mark_flushed(&mut self, id: DocumentId) {
        if let Some(d) = self.docs.get_mut(&id) {
            d.is_dirty = false;
        }
    }
}
