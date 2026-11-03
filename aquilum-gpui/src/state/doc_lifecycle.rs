use super::*;

impl AppState {
    /// Returns the set of all DocumentIds that are currently visible
    /// as active tabs in ANY leaf/split (1, 4, 8, etc.).
    /// Visible documents are never evicted.
    pub fn visible_document_ids(&self) -> HashSet<DocumentId> {
        let mut visible = HashSet::new();
        for leaf in self.leaves.values() {
            if let Some(tab) = leaf.active_tab() {
                if let crate::workspace::TabContent::Document { id } = tab.content {
                    visible.insert(id);
                }
            }
        }
        visible
    }

    /// Update last access timestamp for a document in the LRU tracker.
    pub fn touch_document_accessed(&mut self, doc_id: DocumentId) {
        if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
            meta.last_accessed = Instant::now();
        }
    }

    /// Path of a document, whether currently loaded into memory or evicted.
    pub fn document_path(&self, id: DocumentId) -> Option<&Path> {
        self.documents
            .get(&id)
            .map(|d| d.path.as_path())
            .or_else(|| self.doc_meta.get(&id).map(|m| m.path.as_path()))
    }

    /// Title of a document, whether currently loaded into memory or evicted.
    pub fn document_title(&self, id: DocumentId) -> Option<&str> {
        self.documents
            .get(&id)
            .map(|d| d.title.as_str())
            .or_else(|| self.doc_meta.get(&id).map(|m| m.title.as_str()))
    }

    /// Dirty status of a document.
    pub fn is_document_dirty(&self, id: DocumentId) -> bool {
        self.documents
            .get(&id)
            .map(|d| d.is_dirty)
            .or_else(|| self.doc_meta.get(&id).map(|m| m.is_dirty))
            .unwrap_or(false)
    }

    /// Checks if a document is currently loaded in memory.
    pub fn is_document_loaded(&self, id: DocumentId) -> bool {
        self.documents.contains_key(&id)
    }

    /// Ensures that the given document is loaded in memory.
    /// If it was evicted or not yet read from disk, it is loaded on demand.
    pub fn ensure_document_loaded(&mut self, doc_id: DocumentId) -> bool {
        self.touch_document_accessed(doc_id);
        if self.documents.contains_key(&doc_id) {
            return true;
        }

        let Some(meta) = self.doc_meta.get(&doc_id) else {
            return false;
        };
        let path = meta.path.clone();
        let title = meta.title.clone();

        let buf = if path.to_string_lossy() == "Welcome.md" {
            DocumentBuffer::new(doc_id, path.clone(), title, DEFAULT_WELCOME_NOTE.to_owned(), 1)
        } else {
            match DocumentBuffer::from_file(doc_id, path.clone(), title.clone(), 1) {
                Ok(b) => b,
                Err(err) => {
                    eprintln!("Failed to read file on lazy load: {:?}: {}", path, err);
                    DocumentBuffer::new(doc_id, path.clone(), title, String::new(), 1)
                }
            }
        };

        self.documents.insert(doc_id, buf);
        let norm = Self::normalize_path(&path);
        self.by_path.insert(norm, doc_id);

        self.evict_inactive_documents_excluding(Some(doc_id));
        true
    }

    /// Evicts inactive document buffers from memory using LRU policy.
    /// Documents that are active in any visible leaf are NEVER evicted.
    /// If an inactive document is dirty, it is saved to disk before eviction.
    pub fn evict_inactive_documents_if_needed(&mut self) {
        self.evict_inactive_documents_excluding(None);
    }

    pub fn evict_inactive_documents_excluding(&mut self, excluded: Option<DocumentId>) {
        let visible = self.visible_document_ids();

        let mut inactive_candidates: Vec<DocumentId> = self
            .documents
            .keys()
            .filter(|id| !visible.contains(id) && Some(**id) != excluded)
            .copied()
            .collect();

        if inactive_candidates.is_empty() {
            return;
        }

        // Sort ascending by last_accessed: oldest accessed first
        inactive_candidates.sort_by_key(|id| {
            self.doc_meta
                .get(id)
                .map(|m| m.last_accessed)
                .unwrap_or_else(Instant::now)
        });

        let max_inactive = self.max_inactive_docs;
        let max_bytes = self.max_inactive_doc_bytes;
        let mut total_inactive_bytes: usize = inactive_candidates
            .iter()
            .filter_map(|id| self.documents.get(id))
            .map(|d| d.len())
            .sum();

        for doc_id in inactive_candidates {
            let inactive_count = self
                .documents
                .keys()
                .filter(|id| !visible.contains(id) && Some(**id) != excluded)
                .count();

            if inactive_count <= max_inactive && total_inactive_bytes <= max_bytes {
                break;
            }

            if let Some(mut doc) = self.documents.remove(&doc_id) {
                total_inactive_bytes = total_inactive_bytes.saturating_sub(doc.len());

                // If dirty, safely persist before eviction
                if doc.is_dirty {
                    if let Err(err) = crate::save_manager::safe_write_rope(&doc.path, doc.rope()) {
                        eprintln!(
                            "Failed to save dirty document before eviction: {:?}: {}",
                            doc.path, err
                        );
                    } else {
                        doc.is_dirty = false;
                    }
                }

                // Keep metadata up to date
                if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
                    meta.is_dirty = doc.is_dirty;
                    meta.title = doc.title;
                }
            }
        }
    }

    pub fn get_or_open_document(&mut self, path: &Path, title: String, content: String) -> DocumentId {
        let norm = Self::normalize_path(path);
        if let Some(&doc_id) = self.by_path.get(&norm) {
            self.ensure_document_loaded(doc_id);
            self.touch_document_accessed(doc_id);
            return doc_id;
        }
        if let Some(doc_id) = self.find_document_by_path(path) {
            self.ensure_document_loaded(doc_id);
            self.touch_document_accessed(doc_id);
            return doc_id;
        }

        let doc_id = DocumentId::new();
        let buf = DocumentBuffer::new(doc_id, path.to_path_buf(), title.clone(), content, 1);
        self.documents.insert(doc_id, buf);
        self.by_path.insert(norm, doc_id);
        self.doc_meta.insert(
            doc_id,
            DocumentMetadata {
                id: doc_id,
                path: path.to_path_buf(),
                title,
                is_dirty: false,
                last_accessed: Instant::now(),
            },
        );
        self.evict_inactive_documents_excluding(Some(doc_id));
        doc_id
    }

    pub fn get_or_open_document_from_file(&mut self, path: &Path, title: String) -> DocumentId {
        let norm = Self::normalize_path(path);
        if let Some(&doc_id) = self.by_path.get(&norm) {
            self.ensure_document_loaded(doc_id);
            self.touch_document_accessed(doc_id);
            return doc_id;
        }
        if let Some(doc_id) = self.find_document_by_path(path) {
            self.ensure_document_loaded(doc_id);
            self.touch_document_accessed(doc_id);
            return doc_id;
        }

        let doc_id = DocumentId::new();
        let buf = DocumentBuffer::from_file(doc_id, path.to_path_buf(), title.clone(), 1)
            .unwrap_or_else(|_| DocumentBuffer::new(doc_id, path.to_path_buf(), title.clone(), String::new(), 1));
        self.documents.insert(doc_id, buf);
        self.by_path.insert(norm, doc_id);
        self.doc_meta.insert(
            doc_id,
            DocumentMetadata {
                id: doc_id,
                path: path.to_path_buf(),
                title,
                is_dirty: false,
                last_accessed: Instant::now(),
            },
        );
        self.evict_inactive_documents_excluding(Some(doc_id));
        doc_id
    }

    /// Cleans up orphaned document buffers, path mappings, views, and active note
    /// after tabs have been closed, truncated, or removed.
    pub fn cleanup_orphaned_documents_and_views(&mut self) {
        // 1. Gather all active TabIds and DocumentIds across all leaves
        let mut active_tab_ids = HashSet::new();
        let mut active_doc_ids = HashSet::new();
        for leaf in self.leaves.values() {
            for tab in &leaf.tabs {
                active_tab_ids.insert(tab.id);
                if let crate::workspace::TabContent::Document { id } = &tab.content {
                    active_doc_ids.insert(*id);
                }
            }
        }

        // 2. Remove views for closed tabs
        self.views.retain(|tab_id, _| active_tab_ids.contains(tab_id));

        // 3. Remove documents no longer open in any leaf
        let mut removed_paths = Vec::new();
        self.documents.retain(|doc_id, doc| {
            if active_doc_ids.contains(doc_id) {
                true
            } else {
                removed_paths.push(doc.path.clone());
                false
            }
        });

        self.doc_meta.retain(|doc_id, meta| {
            if active_doc_ids.contains(doc_id) {
                true
            } else {
                removed_paths.push(meta.path.clone());
                false
            }
        });

        // 4. Clean up by_path and open_tabs
        for p in &removed_paths {
            let norm = Self::normalize_path(p);
            self.by_path.remove(&norm);
            self.open_tabs.retain(|x| x != p);
        }
    }

    /// Closes an entire leaf (split window) and unloads any orphaned documents and views.
    pub fn close_leaf(&mut self, leaf_id: LeafId) -> Option<LeafId> {
        if self.workspace.leaves().len() <= 1 {
            return None;
        }
        self.leaves.remove(&leaf_id);
        self.cleanup_orphaned_documents_and_views();
        let next = self.workspace.close_leaf(leaf_id);
        if let Some(na) = next {
            self.workspace.active_leaf = na;
        }
        self.invalidate_layout_caches();
        next
    }

    /// Captures a point-in-time snapshot of document memory allocations and open counts.
    pub fn memory_snapshot(&self) -> MemorySnapshot {
        let doc_bytes: usize = self.documents.values().map(|d| d.len()).sum();
        MemorySnapshot {
            document_count: self.documents.len(),
            document_bytes: doc_bytes,
            by_path_count: self.by_path.len(),
            view_count: self.views.len(),
            active_note_bytes: 0,
            open_tabs_count: self.open_tabs.len(),
        }
    }
}
