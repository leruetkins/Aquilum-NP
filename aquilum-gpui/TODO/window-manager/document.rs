//! Document registry: the single owner of text, undo/redo, path and title.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ops::{DocumentId, Edit, EditEvent, TabId};
use crate::WsError;

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
    pub undo_stack: Vec<Edit>,
    pub redo_stack: Vec<Edit>,
}

impl DocumentBuffer {
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// Path key: separators and case are normalized (Windows).
/// For case-sensitive filesystems this is deliberately conservative.
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

    /// One file = one document: reopening returns the same id
    /// (the passed content is ignored).
    pub fn open(&mut self, path: &Path, content: &str) -> DocumentId {
        let key = path_key(path);
        if let Some(&id) = self.by_path.get(&key) {
            return id;
        }
        let id = DocumentId(self.next);
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
                undo_stack: vec![],
                redo_stack: vec![],
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

    /// The only path for changing text (including external changes: origin = None).
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
        d.undo_stack.push(edit);
        d.redo_stack.clear();
        Ok(ev)
    }

    /// Undo is per document: it reverts the last edit regardless of the tab.
    pub fn undo(&mut self, id: DocumentId, origin: TabId) -> Option<EditEvent> {
        let d = self.docs.get_mut(&id)?;
        let edit = d.undo_stack.pop()?;
        let ev = Self::run(d, &edit.inverse(), Some(origin));
        d.redo_stack.push(edit);
        Some(ev)
    }

    pub fn redo(&mut self, id: DocumentId, origin: TabId) -> Option<EditEvent> {
        let d = self.docs.get_mut(&id)?;
        let edit = d.redo_stack.pop()?;
        let ev = Self::run(d, &edit, Some(origin));
        d.undo_stack.push(edit);
        Some(ev)
    }

    fn run(d: &mut DocumentBuffer, edit: &Edit, origin: Option<TabId>) -> EditEvent {
        edit.apply(&mut d.content);
        d.version += 1;
        d.is_dirty = true;
        EditEvent {
            doc: d.id,
            version: d.version,
            ops: vec![edit.op()],
            origin,
            cursor_after: Some(edit.cursor_after()),
        }
    }

    pub fn mark_flushed(&mut self, id: DocumentId) {
        if let Some(d) = self.docs.get_mut(&id) {
            d.is_dirty = false;
        }
    }
}
