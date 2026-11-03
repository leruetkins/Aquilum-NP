use super::*;

impl AppState {
    /// Default new-file stem in the current UI language.
    ///
    /// Used for every created note/canvas so the file name always matches
    /// the interface locale instead of a hardcoded language.
    pub fn untitled_base(&self) -> String {
        let lang = self.settings.get_config().ui.language;
        crate::i18n::t("common.untitled", &lang)
    }

    pub fn create_new_note(&mut self) -> Option<PathBuf> {
        self.create_new_note_placed(false)
    }

    /// Same as [`create_new_note`], but the new tab is inserted first in the
    /// tab strip (used by the leading new-note button).
    pub fn create_new_note_at_front(&mut self) -> Option<PathBuf> {
        self.create_new_note_placed(true)
    }

    pub(super) fn create_new_note_placed(&mut self, at_front: bool) -> Option<PathBuf> {
        let ws = self.workspace_path.clone()?;
        // Default file name comes from the UI locale ("Без названия" for
        // Russian, "Untitled" for English, ...), never hardcoded.
        let base = self.untitled_base();
        let mut candidate = ws.join(format!("{}.md", base));
        let mut counter = 1;
        while candidate.exists() {
            candidate = ws.join(format!("{} {}.md", base, counter));
            counter += 1;
        }
        let default_text = String::new();
        let _ = std::fs::write(&candidate, &default_text);
        self.reload_file_tree();
        if at_front {
            self.open_note_at_front(candidate.clone());
        } else {
            self.open_note(candidate.clone());
        }
        if let Some((view, doc)) = self.active_view_and_doc_mut() {
            view.cursor_offset = doc.len();
        }
        Some(candidate)
    }

    /// Creates a new note with the given title as the file name.
    ///
    /// The title is sanitized (same rules as rename) and uniquified
    /// (`Title.md`, `Title 2.md`, ...). Used by "Shift+Enter — create"
    /// in the Go to file dialog.
    pub fn create_named_note(&mut self, title: &str) -> Option<PathBuf> {
        let ws = self.workspace_path.clone()?;
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return None;
        }
        let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        let sanitized: String = trimmed
            .chars()
            .filter(|c| !invalid_chars.contains(c) && !c.is_control())
            .collect();
        let base: String = sanitized.trim().chars().take(80).collect();
        if base.is_empty() {
            return None;
        }
        let mut candidate = ws.join(format!("{}.md", base));
        let mut counter = 2;
        while candidate.exists() {
            candidate = ws.join(format!("{} {}.md", base, counter));
            counter += 1;
        }
        let _ = std::fs::write(&candidate, String::new());
        self.reload_file_tree();
        self.open_note(candidate.clone());
        if let Some((view, doc)) = self.active_view_and_doc_mut() {
            view.cursor_offset = doc.len();
        }
        Some(candidate)
    }

    /// Opens (activates) an empty tab: no file on disk, the New Tab page
    /// is shown instead of an editor. The sentinel is added to `open_tabs`
    /// like a regular tab so it renders in the tab strip.
    pub fn open_empty_tab(&mut self) -> PathBuf {
        // Save current view's cursor and selection before switching
        let active_info = self.active_view().and_then(|v| {
            self.active_document().map(|d| (d.path.clone(), v.cursor_offset, v.selection_anchor))
        });
        if let Some((path, cursor, anchor)) = active_info {
            let entry = self.note_states.entry(path.clone()).or_default();
            entry.cursor_offset = cursor;
            entry.selection_anchor = anchor;
            self.session.cursor_positions.insert(path.clone(), cursor);
            self.session.note_states.insert(path, entry.clone());
        }
        let path = empty_tab_path();
        self.push_nav_history(path.clone());
        if !self.open_tabs.iter().any(|p| p == &path) {
            self.open_tabs.push(path.clone());
        }
        self.active_empty_tab = Some(path.clone());
        self.session.active_note = Some(path.clone());
        self.save_session_state();
        path
    }

    /// Activates an already open empty tab.
    pub fn activate_empty_tab(&mut self, path: PathBuf) {
        let active_info = self.active_view().and_then(|v| {
            self.active_document().map(|d| (d.path.clone(), v.cursor_offset, v.selection_anchor))
        });
        if let Some((p, cursor, anchor)) = active_info {
            let entry = self.note_states.entry(p.clone()).or_default();
            entry.cursor_offset = cursor;
            entry.selection_anchor = anchor;
            self.session.cursor_positions.insert(p.clone(), cursor);
            self.session.note_states.insert(p, entry.clone());
        }
        self.push_nav_history(path.clone());
        if !self.open_tabs.iter().any(|p| p == &path) {
            self.open_tabs.push(path.clone());
        }
        self.active_empty_tab = Some(path.clone());
        self.session.active_note = Some(path);
        self.save_session_state();
    }

    pub fn close_tab(&mut self, path: &Path) {
        let opt_doc_id = self.find_document_by_path(path);
        let is_empty = is_empty_tab_path(path) || path.to_string_lossy() == "newtab";
        let mut target = None;
        for (leaf_id, leaf) in &self.leaves {
            for (idx, tab) in leaf.tabs.iter().enumerate() {
                match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        if Some(*id) == opt_doc_id {
                            target = Some((*leaf_id, idx));
                            break;
                        }
                    }
                    crate::workspace::TabContent::Empty => {
                        if is_empty {
                            target = Some((*leaf_id, idx));
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if target.is_some() {
                break;
            }
        }
        if let Some((leaf_id, tab_idx)) = target {
            self.close_leaf_tab(leaf_id, tab_idx);
        }

        if let Some(pos) = self.open_tabs.iter().position(|p| p == path) {
            self.open_tabs.remove(pos);
            if self.session.active_note.as_deref() == Some(path) {
                self.activate_neighbor_after_close(pos);
            } else if self.active_empty_tab.as_deref() == Some(path) {
                self.active_empty_tab = None;
                self.activate_neighbor_after_close(pos);
            }
        } else if self.session.active_note.as_deref() == Some(path) {
            self.session.active_note = None;
        } else if self.active_empty_tab.as_deref() == Some(path) {
            self.active_empty_tab = None;
            self.session.active_note = None;
        }

        if let Some(doc_id) = opt_doc_id {
            let still_open = self.leaves.values().any(|leaf| {
                leaf.tabs.iter().any(|t| match &t.content {
                    crate::workspace::TabContent::Document { id } => id == &doc_id,
                    _ => false,
                })
            });
            if !still_open {
                let doc_path = self.documents.remove(&doc_id).map(|d| d.path)
                    .or_else(|| self.doc_meta.remove(&doc_id).map(|m| m.path));
                self.doc_meta.remove(&doc_id);
                if let Some(doc_path) = doc_path {
                    let norm = Self::normalize_path(&doc_path);
                    self.by_path.remove(&norm);
                }
            }
        }

        self.save_session_state();
    }

    /// After closing the active tab at `pos`, activates the tab that slid
    /// into its place (or the last one). Handles both real and empty tabs.
    pub(super) fn activate_neighbor_after_close(&mut self, pos: usize) {
        let next_path = if pos < self.open_tabs.len() {
            Some(self.open_tabs[pos].clone())
        } else if !self.open_tabs.is_empty() {
            Some(self.open_tabs[self.open_tabs.len() - 1].clone())
        } else {
            None
        };
        match next_path {
            Some(target) => {
                if is_empty_tab_path(&target) {
                    self.active_empty_tab = Some(target.clone());
                    self.session.active_note = Some(target);
                } else {
                    self.active_empty_tab = None;
                    self.open_note(target);
                }
            }
            None => {
                self.active_empty_tab = None;
                self.session.active_note = None;
            }
        }
    }

    pub fn reload_files(&mut self) {
        // file_tree is maintained by reload_file_tree()
    }

    pub fn push_nav_history(&mut self, path: PathBuf) {
        if let Some(current) = self.nav_history.get(self.nav_history_index) {
            if current == &path {
                return;
            }
        }
        if !self.nav_history.is_empty() && self.nav_history_index + 1 < self.nav_history.len() {
            self.nav_history.truncate(self.nav_history_index + 1);
        }
        self.nav_history.push(path);
        if self.nav_history.len() > 100 {
            self.nav_history.remove(0);
        }
        self.nav_history_index = self.nav_history.len().saturating_sub(1);
    }

    pub fn can_navigate_back(&self) -> bool {
        self.nav_history_index > 0 && !self.nav_history.is_empty()
    }

    pub fn can_navigate_forward(&self) -> bool {
        !self.nav_history.is_empty() && self.nav_history_index + 1 < self.nav_history.len()
    }

    pub fn navigate_back(&mut self) -> Option<PathBuf> {
        if self.can_navigate_back() {
            self.nav_history_index -= 1;
            let target = self.nav_history[self.nav_history_index].clone();
            self.open_note_internal(target.clone());
            Some(target)
        } else {
            None
        }
    }

    pub fn navigate_forward(&mut self) -> Option<PathBuf> {
        if self.can_navigate_forward() {
            self.nav_history_index += 1;
            let target = self.nav_history[self.nav_history_index].clone();
            self.open_note_internal(target.clone());
            Some(target)
        } else {
            None
        }
    }

    pub fn note_breadcrumbs(&self) -> Vec<String> {
        let Some(doc) = self.active_document() else {
            return Vec::new();
        };
        if let Some(ref ws) = self.workspace_path {
            if let Ok(rel) = doc.path.strip_prefix(ws) {
                let mut parts = Vec::new();
                parts.push(self.workspace_name.clone());
                for comp in rel.components() {
                    let comp_str = comp.as_os_str().to_string_lossy().to_string();
                    if !comp_str.is_empty() {
                        parts.push(comp_str);
                    }
                }
                if let Some(last) = parts.last_mut() {
                    if let Some(stripped) = last.strip_suffix(".md") {
                        *last = stripped.to_string();
                    } else if let Some(stripped) = last.strip_suffix(".markdown") {
                        *last = stripped.to_string();
                    }
                }
                if !parts.is_empty() {
                    return parts;
                }
            }
        }
        vec![doc.title.clone()]
    }

    pub fn open_note(&mut self, path: PathBuf) {
        self.push_nav_history(path.clone());
        self.open_note_internal(path);
    }

    /// Same as [`open_note`], but a new tab is inserted at the front of the
    /// tab strip instead of appended at the end.
    pub fn open_note_at_front(&mut self, path: PathBuf) {
        self.push_nav_history(path.clone());
        self.open_note_internal_at(path, true);
    }

    pub fn open_note_internal(&mut self, path: PathBuf) {
        self.open_note_internal_at(path, false);
    }

    /// Records an opened note in the recent files list.
    pub fn record_recent_file(&mut self, path: &Path) {
        if is_empty_tab_path(path) || path.to_string_lossy() == "Welcome.md" {
            return;
        }
        let (rel_path, basename) = if let Some(ref ws) = self.workspace_path {
            let rel = path.strip_prefix(ws).unwrap_or(path);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| rel_str.clone());
            (rel_str, stem)
        } else {
            let p_str = path.to_string_lossy().replace('\\', "/");
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p_str.clone());
            (p_str, stem)
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        self.recent_files.add_file(&rel_path, &basename, now);
    }

    pub fn open_note_internal_at(&mut self, path: PathBuf, at_front: bool) {
        // Empty-tab sentinels carry no file: just activate the New Tab page.
        if is_empty_tab_path(&path) {
            self.activate_empty_tab(path);
            return;
        }
        self.record_recent_file(&path);
        self.reveal_path_in_tree(&path);

        // Save current view's cursor, selection before switching
        let active_info = self.active_view().and_then(|v| {
            self.active_document().map(|d| (d.path.clone(), v.cursor_offset, v.selection_anchor))
        });
        if let Some((p, cursor, anchor)) = active_info {
            let entry = self.note_states.entry(p.clone()).or_default();
            entry.cursor_offset = cursor;
            entry.selection_anchor = anchor;
            self.session.cursor_positions.insert(p.clone(), cursor);
            self.session.note_states.insert(p, entry.clone());
        }

        if !self.open_tabs.iter().any(|p| p == &path) {
            if at_front {
                self.open_tabs.insert(0, path.clone());
            } else {
                self.open_tabs.push(path.clone());
            }
        }
        let fallback = self.untitled_base();
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or(fallback);

        let doc_id = if let Some(doc_id) = self.find_document_by_path(&path) {
            self.status_text = format!("Switched: {}", path.file_name().unwrap_or_default().to_string_lossy());
            self.ensure_document_loaded(doc_id);
            doc_id
        } else {
            self.status_text = format!("Opened: {}", path.file_name().unwrap_or_default().to_string_lossy());
            if path.to_string_lossy() == "Welcome.md" {
                self.get_or_open_document(&path, title.clone(), DEFAULT_WELCOME_NOTE.to_owned())
            } else {
                self.get_or_open_document_from_file(&path, title.clone())
            }
        };

        self.ensure_document_loaded(doc_id);
        if let Some(doc) = self.documents.get(&doc_id) {
            self.workspace.open_file(&mut self.leaves, doc_id, self.workspace.active_leaf, crate::workspace::OpenMode::Reuse);
            if at_front {
                if let Some(leaf) = self.leaves.get_mut(&self.workspace.active_leaf) {
                    if leaf.tabs.len() > 1 && leaf.active_tab_index != 0 {
                        let tab = leaf.tabs.remove(leaf.active_tab_index);
                        leaf.tabs.insert(0, tab);
                        leaf.active_tab_index = 0;
                    }
                }
            }
            let active_tab_id = self.leaves.get(&self.workspace.active_leaf).and_then(|leaf| leaf.active_tab().map(|t| t.id));
            if let Some(tab_id) = active_tab_id {
                let view = self.views.entry(tab_id).or_insert_with(|| crate::workspace::EditorViewState::new(tab_id, doc_id));

                // Restore saved settings (cursor and selection) for this note
                if let Some(settings) = self.note_states.get(&path) {
                    let mut offset = settings.cursor_offset.min(doc.len());
                    while offset > 0 && !doc.is_char_boundary(offset) {
                        offset -= 1;
                    }
                    view.cursor_offset = offset;

                    view.selection_anchor = settings.selection_anchor.map(|anchor| {
                        let mut a = anchor.min(doc.len());
                        while a > 0 && !doc.is_char_boundary(a) {
                            a -= 1;
                        }
                        a
                    });
                }
            }
            self.active_empty_tab = None;
            self.session.active_note = Some(path.clone());
            self.save_session_state();

            let rel = self.vault.as_ref()
                .and_then(|v| path.strip_prefix(&v.root).ok())
                .map(|p| p.to_string_lossy().replace('\\', "/"));
            self.events.emit_workspace(&crate::events::WorkspaceEvent::FileOpen { path: rel });
            self.publish_snapshot();
        }
        self.evict_inactive_documents_if_needed();
    }
}
