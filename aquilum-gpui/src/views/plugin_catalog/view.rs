use super::*;

/// Independent top-level GPUI view for the Community Plugins Catalog window.
pub struct PluginCatalogView {
    pub state: PluginCatalogState,
    pub focus_handle: FocusHandle,
    pub scroll_handle: ScrollHandle,
    pub details_scroll_handle: ScrollHandle,
    pub scrollbar_drag: ScrollbarDrag,
    pub details_scrollbar_drag: ScrollbarDrag,
    pub caret_visible: bool,
    pub caret_epoch: usize,
    pub weak_app: WeakEntity<AppView>,
    pub theme: Theme,
    pub vault_name: String,
    pub vault_root: Option<PathBuf>,
    pub is_ru: bool,
    pub is_hovering_close: bool,
    /// Compatibility filter dropdown (the shared `TabListPopup` list).
    pub compat_popup: Option<gpui::Entity<crate::views::tab_list::TabListPopup>>,
    /// Keeps the compat dropdown's event subscription alive.
    pub compat_popup_subscription: Option<gpui::Subscription>,
}

impl PluginCatalogView {
    pub fn new(
        weak_app: WeakEntity<AppView>,
        vault_name: String,
        vault_root: Option<PathBuf>,
        theme: Theme,
        is_ru: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut state = PluginCatalogState::default();
        if let Some(first) = state.entries.first() {
            state.selected_plugin_id = Some(first.id.clone());
        }

        let mut this = Self {
            state,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            details_scroll_handle: ScrollHandle::new(),
            scrollbar_drag: ScrollbarDrag::default(),
            details_scrollbar_drag: ScrollbarDrag::default(),
            caret_visible: true,
            caret_epoch: 0,
            weak_app,
            theme,
            vault_name,
            vault_root,
            is_ru,
            is_hovering_close: false,
            compat_popup: None,
            compat_popup_subscription: None,
        };

        if let Some(sel_id) = this.state.selected_plugin_id.clone() {
            this.ensure_readme_loaded(&sel_id, cx);
        }

        if this.state.entries.len() <= 5 && !this.state.is_loading {
            this.refresh_remote_catalog(cx);
        }

        this.start_caret_blinking(cx);
        this
    }

    pub fn start_caret_blinking(&mut self, cx: &mut Context<Self>) {
        self.caret_visible = true;
        self.caret_epoch += 1;
        let epoch = self.caret_epoch;
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();

        cx.foreground_executor()
            .spawn(async move {
                loop {
                    timer_executor.timer(Duration::from_millis(530)).await;
                    let alive = entity
                        .update(&mut async_cx, |view, cx| {
                            if view.caret_epoch == epoch {
                                view.caret_visible = !view.caret_visible;
                                cx.notify();
                                true
                            } else {
                                false
                            }
                        })                        .unwrap_or(false);
                    if !alive {
                        break;
                    }
                }
            })
            .detach();
    }

    /// Options of the compatibility dropdown in `TabEntry` form; the current
    /// filter's row is pre-highlighted via `active_ix`. Shared by the standalone
    /// catalog window and the in-settings panel (`open_*_compat_popup`).
    pub(super) fn compat_entries(
        filter: CatalogCompatFilter,
    ) -> (Vec<crate::views::tab_list::TabEntry>, usize) {
        compat_popup_entries(filter)
    }

    /// Opens the compatibility dropdown anchored to the given toggle button.
    /// The parent owns lifecycle: opening replaces the entity and its
    /// subscription; `TabListEvent::Dismiss` (click outside) closes it.
    pub fn open_compat_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (tabs, active_ix) = Self::compat_entries(self.state.compat_filter);
        let theme = self.theme.clone();
        let popup = cx.new(|cx| {
            crate::views::tab_list::TabListPopup::new(
                tabs,
                Some(active_ix),
                theme,
                None,
                None,
                None,
                cx,
            )
        });
        let subscription = cx.subscribe_in(
            &popup,
            window,
            move |this, _popup, event: &crate::views::tab_list::TabListEvent, _window, cx| {
                match event {
                    crate::views::tab_list::TabListEvent::Activate(id) => {
                        if let Some(option) = CatalogCompatFilter::ALL.get(*id) {
                            this.state.compat_filter = *option;
                            this.on_search_query_changed();
                        }
                        this.close_compat_popup(cx);
                    }
                    crate::views::tab_list::TabListEvent::Dismiss => this.close_compat_popup(cx),
                    _ => {}
                }
            },
        );
        window.focus(&popup.read(cx).focus_handle());
        self.compat_popup = Some(popup);
        self.compat_popup_subscription = Some(subscription);
        cx.notify();
    }

    pub fn close_compat_popup(&mut self, cx: &mut Context<Self>) {
        if self.compat_popup.take().is_none()
            && self.compat_popup_subscription.take().is_none()
        {
            return;
        }
        self.compat_popup_subscription = None;
        cx.notify();
    }

    pub fn compat_popup_is_open(&self) -> bool {
        self.compat_popup.is_some()
    }

    pub fn pause_and_show_caret(&mut self, cx: &mut Context<Self>) {
        self.caret_visible = true;
        self.caret_epoch += 1;
        let epoch = self.caret_epoch;
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();

        cx.foreground_executor()
            .spawn(async move {
                timer_executor.timer(Duration::from_millis(530)).await;
                let alive = entity
                    .update(&mut async_cx, |view, cx| {
                        if view.caret_epoch == epoch {
                            view.caret_visible = !view.caret_visible;
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if !alive {
                    return;
                }
                loop {
                    timer_executor.timer(Duration::from_millis(530)).await;
                    let alive = entity
                        .update(&mut async_cx, |view, cx| {
                            if view.caret_epoch == epoch {
                                view.caret_visible = !view.caret_visible;
                                cx.notify();
                                true
                            } else {
                                false
                            }
                        })
                        .unwrap_or(false);
                    if !alive {
                        break;
                    }
                }
            })
            .detach();
    }

    pub fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        if key == "escape" {
            window.remove_window();
            return;
        }

        let ctrl = event.keystroke.modifiers.control;
        let alt = event.keystroke.modifiers.alt;
        let shift = event.keystroke.modifiers.shift;

        let filtered = self.state.filtered_entries();

        // Arrow Up navigation
        if key == "up" {
            if !filtered.is_empty() {
                let current_idx = filtered
                    .iter()
                    .position(|e| Some(&e.id) == self.state.selected_plugin_id.as_ref())
                    .unwrap_or(0);
                let new_idx = current_idx.saturating_sub(1);
                let new_id = filtered[new_idx].id.clone();
                self.state.selected_plugin_id = Some(new_id.clone());
                self.details_scroll_handle.set_offset(point(px(0.0), px(0.0)));
                self.scroll_to_selected(new_idx);
                self.ensure_readme_loaded(&new_id, cx);
                cx.notify();
            }
            return;
        }

        // Arrow Down navigation
        if key == "down" {
            if !filtered.is_empty() {
                let current_idx = filtered
                    .iter()
                    .position(|e| Some(&e.id) == self.state.selected_plugin_id.as_ref())
                    .unwrap_or(0);
                let new_idx = (current_idx + 1).min(filtered.len().saturating_sub(1));
                let new_id = filtered[new_idx].id.clone();
                self.state.selected_plugin_id = Some(new_id.clone());
                self.details_scroll_handle.set_offset(point(px(0.0), px(0.0)));
                self.scroll_to_selected(new_idx);
                self.ensure_readme_loaded(&new_id, cx);
                cx.notify();
            }
            return;
        }

        // Home / End
        if key == "home" || key == "end" {
            let total = self.state.search_query.chars().count();
            let pos = if key == "home" { 0 } else { total };
            if shift {
                if self.state.selection_anchor.is_none() {
                    self.state.selection_anchor = Some(self.state.cursor_pos);
                }
            } else {
                self.state.selection_anchor = None;
            }
            self.state.cursor_pos = pos;
            self.pause_and_show_caret(cx);
            cx.notify();
            return;
        }

        // Left / Right cursor movement
        if key == "left" || key == "right" {
            let total = self.state.search_query.chars().count();
            if shift && self.state.selection_anchor.is_none() {
                self.state.selection_anchor = Some(self.state.cursor_pos);
            }
            if key == "left" {
                if shift {
                    self.state.cursor_pos = self.state.cursor_pos.saturating_sub(1);
                } else if let Some((start, _)) = self.state.selection_range() {
                    self.state.cursor_pos = start;
                    self.state.selection_anchor = None;
                } else {
                    self.state.cursor_pos = self.state.cursor_pos.saturating_sub(1);
                }
            } else if shift {
                self.state.cursor_pos = (self.state.cursor_pos + 1).min(total);
            } else if let Some((_, end)) = self.state.selection_range() {
                self.state.cursor_pos = end;
                self.state.selection_anchor = None;
            } else {
                self.state.cursor_pos = (self.state.cursor_pos + 1).min(total);
            }
            self.pause_and_show_caret(cx);
            cx.notify();
            return;
        }

        // Ctrl+A select all
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.state.selection_anchor = Some(0);
            self.state.cursor_pos = self.state.search_query.chars().count();
            self.pause_and_show_caret(cx);
            cx.notify();
            return;
        }

        // Ctrl+C copy
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s, e)) = self.state.selection_range() {
                let text: String = self.state.search_query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
            return;
        }

        // Ctrl+X cut
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s, e)) = self.state.selection_range() {
                let text: String = self.state.search_query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                let mut chars: Vec<char> = self.state.search_query.chars().collect();
                chars.drain(s..e);
                self.state.search_query = chars.into_iter().collect();
                self.state.cursor_pos = s;
                self.state.selection_anchor = None;
                self.on_search_query_changed();
                self.pause_and_show_caret(cx);
                cx.notify();
            }
            return;
        }

        // Ctrl+V paste
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text
                        .chars()
                        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                        .collect();
                    if !filtered.is_empty() {
                        self.insert_search_text(&filtered);
                        self.on_search_query_changed();
                        self.pause_and_show_caret(cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        if ctrl || alt || event.keystroke.modifiers.platform {
            return;
        }

        // Backspace / Delete
        if key == "backspace" || key == "delete" {
            let mut chars: Vec<char> = self.state.search_query.chars().collect();
            if let Some((s, e)) = self.state.selection_range() {
                chars.drain(s..e);
                self.state.cursor_pos = s;
                self.state.selection_anchor = None;
            } else if key == "backspace" {
                let pos = self.state.cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.state.cursor_pos = pos - 1;
                } else {
                    return;
                }
            } else {
                let pos = self.state.cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                } else {
                    return;
                }
            }
            self.state.search_query = chars.into_iter().collect();
            self.on_search_query_changed();
            self.pause_and_show_caret(cx);
            cx.notify();
            return;
        }

        // Plain text input
        let text_to_insert = if key == "space" {
            Some(" ".to_string())
        } else if let Some(ch) = &event.keystroke.key_char {
            Some(ch.to_string())
        } else if key.chars().count() == 1 {
            Some(key.clone())
        } else {
            None
        };
        if let Some(text) = text_to_insert {
            let filtered: String = text
                .chars()
                .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                .collect();
            if !filtered.is_empty() {
                self.insert_search_text(&filtered);
                self.on_search_query_changed();
                self.pause_and_show_caret(cx);
                cx.notify();
            }
        }
    }

    pub(super) fn insert_search_text(&mut self, text: &str) {
        let mut chars: Vec<char> = self.state.search_query.chars().collect();
        let pos = if let Some((s, e)) = self.state.selection_range() {
            chars.drain(s..e);
            self.state.selection_anchor = None;
            s
        } else {
            self.state.cursor_pos.min(chars.len())
        };
        for (i, c) in text.chars().enumerate() {
            chars.insert(pos + i, c);
        }
        self.state.cursor_pos = pos + text.chars().count();
        self.state.search_query = chars.into_iter().collect();
    }

    pub(super) fn on_search_query_changed(&mut self) {
        let filtered = self.state.filtered_entries();
        if !filtered.iter().any(|e| Some(&e.id) == self.state.selected_plugin_id.as_ref()) {
            self.state.selected_plugin_id = filtered.first().map(|e| e.id.clone());
        }
        self.scroll_handle.set_offset(point(px(0.0), px(0.0)));
    }

    pub(super) fn scroll_to_selected(&mut self, index: usize) {
        let target_y = index as f32 * CATALOG_ROW_H;
        let viewport_h = f32::from(self.scroll_handle.bounds().size.height).max(100.0);
        let cur_y = -f32::from(self.scroll_handle.offset().y);
        if target_y < cur_y {
            self.scroll_handle.set_offset(point(px(0.0), px(-target_y)));
        } else if target_y + CATALOG_ROW_H > cur_y + viewport_h {
            let next_y = target_y + CATALOG_ROW_H - viewport_h;
            self.scroll_handle.set_offset(point(px(0.0), px(-next_y)));
        }
    }

    pub fn handle_list_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.scrollbar_drag.active {
            return;
        }
        let style = catalog_scrollbar_style();
        let Some(geometry) = ScrollbarGeometry::from_handle(&self.scroll_handle, &style) else {
            return;
        };
        geometry.apply_drag(&self.scroll_handle, &self.scrollbar_drag, pointer_y);
        cx.notify();
    }

    pub fn handle_details_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.details_scrollbar_drag.active {
            return;
        }
        let style = catalog_scrollbar_style();
        let Some(geometry) = ScrollbarGeometry::from_handle(&self.details_scroll_handle, &style) else {
            return;
        };
        geometry.apply_drag(&self.details_scroll_handle, &self.details_scrollbar_drag, pointer_y);
        cx.notify();
    }

    pub fn search_click_pos(&self, click_x: f32, window: &Window) -> usize {
        let query = &self.state.search_query;
        if query.is_empty() {
            return 0;
        }
        let box_x = self.state.box_origin_x.get();
        let rel_x = (click_x - box_x).max(0.0);
        let run = gpui::TextRun {
            len: query.len(),
            font: gpui::font("Segoe UI"),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(query.clone()),
            px(14.0),
            &[run],
            None,
        );
        let byte_idx = shaped.closest_index_for_x(px(rel_x));
        query[..byte_idx.min(query.len())].chars().count()
    }

    pub fn handle_search_mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.is_mouse_selecting {
            return;
        }
        let x: f32 = event.position.x.into();
        let new_pos = self.search_click_pos(x, window);
        if self.state.cursor_pos != new_pos {
            self.state.cursor_pos = new_pos;
            self.pause_and_show_caret(cx);
            cx.notify();
        }
    }

    pub fn handle_search_mouse_up(&mut self, cx: &mut Context<Self>) {
        if self.state.is_mouse_selecting {
            self.state.is_mouse_selecting = false;
            if self.state.selection_anchor == Some(self.state.cursor_pos) {
                self.state.selection_anchor = None;
            }
            cx.notify();
        }
    }

    pub fn refresh_remote_catalog(&mut self, cx: &mut Context<Self>) {
        if self.state.is_loading {
            return;
        }

        self.state.is_loading = true;
        self.state.status_message = Some(if self.is_ru {
            "Загрузка реестра с GitHub...".to_string()
        } else {
            "Fetching registry from GitHub...".to_string()
        });
        cx.notify();

        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let bg_executor = cx.background_executor().clone();

        cx.foreground_executor()
            .spawn(async move {
                let res = bg_executor
                    .spawn(async { crate::plugins::registry::fetch_remote_community_plugins() })
                    .await;

                let _ = entity.update(&mut async_cx, |view, cx| {
                    view.state.is_loading = false;
                    match res {
                        Ok(entries) => {
                            let count = entries.len();
                            view.state.entries = entries;
                            view.state.status_message = Some(if view.is_ru {
                                format!("Загружено плагинов: {}", count)
                            } else {
                                format!("Loaded plugins: {}", count)
                            });
                            if view.state.selected_plugin_id.is_none() {
                                if let Some(first) = view.state.entries.first() {
                                    view.state.selected_plugin_id = Some(first.id.clone());
                                }
                            }
                        }
                        Err(err) => {
                            view.state.status_message = Some(if view.is_ru {
                                format!("Ошибка сети: {}. Используется кэш.", err)
                            } else {
                                format!("Network error: {}. Using cache.", err)
                            });
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
    }

    pub fn ensure_readme_loaded(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        if self.state.readme_cache.contains_key(plugin_id) {
            return;
        }

        // Check local disk first if vault exists
        if let Some(vault_root) = &self.vault_root {
            let p_dir = vault_root.join(".vault").join("plugins").join(plugin_id);
            for name in &["README.md", "readme.md"] {
                let file = p_dir.join(name);
                if file.is_file() {
                    if let Ok(content) = std::fs::read_to_string(&file) {
                        self.state.readme_cache.insert(plugin_id.to_string(), content);
                        return;
                    }
                }
            }
        }

        // Find repository in catalog entries
        let Some(entry) = self.state.entries.iter().find(|e| e.id == plugin_id) else {
            return;
        };

        let repo = entry.repo.clone();
        let p_id = plugin_id.to_string();
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let bg_executor = cx.background_executor().clone();

        self.state.is_loading_readme = true;

        cx.foreground_executor()
            .spawn(async move {
                let res = bg_executor
                    .spawn(async move { crate::plugins::registry::fetch_readme_for_plugin(&repo) })
                    .await;

                let _ = entity.update(&mut async_cx, move |view, cx| {
                    view.state.is_loading_readme = false;
                    match res {
                        Ok(content) => {
                            view.state.readme_cache.insert(p_id, content);
                        }
                        Err(_) => {
                            let msg = if view.is_ru {
                                "# Описание отсутствует\n\nНе удалось загрузить README.md из репозитория GitHub.".to_string()
                            } else {
                                "# No description\n\nFailed to load README.md from GitHub repository.".to_string()
                            };
                            view.state.readme_cache.insert(p_id, msg);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
    }

    pub fn install_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entries.iter().find(|e| e.id == plugin_id).cloned() else {
            return;
        };
        let Some(vault_root) = &self.vault_root else {
            self.state.status_message = Some(if self.is_ru {
                "Невозможно установить: хранилище не открыто".to_string()
            } else {
                "Cannot install: No vault open".to_string()
            });
            cx.notify();
            return;
        };

        if self.state.is_downloading {
            return;
        }

        self.state.is_downloading = true;
        self.state.downloading_plugin_id = Some(entry.id.clone());
        self.state.download_step = Some(if self.is_ru {
            "Подключение к GitHub Releases...".to_string()
        } else {
            "Connecting to GitHub Releases...".to_string()
        });
        cx.notify();

        let plugin_dir = vault_root.join(".vault").join("plugins").join(&entry.id);
        let repo = entry.repo.clone();
        let plugin_name = entry.name.clone();
        let entry_id = entry.id.clone();
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let bg_executor = cx.background_executor().clone();
        let v_root = vault_root.clone();
        let weak_app = self.weak_app.clone();

        cx.foreground_executor()
            .spawn(async move {
                let p_dir = plugin_dir.clone();
                let res = bg_executor
                    .spawn(async move {
                        crate::plugins::registry::download_plugin_release(&repo, &p_dir, |_step| {})
                    })
                    .await;

                let _ = entity.update(&mut async_cx, move |view, cx| {
                    view.state.is_downloading = false;
                    view.state.downloading_plugin_id = None;
                    view.state.download_step = None;

                    match res {
                        Ok(()) => {
                            // Update .vault/community-plugins.json
                            let list_file = v_root.join(".vault").join("community-plugins.json");
                            let mut list: Vec<String> = std::fs::read_to_string(&list_file)
                                .ok()
                                .and_then(|c| serde_json::from_str(&c).ok())
                                .unwrap_or_default();
                            if !list.contains(&entry_id) {
                                list.push(entry_id.clone());
                                let _ = std::fs::write(&list_file, serde_json::to_string_pretty(&list).unwrap_or_default());
                            }

                            view.state.status_message = Some(if view.is_ru {
                                format!("Плагин '{}' успешно установлен и активирован", plugin_name)
                            } else {
                                format!("Plugin '{}' successfully installed and activated", plugin_name)
                            });

                            // Safely notify main window
                            if let Some(app) = weak_app.upgrade() {
                                let mut async_cx = cx.to_async();
                                cx.foreground_executor().spawn(async move {
                                    let _ = app.update(&mut async_cx, |view, cx| {
                                        if !view.config.community_plugins.safe_mode {
                                            view.load_installed_plugins_for_vault();
                                        }
                                        view.state.status_text = format!("Плагин '{}' установлен", plugin_name);
                                        cx.notify();
                                    });
                                }).detach();
                            }
                        }
                        Err(e) => {
                            view.state.status_message = Some(if view.is_ru {
                                format!("Ошибка установки: {}", e)
                            } else {
                                format!("Installation error: {}", e)
                            });
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
    }

    pub fn toggle_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let Some(vault_root) = &self.vault_root else { return; };
        let list_file = vault_root.join(".vault").join("community-plugins.json");
        let mut list: Vec<String> = std::fs::read_to_string(&list_file)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default();

        let p_id_str = plugin_id.to_string();
        let was_enabled = list.contains(&p_id_str);
        if was_enabled {
            list.retain(|id| id != &p_id_str);
        } else {
            list.push(p_id_str.clone());
        }
        let _ = std::fs::write(&list_file, serde_json::to_string_pretty(&list).unwrap_or_default());

        let weak_app = self.weak_app.clone();
        let p_id = p_id_str.clone();
        if let Some(app) = weak_app.upgrade() {
            let mut async_cx = cx.to_async();
            cx.foreground_executor().spawn(async move {
                let _ = app.update(&mut async_cx, move |view, cx| {
                    if was_enabled {
                        view.unload_plugin(&p_id, cx);
                    } else if !view.config.community_plugins.safe_mode {
                        view.load_installed_plugins_for_vault();
                    }
                    cx.notify();
                });
            }).detach();
        }
        cx.notify();
    }

    pub fn uninstall_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let Some(vault_root) = &self.vault_root else { return; };
        let plugin_dir = vault_root.join(".vault").join("plugins").join(plugin_id);
        if plugin_dir.exists() {
            let _ = std::fs::remove_dir_all(&plugin_dir);
        }

        let list_file = vault_root.join(".vault").join("community-plugins.json");
        let mut list: Vec<String> = std::fs::read_to_string(&list_file)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default();
        list.retain(|id| id != plugin_id);
        let _ = std::fs::write(&list_file, serde_json::to_string_pretty(&list).unwrap_or_default());

        let weak_app = self.weak_app.clone();
        let p_id = plugin_id.to_string();
        if let Some(app) = weak_app.upgrade() {
            let mut async_cx = cx.to_async();
            cx.foreground_executor().spawn(async move {
                let _ = app.update(&mut async_cx, move |view, cx| {
                    view.unload_plugin(&p_id, cx);
                    cx.notify();
                });
            }).detach();
        }
        cx.notify();
    }

    pub(super) fn render_titlebar(&self, window: &Window, cx: &Context<Self>) -> Div {
        let theme = &self.theme;
        let is_maximized = window.is_maximized();
        let title = if self.is_ru {
            format!("Сторонние плагины — {} — Aquilum", self.vault_name)
        } else {
            format!("Community plugins — {} — Aquilum", self.vault_name)
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(px(34.0))
            .w_full()
            .bg(theme.bg_sidebar)
            .border_b_1()
            .border_color(theme.border_subtle)
            .flex_shrink_0()
            // Drag and title area spanning from left edge to controls (draggable, double-click to maximize/restore)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .items_center()
                    .h_full()
                    .window_control_area(WindowControlArea::Drag)
                    // `occlude()` keeps the full-window root out of the hit test
                    // over this region. The root carries `track_focus`, and a
                    // tracked element calls `Window::prevent_default()` on
                    // mouse-down; the Windows backend treats a prevented-default
                    // non-client mouse-down as handled and therefore skips the
                    // OS window move, so the title bar stopped dragging. With the
                    // root blocked behind this hitbox `default_prevented` stays
                    // false and the platform handles the drag itself.
                    .occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, event: &MouseDownEvent, window, _| {
                        if event.click_count >= 2 {
                            // The click reaches DefWindowProc now, so it would
                            // toggle maximize on its own. Claim the event to keep
                            // our zoom the only toggle (and only on the 2nd click).
                            window.prevent_default();
                            if event.click_count == 2 {
                                window.zoom_window();
                            }
                        }
                    }))
                    .child(
                        // Left spacer / icon (138px to match right controls for exact centering)
                        div()
                            .flex()
                            .items_center()
                            .w(px(138.0))
                            .px_3()
                            .gap_2()
                            .h_full()
                            .child(icons::icon(icons::ICON_PLUG, 14.0, theme.accent))
                    )
                    .child(
                        // Center title
                        div()
                            .flex()
                            .flex_1()
                            .items_center()
                            .justify_center()
                            .h_full()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text_secondary)
                                    .child(title)
                            )
                    )
            )
            // Right window controls (Minimize, Maximize/Restore, Close)
            //
            // Each button claims its own mouse-down. A platform control area makes
            // the OS repeat the action on mouse-up (`nc_button_pressed` is recorded
            // only when the non-client mouse-down is NOT handled); without the guard
            // this window only escaped the double action through the root's
            // `track_focus`/`prevent_default`, which is fragile. Same guard as in the
            // settings window and the main title bar.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .w(px(138.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("cat-win-ctrl-min")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(46.0))
                            .h_full()
                            .cursor_pointer()
                            .window_control_area(WindowControlArea::Min)
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_MINIMIZE, 11.0, theme.text_secondary))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|_, _, window, _| {
                                window.minimize_window();
                            }))
                    )
                    .child(
                        div()
                            .id("cat-win-ctrl-max")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(46.0))
                            .h_full()
                            .cursor_pointer()
                            .window_control_area(WindowControlArea::Max)
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                if is_maximized { icons::ICON_RESTORE } else { icons::ICON_MAXIMIZE },
                                11.0,
                                theme.text_secondary,
                            ))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|_, _, window, _| {
                                window.zoom_window();
                            }))
                    )
                    .child({
                        let is_hovering_close = self.is_hovering_close;
                        div()
                            .id("cat-win-ctrl-close")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(46.0))
                            .h_full()
                            .cursor_pointer()
                            .window_control_area(WindowControlArea::Close)
                            .hover(|h| h.bg(rgb(0xe81123)))
                            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                if this.is_hovering_close != *hovered {
                                    this.is_hovering_close = *hovered;
                                    cx.notify();
                                }
                            }))
                            .child(icons::icon(
                                icons::ICON_WINDOW_CLOSE,
                                11.0,
                                if is_hovering_close { rgb(0xffffff) } else { theme.text_secondary },
                            ))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|_, _, window, _| {
                                window.remove_window();
                            }))
                    })
            )
    }

    pub(super) fn render_header(&self, window: &Window, cx: &Context<Self>) -> Div {
        let theme = &self.theme;
        let is_ru = self.is_ru;
        let query = self.state.search_query.clone();
        let total_count = self.state.entries.len();
        let filtered_count = self.state.filtered_entries().len();
        let is_loading = self.state.is_loading;

        let query_chars: Vec<char> = query.chars().collect();
        let cursor_pos = self.state.cursor_pos.min(query_chars.len());
        let before_cursor: String = query_chars[..cursor_pos].iter().collect();

        let run = gpui::TextRun {
            len: before_cursor.len(),
            font: gpui::font("Segoe UI"),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(before_cursor),
            px(14.0),
            &[run],
            None,
        );
        let cursor_x = f32::from(shaped.width).round();

        let selection_range = self.state.selection_range();
        let (sel_x1, sel_w) = if let Some((start, end)) = selection_range {
            let s_pos = start.min(query_chars.len());
            let e_pos = end.min(query_chars.len());
            let s_str: String = query_chars[..s_pos].iter().collect();
            let e_str: String = query_chars[..e_pos].iter().collect();

            let s_shaped = window.text_system().shape_line(
                SharedString::from(s_str.clone()),
                px(14.0),
                &[gpui::TextRun { len: s_str.len(), font: gpui::font("Segoe UI"), color: Default::default(), underline: Default::default(), strikethrough: None, background_color: None }],
                None,
            );
            let e_shaped = window.text_system().shape_line(
                SharedString::from(e_str.clone()),
                px(14.0),
                &[gpui::TextRun { len: e_str.len(), font: gpui::font("Segoe UI"), color: Default::default(), underline: Default::default(), strikethrough: None, background_color: None }],
                None,
            );
            let x1 = f32::from(s_shaped.width).round();
            let x2 = f32::from(e_shaped.width).round();
            (x1, (x2 - x1).abs().max(1.0))
        } else {
            (0.0, 0.0)
        };

        let caret_visible = self.caret_visible && self.focus_handle.is_focused(window);

        div()
            .flex()
            .flex_col()
            .px_6()
            .py_4()
            .bg(theme.bg_sidebar)
            .border_b_1()
            .border_color(theme.border_subtle)
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2p5()
                            .child(icons::icon(icons::ICON_PLUG, 20.0, theme.accent))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(theme.text_primary)
                                            .child(if is_ru {
                                                "Каталог сторонних плагинов"
                                            } else {
                                                "Community Plugins Catalog"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .child(format!(
                                                "{}: {} | {}: {}",
                                                if is_ru { "Всего плагинов" } else { "Total" },
                                                total_count,
                                                if is_ru { "Найдено" } else { "Matches" },
                                                filtered_count,
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            // Refresh button
                            .child(
                                div()
                                    .id("btn-refresh-catalog")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .px_2p5()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(theme.bg_card)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_REFRESH, 13.0, if is_loading { theme.accent } else { theme.text_muted }))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_secondary)
                                            .child(if is_ru { "Обновить реестр" } else { "Update registry" }),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_remote_catalog(cx);
                                    })),
                            ),
                    ),
            )
            // Search Input Row & Sorting Options
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .items_center()
                            .h(px(36.0))
                            .px_3()
                            .rounded_md()
                            .bg(theme.bg_card)
                            .border_1()
                            .border_color(if self.focus_handle.is_focused(window) {
                                theme.accent
                            } else {
                                theme.border_subtle
                            })
                            .gap_2()
                            .child(icons::icon(icons::ICON_SEARCH, 15.0, theme.text_muted))
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .h(px(22.0))
                                    .overflow_hidden()
                                    .cursor(gpui::CursorStyle::IBeam)
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                        let pos = this.search_click_pos(event.position.x.into(), window);
                                        window.focus(&this.focus_handle);
                                        if event.click_count == 3 {
                                            let total_chars = this.state.search_query.chars().count();
                                            this.state.selection_anchor = Some(0);
                                            this.state.cursor_pos = total_chars;
                                            this.state.is_mouse_selecting = false;
                                        } else if event.click_count == 2 {
                                            let (s, e) = word_range_at(&this.state.search_query, pos);
                                            this.state.selection_anchor = Some(s);
                                            this.state.cursor_pos = e;
                                            this.state.is_mouse_selecting = false;
                                        } else if event.modifiers.shift {
                                            if this.state.selection_anchor.is_none() {
                                                this.state.selection_anchor = Some(this.state.cursor_pos);
                                            }
                                            this.state.cursor_pos = pos;
                                            this.state.is_mouse_selecting = true;
                                        } else {
                                            this.state.cursor_pos = pos;
                                            this.state.selection_anchor = Some(pos);
                                            this.state.is_mouse_selecting = true;
                                        }
                                        this.pause_and_show_caret(cx);
                                        cx.notify();
                                    }))
                                    .child(
                                        gpui::canvas(
                                            {
                                                let cell = self.state.box_origin_x.clone();
                                                move |bounds, _, _| {
                                                    cell.set(f32::from(bounds.origin.x));
                                                }
                                            },
                                            |_, _, _, _| {},
                                        )
                                        .absolute()
                                        .size_full(),
                                    )
                                    .when(selection_range.is_some() && sel_w > 0.0, |s| {
                                        s.child(
                                            div()
                                                .absolute()
                                                .left(px(sel_x1))
                                                .top(px(0.0))
                                                .w(px(sel_w))
                                                .h(px(22.0))
                                                .bg(theme.selection_bg)
                                                .rounded_xs(),
                                        )
                                    })
                                    .child(if query.is_empty() {
                                        div()
                                            .font_family("Segoe UI")
                                            .text_size(px(14.0))
                                            .line_height(px(22.0))
                                            .text_color(theme.text_muted)
                                            .child(if is_ru {
                                                "Поиск по названию, автору или описанию...".to_string()
                                            } else {
                                                "Search community plugins...".to_string()
                                            })
                                    } else {
                                        div()
                                            .font_family("Segoe UI")
                                            .text_size(px(14.0))
                                            .line_height(px(22.0))
                                            .text_color(theme.text_primary)
                                            .child(query.clone())
                                    })
                                    .when(caret_visible, |s| {
                                        s.child(
                                            div()
                                                .absolute()
                                                .left(px(cursor_x))
                                                .top(px(2.0))
                                                .w(px(1.5))
                                                .h(px(18.0))
                                                .bg(theme.accent)
                                                .rounded_xs(),
                                        )
                                    }),
                            )
                            .when(!query.is_empty(), |parent| {
                                parent.child(
                                    div()
                                        .id("btn-clear-search")
                                        .cursor_pointer()
                                        .hover(|h| h.text_color(theme.text_primary))
                                        .child(icons::icon(icons::ICON_CLOSE, 12.0, theme.text_muted))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.state.search_query.clear();
                                            this.state.cursor_pos = 0;
                                            this.state.selection_anchor = None;
                                            this.on_search_query_changed();
                                            window.focus(&this.focus_handle);
                                            this.pause_and_show_caret(cx);
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    // Sort order buttons
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1p5()
                            .bg(theme.bg_card)
                            .p_1()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border_subtle)
                            .child(
                                div()
                                    .id("sort-popular")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_xs()
                                    .cursor_pointer()
                                    .text_xs()
                                    .bg(if self.state.sort_order == CatalogSortOrder::MostPopular { theme.bg_hover } else { rgba(0x00000000) })
                                    .text_color(if self.state.sort_order == CatalogSortOrder::MostPopular { theme.text_primary } else { theme.text_muted })
                                    .child(if is_ru { "Популярные" } else { "Popular" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.sort_order = CatalogSortOrder::MostPopular;
                                        this.on_search_query_changed();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("sort-alpha")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_xs()
                                    .cursor_pointer()
                                    .text_xs()
                                    .bg(if self.state.sort_order == CatalogSortOrder::Alphabetical { theme.bg_hover } else { rgba(0x00000000) })
                                    .text_color(if self.state.sort_order == CatalogSortOrder::Alphabetical { theme.text_primary } else { theme.text_muted })
                                    .child(if is_ru { "А-Я" } else { "A-Z" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.sort_order = CatalogSortOrder::Alphabetical;
                                        this.on_search_query_changed();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("sort-updated")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_xs()
                                    .cursor_pointer()
                                    .text_xs()
                                    .bg(if self.state.sort_order == CatalogSortOrder::RecentlyUpdated { theme.bg_hover } else { rgba(0x00000000) })
                                    .text_color(if self.state.sort_order == CatalogSortOrder::RecentlyUpdated { theme.text_primary } else { theme.text_muted })
                                    .child(if is_ru { "Новые" } else { "Updated" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.sort_order = CatalogSortOrder::RecentlyUpdated;
                                        this.on_search_query_changed();
                                        cx.notify();
                                    })),
                            ),
                    )
                    // Compatibility dropdown over the embedded corpus registry.
                    // The shared `TabListPopup` hangs from this button (deferred:
                    // paints above the details pane and outside the header's
                    // stacking); a click outside it emits `Dismiss`.
                    .child(
                        div()
                            .relative()
                            .child(
                                div()
                                    .id("compat-dropdown-toggle")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_xs()
                                    .bg(theme.bg_card)
                                    .border_1()
                                    .border_color(if self.compat_popup_is_open() { theme.accent } else { theme.border_subtle })
                                    .child(div().w(px(7.0)).h(px(7.0)).rounded_full().bg(self.state.compat_filter.dot_color(theme)))
                                    .child(self.state.compat_filter.label(is_ru))
                                    .child(icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if this.compat_popup_is_open() {
                                            this.close_compat_popup(cx);
                                        } else {
                                            this.open_compat_popup(window, cx);
                                        }
                                    })),
                            )
                            .when_some(self.compat_popup.clone(), |parent, popup| {
                                parent.child(
                                    gpui::deferred(
                                        div()
                                            .id("compat-dropdown-popup")
                                            .absolute()
                                            .top_full()
                                            .right_0()
                                            .mt_1()
                                            .child(popup),
                                    )
                                    .with_priority(999),
                                )
                            }),
                    ),
            )
    }

    pub(super) fn render_list(&self, entries: &[CatalogPluginEntry], cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let selected_id = self.state.selected_plugin_id.as_deref().unwrap_or("");
        let is_ru = self.is_ru;

        let total_items = entries.len();
        let total_height = total_items as f32 * CATALOG_ROW_H;

        let viewport_h = f32::from(self.scroll_handle.bounds().size.height).max(100.0);
        let max_offset = f32::from(self.scroll_handle.max_offset().height);
        let current_scroll_y = (-f32::from(self.scroll_handle.offset().y)).clamp(0.0, max_offset.max(0.0));

        let start_row = (current_scroll_y / CATALOG_ROW_H).floor() as usize;
        let visible_count = (viewport_h / CATALOG_ROW_H).ceil() as usize;

        let visible_start = start_row.saturating_sub(CATALOG_OVERSCAN_ROWS);
        let visible_end = (start_row + visible_count + CATALOG_OVERSCAN_ROWS).min(total_items);

        let top_spacer_h = visible_start as f32 * CATALOG_ROW_H;
        let bottom_spacer_h = (total_height - (visible_end as f32 * CATALOG_ROW_H)).max(0.0);

        let mut item_elements: Vec<AnyElement> = Vec::with_capacity((visible_end - visible_start) + 2);

        if top_spacer_h > 0.0 {
            item_elements.push(
                div()
                    .id("catalog-top-spacer")
                    .w_full()
                    .h(px(top_spacer_h))
                    .flex_shrink_0()
                    .into_any_element(),
            );
        }

        let query = self.state.search_query.clone();
        let terms = highlight_query_terms(&query);

        for (rel_idx, entry) in entries[visible_start..visible_end].iter().enumerate() {
            let actual_idx = visible_start + rel_idx;
            let is_selected = entry.id == selected_id;
            let is_installed = self
                .vault_root
                .as_ref()
                .map(|root| root.join(".vault").join("plugins").join(&entry.id).exists())
                .unwrap_or(false);

            let entry_id = entry.id.clone();
            let dl_formatted = format_download_count(entry.downloads);
            let author_display = format!("{}: {}", if is_ru { "Автор" } else { "Author" }, entry.author);
            // Status marker in the row: colored dot + short word, colored by verdict.
            let compat_row = crate::plugins::compat::catalog_compat(&entry.id);
            let (compat_dot, compat_word) = match compat_row.as_ref().map(|r| &r.status) {
                Some(crate::plugins::compat::CompatibilityStatus::Full) => (rgba(0x3fb950ff), if is_ru { "Совместим" } else { "OK" }.to_string()),
                Some(crate::plugins::compat::CompatibilityStatus::Partial(_)) => (rgba(0xd29922ff), if is_ru { "Частично" } else { "Partial" }.to_string()),
                Some(crate::plugins::compat::CompatibilityStatus::Incompatible) => (rgba(0xf85149ff), if is_ru { "Блокирован" } else { "Blocked" }.to_string()),
                Some(crate::plugins::compat::CompatibilityStatus::Unknown) | None => (theme.text_muted, String::new()),
            };

            let item_el = div()
                .id(ElementId::from(SharedString::from(format!("cat-item-{}", entry.id))))
                .flex()
                .flex_col()
                .gap_2()
                .h(px(160.0))
                .mb(px(8.0))
                .px_3p5()
                .py_3()
                .rounded_lg()
                .border_1()
                .border_color(if is_selected { theme.accent } else { theme.border_subtle })
                .bg(if is_selected {
                    theme.bg_hover
                } else if theme.is_dark {
                    theme.bg_card
                } else {
                    rgba(0xffffffff)
                })
                .hover(|h| h.bg(theme.bg_hover))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.state.selected_plugin_id = Some(entry_id.clone());
                    this.details_scroll_handle.set_offset(point(px(0.0), px(0.0)));
                    this.ensure_readme_loaded(&entry_id, cx);
                    this.scroll_to_selected(actual_idx);
                    window.focus(&this.focus_handle);
                    cx.notify();
                }))
                // Top row
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(icons::icon(icons::ICON_PLUG, 18.0, theme.accent))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(theme.text_primary)
                                        .truncate()
                                        .child(highlight_terms(&entry.name, &terms, theme.text_primary, true, theme)),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .child(icons::icon(icons::ICON_DOWNLOAD, 12.0, theme.text_muted))
                                        .child(dl_formatted),
                                )
                                // Compatibility marker from the corpus registry: a dot-style
                                // badge, unobtrusive next to the download counter.
                                .when(!compat_word.is_empty(), |p| {
                                    p.child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .child(
                                                div()
                                                    .w(px(7.0))
                                                    .h(px(7.0))
                                                    .rounded_full()
                                                    .bg(compat_dot),
                                            )
                                            .child(compat_word),
                                    )
                                })
                                .when(is_installed, |p| {
                                    p.child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_full()
                                            .bg(rgba(0x7c4dff22))
                                            .border_1()
                                            .border_color(rgba(0x7c4dff88))
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.accent)
                                            .child(if is_ru { "Установлен" } else { "Installed" }),
                                    )
                                }),
                        ),
                )
                // Author row
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .truncate()
                        .child(author_display),
                )
                // Description
                .child(
                    div()
                        .text_xs()
                        .line_height(gpui::relative(1.4))
                        .text_color(theme.text_secondary)
                        .h(px(54.0))
                        .overflow_hidden()
                        .child(highlight_terms(&entry.description, &terms, theme.text_secondary, false, theme)),
                );

            item_elements.push(item_el.into_any_element());
        }

        if bottom_spacer_h > 0.0 {
            item_elements.push(
                div()
                    .id("catalog-bottom-spacer")
                    .w_full()
                    .h(px(bottom_spacer_h))
                    .flex_shrink_0()
                    .into_any_element(),
            );
        }

        let scroll_container = div()
            .id("catalog-list-scroll")
            .track_scroll(&self.scroll_handle)
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .overflow_x_hidden()
            .p_3()
            .pr_5()
            .children(item_elements);

        let scroll_style = catalog_scrollbar_style();
        let mut viewport = div()
            .id("catalog-list-viewport")
            .relative()
            .flex()
            .flex_col()
            .w(px(360.0))
            .min_w(px(360.0))
            .h_full()
            .border_r_1()
            .border_color(theme.border_subtle)
            .child(scroll_container);

        if let Some(geometry) = ScrollbarGeometry::from_handle(&self.scroll_handle, &scroll_style) {
            let dragging = self.scrollbar_drag.active;
            viewport = viewport.child(overlay_scrollbar(
                "catalog-scrollbar-track",
                &geometry,
                &scroll_style,
                theme,
                dragging,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let pointer_y: f32 = event.position.y.into();
                    let target_y = geometry.offset_for_track_click(pointer_y);
                    this.scroll_handle.set_offset(point(px(0.0), px(-target_y)));
                    cx.notify();
                }),
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.scrollbar_drag.begin(event.position.y.into(), geometry.scroll_y);
                    cx.notify();
                }),
            ));
        }

        viewport
    }

    pub(super) fn render_details(
        &self,
        entry_opt: Option<&CatalogPluginEntry>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        let is_ru = self.is_ru;
        let query = self.state.search_query.clone();
        let terms = highlight_query_terms(&query);

        let Some(entry) = entry_opt else {
            return div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.text_muted)
                .child(if is_ru { "Выберите плагин из списка" } else { "Select a plugin from the list" })
                .into_any_element();
        };

        let plugin_dir = self
            .vault_root
            .as_ref()
            .map(|root| root.join(".vault").join("plugins").join(&entry.id));

        let is_installed = plugin_dir.as_ref().map(|d| d.exists()).unwrap_or(false);

        let is_enabled = if is_installed {
            self.vault_root
                .as_ref()
                .map(|root| {
                    let list_file = root.join(".vault").join("community-plugins.json");
                    std::fs::read_to_string(&list_file)
                        .ok()
                        .and_then(|c| serde_json::from_str::<Vec<String>>(&c).ok())
                        .map(|list| list.contains(&entry.id))
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        } else {
            false
        };

        let is_this_downloading = self.state.is_downloading
            && self.state.downloading_plugin_id.as_deref() == Some(&entry.id);

        let plugin_id = entry.id.clone();
        let plugin_id_uninstall = entry.id.clone();
        let plugin_id_toggle = entry.id.clone();
        let author_name = entry.author.clone();
        let author_url = format!("https://github.com/{}", entry.author);
        let repo_url = format!("https://github.com/{}", entry.repo);
        let author_click = author_url.clone();
        let repo_click = repo_url.clone();

        let author_line_prefix = if is_ru { "Автор: " } else { "Author: " };
        let repo_line_prefix = if is_ru { "Репозиторий: " } else { "Repository: " };
        let updated_line_prefix = if is_ru { "Последнее обновление: " } else { "Last updated: " };
        let updated_text = format_updated_date(entry.updated, is_ru);

        // Compatibility from the embedded corpus registry (static doctor scan of the
        // real top-1500 Obsidian corpus); absent for plugins outside the corpus.
        let compat = crate::plugins::compat::catalog_compat(&entry.id);
        let (compat_text, compat_bg, compat_fg, compat_detail) = match &compat {
            Some(record) => {
                let (text, bg, fg) = match record.status {
                    crate::plugins::compat::CompatibilityStatus::Full => (
                        if is_ru { "Совместим" } else { "Compatible" }.to_string(),
                        rgba(0x2ea44f22),
                        rgba(0x3fb950ff),
                    ),
                    crate::plugins::compat::CompatibilityStatus::Partial(score) => (
                        if is_ru {
                            format!("Совместим {:.0}%", score * 100.0)
                        } else {
                            format!("Compatible {:.0}%", score * 100.0)
                        },
                        rgba(0xdbab0922),
                        rgba(0xd29922ff),
                    ),
                    crate::plugins::compat::CompatibilityStatus::Incompatible => (
                        if is_ru { "Не совместим" } else { "Incompatible" }.to_string(),
                        rgba(0xf8514922),
                        rgba(0xf85149ff),
                    ),
                    crate::plugins::compat::CompatibilityStatus::Unknown => (
                        if is_ru { "Совместимость неизвестна" } else { "Compatibility unknown" }.to_string(),
                        rgba(0x8b949e22),
                        rgba(0x8b949eff),
                    ),
                };
                let mut detail = String::new();
                if !record.blockers.is_empty() {
                    detail = if is_ru {
                        format!("Требует: {}", record.blockers.join(", "))
                    } else {
                        format!("Requires: {}", record.blockers.join(", "))
                    };
                } else if !record.missing_exports.is_empty() {
                    detail = if is_ru {
                        format!("Нет API: {}", record.missing_exports.join(", "))
                    } else {
                        format!("Missing API: {}", record.missing_exports.join(", "))
                    };
                } else if !record.missing_app.is_empty() {
                    detail = if is_ru {
                        format!("Нет: app.{}", record.missing_app.join(", app."))
                    } else {
                        format!("Missing: app.{}", record.missing_app.join(", app."))
                    };
                }
                (text, bg, fg, detail)
            }
            None => (
                if is_ru { "Не проверен" } else { "Unverified" }.to_string(),
                rgba(0x8b949e18),
                theme.text_muted,
                String::new(),
            ),
        };

        // Top actions bar
        let top_actions_bar = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap_2()
            .when(!is_installed && !is_this_downloading, |parent| {
                parent.child(
                    div()
                        .id("btn-install-catalog-plugin")
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .bg(theme.accent)
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.accent_hover))
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_on_accent)
                        .child(if is_ru { "Установить" } else { "Install" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.install_plugin(&plugin_id, cx);
                        })),
                )
            })
            .when(!is_installed && is_this_downloading, |parent| {
                parent.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.accent)
                        .child(icons::icon(icons::ICON_REFRESH, 12.0, theme.accent))
                        .child(self.state.download_step.clone().unwrap_or_else(|| {
                            if is_ru { "Скачивание...".into() } else { "Downloading...".into() }
                        })),
                )
            })
            .when(is_installed, |parent| {
                parent
                    .child(
                        div()
                            .id("btn-toggle-catalog-plugin")
                            .px_3p5()
                            .py_2()
                            .rounded_md()
                            .bg(if is_enabled { theme.bg_card } else { theme.accent })
                            .border_1()
                            .border_color(theme.border_subtle)
                            .cursor_pointer()
                            .hover(|h| h.bg(if is_enabled { theme.bg_hover } else { theme.accent_hover }))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(if is_enabled { theme.text_primary } else { theme.text_on_accent })
                            .child(if is_enabled {
                                if is_ru { "Отключить" } else { "Disable" }
                            } else {
                                if is_ru { "Включить" } else { "Enable" }
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_plugin(&plugin_id_toggle, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-uninstall-catalog-plugin")
                            .px_3p5()
                            .py_2()
                            .rounded_md()
                            .bg(rgba(0xe0313122))
                            .border_1()
                            .border_color(rgba(0xe0313155))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgba(0xe0313144)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgba(0xff6b6bff))
                            .child(if is_ru { "Удалить" } else { "Uninstall" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.uninstall_plugin(&plugin_id_uninstall, cx);
                            })),
                    )
            });

        // Info Card
        let info_card = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_2p5()
            .p_5()
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.text_primary)
                                    .child(highlight_terms(&entry.name, &terms, theme.text_primary, true, theme)),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_full()
                                    .bg(theme.bg_editor)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .text_xs()
                                    .text_color(theme.text_secondary)
                                    .child(format!("v{}", entry.version)),
                            ),
                    )
                    .when(is_installed, |p| {
                        p.child(
                            div()
                                .px_2p5()
                                .py_1()
                                .rounded_full()
                                .bg(if is_enabled { rgba(0x34a85322) } else { theme.bg_editor })
                                .border_1()
                                .border_color(if is_enabled { rgba(0x34a85366) } else { theme.border_subtle })
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(if is_enabled { rgba(0x34a853ff) } else { theme.text_muted })
                                .child(if is_enabled {
                                    if is_ru { "АКТИВЕН" } else { "ACTIVE" }
                                } else {
                                    if is_ru { "ОТКЛЮЧЕН" } else { "DISABLED" }
                                }),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_x_4()
                    .gap_y_2()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(icons::icon(icons::ICON_DOWNLOAD, 13.0, theme.text_muted))
                            .child(format!("{} {}", format_download_count(entry.downloads), if is_ru { "загрузок" } else { "downloads" })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(div().child(author_line_prefix))
                            .child(
                                div()
                                    .id("catalog-author-link")
                                    .text_color(theme.accent)
                                    .cursor_pointer()
                                    .hover(|h| h.underline())
                                    .child(author_name)
                                    .on_click(move |_, _, cx| {
                                        cx.open_url(&author_click);
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(div().child(repo_line_prefix))
                            .child(
                                div()
                                    .id("catalog-repo-link")
                                    .text_color(theme.accent)
                                    .cursor_pointer()
                                    .hover(|h| h.underline())
                                    .child(entry.repo.clone())
                                    .on_click(move |_, _, cx| {
                                        cx.open_url(&repo_click);
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(div().child(updated_line_prefix))
                            .child(div().child(updated_text)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(div().child(if is_ru { "Совместимость: " } else { "Compatibility: " }))
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(compat_bg)
                                    .text_color(compat_fg)
                                    .child(compat_text.clone()),
                            ),
                    )
                    .when(!compat_detail.is_empty(), |parent| {
                        parent.child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(compat_detail.clone()),
                        )
                    }),
            )
            .child(
                div()
                    .w_full()
                    .text_sm()
                    .line_height(gpui::relative(1.5))
                    .text_color(theme.text_secondary)
                    .child(highlight_terms(&entry.description, &terms, theme.text_secondary, false, theme)),
            );

        // Permissions notice
        let permissions_card = div()
            .flex()
            .flex_row()
            .items_start()
            .gap_2p5()
            .p_3()
            .rounded_md()
            .bg(rgba(0x4285f411))
            .border_1()
            .border_color(rgba(0x4285f433))
            .child(
                div()
                    .pt_0p5()
                    .flex_shrink_0()
                    .child(icons::icon(icons::ICON_BOLT, 15.0, theme.accent)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .line_height(gpui::relative(1.4))
                    .text_color(theme.text_muted)
                    .child(if is_ru {
                        "Безопасная песочница: плагин запускается в изолированном JS-рантайме с гранулярными правами доступа."
                    } else {
                        "Sandbox isolation: Plugin executes in an isolated JS runtime with granular permission controls."
                    }),
            );

        // README Section
        let readme_text = self.state.readme_cache.get(&entry.id).cloned().or_else(|| {
            plugin_dir.as_ref().and_then(|d| {
                for name in &["README.md", "readme.md"] {
                    let f = d.join(name);
                    if f.is_file() {
                        if let Ok(c) = std::fs::read_to_string(f) {
                            return Some(c);
                        }
                    }
                }
                None
            })
        });

        let readme_section = if self.state.is_loading_readme {
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .p_8()
                .rounded_lg()
                .bg(theme.bg_card)
                .border_1()
                .border_color(theme.border_subtle)
                .gap_2()
                .child(icons::icon(icons::ICON_REFRESH, 18.0, theme.accent))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(if is_ru { "Загрузка README.md из GitHub..." } else { "Loading README.md from GitHub..." }),
                )
        } else if let Some(text) = readme_text {
            let rendered_markdown_lines = self.render_markdown_content(&text, cx);
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .p_6()
                .rounded_lg()
                .bg(theme.bg_card)
                .border_1()
                .border_color(theme.border_subtle)
                .gap_3()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child("README.MD"),
                )
                .children(rendered_markdown_lines)
        } else {
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .p_8()
                .rounded_lg()
                .bg(theme.bg_card)
                .border_1()
                .border_color(theme.border_subtle)
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(if is_ru { "README.md не найден" } else { "README.md not available" }),
                )
        };

        let scroll_container = div()
            .id("catalog-details-scroll")
            .track_scroll(&self.details_scroll_handle)
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .overflow_x_hidden()
            .bg(theme.bg_editor)
            .p_6()
            .pr_8()
            .gap_5()
            .child(top_actions_bar)
            .child(info_card)
            .child(permissions_card)
            .child(readme_section);

        let scroll_style = catalog_scrollbar_style();
        let mut viewport = div()
            .id("catalog-details-viewport")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .child(scroll_container);

        if let Some(geometry) = ScrollbarGeometry::from_handle(&self.details_scroll_handle, &scroll_style) {
            let dragging = self.details_scrollbar_drag.active;
            viewport = viewport.child(overlay_scrollbar(
                "catalog-details-scrollbar-track",
                &geometry,
                &scroll_style,
                theme,
                dragging,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let pointer_y: f32 = event.position.y.into();
                    let target_y = geometry.offset_for_track_click(pointer_y);
                    this.details_scroll_handle.set_offset(point(px(0.0), px(-target_y)));
                    cx.notify();
                }),
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.details_scrollbar_drag.begin(event.position.y.into(), geometry.scroll_y);
                    cx.notify();
                }),
            ));
        }

        viewport.into_any_element()
    }

    pub(super) fn render_markdown_content(&self, text: &str, cx: &Context<Self>) -> Vec<Div> {
        let theme = &self.theme;
        let mut rendered_markdown_lines: Vec<Div> = Vec::new();
        let mut lines_iter = text.lines().peekable();
        let mut line_counter = 0usize;

        while let Some(line) = lines_iter.next() {
            let trimmed = line.trim();

            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let fence = if trimmed.starts_with("```") { "```" } else { "~~~" };
                let lang = trimmed.trim_start_matches(fence).trim().to_string();
                let lang_opt = if !lang.is_empty() { Some(lang) } else { None };

                let mut code_lines = Vec::new();
                while let Some(next_line) = lines_iter.peek() {
                    let trimmed_peek = next_line.trim();
                    if trimmed_peek.starts_with(fence) {
                        lines_iter.next();
                        break;
                    }
                    code_lines.push(lines_iter.next().unwrap());
                }

                let code_str = code_lines.join("\n");
                rendered_markdown_lines.push(
                    div()
                        .flex()
                        .flex_col()
                        .my_1p5()
                        .rounded_md()
                        .bg(theme.bg_editor)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .overflow_x_hidden()
                        .when(lang_opt.is_some(), |parent| {
                            parent.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px_3()
                                    .py_1()
                                    .bg(theme.bg_sidebar)
                                    .border_b_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.text_muted)
                                            .child(lang_opt.unwrap()),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .p_3()
                                .font_family("Consolas")
                                .text_xs()
                                .line_height(px(18.0))
                                .text_color(theme.text_primary)
                                .child(code_str),
                        ),
                );
                continue;
            }

            if trimmed.is_empty() {
                rendered_markdown_lines.push(div().h(px(8.0)));
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("### ") {
                rendered_markdown_lines.push(
                    div()
                        .text_base()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .mt_3()
                        .mb_1()
                        .child(rest.to_string()),
                );
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("## ") {
                rendered_markdown_lines.push(
                    div()
                        .text_lg()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .mt_4()
                        .mb_1p5()
                        .child(rest.to_string()),
                );
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("# ") {
                rendered_markdown_lines.push(
                    div()
                        .text_xl()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .mt_4()
                        .mb_2()
                        .child(rest.to_string()),
                );
                continue;
            }

            if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
                let checked = !trimmed.starts_with("- [ ] ");
                let item_text = &trimmed[6..];
                let spans = crate::markdown::parse_inline_spans(item_text);
                let span_elements: Vec<AnyElement> = spans
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| self.render_markdown_inline_span(s, i, cx))
                    .collect();

                let checkbox = div()
                    .w(px(14.0))
                    .h(px(14.0))
                    .mt(px(2.0))
                    .rounded_xs()
                    .border_1()
                    .border_color(if checked { theme.accent } else { theme.border })
                    .bg(if checked { theme.accent } else { theme.bg_editor })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(checked, |c| c.child(icons::icon(icons::ICON_CHECK, 10.0, theme.text_on_accent)));

                rendered_markdown_lines.push(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap_2()
                        .my_0p5()
                        .child(checkbox)
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .flex_wrap()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .text_sm()
                                .children(span_elements),
                        ),
                );
                continue;
            }

            if let Some(bullet) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("+ ")) {
                let spans = crate::markdown::parse_inline_spans(bullet);
                let span_elements: Vec<AnyElement> = spans
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| self.render_markdown_inline_span(s, i, cx))
                    .collect();

                rendered_markdown_lines.push(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap_2()
                        .my_0p5()
                        .child(div().text_color(theme.accent).child("•"))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .flex_wrap()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .text_sm()
                                .children(span_elements),
                        ),
                );
                continue;
            }

            let spans = crate::markdown::parse_inline_spans(line);
            line_counter += 1;
            let span_elements: Vec<AnyElement> = spans
                .into_iter()
                .enumerate()
                .map(|(i, s)| self.render_markdown_inline_span(s, line_counter * 100 + i, cx))
                .collect();

            rendered_markdown_lines.push(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .text_sm()
                    .line_height(gpui::relative(1.4))
                    .children(span_elements),
            );
        }

        rendered_markdown_lines
    }

    pub(super) fn render_markdown_inline_span(
        &self,
        span: InlineSpan,
        _idx: usize,
        _cx: &Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        match span {
            InlineSpan::Text(t) => div().text_color(theme.text_secondary).child(t).into_any_element(),
            InlineSpan::Bold(t) => div().font_weight(gpui::FontWeight::BOLD).text_color(theme.text_primary).child(t).into_any_element(),
            InlineSpan::Italic(t) => div().italic().text_color(theme.text_secondary).child(t).into_any_element(),
            InlineSpan::BoldItalic(t) => div().font_weight(gpui::FontWeight::BOLD).italic().text_color(theme.text_primary).child(t).into_any_element(),
            InlineSpan::InlineCode(t) => div()
                .px_1p5()
                .py_0p5()
                .mx_0p5()
                .rounded_xs()
                .bg(theme.bg_editor)
                .border_1()
                .border_color(theme.border_subtle)
                .font_family("Consolas")
                .text_xs()
                .text_color(theme.accent)
                .child(t)
                .into_any_element(),
            InlineSpan::Link { label, url } => {
                let click_url = url.clone();
                div()
                    .id(ElementId::Name(format!("cat-md-link-{}", _idx).into()))
                    .text_color(theme.accent)
                    .cursor_pointer()
                    .hover(|h| h.underline())
                    .child(if label.is_empty() { url } else { label })
                    .on_click(move |_, _, cx| {
                        cx.open_url(&click_url);
                    })
                    .into_any_element()
            }
            InlineSpan::Strikethrough(t) => div().line_through().text_color(theme.text_muted).child(t).into_any_element(),
        }
    }

    pub(super) fn render_footer_status(&self, _cx: &Context<Self>) -> Div {
        let theme = &self.theme;
        let total = self.state.entries.len();
        let filtered = self.state.filtered_entries().len();

        let status_text = self.state.status_message.clone().unwrap_or_else(|| {
            if self.is_ru {
                format!("Всего плагинов: {} • Показано: {}", total, filtered)
            } else {
                format!("Total plugins: {} • Shown: {}", total, filtered)
            }
        });

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_4()
            .py_1p5()
            .bg(theme.bg_sidebar)
            .border_t_1()
            .border_color(theme.border_subtle)
            .text_xs()
            .text_color(theme.text_muted)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .when(self.state.is_loading || self.state.is_downloading, |p| {
                        p.child(icons::icon(icons::ICON_REFRESH, 12.0, theme.accent))
                    })
                    .child(status_text),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(format!("Vault: {}", self.vault_name)),
            )
    }
}

impl Render for PluginCatalogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let entries = self.state.filtered_entries();
        let selected_id = self.state.selected_plugin_id.clone();
        let selected_entry = entries
            .iter()
            .find(|e| Some(&e.id) == selected_id.as_ref())
            .or_else(|| entries.first());

        div()
            .id("plugin-catalog-window")
            .key_context("plugin_catalog_window")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg_editor)
            .text_color(theme.text_primary)
            .font_family("Segoe UI")
            .cursor(gpui::CursorStyle::Arrow)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_list_scrollbar_drag(y, cx);
                    cx.stop_propagation();
                } else if this.details_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_details_scrollbar_drag(y, cx);
                    cx.stop_propagation();
                } else if this.state.is_mouse_selecting {
                    this.handle_search_mouse_move(event, window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                let mut changed = false;
                if this.scrollbar_drag.active {
                    this.scrollbar_drag.end();
                    changed = true;
                }
                if this.details_scrollbar_drag.active {
                    this.details_scrollbar_drag.end();
                    changed = true;
                }
                if this.state.is_mouse_selecting {
                    this.handle_search_mouse_up(cx);
                    changed = true;
                }
                if changed {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(self.render_titlebar(window, cx))
            .child(self.render_header(window, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.render_list(&entries, cx))
                    .child(self.render_details(selected_entry, cx)),
            )
            .child(self.render_footer_status(cx))
    }
}
