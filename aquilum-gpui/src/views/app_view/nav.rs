use super::*;

impl AppView {
    pub(crate) fn handle_link_click(&mut self, raw_url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let clean = crate::markdown::clean_link_url(raw_url);
        if clean.is_empty() {
            return;
        }

        // Web and mail links open externally
        if clean.starts_with("http://") || clean.starts_with("https://") || clean.starts_with("mailto:") {
            cx.open_url(&clean);
            return;
        }

        // Document / local file link
        if let Some(target_path) = self.resolve_document_link(&clean) {
            self.select_note(target_path, true, window, cx);
        } else {
            // Fallback: try opening via system open_url
            cx.open_url(&clean);
        }
    }

    pub(crate) fn resolve_document_link(&self, target: &str) -> Option<PathBuf> {
        let path_part = target.split('#').next()?.trim();
        if path_part.is_empty() {
            return None;
        }

        let decoded = crate::markdown::percent_decode(path_part);
        let cleaned_path = decoded.strip_prefix("file://").unwrap_or(&decoded);
        let rel_path = Path::new(cleaned_path);

        // 1. If absolute path:
        if rel_path.is_absolute() {
            if rel_path.is_file() {
                return Some(rel_path.to_path_buf());
            }
            let with_md = rel_path.with_extension("md");
            if with_md.is_file() {
                return Some(with_md);
            }
        }

        // 2. Resolve relative to active note directory:
        if let Some(active_doc) = self.state.active_document() {
            if let Some(parent) = active_doc.path.parent() {
                let candidate = parent.join(rel_path);
                if candidate.is_file() {
                    return Some(candidate);
                }
                if candidate.extension().is_none() {
                    let with_md = candidate.with_extension("md");
                    if with_md.is_file() {
                        return Some(with_md);
                    }
                }
            }
        }

        // 3. Resolve relative to vault root:
        if let Some(ref vault) = self.state.vault {
            let candidate = vault.root.join(rel_path);
            if candidate.is_file() {
                return Some(candidate);
            }
            if candidate.extension().is_none() {
                let with_md = candidate.with_extension("md");
                if with_md.is_file() {
                    return Some(with_md);
                }
            }

            // 4. Search in vault by filename or stem (Obsidian-style shortest link match)
            let file_name = rel_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let stem = rel_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if !stem.is_empty() {
                let mut matched = Vec::new();
                Self::collect_matching_notes_in_vault(&vault.root, stem, file_name, &mut matched);
                if let Some(p) = matched.into_iter().next() {
                    return Some(p);
                }
            }
        }

        // 5. Fallback: return joined path with .md extension so clicking creates/opens the new note
        if let Some(active_doc) = self.state.active_document() {
            if let Some(parent) = active_doc.path.parent() {
                let mut candidate = parent.join(rel_path);
                if candidate.extension().is_none() {
                    candidate.set_extension("md");
                }
                return Some(candidate);
            }
        }
        if let Some(ref vault) = self.state.vault {
            let mut candidate = vault.root.join(rel_path);
            if candidate.extension().is_none() {
                candidate.set_extension("md");
            }
            return Some(candidate);
        }

        None
    }

    pub(super) fn collect_matching_notes_in_vault(dir: &Path, stem: &str, file_name: &str, acc: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            if ft.is_dir() {
                Self::collect_matching_notes_in_vault(&p, stem, file_name, acc);
            } else if ft.is_file() {
                if let Some(f_name) = p.file_name().and_then(|n| n.to_str()) {
                    if f_name.eq_ignore_ascii_case(file_name) {
                        acc.push(p);
                        return;
                    }
                }
                if let Some(f_stem) = p.file_stem().and_then(|s| s.to_str()) {
                    if f_stem.eq_ignore_ascii_case(stem) && p.extension().map_or(false, |ext| ext == "md") {
                        acc.push(p);
                        return;
                    }
                }
            }
        }
    }

    pub(crate) fn close_tab(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.state.active_document().map(|d| d.path == path).unwrap_or(false) {
            self.commit_title_rename_internal();
        }
        if let Some(doc_id) = self.state.find_document_by_path(path) {
            self.flush_document_now(doc_id, cx);
        }
        let is_closing_active = self.state.active_document().map(|d| d.path == path).unwrap_or(false);
        if is_closing_active {
            let current_scroll = self.current_scroll_top();
            self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap, self.source_mode);
        }
        self.state.close_tab(path);
        if let Some((target_path, _title)) = self.state.active_document().map(|d| (d.path.clone(), d.title.clone())) {
            self.apply_note_settings_for_path(&target_path);
            self.need_scroll_sidebar_to_active.set(true);
        }
        cx.notify();
    }

    pub(super) fn navigate_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        self.flush_active_note_now(cx);
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap, self.source_mode);
        if let Some(target) = self.state.navigate_back() {
            self.apply_note_settings_for_path(&target);
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    pub(super) fn navigate_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        self.flush_active_note_now(cx);
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap, self.source_mode);
        if let Some(target) = self.state.navigate_forward() {
            self.apply_note_settings_for_path(&target);
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    pub(super) fn reload_files(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reload_file_tree_async(cx);
    }

    /// Walks the vault on a background thread and swaps the finished tree in
    /// on the UI thread (`AppState::apply_file_tree`).
    ///
    /// `build_tree` is a recursive `read_dir` over the whole vault; on a large
    /// vault a synchronous call stalls a frame, so everything that does not
    /// need the result *inside the same click handler* (watcher deletions,
    /// rescans) goes through this. Handlers that immediately open/navigate a
    /// path they just created keep the synchronous `reload_file_tree`.
    pub fn reload_file_tree_async(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.state.workspace_path.clone() else {
            return;
        };
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();
        cx.foreground_executor().spawn(async move {
            let tree = background
                .spawn(async move { AppState::build_tree(&ws) })
                .await;
            let _ = entity.update(&mut async_cx, |this, cx| {
                this.state.apply_file_tree(tree);
                this.state.publish_snapshot();
                cx.notify();
            });
        }).detach();
    }

    pub(crate) fn save_note(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.flush_active_note_now(cx);
        cx.notify();
    }
}
