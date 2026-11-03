use super::*;

impl AppView {
    pub(crate) fn open_folder_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let mut dialog = rfd::AsyncFileDialog::new();
        if let Some(ref root) = self.state.workspace_path {
            dialog = dialog.set_directory(root);
        }
        cx.foreground_executor().spawn(async move {
            if let Some(folder_handle) = dialog.pick_folder().await {
                let folder = folder_handle.path().to_path_buf();
                let _ = entity.update(&mut async_cx, |this, cx| {
                    this.unload_all_plugins();
                    this.selected_sidebar_folder = None;
                    this.state.open_workspace(folder);
                    this.load_installed_plugins_for_vault();
                    if this.state.active_document().is_none() {
                        if let Some(first_file) = AppState::first_file_in_tree(&this.state.file_tree) {
                            this.state.open_note(first_file);
                        }
                    }
                    this.need_scroll_sidebar_to_active.set(true);
                    cx.notify();
                });
            }
        }).detach();
    }

    /// Opens the native folder picker asynchronously without blocking the GPUI update loop.
    pub(crate) fn open_folder_dialog_deferred(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_folder_dialog(window, cx);
    }

    /// Asynchronously exports the active note (or note at target_path) to PDF using Typst.
    pub(crate) fn export_note_to_pdf(&mut self, target_path: Option<PathBuf>, cx: &mut Context<Self>) {
        let (title, content, initial_dir, note_path) = if let Some(ref path) = target_path {
            let title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Note".to_string());
            let content = if let Some(doc) = self.state.active_document().filter(|d| &d.path == path) {
                doc.content().to_string()
            } else if let Some(id) = self.state.find_document_by_path(path) {
                self.state.documents.get(&id).map(|d| d.content().to_string()).unwrap_or_else(|| {
                    std::fs::read_to_string(path).unwrap_or_default()
                })
            } else {
                std::fs::read_to_string(path).unwrap_or_default()
            };
            let dir = path.parent().map(|p| p.to_path_buf());
            (title, content, dir, Some(path.clone()))
        } else if let Some(doc) = self.state.active_document() {
            let title = doc.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Note".to_string());
            let content = doc.content().to_string();
            let dir = doc.path.parent().map(|p| p.to_path_buf());
            (title, content, dir, Some(doc.path.clone()))
        } else {
            eprintln!("[PDF Export] ERROR: No active document or target path found to export.");
            return;
        };

        println!("[PDF Export] Starting export for note '{}' (content length: {} bytes)", title, content.len());

        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let mut dialog = rfd::AsyncFileDialog::new()
            .add_filter("PDF Document (*.pdf)", &["pdf"])
            .set_file_name(&format!("{}.pdf", title));
        if let Some(ref dir) = initial_dir {
            dialog = dialog.set_directory(dir);
        } else if let Some(ref ws) = self.state.workspace_path {
            dialog = dialog.set_directory(ws);
        }

        let headings_config = self.config.editor.headings.clone();
        let vault_root = self.state.workspace_path.clone();
        let folder = self.config.files.folder.clone();
        let caption_template = self.config.images.caption_template.clone();
        let default_radius = self.config.images.corner_radius as f32;
        let bg = cx.background_executor().clone();
        cx.foreground_executor().spawn(async move {
            println!("[PDF Export] Native file save dialog opened...");
            let file_handle_opt = dialog.save_file().await;
            if let Some(file_handle) = file_handle_opt {
                let raw_path = file_handle.path().to_path_buf();
                println!("[PDF Export] Dialog returned selection: {:?}", raw_path);

                let mut out_path = raw_path;
                if out_path.is_dir() {
                    let default_filename = format!("{}.pdf", title);
                    out_path = out_path.join(&default_filename);
                    println!("[PDF Export] Selected path is a directory; appended default filename -> {:?}", out_path);
                }
                if out_path.extension().map(|ext| ext.to_string_lossy().to_lowercase() != "pdf").unwrap_or(true) {
                    out_path.set_extension("pdf");
                    println!("[PDF Export] Appended .pdf extension -> {:?}", out_path);
                }

                let title_clone = title.clone();
                let headings_clone = headings_config.clone();
                let images = note_path
                    .as_deref()
                    .map(|path| {
                        crate::pdf_export::build_pdf_images(
                            path,
                            &content,
                            vault_root.as_deref(),
                            &folder,
                            &caption_template,
                            default_radius,
                        )
                    })
                    .unwrap_or_default();
                let pdf_res = bg.spawn(async move {
                    crate::pdf_export::export_markdown_to_pdf_with_images(
                        &title_clone,
                        &content,
                        Some(&headings_clone),
                        images,
                    )
                }).await;

                let _ = entity.update(&mut async_cx, |this, cx| {
                    match pdf_res {
                        Ok(bytes) => {
                            match std::fs::write(&out_path, &bytes) {
                                Ok(()) => {
                                    println!("[PDF Export] SUCCESS: Wrote {} bytes to {:?}", bytes.len(), out_path);
                                    let is_ru = this.config.ui.language.starts_with("ru");
                                    let fname = out_path.file_name().unwrap_or_default().to_string_lossy();
                                    this.state.status_text = if is_ru {
                                        format!("PDF сохранён: {}", fname)
                                    } else {
                                        format!("PDF saved: {}", fname)
                                    };
                                }
                                Err(err) => {
                                    eprintln!("[PDF Export] ERROR: Failed to write PDF to {:?}: {}", out_path, err);
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("[PDF Export] ERROR: Typst compilation failed: {}", err);
                        }
                    }
                    cx.notify();
                });
            } else {
                println!("[PDF Export] Save cancelled by user (dialog closed without selection).");
            }
        }).detach();
    }

    pub(crate) fn create_new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.create_new_note_placed(window, cx, false);
    }

    /// Same as [`create_new_note`], but the new tab opens first in the tab
    /// strip (used by the leading new-note button).
    #[allow(dead_code)]
    pub(crate) fn create_new_note_at_front(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.create_new_note_placed(window, cx, true);
    }

    pub(super) fn create_new_note_placed(&mut self, window: &mut Window, cx: &mut Context<Self>, at_front: bool) {
        self.commit_title_rename_internal();
        self.flush_active_note_now(cx);
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap, self.source_mode);
        let created = if at_front {
            self.state.create_new_note_at_front()
        } else {
            self.state.create_new_note()
        };
        if let Some(_) = created {
            self.state.active_empty_tab = None;
            self.edit_mode = true;
            self.word_wrap = true;
            self.set_editor_scroll_top(0.0);
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            self.reset_editor_cursor_blink(cx);
            cx.notify();
        }
    }

    /// Activates the tab strip tab for `path` (shared by the tab body and the
    /// invisible gap extenders around it, so every pixel opens a tab).
    #[allow(dead_code)]
    pub(crate) fn select_tab_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.active_document().map(|d| &d.path != &path).unwrap_or(true) {
            self.select_note(path, true, window, cx);
        } else {
            self.need_scroll_sidebar_to_active.set(true);
            self.state.reveal_path_in_tree(&path);
            cx.notify();
        }
    }

    pub(crate) fn select_note(&mut self, path: PathBuf, center_in_sidebar: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_sidebar_folder = None;
        // A match list belongs to the note it was scanned from.
        if self.find_bar.is_open {
            self.close_find_bar(window, cx);
        }
        // Empty-tab sentinels carry no file: activate the New Tab page instead.
        if crate::state::is_empty_tab_path(&path) {
            self.select_empty_tab(path, center_in_sidebar, window, cx);
            return;
        }
        self.commit_title_rename_internal();
        self.flush_active_note_now(cx);
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap, self.source_mode);
        let sidebar_offset_y: f32 = self.sidebar_scroll_handle.offset().y.into();
        self.state.session.sidebar_scroll_top = sidebar_offset_y;
        self.state.open_note(path.clone());
        self.save_recent_files();
        self.active_center_leaf = None;
        let rel_str = if let Some(ref vault) = self.state.vault {
            path.strip_prefix(&vault.root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
        } else {
            path.to_string_lossy().replace('\\', "/")
        };
        self.state.plugin_host.broadcast_event(&crate::plugins::api::HostEvent::FileOpen {
            path: Some(rel_str),
        });
        self.apply_note_settings_for_path(&path);
        if center_in_sidebar {
            self.need_scroll_sidebar_to_active.set(true);
        }
        window.focus(&self.focus_handle);
        self.reset_editor_cursor_blink(cx);
        cx.notify();
    }

    /// Selects a folder in the sidebar file tree, expanding its parent folders and scrolling to it.
    pub(crate) fn select_sidebar_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
        self.sidebar_collapsed = false;
        self.selected_sidebar_folder = Some(path.clone());
        if let Some(ref ws) = self.state.workspace_path {
            if &path == ws {
                self.sidebar_scroll_handle.set_offset(gpui::Point::default());
                self.state.session.sidebar_scroll_top = 0.0;
            } else {
                self.state.reveal_path_in_tree(&path);
                self.state.expanded_folders.insert(path);
                self.state.save_session_state();
                self.need_scroll_sidebar_to_active.set(true);
            }
        }
        cx.notify();
    }

    /// Recursively collects all folder paths from a slice of FileNodes.
    pub(crate) fn collect_all_folder_paths(nodes: &[crate::state::FileNode], out: &mut Vec<PathBuf>) {
        for node in nodes {
            if let crate::state::FileNode::Folder { path, children, .. } = node {
                out.push(path.clone());
                Self::collect_all_folder_paths(children, out);
            }
        }
    }

    /// Creates a new folder in the currently selected directory (or active document parent / workspace root).
    pub(crate) fn create_new_folder(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.state.workspace_path.clone() else { return; };
        let target_dir = if let Some(ref sel) = self.selected_sidebar_folder {
            if sel.is_dir() {
                sel.clone()
            } else {
                sel.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| ws.clone())
            }
        } else if let Some(doc) = self.state.active_document() {
            doc.path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| ws.clone())
        } else {
            ws.clone()
        };

        let target_dir = if target_dir.starts_with(&ws) && target_dir.is_dir() {
            target_dir
        } else {
            ws.clone()
        };

        if let Ok(new_folder) = crate::views::sidebar::folder_actions::create_folder_in_folder(&target_dir) {
            self.state.reveal_path_in_tree(&new_folder);
            self.state.expanded_folders.insert(target_dir);
            self.state.expanded_folders.insert(new_folder.clone());
            self.state.reload_file_tree();
            self.select_sidebar_folder(new_folder, cx);
        }
    }

    /// Toggles expanding all folders vs collapsing all folders in the sidebar file tree.
    pub(crate) fn toggle_expand_all_folders(&mut self, cx: &mut Context<Self>) {
        if self.state.expanded_folders.is_empty() {
            let mut all = Vec::new();
            Self::collect_all_folder_paths(&self.state.file_tree, &mut all);
            for p in all {
                self.state.expanded_folders.insert(p);
            }
        } else {
            self.state.expanded_folders.clear();
        }
        self.state.save_session_state();
        cx.notify();
    }
}
