use super::*;

impl AppState {
    /// Centralized handling when a file or directory has been deleted on disk or by a plugin.
    ///
    /// Closes all matching tabs across all windows/leaves, clears document buffers,
    /// removes from `open_tabs`, `session.open_tabs`, `nav_history`, and unloads resources.
    /// If `keep_if_dirty` is true and the document has unsaved edits, it is kept open so the
    /// user has an opportunity to save or recover it.
    ///
    /// Returns `true` if any tabs/documents were closed.
    pub fn handle_file_deleted(&mut self, path: &Path, keep_if_dirty: bool) -> bool {
        let norm_target = Self::normalize_path(path);

        // Find all documents matching this path or starting with this folder
        let mut matching_doc_ids: Vec<DocumentId> = self
            .documents
            .iter()
            .filter_map(|(id, doc)| {
                let norm_doc = Self::normalize_path(&doc.path);
                if doc.path == path
                    || norm_doc == norm_target
                    || doc.path.starts_with(path)
                    || norm_doc.starts_with(&norm_target)
                {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        for (id, meta) in &self.doc_meta {
            let norm_doc = Self::normalize_path(&meta.path);
            if (meta.path == path
                || norm_doc == norm_target
                || meta.path.starts_with(path)
                || norm_doc.starts_with(&norm_target))
                && !matching_doc_ids.contains(id)
            {
                matching_doc_ids.push(*id);
            }
        }

        let mut any_closed = false;
        let mut any_kept_dirty = false;

        for doc_id in matching_doc_ids {
            let doc_path = if let Some(p) = self.document_path(doc_id) {
                p.to_path_buf()
            } else {
                continue;
            };

            let is_doc_dirty = self.is_document_dirty(doc_id);

            if keep_if_dirty && is_doc_dirty {
                self.status_text = format!(
                    "Файл удалён на диске, но сохранён в редакторе (несохранённые правки): {}",
                    doc_path.file_name().unwrap_or_default().to_string_lossy()
                );
                any_kept_dirty = true;
                continue;
            }

            // Close all tabs referencing this document in every leaf/window
            let mut empty_leaves = Vec::new();
            for (leaf_id, leaf) in self.leaves.iter_mut() {
                let mut i = 0;
                while i < leaf.tabs.len() {
                    let matches = match &leaf.tabs[i].content {
                        crate::workspace::TabContent::Document { id } => *id == doc_id,
                        _ => false,
                    };
                    if matches {
                        let tab = leaf.tabs.remove(i);
                        self.views.remove(&tab.id);
                        if !leaf.tabs.is_empty() {
                            if i < leaf.active_tab_index {
                                leaf.active_tab_index = leaf.active_tab_index.saturating_sub(1);
                            } else if leaf.active_tab_index >= leaf.tabs.len() {
                                leaf.active_tab_index = leaf.tabs.len() - 1;
                            }
                        }
                    } else {
                        i += 1;
                    }
                }
                if leaf.tabs.is_empty() {
                    empty_leaves.push(*leaf_id);
                }
            }

            for leaf_id in empty_leaves {
                if self.workspace.leaves().len() > 1 {
                    self.workspace.close_leaf(leaf_id);
                    self.leaves.remove(&leaf_id);
                    if self.workspace.active_leaf == leaf_id {
                        if let Some(&first) = self.workspace.leaves().first() {
                            self.workspace.active_leaf = first;
                        }
                    }
                } else if let Some(leaf) = self.leaves.get_mut(&leaf_id) {
                    leaf.tabs.push(crate::workspace::Tab::new_empty());
                    leaf.active_tab_index = 0;
                }
            }

            // Remove from document buffers, doc_meta and by_path mapping
            let removed_path = self.documents.remove(&doc_id).map(|d| d.path)
                .or_else(|| self.doc_meta.remove(&doc_id).map(|m| m.path));
            self.doc_meta.remove(&doc_id);

            if let Some(target_p) = removed_path {
                let norm = Self::normalize_path(&target_p);
                self.by_path.remove(&norm);
                self.by_path.remove(&Self::normalize_path(path));
                self.open_tabs.retain(|p| p != &target_p && p != path && p != &norm);
                self.session.open_tabs.retain(|p| p != &target_p && p != path && p != &norm);
                self.nav_history.retain(|p| p != &target_p && p != path && p != &norm);
                self.note_states.remove(&target_p);
                self.note_states.remove(path);

                if self.session.active_note.as_ref().map(|p| p == &target_p || p == path).unwrap_or(false) {
                    self.session.active_note = None;
                }
                any_closed = true;
            }
        }

        if !any_kept_dirty {
            // Also clean up any loose references in open_tabs/session if doc was not in documents map
            let norm_p = Self::normalize_path(path);
            self.open_tabs.retain(|p| p != path && Self::normalize_path(p) != norm_p && !p.starts_with(path));
            self.session.open_tabs.retain(|p| p != path && Self::normalize_path(p) != norm_p && !p.starts_with(path));
            self.nav_history.retain(|p| p != path && Self::normalize_path(p) != norm_p && !p.starts_with(path));

            if self.session.active_note.as_ref().map(|p| p == path || Self::normalize_path(p) == norm_p).unwrap_or(false) {
                self.session.active_note = None;
                any_closed = true;
            }
        }

        // Also remove from recent files
        let ws = self.workspace_path.as_deref();
        if self.recent_files.remove_path_or_prefix(path, ws) {
            self.session.recent_files = self.recent_files.clone();
        }

        self.cleanup_orphaned_documents_and_views();

        // If session active_note is None, try activating whatever tab is in active_leaf
        if self.session.active_note.is_none() {
            let active_leaf_id = self.workspace.active_leaf;
            if let Some(leaf) = self.leaves.get(&active_leaf_id) {
                if let Some(tab) = leaf.active_tab() {
                    match &tab.content {
                        crate::workspace::TabContent::Document { id } => {
                            if let Some(doc) = self.documents.get(id) {
                                self.session.active_note = Some(doc.path.clone());
                            }
                        }
                        crate::workspace::TabContent::Empty => {
                            self.active_empty_tab = Some(empty_tab_path());
                            self.session.active_note = Some(empty_tab_path());
                        }
                        _ => {}
                    }
                }
            }
        }

        self.save_session_state();
        // The file set changed on disk: the plugin snapshot's file list (and the
        // vault cache behind it) must be rebuilt, or plugins keep seeing the
        // pre-change list. The rename case is covered by the same publish in
        // handle_file_renamed.
        self.invalidate_markdown_cache_and_tree();
        self.publish_snapshot();
        any_closed
    }

    /// Centralized handling when a file or directory has been renamed or moved on disk or by a plugin.
    ///
    /// Synchronously updates all matching documents, tabs across all leaves/windows,
    /// paths, titles, `open_tabs`, `session.open_tabs`, and `active_note`.
    pub fn handle_file_renamed(&mut self, old_path: &Path, new_path: &Path) -> bool {
        let norm_old = Self::normalize_path(old_path);
        let norm_new = Self::normalize_path(new_path);

        let mut matching_docs: Vec<(DocumentId, PathBuf)> = self
            .documents
            .iter()
            .filter_map(|(id, doc)| {
                let norm_doc = Self::normalize_path(&doc.path);
                if doc.path == old_path || norm_doc == norm_old {
                    Some((*id, new_path.to_path_buf()))
                } else if let Ok(suffix) = doc.path.strip_prefix(old_path) {
                    Some((*id, new_path.join(suffix)))
                } else if let Ok(suffix) = norm_doc.strip_prefix(&norm_old) {
                    Some((*id, norm_new.join(suffix)))
                } else {
                    None
                }
            })
            .collect();

        for (id, meta) in &self.doc_meta {
            let norm_doc = Self::normalize_path(&meta.path);
            let target = if meta.path == old_path || norm_doc == norm_old {
                Some(new_path.to_path_buf())
            } else if let Ok(suffix) = meta.path.strip_prefix(old_path) {
                Some(new_path.join(suffix))
            } else if let Ok(suffix) = norm_doc.strip_prefix(&norm_old) {
                Some(norm_new.join(suffix))
            } else {
                None
            };
            if let Some(target_p) = target {
                if !matching_docs.iter().any(|(d, _)| d == id) {
                    matching_docs.push((*id, target_p));
                }
            }
        }

        let mut any_renamed = false;

        for (doc_id, target_new_path) in matching_docs {
            let new_title = target_new_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "untitled".to_string());

            let old_doc_path = if let Some(doc) = self.documents.get_mut(&doc_id) {
                let prev = std::mem::replace(&mut doc.path, target_new_path.clone());
                doc.title = new_title.clone();
                if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
                    meta.path = target_new_path.clone();
                    meta.title = new_title.clone();
                }
                prev
            } else if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
                let prev = std::mem::replace(&mut meta.path, target_new_path.clone());
                meta.title = new_title.clone();
                prev
            } else {
                continue;
            };

            // Update by_path index
            let norm_old_doc = Self::normalize_path(&old_doc_path);
            let norm_new_doc = Self::normalize_path(&target_new_path);
            self.by_path.remove(&norm_old_doc);
            self.by_path.insert(norm_new_doc, doc_id);


            // Update path lists
            for p in self.open_tabs.iter_mut() {
                if *p == old_doc_path || Self::normalize_path(p) == norm_old_doc {
                    *p = target_new_path.clone();
                }
            }
            for p in self.session.open_tabs.iter_mut() {
                if *p == old_doc_path || Self::normalize_path(p) == norm_old_doc {
                    *p = target_new_path.clone();
                }
            }
            for p in self.nav_history.iter_mut() {
                if *p == old_doc_path || Self::normalize_path(p) == norm_old_doc {
                    *p = target_new_path.clone();
                }
            }

            // Migrate note settings if present
            if let Some(settings) = self.note_states.remove(&old_doc_path) {
                self.note_states.insert(target_new_path.clone(), settings);
            }

            if self.session.active_note.as_ref().map(|p| p == &old_doc_path || Self::normalize_path(p) == norm_old_doc).unwrap_or(false) {
                self.session.active_note = Some(target_new_path);
            }

            any_renamed = true;
        }

        // Also check if open_tabs match old_path directly even if not in documents
        if !any_renamed {
            for p in self.open_tabs.iter_mut() {
                if *p == old_path || Self::normalize_path(p) == norm_old {
                    *p = new_path.to_path_buf();
                    any_renamed = true;
                }
            }
            for p in self.session.open_tabs.iter_mut() {
                if *p == old_path || Self::normalize_path(p) == norm_old {
                    *p = new_path.to_path_buf();
                    any_renamed = true;
                }
            }
            for p in self.nav_history.iter_mut() {
                if *p == old_path || Self::normalize_path(p) == norm_old {
                    *p = new_path.to_path_buf();
                    any_renamed = true;
                }
            }
        }

        // Also update recent files
        let ws = self.workspace_path.as_deref();
        if self.recent_files.rename_path_or_prefix(old_path, new_path, ws) {
            self.session.recent_files = self.recent_files.clone();
            any_renamed = true;
        }

        self.save_session_state();
        self.invalidate_markdown_cache_and_tree();
        self.publish_snapshot();
        any_renamed
    }

    /// Centralized handling when a file has been modified externally on disk.
    ///
    /// If the file is open and clean (not dirty), reloads content from disk.
    /// If the file has unsaved edits, keeps the user's edits and warns in status.
    /// A modified event for a *not open* note is how the watcher reports a
    /// note created outside the vault API (new-note button, external editor):
    /// the plugin file list must be rebuilt or plugins keep seeing the old set.
    pub fn handle_file_modified(&mut self, path: &Path) -> bool {
        let opt_doc_id = self.find_document_by_path(path);

        if opt_doc_id.is_none() {
            // Cheap: the vault cache is just an Option; the publish shares the
            // rebuilt list through an Arc. A write echo of a deleted-then-recreated
            // tab also lands here, which is the right moment to resync anyway.
            self.invalidate_markdown_cache_and_tree();
            self.publish_snapshot();
        }

        if let Some(doc_id) = opt_doc_id {
            let is_dirty = self.documents.get(&doc_id).map(|d| d.is_dirty).unwrap_or(false);

            // Dirty buffer: an external change normally only gets a status —
            // but the watcher also fires for our own background save (a plain
            // in-place write is indistinguishable from a foreign write by the
            // event alone). When the file content already equals the buffer,
            // this is our own write echo: reconcile the hash silently.
            if is_dirty {
                if let Some(actual) = crate::save_manager::hash_file(path) {
                    let buffer_matches = self.documents.get(&doc_id)
                        .map(|d| {
                            d.disk_hash.as_deref() == Some(actual.as_str())
                                || crate::save_manager::hash_rope(&d.rope) == actual
                        })
                        .unwrap_or(false);
                    if buffer_matches {
                        if let Some(doc) = self.documents.get_mut(&doc_id) {
                            doc.disk_hash = Some(actual);
                            doc.disk_stamp = crate::save_manager::file_stamp(path);
                        }
                        return false;
                    }
                }
                self.status_text = format!(
                    "Файл изменён внешним приложением (в редакторе есть несохранённые правки): {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
                return false;
            }

            // Pull protocol (fs-watcher-plan §3): skip both our own write
            // echoes and pointless re-reads with two cheap checks before any
            // read. Unknown hash ("not reconciled yet") must not block a
            // reload — the on-disk content is authoritative for a clean doc.
            let known_hash = self.documents.get(&doc_id).and_then(|d| d.disk_hash.clone());
            let known_stamp = self.documents.get(&doc_id).and_then(|d| d.disk_stamp);
            if let Some(stamp) = crate::save_manager::file_stamp(path) {
                if Some(stamp) == known_stamp {
                    return false;
                }
            }
            if let (Some(known), Some(actual)) = (&known_hash, crate::save_manager::hash_file(path)) {
                if *known == actual {
                    // Same content, new stamp: remember it and skip the read.
                    if let Some(doc) = self.documents.get_mut(&doc_id) {
                        doc.disk_stamp = crate::save_manager::file_stamp(path);
                    }
                    return false;
                }
            }

            if let Ok(new_content) = std::fs::read_to_string(path) {
                if let Some(doc) = self.documents.get_mut(&doc_id) {
                    doc.reload(new_content);
                }
                for view in self.views.values_mut() {
                    if view.doc_id == doc_id {
                        view.visual_cache_dirty = true;
                    }
                }
                self.status_text = format!(
                    "Файл обновлён: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
                self.invalidate_markdown_cache_and_tree();
                self.publish_snapshot();
                return true;
            }
        }
        false
    }

    /// Drains coalesced watcher events and applies the deletion debounce.
    ///
    /// The watcher (or a test, through the same channel) delivers
    /// `Deleted`-worthy paths; a path absent from disk for longer than
    /// `debounce_duration` is confirmed deleted. **No filesystem calls are
    /// made when nothing is pending** — the old per-document `path.exists()`
    /// polling on the UI thread is gone. The returned events are exactly what
    /// `poll_fs_changes` handles.
    pub fn check_fs_changes(
        &mut self,
        debounce_duration: Duration,
    ) -> Vec<FsChangeEvent> {
        let mut events = Vec::new();

        // 1. Drain every batch already coalesced by the delivery thread.
        // Deleting and renaming are acted on immediately; writes are fed
        // through the same missing-on-disk debounce, because an atomic save
        // (tmp → rename) can transiently make the target look gone.
        if let Some(rx) = &self.fs_events_rx {
            let mut deleted: Vec<PathBuf> = Vec::new();
            let mut renamed: Vec<(PathBuf, PathBuf)> = Vec::new();
            let mut modified: Vec<PathBuf> = Vec::new();
            let mut rescan = false;
            let mut attachments_touched = false;
            while let Ok(batch) = rx.try_recv() {
                attachments_touched |= batch.attachments_touched;
                deleted.extend(batch.deleted);
                renamed.extend(batch.renamed);
                modified.extend(batch.modified);
                rescan |= batch.rescan;
            }
            for (from, to) in renamed {
                events.push(FsChangeEvent::Renamed {
                    old_path: from,
                    new_path: to,
                });
            }
            // Deletions confirmed through the debounce below; writes of
            // existing files go straight to the modified path (the pull
            // protocol in `handle_file_modified` decides whether a reload is
            // needed).
            for path in modified {
                if path.exists() {
                    events.push(FsChangeEvent::Modified(path));
                }
            }
            let mut seen_missing: Vec<PathBuf> = Vec::new();
            for path in deleted {
                if !seen_missing.contains(&path) {
                    seen_missing.push(path.clone());
                    self.pending_file_deletions.entry(path).or_insert_with(Instant::now);
                }
            }
            if rescan {
                events.push(FsChangeEvent::Rescan);
            }
            if attachments_touched {
                events.push(FsChangeEvent::AttachmentsTouched);
            }
        }

        // 2. Apply the deletion debounce to every pending path: a path that
        // reappeared (atomic save / rapid recreation) is cancelled even
        // before its debounce expires.
        let now_expired: Vec<PathBuf> = self
            .pending_file_deletions
            .iter()
            .filter(|(_, first_seen)| first_seen.elapsed() >= debounce_duration)
            .map(|(path, _)| path.clone())
            .collect();
        let still_pending: Vec<PathBuf> = self
            .pending_file_deletions
            .keys()
            .filter(|path| !now_expired.contains(path))
            .cloned()
            .collect();
        for path in still_pending {
            if path.exists() {
                self.pending_file_deletions.remove(&path);
            }
        }
        for path in now_expired {
            if path.exists() {
                self.pending_file_deletions.remove(&path);
            } else {
                self.pending_file_deletions.remove(&path);
                events.push(FsChangeEvent::Deleted(path));
            }
        }

        // 3. Clean up pending deletions for paths that are no longer open or in recent files.
        let mut tracked_paths: Vec<PathBuf> = self
            .documents
            .values()
            .map(|d| d.path.clone())
            .chain(self.doc_meta.values().map(|m| m.path.clone()))
            .collect();
        if let Some(ws) = &self.workspace_path {
            for item in &self.recent_files.recent_files {
                let p = Path::new(&item.path);
                let abs = if p.is_absolute() { p.to_path_buf() } else { ws.join(p) };
                tracked_paths.push(abs);
            }
        }
        self.pending_file_deletions.retain(|p, _| tracked_paths.contains(p));

        events
    }
}
