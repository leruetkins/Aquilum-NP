use super::*;

impl AppView {
    /// Publishes the registered plugin manifests into the snapshot so `app.plugins`
    /// in every plugin runtime sees the current registry without a protocol call.
    pub fn refresh_plugin_registry_snapshot(&mut self) {
        let list: Vec<crate::state::PluginInfo> = self
            .plugin_runtimes
            .iter()
            .map(|h| crate::state::PluginInfo {
                id: h.manifest.id.clone(),
                name: h.manifest.name.clone(),
                version: h.manifest.version.clone(),
                description: h.manifest.description.clone(),
            })
            .collect();
        self.state
            .snapshot_plugins
            .store(std::sync::Arc::new(std::sync::Arc::from(list)));
        self.state.publish_snapshot();
    }

    /// Discovers and spawns all installed plugins located in `<vault>/.vault/plugins/`.
    pub fn load_installed_plugins_for_vault(&mut self) {
        self.unload_all_plugins();
        crate::plugins::editor_toolbar::sync_editor_toolbar_with_vault(&self.state, self.state.vault.as_ref());

        let Some(api_tx) = self.plugin_api_tx.clone() else {
            return;
        };
        let Some(vault) = &self.state.vault else {
            return;
        };
        // If safe mode is enabled, skip loading all community plugins.
        if self.config.community_plugins.safe_mode {
            return;
        }
        let plugins_dir = vault.root.join(".vault").join("plugins");
        let enabled = vault.load_community_plugins();
        let handles = crate::plugins::loader::load_installed_plugins(
            &plugins_dir,
            Some(&enabled),
            api_tx,
            self.state.snapshot.clone(),
            &self.state.plugin_host,
        );
        self.plugin_runtimes = handles;
        // app.plugins reads the registry from the snapshot: publish the freshly
        // loaded manifests before any plugin code runs against it.
        self.refresh_plugin_registry_snapshot();

        // TEMPORARILY (detach the plugin window to check the layout without clicks):
        // AQUILUM_DEBUG_PLUGIN_CMD="<plugin-id>:<command-id>"
        if let Ok(spec) = std::env::var("AQUILUM_DEBUG_PLUGIN_CMD") {
            if let Some((plugin_id, command_id)) = spec.split_once(':') {
                if let Some(handle) = self.plugin_runtimes.iter().find(|h| h.manifest.id == plugin_id) {
                    let tx = handle.host_events_tx.clone();
                    let command_id = command_id.to_string();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        let _ = tx.unbounded_send(crate::plugins::api::HostEvent::ExecuteCommand { command_id });
                    });
                }
            }
        }
    }

    /// Gracefully unloads all active plugins by sending HostEvent::Unload and clearing registrations.
    pub fn unload_all_plugins(&mut self) {
        for handle in &self.plugin_runtimes {
            let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
            self.state.plugin_host.unregister_plugin(&handle.manifest.id);
            self.state.commands.unregister_all_of(&handle.manifest.id);
            self.state.ribbon.unregister_all_of(&handle.manifest.id);
            self.state.status_bar.unregister_all_of(&handle.manifest.id);
            self.state.notifications.unregister_all_of(&handle.manifest.id);
            self.state.toolbars.remove_all_of(&handle.manifest.id);
        }
        self.state.code_block_processors.clear();
        self.code_block_views.clear();
        self.plugin_runtimes.clear();
        self.plugin_views.clear();
        self.plugin_leaf_views.clear();
        self.refresh_plugin_registry_snapshot();
        self.active_right_leaf = None;
        self.active_left_leaf = None;
        self.active_center_leaf = None;
        if matches!(self.sidebar_mode, crate::views::sidebar::SidebarMode::Plugin(_)) {
            self.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
        }
        self.active_plugin_input = None;
        self.active_plugin_select = None;
    }

    /// Completely unloads a single plugin and cleans up all its UI components, panels, tabs, buttons, and resources.
    pub fn unload_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        println!("[app_view] Unloading plugin '{}'...", plugin_id);

        // 1. Send Unload event to the plugin worker thread and drop the runtime handle
        if let Some(pos) = self.plugin_runtimes.iter().position(|r| r.manifest.id == plugin_id) {
            let handle = self.plugin_runtimes.remove(pos);
            let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
        }

        // 2. Unregister from plugin host dispatcher
        self.state.plugin_host.unregister_plugin(plugin_id);

        // 3. Remove all commands registered by this plugin
        self.state.commands.unregister_all_of(plugin_id);

        // 4. Remove all ribbon icons/actions registered by this plugin
        self.state.ribbon.unregister_all_of(plugin_id);

        // 5. Remove all status bar items registered by this plugin
        self.state.status_bar.unregister_all_of(plugin_id);

        // 6. Remove all toolbars registered by this plugin
        self.state.toolbars.remove_all_of(plugin_id);
        self.state.notifications.unregister_all_of(plugin_id);

        // 6. Remove all code block processors and views registered by this plugin
        self.state.code_block_processors.unregister_all_of(plugin_id);
        self.code_block_views.retain(|_, (pid, _)| pid != plugin_id);

        // 7. Remove all workspace leaves (sidebar panels / center tabs) registered by this plugin
        let removed_leaf_ids: Vec<String> = self
            .plugin_leaf_views
            .iter()
            .filter(|(_, leaf)| leaf.plugin_id == plugin_id)
            .map(|(id, _)| id.clone())
            .collect();

        for leaf_id in &removed_leaf_ids {
            println!("[app_view] Removing plugin leaf '{}' for plugin '{}'", leaf_id, plugin_id);
            self.plugin_leaf_views.remove(leaf_id);
        }

        // 8. Update active sidebar leaves and modes if the active leaf belonged to this plugin
        if let Some(ref right_id) = self.active_right_leaf {
            if removed_leaf_ids.contains(right_id) {
                self.active_right_leaf = self
                    .plugin_leaf_views
                    .values()
                    .find(|l| l.location == "right")
                    .map(|l| l.leaf_id.clone());
            }
        }

        if let Some(ref left_id) = self.active_left_leaf {
            if removed_leaf_ids.contains(left_id) {
                self.active_left_leaf = self
                    .plugin_leaf_views
                    .values()
                    .find(|l| l.location == "left")
                    .map(|l| l.leaf_id.clone());
            }
        }

        // If sidebar is currently displaying a plugin leaf that was removed, switch back to Files
        if let crate::views::sidebar::SidebarMode::Plugin(ref current_leaf_id) = self.sidebar_mode {
            if removed_leaf_ids.contains(current_leaf_id) {
                if let Some(ref next_left) = self.active_left_leaf {
                    self.sidebar_mode = crate::views::sidebar::SidebarMode::Plugin(next_left.clone());
                } else {
                    self.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
                }
            }
        }

        if let Some(ref center_id) = self.active_center_leaf {
            if removed_leaf_ids.contains(center_id) {
                self.active_center_leaf = None;
            }
        }

        // 9. Remove any custom view registrations and modals belonging to this plugin
        self.plugin_views.retain(|(pid, _), _| pid != plugin_id);
        if let Some(ref active_input) = self.active_plugin_input {
            if active_input.plugin_id == plugin_id {
                self.active_plugin_input = None;
            }
        }
        if let Some((ref pid, _, _)) = self.active_plugin_select {
            if pid == plugin_id {
                self.active_plugin_select = None;
                self.plugin_select_popup = None;
            }
        }

        println!("[app_view] Plugin '{}' completely unloaded and UI cleaned up.", plugin_id);
        cx.notify();
    }

    pub fn editor_font_family(&self) -> gpui::SharedString {
        let f = self.config.editor.font.font_family.trim();
        if f.is_empty() {
            "Roboto Mono".into()
        } else {
            f.to_string().into()
        }
    }

    pub fn editor_typography(&self) -> crate::settings::EditorTypography {
        crate::settings::EditorTypography::from_editor(&self.config.editor)
    }

    /// Advance of one editor cell at the configured font size.
    ///
    /// Every path that maps between columns and pixels must measure this the
    /// same way, or the caret and the painted glyphs drift apart.
    pub fn editor_char_width(&self, cx: &gpui::App) -> f32 {
        let font_size = self.editor_typography().base_font_size;
        let font_id = cx
            .text_system()
            .resolve_font(&gpui::font(self.editor_font_family()));
        cx.text_system()
            .advance(font_id, gpui::px(font_size), 'x')
            .map(|size| f32::from(size.width))
            .unwrap_or(font_size * 7.7 / crate::settings::REFERENCE_FONT_SIZE)
    }

    pub fn hex_selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.hex_selection_anchor {
            let pos = self.hex_cursor_pos;
            if anchor != pos {
                return Some((anchor.min(pos), anchor.max(pos)));
            }
        }
        None
    }

    pub fn reset_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.pause_and_show(BlinkTarget::Hex, cx);
    }

    pub fn enable_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.enable(BlinkTarget::Hex, cx);
    }

    pub fn disable_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.disable(cx);
    }

    pub fn title_selection_range(&self) -> Option<(usize, usize)> {
        self.state
            .active_view()
            .and_then(|v| v.title_input.as_ref())
            .and_then(|ti| ti.selection_range())
    }

    pub fn reset_title_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.title_blink.pause_and_show(BlinkTarget::Title, cx);
    }

    pub fn enable_title_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.title_blink.enable(BlinkTarget::Title, cx);
    }

    pub fn disable_title_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.title_blink.disable(cx);
    }

    pub fn reset_editor_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.editor_blink.pause_and_show(BlinkTarget::Editor, cx);
    }

    pub fn enable_editor_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.editor_blink.enable(BlinkTarget::Editor, cx);
    }

    pub fn disable_editor_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.editor_blink.disable(cx);
    }

    /// Freezes every focused caret visible and kills its blink timer. Used when the OS
    /// window loses activation: the caret stays on screen, but the view stops re-rendering.
    pub(super) fn suspend_cursor_blinks(&mut self, cx: &mut Context<Self>) {
        if self.editor_is_focused {
            self.editor_blink.suspend_visible(cx);
        }
        if self.was_title_focused {
            self.title_blink.suspend_visible(cx);
        }
        if self.hex_is_focused {
            self.hex_blink.suspend_visible(cx);
        }
        if self.quick_filter.is_focused {
            self.quick_filter.blink.suspend_visible(cx);
        }
        if self.search_dialog.is_open {
            self.goto_blink.suspend_visible(cx);
        }
        if self.sidebar_mode == crate::views::sidebar::SidebarMode::Search {
            self.sidebar_search.blink.suspend_visible(cx);
        }
        if self.command_palette.is_open {
            self.command_palette_blink.suspend_visible(cx);
        }
        if self.plugin_catalog.is_open {
            self.catalog_blink.suspend_visible(cx);
        }
        if self.active_plugin_input.is_some() {
            self.plugin_input_blink.suspend_visible(cx);
        }
        if self.bookmark_rename.is_some() {
            self.bookmark_rename_blink.suspend_visible(cx);
        }
    }

    /// Re-arms blinking for whichever caret holds in-app focus. Used when the OS window
    /// becomes active again.
    pub(super) fn resume_cursor_blinks(&mut self, cx: &mut Context<Self>) {
        if self.editor_is_focused {
            self.editor_blink.enable(BlinkTarget::Editor, cx);
        }
        if self.was_title_focused {
            self.title_blink.enable(BlinkTarget::Title, cx);
        }
        if self.hex_is_focused {
            self.hex_blink.enable(BlinkTarget::Hex, cx);
        }
        if self.quick_filter.is_focused {
            self.quick_filter.enable_cursor_blink(cx);
        }
        if self.search_dialog.is_open {
            self.goto_blink.enable(BlinkTarget::GotoSearch, cx);
        }
        if self.sidebar_mode == crate::views::sidebar::SidebarMode::Search {
            self.sidebar_search.blink.enable(BlinkTarget::SidebarSearch, cx);
        }
        if self.command_palette.is_open {
            self.command_palette_blink.enable(BlinkTarget::CommandPalette, cx);
        }
        if self.plugin_catalog.is_open {
            self.catalog_blink.enable(BlinkTarget::PluginCatalog, cx);
        }
        if self.active_plugin_input.is_some() {
            self.plugin_input_blink.enable(BlinkTarget::PluginInput, cx);
        }
        if self.bookmark_rename.is_some() {
            self.bookmark_rename_blink
                .enable(BlinkTarget::SidebarRename, cx);
        }
    }

    /// Immediately dispatches writing of a dirty document to disk in the background,
    /// without blocking the UI thread.
    pub fn flush_document_now(&mut self, doc_id: crate::document_buffer::DocumentId, cx: &mut Context<Self>) {
        let to_save = match self.state.documents.get(&doc_id) {
            Some(doc) if doc.is_dirty => {
                Some((doc.path.clone(), doc.rope.clone(), doc.title.clone(), doc.version, doc.disk_hash.clone()))
            }
            _ => None,
        };

        let Some((path, rope, _title, version, expected_hash)) = to_save else {
            return;
        };
        let saved_refs = crate::images::references_in_rope(&rope);

        self.auto_save_epoch = self.auto_save_epoch.wrapping_add(1);
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();

        cx.foreground_executor().spawn(async move {
            let write_path = path.clone();
            let write_res = background.spawn(async move {
                crate::save_manager::safe_write_rope_checked(&write_path, &rope, expected_hash.as_deref(), version)
            }).await;

            let _ = entity.update(&mut async_cx, |this, cx| {
                if let Some(doc_id) = this.state.find_document_by_path(&path) {
                    let mut notif_to_push = None;
                    let mut reconcile: Option<(PathBuf, std::collections::HashSet<String>)> = None;
                    if let Some(doc) = this.state.documents.get_mut(&doc_id) {
                        match write_res {
                            crate::save_manager::CheckedWrite::Written(hash) => {
                                // The hash/stamp update must happen even when
                                // the user kept typing while the write ran
                                // (`doc.version != version`): the disk now
                                // holds exactly `hash`, and a watcher `Modified`
                                // event for our own write must be recognised
                                // as an echo instead of triggering a reload —
                                // `reload` drops the undo stack and the
                                // caret/selection visibly jump.
                                doc.disk_hash = Some(hash);
                                doc.disk_stamp = crate::save_manager::file_stamp(&doc.path);
                                if doc.version == version {
                                    doc.mark_clean();
                                    if let Some(meta) = this.state.doc_meta.get_mut(&doc_id) {
                                        meta.is_dirty = false;
                                    }
                                    let previous = doc.attachment_refs.replace(saved_refs.clone());
                                    reconcile = Some((doc.path.clone(), previous));
                                }
                                let doc_path = doc.path.clone();
                                let rel_path = this.state.vault.as_ref()
                                    .and_then(|v| doc_path.strip_prefix(&v.root).ok())
                                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                                    .unwrap_or_else(|| doc_path.to_string_lossy().into_owned());
                                this.state.events.emit_vault(&crate::events::VaultEvent::Modify { path: rel_path });
                                this.state.publish_snapshot();
                            }
                            crate::save_manager::CheckedWrite::Conflict { copy, disk_text } => {
                                // Two-sided conflict protocol: our version is
                                // preserved in the copy, the buffer adopts the
                                // external content — no data loss either way.
                                doc.reload(disk_text);
                                for view in this.state.views.values_mut() {
                                    if view.doc_id == doc_id {
                                        view.visual_cache_dirty = true;
                                    }
                                }
                                let copy_name = copy.file_name().unwrap_or_default().to_string_lossy().into_owned();
                                notif_to_push = Some((
                                    crate::views::notification::NotificationType::Warning,
                                    Some("Conflict".into()),
                                    format!(
                                        "Файл изменён извне; ваша версия сохранена в {}",
                                        copy_name
                                    ),
                                    8000,
                                ));
                            }
                            crate::save_manager::CheckedWrite::Failed(err) => {
                                notif_to_push = Some((
                                    crate::views::notification::NotificationType::Error,
                                    Some("Save Failed".into()),
                                    format!("Save error: {}", err),
                                    6000,
                                ));
                            }
                        }
                    }
                    if let Some((note_path, previous)) = reconcile {
                        let ws = this.state.workspace_path.clone();
                        let folder = crate::images::folder();
                        let current = saved_refs.clone();
                        let auto_trash = this.config.files.auto_trash_attachments;
                        background.spawn(async move {
                            crate::images::reconcile_saved_references(
                                ws.as_deref(),
                                &note_path,
                                &folder,
                                &previous,
                                &current,
                                auto_trash,
                            );
                        }).detach();
                    }
                    if let Some((notif_type, title, msg, duration)) = notif_to_push {
                        this.push_notification(notif_type, title, msg, Some(duration), cx);
                    }
                    cx.notify();
                }
                this.state.save_session_state();
            });
        }).detach();
    }

    /// Immediately dispatches writing of any dirty note to disk in the background,
    /// without blocking the UI thread. Used on blur/minimize, auto-save timers, and manual save.
    pub fn flush_active_note_now(&mut self, cx: &mut Context<Self>) {
        if let Some(doc_id) = self.state.active_doc_id() {
            self.flush_document_now(doc_id, cx);
        }
    }

    pub fn schedule_auto_save(&mut self, cx: &mut Context<Self>) {
        let (content_len, first_dirty) = match self.state.active_document() {
            Some(doc) if doc.is_dirty => (
                doc.len(),
                doc.first_dirty_time.unwrap_or_else(std::time::Instant::now),
            ),
            _ => return,
        };

        let (debounce, max_delay) = if self.config.editor.adaptive_save {
            crate::save_manager::adaptive_delays(content_len)
        } else {
            let d = std::time::Duration::from_millis(self.config.editor.save_debounce_ms as u64);
            (d, d * 5)
        };

        let now = std::time::Instant::now();
        let deadline = first_dirty + max_delay;
        let debounce_target = now + debounce;
        let target_time = debounce_target.min(deadline);
        let wait_dur = target_time.saturating_duration_since(now);

        self.auto_save_epoch = self.auto_save_epoch.wrapping_add(1);
        let epoch = self.auto_save_epoch;
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();
        cx.foreground_executor().spawn(async move {
            background.timer(wait_dur).await;
            let _ = entity.update(&mut async_cx, |this, cx| {
                if let Some(doc) = this.state.active_document() {
                    let deadline_reached = std::time::Instant::now() >= deadline;
                    if (this.auto_save_epoch == epoch || deadline_reached) && doc.is_dirty {
                        this.flush_active_note_now(cx);
                    }
                }
            });
        }).detach();
    }

    pub fn current_scroll_top(&self) -> f32 {
        self.current_scroll_top_f64() as f32
    }

    /// Flips the checkbox of a task row in the active document.
    ///
    /// The replacement is a single mark character, so the caret and the
    /// selection stay where they were: clicking a checkbox must not steal the
    /// cursor from the text being edited. The change is recorded in the
    /// document history like any other edit and saved by the usual debounce.
    pub(super) fn toggle_task_mark(
        &mut self,
        line_idx: usize,
        mark: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.active_toggle_task_mark(line_idx, mark) {
            return;
        }
        window.focus(&self.focus_handle);
        self.reset_editor_cursor_blink(cx);
        self.schedule_auto_save(cx);
        cx.notify();
    }

    /// Full-precision scroll position. Geometry must use this one: at the tail of
    /// a huge note the f32 grid is 16 px wide, i.e. coarser than a 24 px row, so
    /// rounding here would snap the slice to the wrong row.
    pub fn current_scroll_top_f64(&self) -> f64 {
        self.editor_scroll.get().max(0.0)
    }

    pub fn apply_note_settings_for_path(&mut self, path: &Path) {
        self.table_layouts.borrow_mut().clear();
        let mut restore_to = 0.0;
        if let Some(settings) = self.state.note_states.get(path).cloned() {
            self.edit_mode = settings.edit_mode;
            self.word_wrap = settings.word_wrap;
            self.source_mode = settings.source_mode;
            if let Some(view) = self.state.active_view_mut() {
                view.source_mode = self.source_mode;
            }
            if settings.scroll_top > 0.0 {
                self.set_editor_scroll_top(settings.scroll_top);
                restore_to = settings.scroll_top;
            } else {
                self.set_editor_scroll_top(0.0);
            }
        } else {
            self.edit_mode = true;
            self.word_wrap = true;
            self.source_mode = false;
            if let Some(view) = self.state.active_view_mut() {
                view.source_mode = false;
            }
            self.set_editor_scroll_top(0.0);
        }
        // Same guard as at startup: re-apply once measured (a no-op write
        // when the direct write above already landed), then validate cursor
        // visibility against the fresh cache — no extra rebuilds involved.
        if self
            .state
            .active_document()
            .map(|d| d.path.as_path() == path)
            .unwrap_or(false)
        {
            self.pending_editor_restore = Some(f64::from(restore_to));
            self.need_reveal_cursor = true;
            // The note changed, so the handle still describes the old content:
            // arm the burst that repaints the scrollbar once it is measured.
            self.layout_settle_frames.set(SCROLL_SETTLE_FRAMES);
        }
    }

    pub fn save_on_exit(&mut self, window: &Window) {
        let is_maximized = window.is_maximized();
        let bounds = window.bounds();
        let mut window_state = crate::state::WindowState::load(&crate::state::app_data_dir());
        window_state.maximized = is_maximized;
        if !is_maximized {
            window_state.x = Some(bounds.origin.x.into());
            window_state.y = Some(bounds.origin.y.into());
            window_state.width = bounds.size.width.into();
            window_state.height = bounds.size.height.into();
        }
        window_state.save(&crate::state::app_data_dir());

        self.commit_title_rename_internal();
        if self.state.active_document().map(|d| d.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }

        // Sync active view before persisting
        let current_leaf_id = self.state.workspace.active_leaf;
        let current_scroll_f64 = self.current_scroll_top_f64();
        if let Some(leaf) = self.state.leaves.get(&current_leaf_id) {
            if let Some(tab) = leaf.active_tab() {
                if let Some(view) = self.state.views.get_mut(&tab.id) {
                    view.scroll_top = current_scroll_f64;
                }
            }
        }

        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap, self.source_mode);
        let sidebar_offset_y: f32 = self.sidebar_scroll_handle.offset().y.into();
        self.state.session.sidebar_scroll_top = sidebar_offset_y;
        self.state.session.sidebar_width = self.sidebar_width;
        self.state.session.sidebar_mode = self.sidebar_mode.clone();
        self.state.session.recent_files_sort_order = self.recent_files_sort_order;
        self.state.session.recent_files = self.state.recent_files.clone();
        // Restored on the next launch by `AppView::new`.
        self.state.session.find_replace_open = self.find_bar.replace_open;
        self.state.queue_session_state_save(true);
    }

    /// Completely flushes data, closes child windows, unloads plugins, cancels subscriptions,
    /// and frees document buffers on application shutdown.
    pub fn prepare_shutdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.save_dirty_documents();
        self.catalog_window_manager.close_if_open(cx);
        self.settings_window_manager.close_if_open(cx);
        self.unload_all_plugins();
        self.plugin_leaf_views.clear();
        self.save_on_exit(window);
        self.close_tabs_popup(cx);
        self.close_tab_context_menu(cx);
        self.close_more_options_menu(cx);
        self.window_activation_subscription = None;
        self.suspend_cursor_blinks(cx);
        self.state.active_empty_tab = None;
        self.state.documents.clear();
        self.state.doc_meta.clear();
        self.state.by_path.clear();
        self.state.views.clear();
        self.state.pending_file_deletions.clear();
    }

    /// Polls for external filesystem changes across all open documents.
    ///
    /// Respects the debounce window so temporary unlinks (atomic saves) do not prematurely close tabs.
    pub fn poll_fs_changes(&mut self, cx: &mut Context<Self>) {
        let debounce = Duration::from_millis(350);
        let changes = self.state.check_fs_changes(debounce);

        if !changes.is_empty() {
            let config = self.state.settings.get_config();
            let mut any_handled = false;
            let mut images_touched = false;

            for change in changes {
                match change {
                    crate::state::FsChangeEvent::Deleted(path) => {
                        let ws = self.state.workspace_path.as_deref();
                        if self.state.recent_files.remove_path_or_prefix(&path, ws) {
                            self.state.session.recent_files = self.state.recent_files.clone();
                            self.save_recent_files();
                            any_handled = true;
                        }
                        if config.files.close_deleted_tabs {
                            let keep_dirty = config.files.keep_modified_on_delete;
                            if self.state.handle_file_deleted(&path, keep_dirty) {
                                any_handled = true;
                            }
                        }
                    }
                    crate::state::FsChangeEvent::Renamed { old_path, new_path } => {
                        let ws = self.state.workspace_path.as_deref();
                        if self.state.recent_files.rename_path_or_prefix(&old_path, &new_path, ws) {
                            self.state.session.recent_files = self.state.recent_files.clone();
                            self.save_recent_files();
                            any_handled = true;
                        }
                        if self.state.handle_file_renamed(&old_path, &new_path) {
                            any_handled = true;
                        }
                    }
                    crate::state::FsChangeEvent::Modified(path) => {
                        if self.state.handle_file_modified(&path) {
                            any_handled = true;
                        }
                    }
                    crate::state::FsChangeEvent::AttachmentsTouched => {
                        images_touched = true;
                    }
                    crate::state::FsChangeEvent::Rescan => {
                        images_touched = true;
                        // Reconcile against the disk: the tree reload and the
                        // vault cache reset happen inside; deletion of open
                        // notes is still debounced through the normal path by
                        // feeding every open path as potentially missing.
                        self.state.invalidate_markdown_cache_and_tree();
                        self.state.recheck_open_paths_after_rescan();
                        let ws = self.state.workspace_path.as_deref();
                        if self.state.recent_files.prune_nonexistent(ws) {
                            self.state.session.recent_files = self.state.recent_files.clone();
                            self.save_recent_files();
                        }
                        any_handled = true;
                    }
                }
            }

            if images_touched {
                crate::images::invalidate_resolution();
                for view in self.state.views.values_mut() {
                    view.visual_cache_dirty = true;
                }
                let word_wrap = self.word_wrap;
                let wrap_cols = self.last_wrap_cols;
                let typography = self.editor_typography();
                if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
                    view.ensure_visual_cache_with_headings(
                        doc,
                        word_wrap,
                        wrap_cols,
                        Some(&typography),
                        view.viewport.offset,
                        true,
                    );
                }
                cx.notify();
            }

            if any_handled {
                self.reload_file_tree_async(cx);
                self.state.publish_snapshot();
                cx.notify();
            }
        }
    }

    /// Polls the search worker for finished replies and applies the newest
    /// one per consumer.
    ///
    /// Called from the 150 ms background loop in `main.rs` (same pattern as
    /// `poll_fs_changes`). A reply is applied only while its sequence number
    /// is still the latest issued for its consumer, so a slow disk cannot
    /// overwrite fresher results; selection is clamped into the new list and
    /// a fresh dialog result set scrolls back to the top.
    pub fn poll_search_replies(&mut self, cx: &mut Context<Self>) {
        // The status bar shows the LIVE build progress; notify when it moves
        // so the bar repaints even with no replies arriving (a reply-driven
        // repaint only happens at search time, not during a quiet build).
        let progress = self.state.search_worker.build_progress();
        if progress != self.last_build_progress {
            self.last_build_progress = progress;
            cx.notify();
        }
        while let Some(reply) = self.state.search_worker.try_recv() {
            match reply.consumer {
                crate::search::worker::SearchConsumer::Dialog => {
                    if reply.seq != self.goto_search_seq {
                        continue;
                    }
                    self.search_dialog.results = reply.results;
                    // Highlights follow the query these results answer.
                    self.search_dialog.results_query = reply.query;
                    self.search_dialog.selected_ix = self
                        .search_dialog
                        .selected_ix
                        .min(self.search_dialog.results.len().saturating_sub(1));
                    // A fresh result set always starts at the top.
                    self.set_goto_scroll_top(0.0);
                    cx.notify();
                }
                crate::search::worker::SearchConsumer::Panel => {
                    if reply.seq != self.panel_search_seq {
                        continue;
                    }
                    self.sidebar_search.results = reply.results;
                    // Highlights follow the query these results answer.
                    self.sidebar_search.results_query = reply.query;
                    // Keep the cursor inside the fresh list, if there is one.
                    let len = self.sidebar_search.results.len();
                    self.sidebar_search.selected_ix = match (self.sidebar_search.selected_ix, len)
                    {
                        (_, 0) => None,
                        (None, _) => None,
                        (Some(ix), _) => Some(ix.min(len - 1)),
                    };
                    self.set_panel_scroll_top(0.0);
                    cx.notify();
                }
            }
        }
    }

    /// Registers OS window observers (e.g. activation / focus change) to handle
    /// background cursor blink pausing and reliable save-on-blur / save-on-minimize.
    pub fn init_window_observers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sub = cx.observe_window_activation(window, |this, window, cx| {
            let is_active = window.is_window_active();
            let changed = this.window_was_active != is_active;
            this.window_was_active = is_active;
            if !is_active {
                this.suspend_cursor_blinks(cx);
                if this.config.editor.save_on_blur {
                    this.flush_active_note_now(cx);
                }
            } else if changed {
                this.resume_cursor_blinks(cx);
                // Reactivating the window must put the keyboard back into the
                // editor: a deactivated window drops its focused element.
                if window.focused(cx).is_none() {
                    window.focus(&this.focus_handle);
                }
            }
        });
        self.window_activation_subscription = Some(sub);
    }

    /// Toggles the left sidebar visibility.
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        cx.notify();
    }

    /// Toggles word wrapping mode in the editor.
    pub fn toggle_word_wrap(&mut self, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap, self.source_mode);
        cx.notify();
    }

    /// Toggles displaying invisible characters (spaces, line breaks, EOF) in the editor.
    pub fn toggle_show_invisibles(&mut self, cx: &mut Context<Self>) {
        self.show_invisibles = !self.show_invisibles;
        let show = self.show_invisibles;
        self.settings_model.update(cx, |settings, cx| {
            settings.set_show_invisibles(show, cx);
        });
        cx.notify();
    }
}
