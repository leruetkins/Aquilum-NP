//! Фасад: координирует реестр, сессии, кэши и раскладку. Чистая логика; IO — через Effect.

use std::collections::HashMap;
use std::path::Path;

use crate::document::{DocumentRegistry, EditCmd};
use crate::layout::{Rect, WorkspaceLayout};
use crate::ops::{DocumentId, EditEvent, LeafId, TabId};
use crate::session::{TabSession, TabSessionStore};
use crate::WsError;

/// Производный кэш: не состояние, перестраивается при смене ключа.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheKey {
    pub doc_version: u64,
    pub viewport_h_bits: u32,
    pub scroll_top_bits: u64,
}

#[derive(Clone, Debug)]
pub struct ViewCache {
    pub key: CacheKey,
    pub rebuilds: u32,
    // сюда: window: EditorWindow, above/below_lines, above/below_h, est_*
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    CommitTitle { tab: TabId, doc: DocumentId, new_title: String },
    FlushDoc(DocumentId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseRequest {
    Ready,
    NeedsEffects(Vec<Effect>),
}

pub struct Workspace {
    pub docs: DocumentRegistry,
    pub tabs: TabSessionStore,
    pub layout: WorkspaceLayout,
    caches: HashMap<TabId, ViewCache>,
    next_tab: u64,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            docs: DocumentRegistry::new(),
            tabs: TabSessionStore::new(),
            layout: WorkspaceLayout::new(),
            caches: HashMap::new(),
            next_tab: 0,
        }
    }

    pub fn open_in_leaf(&mut self, leaf: LeafId, path: &str, content: &str) -> Result<TabId, WsError> {
        self.layout.leaf(leaf).ok_or(WsError::UnknownLeaf)?;
        let doc = self.docs.open(Path::new(path), content);
        let id = TabId(self.next_tab);
        self.next_tab += 1;
        self.tabs.insert(TabSession::new(id, doc));
        let l = self.layout.leaf_mut(leaf).unwrap();
        l.tabs.push(id);
        l.active = Some(id);
        self.layout.active_leaf = leaf;
        Ok(id)
    }

    pub fn select_tab(&mut self, leaf: LeafId, tab: TabId) -> Result<(), WsError> {
        let l = self.layout.leaf_mut(leaf).ok_or(WsError::UnknownLeaf)?;
        if !l.tabs.contains(&tab) {
            return Err(WsError::UnknownTab);
        }
        l.active = Some(tab);
        self.layout.active_leaf = leaf;
        Ok(())
    }

    pub fn title_of(&self, tab: TabId) -> Option<&str> {
        let doc = self.tabs.get(tab)?.doc_id;
        self.docs.get(doc).map(|d| d.title.as_str())
    }

    pub fn edit(&mut self, tab: TabId, cmd: EditCmd) -> Result<EditEvent, WsError> {
        let doc = self.tabs.get(tab).ok_or(WsError::UnknownTab)?.doc_id;
        let ev = self.docs.apply_edit(doc, cmd, Some(tab))?;
        self.tabs.apply_edit_event(&ev);
        Ok(ev)
    }

    pub fn undo(&mut self, tab: TabId) -> Result<Option<EditEvent>, WsError> {
        let doc = self.tabs.get(tab).ok_or(WsError::UnknownTab)?.doc_id;
        let ev = self.docs.undo(doc, tab);
        if let Some(ev) = &ev {
            self.tabs.apply_edit_event(ev);
        }
        Ok(ev)
    }

    pub fn redo(&mut self, tab: TabId) -> Result<Option<EditEvent>, WsError> {
        let doc = self.tabs.get(tab).ok_or(WsError::UnknownTab)?.doc_id;
        let ev = self.docs.redo(doc, tab);
        if let Some(ev) = &ev {
            self.tabs.apply_edit_event(ev);
        }
        Ok(ev)
    }

    pub fn split_leaf(&mut self, leaf: LeafId, axis: crate::layout::Axis) -> Result<LeafId, WsError> {
        self.layout.split_leaf(leaf, axis).ok_or(WsError::UnknownLeaf)
    }

    pub fn set_leaf_bounds(&mut self, leaf: LeafId, bounds: Rect) -> Result<(), WsError> {
        self.layout.leaf_mut(leaf).ok_or(WsError::UnknownLeaf)?.bounds = bounds;
        Ok(())
    }

    /// true, если кэш вкладки был перестроен.
    pub fn refresh_cache(&mut self, tab: TabId) -> Result<bool, WsError> {
        let s = self.tabs.get(tab).ok_or(WsError::UnknownTab)?;
        let leaf = self.layout.leaf_of(tab).ok_or(WsError::UnknownLeaf)?;
        let h = self.layout.leaf(leaf).unwrap().bounds.h;
        let ver = self.docs.get(s.doc_id).ok_or(WsError::UnknownDoc)?.version;
        let key = CacheKey {
            doc_version: ver,
            viewport_h_bits: h.to_bits(),
            scroll_top_bits: s.view.scroll_top.to_bits(),
        };
        match self.caches.get_mut(&tab) {
            Some(c) if c.key == key => Ok(false),
            Some(c) => {
                c.key = key;
                c.rebuilds += 1;
                Ok(true)
            }
            None => {
                self.caches.insert(tab, ViewCache { key, rebuilds: 1 });
                Ok(true)
            }
        }
    }

    pub fn cache(&self, tab: TabId) -> Option<&ViewCache> {
        self.caches.get(&tab)
    }

    // ── закрытие: двухфазное ───────────────────────────────────────────

    pub fn request_close(&self, tab: TabId) -> Result<CloseRequest, WsError> {
        let s = self.tabs.get(tab).ok_or(WsError::UnknownTab)?;
        let mut fx = vec![];
        if let Some(t) = &s.title_input {
            fx.push(Effect::CommitTitle { tab, doc: s.doc_id, new_title: t.text.clone() });
        }
        let last = self.tabs.tabs_of(s.doc_id).len() == 1;
        let dirty = self.docs.get(s.doc_id).map_or(false, |d| d.is_dirty);
        if last && dirty {
            fx.push(Effect::FlushDoc(s.doc_id));
        }
        Ok(if fx.is_empty() { CloseRequest::Ready } else { CloseRequest::NeedsEffects(fx) })
    }

    /// Выполнение эффекта; в приложении здесь реальный IO.
    /// Ошибка (напр. NameTaken) оставляет всё как есть, закрытие отменяется.
    pub fn run_effect(&mut self, e: &Effect) -> Result<(), WsError> {
        match e {
            Effect::CommitTitle { tab, doc, new_title } => {
                self.docs.rename(*doc, new_title)?;
                if let Some(s) = self.tabs.get_mut(*tab) {
                    s.title_input = None;
                }
                Ok(())
            }
            Effect::FlushDoc(doc) => {
                self.docs.mark_flushed(*doc);
                Ok(())
            }
        }
    }

    /// Удаляет сессию. Отказывает, пока не выполнены эффекты.
    pub fn finish_close(&mut self, tab: TabId) -> Result<(), WsError> {
        let s = self.tabs.get(tab).ok_or(WsError::UnknownTab)?;
        let doc = s.doc_id;
        let last = self.tabs.tabs_of(doc).len() == 1;
        let dirty = self.docs.get(doc).map_or(false, |d| d.is_dirty);
        if s.title_input.is_some() || (last && dirty) {
            return Err(WsError::PendingEffects);
        }
        if let Some(leaf) = self.layout.leaf_of(tab) {
            self.layout.remove_tab(leaf, tab);
        }
        self.tabs.remove(tab);
        self.caches.remove(&tab);
        if self.tabs.tabs_of(doc).is_empty() {
            self.docs.release(doc);
        }
        Ok(())
    }
}
