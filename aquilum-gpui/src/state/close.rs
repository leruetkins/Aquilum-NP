use super::*;

impl AppState {
    /// Request closing a tab in a leaf. Returns any effects (title commit, disk flush)
    /// that must be successfully executed before the tab can actually be closed.
    pub fn request_close_leaf_tab(&self, leaf_id: LeafId, tab_index: usize) -> Result<(TabId, crate::workspace::CloseRequest), String> {
        let leaf = self.leaves.get(&leaf_id).ok_or_else(|| "Unknown leaf".to_string())?;
        let tab = leaf.tabs.get(tab_index).ok_or_else(|| "Unknown tab index".to_string())?;
        let tab_id = tab.id;
        let mut effects = Vec::new();

        if let Some(view) = self.views.get(&tab_id) {
            if let Some(title_input) = &view.title_input {
                if let crate::workspace::TabContent::Document { id } = tab.content {
                    effects.push(crate::workspace::CloseEffect::CommitTitle {
                        tab_id,
                        doc_id: id,
                        new_title: title_input.text.clone(),
                    });
                }
            }
        }

        if let crate::workspace::TabContent::Document { id } = tab.content {
            let doc_id = id;
            let tab_count = self
                .leaves
                .values()
                .flat_map(|l| &l.tabs)
                .filter(|t| matches!(t.content, crate::workspace::TabContent::Document { id } if id == doc_id))
                .count();
            let is_last = tab_count <= 1;
            let is_dirty = self.is_document_dirty(doc_id);
            if is_last && is_dirty {
                effects.push(crate::workspace::CloseEffect::FlushDoc(doc_id));
            }
        }

        let req = if effects.is_empty() {
            crate::workspace::CloseRequest::Ready
        } else {
            crate::workspace::CloseRequest::NeedsEffects(effects)
        };
        Ok((tab_id, req))
    }

    /// Request closing a tab by its TabId.
    pub fn request_close_tab(&self, tab_id: TabId) -> Result<crate::workspace::CloseRequest, String> {
        for (leaf_id, leaf) in &self.leaves {
            if let Some(idx) = leaf.tabs.iter().position(|t| t.id == tab_id) {
                let (_, req) = self.request_close_leaf_tab(*leaf_id, idx)?;
                return Ok(req);
            }
        }
        Err("Unknown tab".to_string())
    }

    /// Executes a single close effect with real IO.
    /// If renaming or saving fails, returns Err and leaves state intact.
    pub fn run_close_effect(&mut self, effect: &crate::workspace::CloseEffect) -> Result<(), String> {
        match effect {
            crate::workspace::CloseEffect::CommitTitle { tab_id, doc_id, new_title } => {
                let (current_path, current_title) = if let Some(doc) = self.documents.get(doc_id) {
                    (doc.path.clone(), doc.title.clone())
                } else if let Some(meta) = self.doc_meta.get(doc_id) {
                    (meta.path.clone(), meta.title.clone())
                } else {
                    return Err("Document not found".to_string());
                };
                let trimmed = new_title.trim();
                if !trimmed.is_empty() && trimmed != current_title {
                    self.rename_note(&current_path, trimmed)?;
                }
                if let Some(view) = self.views.get_mut(tab_id) {
                    view.title_input = None;
                }
                Ok(())
            }
            crate::workspace::CloseEffect::FlushDoc(doc_id) => {
                if self.save_document(*doc_id) {
                    Ok(())
                } else {
                    Err("Failed to save document".to_string())
                }
            }
        }
    }

    /// Performs the final step of closing a leaf tab after all effects have been satisfied.
    pub fn finish_close_leaf_tab(&mut self, leaf_id: LeafId, tab_index: usize) -> Result<Option<PathBuf>, String> {
        let (_tab_id, req) = self.request_close_leaf_tab(leaf_id, tab_index)?;
        if let crate::workspace::CloseRequest::NeedsEffects(_) = req {
            return Err("Pending effects must be executed before finish_close".to_string());
        }
        Ok(self.close_leaf_tab_direct(leaf_id, tab_index))
    }

    /// Performs the final step of closing a tab by TabId after all effects have been satisfied.
    pub fn finish_close_tab(&mut self, tab_id: TabId) -> Result<Option<PathBuf>, String> {
        let mut target = None;
        for (leaf_id, leaf) in &self.leaves {
            if let Some(idx) = leaf.tabs.iter().position(|t| t.id == tab_id) {
                target = Some((*leaf_id, idx));
                break;
            }
        }
        let (leaf_id, idx) = target.ok_or_else(|| "Unknown tab".to_string())?;
        self.finish_close_leaf_tab(leaf_id, idx)
    }

    /// Updates the cached geometry bounds for all leaves based on the window size,
    /// sidebar widths, and current split ratios.
    pub fn update_leaf_bounds_from_layout(
        &mut self,
        win_w: f32,
        win_h: f32,
        left_sidebar_w: f32,
        right_sidebar_w: f32,
    ) {
        let workspace_w = (win_w - left_sidebar_w - right_sidebar_w).max(100.0);
        let workspace_h = (win_h - 66.0).max(100.0);
        let leaf_ids: Vec<LeafId> = self.leaves.keys().copied().collect();
        for lid in leaf_ids {
            let w_ratio = self.workspace.horizontal_width_ratio_of_leaf(lid);
            let h_ratio = self.workspace.vertical_height_ratio_of_leaf(lid);
            let w = (workspace_w * w_ratio).max(60.0);
            let h = (workspace_h * h_ratio).max(50.0);
            if let Some(leaf) = self.leaves.get_mut(&lid) {
                leaf.set_bounds(w, h);
            }
        }
    }

    /// Invalidates geometry and viewport caches across all leaves and views
    /// after layout structure changes (e.g. split or leaf close).
    pub fn invalidate_layout_caches(&mut self) {
        for leaf in self.leaves.values_mut() {
            leaf.bounds = None;
        }
        for view in self.views.values_mut() {
            view.measured_viewport_h = None;
        }
    }

    /// Selects a tab within a leaf by TabId, switching only pointers.
    pub fn select_tab(&mut self, leaf_id: LeafId, tab_id: TabId) -> Result<(), String> {
        let leaf = self.leaves.get_mut(&leaf_id).ok_or_else(|| "Unknown leaf".to_string())?;
        let pos = leaf.tabs.iter().position(|t| t.id == tab_id).ok_or_else(|| "Unknown tab".to_string())?;
        leaf.active_tab_index = pos;
        self.workspace.active_leaf = leaf_id;
        Ok(())
    }

    pub fn close_leaf_tab(&mut self, leaf_id: LeafId, tab_index: usize) -> Option<PathBuf> {
        if let Ok((_tab_id, req)) = self.request_close_leaf_tab(leaf_id, tab_index) {
            if let crate::workspace::CloseRequest::NeedsEffects(effects) = req {
                for eff in effects {
                    if let Err(e) = self.run_close_effect(&eff) {
                        self.status_text = format!("Close cancelled: {}", e);
                        return None;
                    }
                }
            }
        }
        self.close_leaf_tab_direct(leaf_id, tab_index)
    }

    pub fn close_leaf_tab_direct(&mut self, leaf_id: LeafId, tab_index: usize) -> Option<PathBuf> {
        let (removed_tab, leaf_is_empty) = if let Some(leaf) = self.leaves.get_mut(&leaf_id) {
            if tab_index < leaf.tabs.len() {
                let tab = leaf.tabs.remove(tab_index);
                self.views.remove(&tab.id);
                let empty = leaf.tabs.is_empty();
                if !empty {
                    if tab_index < leaf.active_tab_index {
                        leaf.active_tab_index -= 1;
                    } else if leaf.active_tab_index >= leaf.tabs.len() {
                        leaf.active_tab_index = leaf.tabs.len() - 1;
                    }
                }
                (Some(tab), empty)
            } else {
                (None, false)
            }
        } else {
            (None, false)
        };

        let Some(tab) = removed_tab else {
            return None;
        };

        let closed_doc_path = match &tab.content {
            crate::workspace::TabContent::Document { id } => {
                let doc_id = *id;
                let still_open = self.leaves.values().any(|leaf| {
                    leaf.tabs.iter().any(|t| match &t.content {
                        crate::workspace::TabContent::Document { id: other_id } => other_id == &doc_id,
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
                        self.open_tabs.retain(|p| p != &doc_path);
                        Some(doc_path)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            crate::workspace::TabContent::Empty => None,
            crate::workspace::TabContent::Plugin { .. } => None,
        };

        if leaf_is_empty {
            if self.workspace.leaves().len() > 1 {
                self.workspace.close_leaf(leaf_id);
                self.leaves.remove(&leaf_id);
                if self.workspace.active_leaf == leaf_id {
                    if let Some(&first) = self.workspace.leaves().first() {
                        self.workspace.active_leaf = first;
                    }
                }
                self.invalidate_layout_caches();
            } else if let Some(leaf) = self.leaves.get_mut(&leaf_id) {
                leaf.tabs.push(crate::workspace::Tab::new_empty());
                leaf.active_tab_index = 0;
            }
        }

        self.cleanup_orphaned_documents_and_views();

        closed_doc_path
    }
}
