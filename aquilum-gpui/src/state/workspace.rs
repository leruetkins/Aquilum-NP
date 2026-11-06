use super::*;

impl AppState {
    pub fn open_workspace(&mut self, path: PathBuf) {
        self.open_workspace_internal(path, true);
    }

    pub fn open_workspace_without_saving(&mut self, path: PathBuf) {
        self.open_workspace_internal(path, false);
    }

    pub(super) fn open_workspace_internal(&mut self, path: PathBuf, save: bool) {
        let vault = crate::vault::VaultCore::open(path.clone());
        let name = vault.name.clone();

        self.workspace_name = name;
        self.workspace_path = Some(path.clone());
        self.session.workspace_path = Some(path.clone());
        self.vault = Some(vault);
        crate::images::set_vault_root(Some(path.clone()));
        crate::images::set_folder(&self.settings.get_config().files.folder);
        self.attach_fs_watcher();
        // Search worker follows the vault: relative-path base and body cache.
        self.search_worker.reset(Some(path.clone()));
        // Remember the vault for the switcher dialog (most recent first).
        self.known_vaults.retain(|p| p != &path);
        self.known_vaults.insert(0, path.clone());
        while self.known_vaults.len() > 20 {
            self.known_vaults.pop();
        }
        self.session.known_vaults = self.known_vaults.clone();
        self.expanded_folders.insert(path.clone());
        self.open_tabs.retain(|p| p.starts_with(&path) || p.to_string_lossy() == "Welcome.md");
        if let Some(ref empty) = self.active_empty_tab {
            if !self.open_tabs.contains(empty) {
                self.active_empty_tab = None;
            }
        }
        self.reload_file_tree();
        // Start/retarget Dropbox sync only after the watcher and the initial
        // tree are in place, so a first pull is observed like any other
        // external change.
        self.bind_sync();
        let vault_bm = path.join(".vault").join("bookmarks.json");
        let obs_bm = path.join(".obsidian").join("bookmarks.json");
        let bm_path = if vault_bm.exists() {
            Some(vault_bm)
        } else if obs_bm.exists() {
            Some(obs_bm)
        } else {
            None
        };
        if let Some(p) = bm_path {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if let Ok(data) = serde_json::from_str::<crate::views::sidebar::bookmarks::BookmarksData>(&content) {
                    self.bookmarks = data;
                }
            }
        }

        let plugin_rf = path.join(".vault").join("plugins").join("recent-files-obsidian").join("data.json");
        let vault_rf = path.join(".vault").join("recent-files.json");
        let rf_path = if plugin_rf.exists() {
            Some(plugin_rf)
        } else if vault_rf.exists() {
            Some(vault_rf)
        } else {
            None
        };
        if let Some(p) = rf_path {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if let Ok(data) = serde_json::from_str::<crate::views::sidebar::recent_files::RecentFilesData>(&content) {
                    self.recent_files = data;
                }
            }
        }
        self.status_text = format!("Workspace: {}", self.workspace_name);

        self.events.emit_workspace(&crate::events::WorkspaceEvent::VaultOpen {
            path: path.to_string_lossy().to_string(),
        });
        if save {
            self.save_session_state();
        }
        self.publish_snapshot();
    }

    /// Starts or retargets the filesystem watcher for the current workspace
    /// root. Called on workspace open/switch; when the open fails or the
    /// workspace is absent, the watcher is simply left pointing at the old
    /// root until the next successful open.
    pub(super) fn attach_fs_watcher(&mut self) {
        let Some(root) = self.workspace_path.clone() else {
            return;
        };
        if self.fs_watcher.watched_root().as_deref() == Some(root.as_path()) {
            if self.fs_events_rx.is_some() {
                return;
            }
            // The watcher is still alive but its receiver was replaced by a
            // later open of the same root; restart it on the fresh channel.
            self.fs_watcher.stop();
        }
        let (tx, rx) = std::sync::mpsc::channel();
        match self.fs_watcher.watch(&root, tx) {
            Ok(()) => self.fs_events_rx = Some(rx),
            Err(error) => {
                // Without a watcher the app still works; deletion tracking
                // falls back to nothing instead of the old 150 ms poll.
                self.fs_events_rx = None;
                self.status_text = format!("File watching unavailable: {error}");
            }
        }
    }

    /// Stops the filesystem watcher (workspace closed).
    pub fn detach_fs_watcher(&mut self) {
        self.fs_watcher.stop();
        self.fs_events_rx = None;
    }

    /// Applies a tree walked off the UI thread (see `AppView::reload_file_tree_async`).
    /// The only mutation is the assignment plus the status text.
    pub fn apply_file_tree(&mut self, tree: Vec<FileNode>) {
        self.file_tree = tree;
        crate::images::invalidate_resolution();
        self.hide_attachment_folder_in_tree();
        self.status_text = format!("Vault: {}", self.workspace_name);
        self.sync_search_worker_files();
    }

    /// The attachments folder holds storage, not notes; by default it is not
    /// part of the file explorer.
    pub(super) fn hide_attachment_folder_in_tree(&mut self) {
        let config = self.settings.get_config();
        if config.files.show_attachments_folder {
            return;
        }
        let folder = config.files.folder;
        Self::retain_without_folder(&mut self.file_tree, &folder);
    }

    pub(super) fn retain_without_folder(nodes: &mut Vec<FileNode>, folder: &str) {
        for node in nodes.iter_mut() {
            if let FileNode::Folder { children, .. } = node {
                Self::retain_without_folder(children, folder);
            }
        }
        nodes.retain(|node| {
            !matches!(node, FileNode::Folder { name, .. } if name.eq_ignore_ascii_case(folder))
        });
    }

    /// Pushes the current scan list to the search worker (up to
    /// [`MAX_SCAN_FILES`] paths, sidebar-tree order). Cheap enough to send
    /// on every tree application; the worker swaps its list wholesale.
    pub(super) fn sync_search_worker_files(&mut self) {
        let files = crate::views::goto_search::collect_goto_files(
            &self.file_tree,
            crate::views::goto_search::MAX_SCAN_FILES,
        );
        self.search_worker.set_files(files);
    }

    /// Rescan reaction: forget the cached vault file list so the next
    /// snapshot rebuilds it. The tree itself is refreshed by
    /// `AppView::reload_file_tree_async`, which walks the vault on a
    /// background thread — a synchronous `reload_file_tree` on the UI thread
    /// stalled a frame on large vaults.
    pub fn invalidate_markdown_cache_and_tree(&mut self) {
        if let Some(vault) = &self.vault {
            vault.invalidate_markdown_cache();
        }
    }

    /// After a rescan, feeds every open document path into the deletion
    /// debounce: paths that truly disappeared will confirm as `Deleted` on a
    /// later tick (after the debounce window), present ones cancel out.
    pub fn recheck_open_paths_after_rescan(&mut self) {
        for path in self.documents.values().map(|d| d.path.clone()).collect::<Vec<_>>() {
            if !path.exists() {
                self.pending_file_deletions.entry(path).or_insert_with(Instant::now);
            }
        }
        for path in self.doc_meta.values().map(|m| m.path.clone()).collect::<Vec<_>>() {
            if !self.documents.values().any(|d| d.path == path) && !path.exists() {
                self.pending_file_deletions.entry(path).or_insert_with(Instant::now);
            }
        }
    }

    /// Publishes the current immutable snapshot of the vault and active file state
    pub fn publish_snapshot(&self) {
        let active_doc = self.active_document();
        let active = active_doc.as_ref().and_then(|doc| {
            self.vault.as_ref().and_then(|v| {
                crate::vault::FileInfo::from_path(&v.root, &doc.path)
            })
        });
        let files: Arc<[crate::vault::FileInfo]> = self.vault
            .as_ref()
            .map(|v| v.list_markdown_files())
            .unwrap_or_default();

        let version = active_doc.as_ref().map(|d| d.version).unwrap_or(0);
        let language = crate::i18n::LocaleInfo::normalize(&self.settings.get_config().ui.language).to_string();
        let commands: std::sync::Arc<[CommandInfo]> = self
            .commands
            .all()
            .into_iter()
            .map(|c| CommandInfo { id: c.id, name: c.name, hotkey: c.hotkey, owner: c.owner })
            .collect();
        let active_doc_snapshot = active_doc.zip(self.active_view()).map(|(doc, view)| {
            let head = view.line_col(doc);
            let anchor = view.selection_anchor.map(|a| {
                let a = doc.clamp_offset(a);
                let anchor_line = doc.line_of_byte(a);
                let anchor_start = doc.line_start_byte(anchor_line).min(a);
                let anchor_ch = doc.slice_to_string(anchor_start..a).chars().count();
                (anchor_line, anchor_ch)
            });
            ActiveDocSnapshot { rope: doc.rope().clone(), cursor: head, anchor }
        });
        self.snapshot.store(Arc::new(Snapshot {
            version,
            language,
            active_file: active,
            files,
            active_doc: active_doc_snapshot,
            commands,
            plugins: std::sync::Arc::from(self.snapshot_plugins.load().as_ref().to_vec()),
        }));
    }

    pub fn build_tree(dir: &Path) -> Vec<FileNode> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };

        let mut folders = Vec::new();
        let mut files = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();

            if name.starts_with('.') || name == "node_modules" || name == "target" || name == ".git" {
                continue;
            }

            if path.is_dir() {
                let children = Self::build_tree(&path);
                folders.push(FileNode::Folder {
                    name,
                    path,
                    children,
                });
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case("md")
                        || ext.eq_ignore_ascii_case("markdown")
                        || ext.eq_ignore_ascii_case("txt")
                    {
                        files.push(FileNode::File { name, path });
                    }
                }
            }
        }

        folders.sort_by(|a, b| a.name().to_lowercase().cmp(&b.name().to_lowercase()));
        files.sort_by(|a, b| a.name().to_lowercase().cmp(&b.name().to_lowercase()));

        let mut all = folders;
        all.extend(files);
        all
    }

    pub fn first_file_in_tree(nodes: &[FileNode]) -> Option<PathBuf> {
        for node in nodes {
            match node {
                FileNode::File { path, .. } => return Some(path.clone()),
                FileNode::Folder { children, .. } => {
                    if let Some(p) = Self::first_file_in_tree(children) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    pub fn reload_file_tree(&mut self) {
        let Some(ws) = &self.workspace_path else {
            self.file_tree.clear();
            self.sync_search_worker_files();
            return;
        };
        self.file_tree = Self::build_tree(ws);
        crate::images::invalidate_resolution();
        self.hide_attachment_folder_in_tree();
        // Every UI path that mutates the vault (create/rename/delete from the
        // sidebar, plugin operations) converges here; the plugin file list must
        // follow or plugins keep seeing the pre-mutation set.
        if let Some(vault) = &self.vault {
            vault.invalidate_markdown_cache();
        }
        self.reload_files();
        self.sync_search_worker_files();
        self.status_text = format!("Vault: {}", self.workspace_name);
        self.publish_snapshot();
    }

    pub fn toggle_folder(&mut self, path: PathBuf) {
        if self.expanded_folders.contains(&path) {
            self.expanded_folders.remove(&path);
        } else {
            self.expanded_folders.insert(path);
        }
        self.save_session_state();
    }

    pub fn reveal_path_in_tree(&mut self, target: &Path) {
        let Some(ref ws) = self.workspace_path else { return; };
        let mut current = target.parent();
        while let Some(parent) = current {
            if parent.starts_with(ws) {
                self.expanded_folders.insert(parent.to_path_buf());
                if parent == ws {
                    break;
                }
                current = parent.parent();
            } else {
                break;
            }
        }
    }
}
