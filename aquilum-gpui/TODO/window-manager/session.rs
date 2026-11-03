//! Tab sessions: only what belongs to a tab.

use std::collections::HashMap;

use crate::ops::{transform_through, DocumentId, EditEvent, TabId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CursorState {
    pub offset: usize,
    pub anchor: Option<usize>,
}

/// Viewport position. Here the anchor is a byte offset; in the project, map it
/// to `ViewportAnchor` (a line) via `apply_external_edit_detailed`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewState {
    pub anchor_offset: usize,
    pub px_in_line: f32,
    pub scroll_top: f64,
    pub measured_viewport_h: Option<f32>,
    pub word_wrap: bool,
    pub edit_mode: bool,
}

/// Some(..) = the title is being edited; is_focused is not needed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleInput {
    pub text: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct TabSession {
    pub id: TabId,
    pub doc_id: DocumentId,
    pub cursor: CursorState,
    pub view: ViewState,
    pub title_input: Option<TitleInput>,
}

impl TabSession {
    pub fn new(id: TabId, doc_id: DocumentId) -> Self {
        Self {
            id,
            doc_id,
            cursor: CursorState::default(),
            view: ViewState::default(),
            title_input: None,
        }
    }
}

#[derive(Default)]
pub struct TabSessionStore {
    tabs: HashMap<TabId, TabSession>,
}

impl TabSessionStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, s: TabSession) {
        self.tabs.insert(s.id, s);
    }
    pub fn remove(&mut self, id: TabId) -> Option<TabSession> {
        self.tabs.remove(&id)
    }
    pub fn get(&self, id: TabId) -> Option<&TabSession> {
        self.tabs.get(&id)
    }
    pub fn get_mut(&mut self, id: TabId) -> Option<&mut TabSession> {
        self.tabs.get_mut(&id)
    }
    pub fn tabs_of(&self, doc: DocumentId) -> Vec<TabId> {
        let mut v: Vec<_> = self.tabs.values().filter(|s| s.doc_id == doc).map(|s| s.id).collect();
        v.sort();
        v
    }

    /// OT for all tabs of the document.
    /// The initiator is not transformed but gets `cursor_after` (otherwise it shifts twice).
    pub fn apply_edit_event(&mut self, ev: &EditEvent) {
        for s in self.tabs.values_mut().filter(|s| s.doc_id == ev.doc) {
            s.view.anchor_offset = transform_through(s.view.anchor_offset, &ev.ops);
            match (Some(s.id) == ev.origin, ev.cursor_after) {
                (true, Some(c)) => {
                    s.cursor.offset = c;
                    s.cursor.anchor = None;
                }
                _ => {
                    s.cursor.offset = transform_through(s.cursor.offset, &ev.ops);
                    s.cursor.anchor = s.cursor.anchor.map(|a| transform_through(a, &ev.ops));
                }
            }
        }
    }
}
