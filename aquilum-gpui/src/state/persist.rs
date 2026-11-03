use super::*;

impl AppState {
    pub fn snapshot_active_note_settings(&mut self, scroll_top: f32, edit_mode: bool, word_wrap: bool, source_mode: bool) {
        if let (Some(view), Some(doc)) = (self.active_view(), self.active_document()) {
            let path = doc.path.clone();
            let cursor_offset = view.cursor_offset;
            let selection_anchor = view.selection_anchor;
            let settings = NoteSettings {
                cursor_offset,
                selection_anchor,
                scroll_top,
                edit_mode,
                word_wrap,
                source_mode,
            };
            self.note_states.insert(path.clone(), settings.clone());
            self.session.note_states.insert(path.clone(), settings);
            self.session.cursor_positions.insert(path, cursor_offset);
        }
    }

    pub fn save_document(&mut self, doc_id: DocumentId) -> bool {
        if let Some(doc) = self.documents.get_mut(&doc_id) {
            if !doc.is_dirty {
                return true;
            }
            let expected_hash = doc.disk_hash.clone();
            let version = doc.version;
            match crate::save_manager::safe_write_rope_checked(&doc.path, doc.rope(), expected_hash.as_deref(), version) {
                crate::save_manager::CheckedWrite::Written(hash) => {
                    doc.disk_hash = Some(hash);
                    doc.disk_stamp = crate::save_manager::file_stamp(&doc.path);
                    doc.mark_clean();
                    // Attachment lifecycle follows the saved text: a removed
                    // reference goes to the trash unless another note uses it,
                    // and a restored reference comes back from the trash.
                    let new_refs = crate::images::references_in_rope(doc.rope());
                    let previous = doc.attachment_refs.replace(new_refs.clone());
                    let auto_trash = self.settings.get_config().files.auto_trash_attachments;
                    crate::images::reconcile_saved_references(
                        self.workspace_path.as_deref(),
                        &doc.path,
                        &crate::images::folder(),
                        &previous,
                        &new_refs,
                        auto_trash,
                    );
                    if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
                        meta.is_dirty = false;
                    }
                    // Search index follows the save (stage B5): reindex just
                    // this file; the worker skips paths not indexed yet.
                    self.search_worker.update_paths(vec![doc.path.clone()]);
                    self.status_text = format!("Saved: {}", doc.title);
                }
                crate::save_manager::CheckedWrite::Conflict { copy, disk_text } => {
                    // Two-sided conflict protocol: our version is preserved in
                    // `copy`, the buffer adopts the external content. No data
                    // loss on either side.
                    doc.reload(disk_text);
                    for view in self.views.values_mut() {
                        if view.doc_id == doc_id {
                            view.visual_cache_dirty = true;
                        }
                    }
                    self.status_text = format!(
                        "Файл изменён извне; ваша версия сохранена в {}",
                        copy.file_name().unwrap_or_default().to_string_lossy()
                    );
                }
                crate::save_manager::CheckedWrite::Failed(err) => {
                    self.status_text = format!("Save error: {}", err);
                    return false;
                }
            }
            let rel_path = self.vault.as_ref()
                .and_then(|v| {
                    self.documents.get(&doc_id).map(|d| d.path.strip_prefix(&v.root).ok()).unwrap_or(None)
                })
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .or_else(|| {
                    self.documents.get(&doc_id).map(|d| d.path.to_string_lossy().into_owned())
                });
            if let Some(rel_path) = rel_path {
                self.events.emit_vault(&crate::events::VaultEvent::Modify { path: rel_path });
            }
            self.publish_snapshot();
            return true;
        }
        false
    }

    pub fn save_dirty_documents(&mut self) {
        let dirty_ids: Vec<DocumentId> = self
            .documents
            .iter()
            .filter(|(_, doc)| doc.is_dirty)
            .map(|(id, _)| *id)
            .collect();

        for id in dirty_ids {
            self.save_document(id);
        }

        self.save_session_state();
    }

    pub fn save_active_note(&mut self) {
        if let Some(doc_id) = self.active_doc_id() {
            self.save_document(doc_id);
        }
        self.save_session_state();
    }

    pub fn export_workspace_session_v2(&self) -> crate::workspace::WorkspaceSessionV2 {
        fn node_to_persisted(
            node: &crate::workspace::WorkspaceNode,
            leaves: &HashMap<LeafId, LeafState>,
            documents: &HashMap<DocumentId, crate::document_buffer::DocumentBuffer>,
            views: &HashMap<crate::workspace::TabId, crate::workspace::EditorViewState>,
        ) -> crate::workspace::PersistedNode {
            match node {
                crate::workspace::WorkspaceNode::Split { direction, children, ratios } => {
                    crate::workspace::PersistedNode::Split {
                        direction: *direction,
                        children: children.iter().map(|c| node_to_persisted(c, leaves, documents, views)).collect(),
                        ratios: ratios.clone(),
                    }
                }
                crate::workspace::WorkspaceNode::Leaf { id } => {
                    let (active_tab_index, tabs) = if let Some(leaf) = leaves.get(id) {
                        let persisted_tabs = leaf.tabs.iter().map(|t| {
                            let (kind, path) = match &t.content {
                                crate::workspace::TabContent::Document { id } => {
                                    let p = documents.get(id).map(|d| d.path.to_string_lossy().to_string());
                                    ("document".to_string(), p)
                                }
                                crate::workspace::TabContent::Empty => ("empty".to_string(), None),
                                crate::workspace::TabContent::Plugin { view_id } => ("plugin".to_string(), Some(view_id.clone())),
                            };
                            let view = views.get(&t.id).map(|v| crate::workspace::PersistedView {
                                anchor_line: v.viewport.offset,
                                px_in_line: v.viewport.px_in_line,
                                cursor_line: 0,
                                cursor_col: 0,
                                scroll_top: v.scroll_top,
                                cursor_offset: v.cursor_offset,
                            });
                            crate::workspace::PersistedTab {
                                id: t.id,
                                kind,
                                path,
                                pinned: t.pinned,
                                external: None,
                                view,
                            }
                        }).collect();
                        (leaf.active_tab_index, persisted_tabs)
                    } else {
                        (0, Vec::new())
                    };

                    crate::workspace::PersistedNode::Leaf {
                        id: *id,
                        active_tab_index,
                        tabs,
                    }
                }
            }
        }

        let layout = node_to_persisted(&self.workspace.root, &self.leaves, &self.documents, &self.views);
        let mut session = crate::workspace::WorkspaceSessionV2 {
            version: crate::workspace::WORKSPACE_SCHEMA_VERSION,
            active_leaf: self.workspace.active_leaf,
            layout,
        };
        session.validate();
        session
    }

    pub fn restore_workspace_session_v2(&mut self, session: crate::workspace::WorkspaceSessionV2) {
        fn persisted_to_node(
            persisted: crate::workspace::PersistedNode,
            leaves: &mut HashMap<LeafId, LeafState>,
            views: &mut HashMap<crate::workspace::TabId, crate::workspace::EditorViewState>,
            state: &mut AppState,
        ) -> crate::workspace::WorkspaceNode {
            match persisted {
                crate::workspace::PersistedNode::Split { direction, children, ratios } => {
                    crate::workspace::WorkspaceNode::Split {
                        direction,
                        children: children.into_iter().map(|c| persisted_to_node(c, leaves, views, state)).collect(),
                        ratios,
                    }
                }
                crate::workspace::PersistedNode::Leaf { id, active_tab_index, tabs } => {
                    let mut leaf_state = LeafState::new(id);
                    leaf_state.active_tab_index = active_tab_index;
                    for p_tab in tabs {
                        let content = match p_tab.kind.as_str() {
                            "document" => {
                                if let Some(path_str) = p_tab.path {
                                    let path = PathBuf::from(path_str);
                                    let title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".to_string());
                                    let norm = AppState::normalize_path(&path);
                                    let doc_id = if let Some(&id) = state.by_path.get(&norm) {
                                        id
                                    } else {
                                        let doc_id = DocumentId::new();
                                        state.by_path.insert(norm, doc_id);
                                        state.doc_meta.insert(
                                            doc_id,
                                            DocumentMetadata {
                                                id: doc_id,
                                                path: path.clone(),
                                                title: title.clone(),
                                                is_dirty: false,
                                                last_accessed: Instant::now(),
                                            },
                                        );
                                        doc_id
                                    };
                                    if let Some(pv) = p_tab.view {
                                        let mut ev = crate::workspace::EditorViewState::new(p_tab.id, doc_id);
                                        ev.viewport.offset = pv.anchor_line;
                                        ev.viewport.px_in_line = pv.px_in_line;
                                        ev.cursor_offset = pv.cursor_offset;
                                        ev.scroll_top = pv.scroll_top;
                                        views.insert(p_tab.id, ev);
                                    } else {
                                        views.insert(p_tab.id, crate::workspace::EditorViewState::new(p_tab.id, doc_id));
                                    }
                                    crate::workspace::TabContent::Document { id: doc_id }
                                } else {
                                    crate::workspace::TabContent::Empty
                                }
                            }
                            "plugin" => {
                                crate::workspace::TabContent::Plugin { view_id: p_tab.path.unwrap_or_default() }
                            }
                            _ => crate::workspace::TabContent::Empty,
                        };

                        leaf_state.tabs.push(crate::workspace::Tab {
                            id: p_tab.id,
                            content,
                            pinned: p_tab.pinned,
                        });
                    }

                    if leaf_state.tabs.is_empty() {
                        leaf_state.tabs.push(crate::workspace::Tab::new_empty());
                    }
                    if leaf_state.active_tab_index >= leaf_state.tabs.len() {
                        leaf_state.active_tab_index = leaf_state.tabs.len() - 1;
                    }

                    leaves.insert(id, leaf_state);
                    crate::workspace::WorkspaceNode::Leaf { id }
                }
            }
        }

        let mut new_leaves = HashMap::new();
        let mut new_views = HashMap::new();
        let root = persisted_to_node(session.layout, &mut new_leaves, &mut new_views, self);

        self.leaves = new_leaves;
        self.views = new_views;
        self.workspace.root = root;
        self.workspace.root.flatten_same_direction_splits();
        self.workspace.root.normalize_ratios();
        self.workspace.active_leaf = session.active_leaf;

        // Ensure active tabs in all leaves are loaded
        let visible_docs: Vec<DocumentId> = self.leaves.values()
            .filter_map(|l| l.active_tab())
            .filter_map(|t| match &t.content {
                crate::workspace::TabContent::Document { id } => Some(*id),
                _ => None,
            })
            .collect();
        for id in visible_docs {
            self.ensure_document_loaded(id);
        }
        self.evict_inactive_documents_if_needed();
    }

    pub fn save_session_state(&mut self) {
        self.queue_session_state_save(true);
    }

    /// Serializes the session off the UI thread.
    ///
    /// Every save serializes two pretty-JSON files (session + workspace), and
    /// their size grows with `note_states` — serializing them on the UI thread
    /// stalled a frame after every typing pause. The mutation of the session
    /// fields stays synchronous; serialization and the two file writes run on
    /// the background executor.
    ///
    /// `sync` also runs the writes inline — used on the exit path, where the
    /// process may not outlive a spawned background job.
    pub fn queue_session_state_save(&mut self, sync: bool) {
        let active_info = self.active_view().and_then(|v| {
            self.active_document().map(|d| (d.path.clone(), v.cursor_offset, v.selection_anchor))
        });
        if let Some((path, cursor, anchor)) = active_info {
            let entry = self.note_states.entry(path.clone()).or_default();
            entry.cursor_offset = cursor;
            entry.selection_anchor = anchor;
            self.session.note_states.insert(path.clone(), entry.clone());
            self.session.cursor_positions.insert(path.clone(), cursor);
            self.session.active_note = Some(path);
            self.active_empty_tab = None;
        } else if let Some(ref empty) = self.active_empty_tab.clone() {
            self.session.active_note = Some(empty.clone());
        }
        self.session.open_tabs = self.open_tabs.clone();
        self.session.workspace_path = self.workspace_path.clone();
        self.session.expanded_folders = self.expanded_folders.clone();
        self.session.known_vaults = self.known_vaults.clone();

        let dir = app_data_dir();
        let session = self.session.clone();
        let ws_v2 = self.export_workspace_session_v2();

        let write_all = move || {
            session.save(&dir);
            let ws_v2_path = dir.join("workspace-v2.json");
            let _ = ws_v2.save_atomic(&ws_v2_path);
        };

        if sync {
            write_all();
            return;
        }

        // The spawned thread owns serialized inputs; nothing here touches
        // `self` or any GPUI entity, so an OS thread is safe (and cheaper than
        // routing through a UI context just to write two files). The exit path
        // uses `sync`, so a torn write from a dying process is not a concern.
        std::thread::Builder::new()
            .name("session-save".to_owned())
            .spawn(write_all)
            .expect("spawning the session-save thread cannot fail on a sane system");
    }

    pub fn rename_note(&mut self, old_path: &Path, new_title: &str) -> Result<PathBuf, String> {
        let trimmed = new_title.trim();
        if trimmed.is_empty() {
            return Err("Note title cannot be empty".to_string());
        }

        let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        if trimmed.chars().any(|c| invalid_chars.contains(&c) || c.is_control()) {
            return Err("Title contains invalid characters (< > : \" / \\ | ? *)".to_string());
        }

        let ext = old_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("md");

        let parent = match old_path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => {
                if let Some(ref ws) = self.workspace_path {
                    ws.clone()
                } else {
                    PathBuf::from(".")
                }
            }
        };

        let new_filename = format!("{}.{}", trimmed, ext);
        let new_path = parent.join(&new_filename);

        if new_path == old_path {
            return Ok(old_path.to_path_buf());
        }

        if new_path.exists() {
            return Err(format!("File '{}' already exists", new_filename));
        }

        // If currently open and dirty, save it before rename
        let opt_doc_id = self.find_document_by_path(old_path);
        if let Some(doc_id) = opt_doc_id {
            if let Some(doc) = self.documents.get_mut(&doc_id) {
                if doc.is_dirty {
                    let _ = crate::save_manager::safe_write_rope(old_path, doc.rope());
                    doc.is_dirty = false;
                }
            }
        }

        // Rename on disk if the old file exists
        if old_path.exists() {
            std::fs::rename(old_path, &new_path)
                .map_err(|e| format!("Failed to rename file: {}", e))?;
        }

        // Update open tabs
        for tab in &mut self.open_tabs {
            if tab == old_path {
                *tab = new_path.clone();
            }
        }

        // Update navigation history
        for nav in &mut self.nav_history {
            if nav == old_path {
                *nav = new_path.clone();
            }
        }

        // Update note states map
        if let Some(settings) = self.note_states.remove(old_path) {
            self.note_states.insert(new_path.clone(), settings);
        }

        // Update multi-document workspace state
        let norm_old = Self::normalize_path(old_path);
        let norm_new = Self::normalize_path(&new_path);
        let opt_doc_id = self.by_path.remove(old_path).or_else(|| self.by_path.remove(&norm_old));
        if let Some(doc_id) = opt_doc_id {
            self.by_path.insert(norm_new, doc_id);
            if let Some(doc) = self.documents.get_mut(&doc_id) {
                doc.path = new_path.clone();
                doc.title = trimmed.to_string();
            }
            if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
                meta.path = new_path.clone();
                meta.title = trimmed.to_string();
            }
        }

        // Update session
        self.session.open_tabs = self.open_tabs.clone();
        if self.session.active_note.as_deref() == Some(old_path) {
            self.session.active_note = Some(new_path.clone());
        }
        if let Some(cursor) = self.session.cursor_positions.remove(old_path) {
            self.session.cursor_positions.insert(new_path.clone(), cursor);
        }
        if let Some(st) = self.session.note_states.remove(old_path) {
            self.session.note_states.insert(new_path.clone(), st);
        }

        self.reload_file_tree();
        self.reload_files();
        self.reveal_path_in_tree(&new_path);
        self.save_session_state();
        self.status_text = format!("Renamed to: {}", trimmed);

        Ok(new_path)
    }
}
