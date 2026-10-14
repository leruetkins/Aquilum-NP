pub use crate::plugins::registry::CatalogPluginEntry;

/// Filter and sorting criteria for the catalog browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSortOrder {
    MostPopular,
    Alphabetical,
    RecentlyUpdated,
}

/// State of the Community Plugins Catalog modal dialog.
#[derive(Debug, Clone)]
pub struct PluginCatalogState {
    pub is_open: bool,
    pub search_query: String,
    pub sort_order: CatalogSortOrder,
    pub selected_plugin_id: Option<String>,
    pub entries: Vec<CatalogPluginEntry>,
    pub is_loading: bool,
    pub status_message: Option<String>,
    pub is_downloading: bool,
    pub downloading_plugin_id: Option<String>,
    pub download_step: Option<String>,
    pub readme_cache: std::collections::HashMap<String, String>,
    pub is_loading_readme: bool,
}

impl Default for PluginCatalogState {
    fn default() -> Self {
        let initial_entries = crate::plugins::registry::load_cached_community_plugins()
            .unwrap_or_else(Self::default_catalog);

        Self {
            is_open: false,
            search_query: String::new(),
            sort_order: CatalogSortOrder::MostPopular,
            selected_plugin_id: None,
            entries: initial_entries,
            is_loading: false,
            status_message: None,
            is_downloading: false,
            downloading_plugin_id: None,
            download_step: None,
            readme_cache: std::collections::HashMap::new(),
            is_loading_readme: false,
        }
    }
}

impl PluginCatalogState {
    /// Default built-in registry entries available offline or while loading remote registry.
    pub fn default_catalog() -> Vec<CatalogPluginEntry> {
        vec![
            CatalogPluginEntry {
                id: "dataview".to_string(),
                name: "Dataview".to_string(),
                author: "blacksmithgu".to_string(),
                description: "Complex queries, JavaScript APIs, and dynamic tables over your markdown vault.".to_string(),
                repo: "blacksmithgu/obsidian-dataview".to_string(),
                branch: None,
                version: "0.5.68".to_string(),
                downloads: 1450000,
                updated: Some(1705000000000),
            },
            CatalogPluginEntry {
                id: "calendar".to_string(),
                name: "Calendar".to_string(),
                author: "liamcain".to_string(),
                description: "Simple calendar view in the sidebar to visualize and navigate daily notes.".to_string(),
                repo: "liamcain/obsidian-calendar-plugin".to_string(),
                branch: None,
                version: "1.5.10".to_string(),
                downloads: 890000,
                updated: Some(1698000000000),
            },
            CatalogPluginEntry {
                id: "kanban".to_string(),
                name: "Kanban".to_string(),
                author: "mgmeyers".to_string(),
                description: "Create Kanban boards inside notes with drag-and-drop cards and lists.".to_string(),
                repo: "mgmeyers/obsidian-kanban".to_string(),
                branch: None,
                version: "2.0.51".to_string(),
                downloads: 980000,
                updated: Some(1712000000000),
            },
            CatalogPluginEntry {
                id: "tasks".to_string(),
                name: "Tasks".to_string(),
                author: "obsidian-tasks-group".to_string(),
                description: "Task management, recurring task schedules, and checklist queries across all notes.".to_string(),
                repo: "obsidian-tasks-group/obsidian-tasks".to_string(),
                branch: None,
                version: "7.19.0".to_string(),
                downloads: 720000,
                updated: Some(1720000000000),
            },
            CatalogPluginEntry {
                id: "linter".to_string(),
                name: "Linter".to_string(),
                author: "platers".to_string(),
                description: "Format and lint markdown files according to customizable style rules.".to_string(),
                repo: "platers/obsidian-linter".to_string(),
                branch: None,
                version: "1.26.0".to_string(),
                downloads: 540000,
                updated: Some(1722000000000),
            },
        ]
    }

    /// Returns filtered and sorted list of catalog entries according to current search query.
    pub fn filtered_entries(&self) -> Vec<CatalogPluginEntry> {
        let q = self.search_query.trim().to_lowercase();
        let mut list: Vec<CatalogPluginEntry> = self
            .entries
            .iter()
            .filter(|e| {
                if q.is_empty() {
                    true
                } else {
                    e.name.to_lowercase().contains(&q)
                        || e.id.to_lowercase().contains(&q)
                        || e.author.to_lowercase().contains(&q)
                        || e.description.to_lowercase().contains(&q)
                }
            })
            .cloned()
            .collect();

        match self.sort_order {
            CatalogSortOrder::MostPopular => {
                list.sort_by(|a, b| b.downloads.cmp(&a.downloads));
            }
            CatalogSortOrder::Alphabetical => {
                list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
            CatalogSortOrder::RecentlyUpdated => {
                list.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| b.version.cmp(&a.version)));
            }
        }
        list
    }
}

use crate::icons;
use crate::views::app_view::AppView;
use crate::views::scrollbar::{overlay_scrollbar, ScrollbarGeometry, ScrollbarStyle};
use gpui::prelude::*;
use gpui::{
    div, point, px, rgba, AnyElement, Context, ElementId, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement,
    ScrollWheelEvent, SharedString, StatefulInteractiveElement, Styled, Window,
};

/// Row height in pixels for each entry in the community plugins list.
pub const CATALOG_ROW_H: f32 = 106.0;
/// Overscan rows above and below the visible viewport to guarantee smooth 120 FPS scrolling.
pub const CATALOG_OVERSCAN_ROWS: usize = 4;

/// Formats large download counts into human-readable strings (e.g. 8.4M, 145K, 950).
pub fn format_download_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{}K", count / 1_000)
    } else {
        count.to_string()
    }
}

/// Formats a unix timestamp (in milliseconds or seconds) into a localized relative time string.
pub fn format_updated_date(timestamp_ms_opt: Option<u64>, is_ru: bool) -> String {
    let Some(raw_ts) = timestamp_ms_opt else {
        return if is_ru { "неизвестно".to_string() } else { "unknown".to_string() };
    };

    let ts_sec = if raw_ts > 10_000_000_000 {
        raw_ts / 1000
    } else {
        raw_ts
    };

    let now_sec = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(ts_sec);

    let diff_sec = now_sec.saturating_sub(ts_sec);
    let diff_days = diff_sec / 86400;

    if diff_days == 0 {
        if diff_sec < 3600 {
            let mins = (diff_sec / 60).max(1);
            if is_ru {
                format!("{} мин. назад", mins)
            } else {
                format!("{} min ago", mins)
            }
        } else {
            let hours = diff_sec / 3600;
            if is_ru {
                format!("{} ч. назад", hours)
            } else {
                format!("{} hours ago", hours)
            }
        }
    } else if diff_days == 1 {
        if is_ru { "вчера".to_string() } else { "yesterday".to_string() }
    } else if diff_days < 7 {
        if is_ru {
            format!("{} дн. назад", diff_days)
        } else {
            format!("{} days ago", diff_days)
        }
    } else if diff_days < 30 {
        let weeks = diff_days / 7;
        if is_ru {
            format!("{} нед. назад", weeks)
        } else {
            format!("{} weeks ago", weeks)
        }
    } else if diff_days < 365 {
        let months = diff_days / 30;
        if is_ru {
            format!("{} мес. назад", months)
        } else {
            format!("{} months ago", months)
        }
    } else {
        let years = diff_days / 365;
        if is_ru {
            format!("{} г. назад", years)
        } else {
            format!("{} years ago", years)
        }
    }
}

/// Scrollbar styling for the plugin catalog list.
pub fn catalog_scrollbar_style() -> ScrollbarStyle {
    ScrollbarStyle {
        track_w: 10.0,
        thumb_w: 5.0,
        thumb_w_active: 7.0,
        thumb_inset: 2.0,
        min_thumb_h: 24.0,
        track_right_offset: 2.0,
    }
}

impl AppView {
    /// Opens the Community Plugins catalog modal.
    pub fn open_plugin_catalog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.plugin_catalog.is_open = true;
        self.plugin_catalog.search_query.clear();
        self.catalog_details_scroll_handle.set_offset(point(px(0.0), px(0.0)));
        if self.plugin_catalog.selected_plugin_id.is_none() {
            if let Some(first) = self.plugin_catalog.entries.first() {
                self.plugin_catalog.selected_plugin_id = Some(first.id.clone());
            }
        }
        if let Some(sel_id) = self.plugin_catalog.selected_plugin_id.clone() {
            self.ensure_catalog_plugin_readme_loaded(&sel_id, cx);
        }
        window.focus(&self.catalog_focus_handle);

        // If catalog entries are only the fallback defaults, automatically trigger background fetch from GitHub
        if self.plugin_catalog.entries.len() <= 5 && !self.plugin_catalog.is_loading {
            self.refresh_remote_plugin_catalog(cx);
        }

        cx.notify();
    }

    /// Closes the Community Plugins catalog modal.
    pub fn close_plugin_catalog(&mut self, cx: &mut Context<Self>) {
        self.plugin_catalog.is_open = false;
        cx.notify();
    }

    /// Performs an asynchronous background fetch of the live Obsidian registry (~2000+ plugins)
    /// without blocking the UI.
    pub fn refresh_remote_plugin_catalog(&mut self, cx: &mut Context<Self>) {
        if self.plugin_catalog.is_loading {
            return;
        }

        self.plugin_catalog.is_loading = true;
        self.plugin_catalog.status_message = Some("Загрузка реестра с GitHub...".to_string());
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
                    view.plugin_catalog.is_loading = false;
                    match res {
                        Ok(entries) => {
                            let count = entries.len();
                            view.plugin_catalog.entries = entries;
                            view.plugin_catalog.status_message =
                                Some(format!("Загружено плагинов: {}", count));
                            if view.plugin_catalog.selected_plugin_id.is_none() {
                                if let Some(first) = view.plugin_catalog.entries.first() {
                                    view.plugin_catalog.selected_plugin_id = Some(first.id.clone());
                                }
                            }
                        }
                        Err(err) => {
                            view.plugin_catalog.status_message =
                                Some(format!("Ошибка сети: {}. Используется кэш.", err));
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
    }

    /// Follows the pointer while the plugin catalog scrollbar thumb is dragged.
    pub fn handle_catalog_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.catalog_scrollbar_drag.active {
            return;
        }
        let style = catalog_scrollbar_style();
        let Some(geometry) = ScrollbarGeometry::from_handle(&self.catalog_scroll_handle, &style) else {
            return;
        };
        geometry.apply_drag(&self.catalog_scroll_handle, &self.catalog_scrollbar_drag, pointer_y);
        cx.notify();
    }

    /// Follows the pointer while the plugin catalog details scrollbar thumb is dragged.
    pub fn handle_catalog_details_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.catalog_details_scrollbar_drag.active {
            return;
        }
        let style = catalog_scrollbar_style();
        let Some(geometry) = ScrollbarGeometry::from_handle(&self.catalog_details_scroll_handle, &style) else {
            return;
        };
        geometry.apply_drag(&self.catalog_details_scroll_handle, &self.catalog_details_scrollbar_drag, pointer_y);
        cx.notify();
    }

    /// Sets the vertical scroll offset of the catalog list.
    pub fn set_catalog_scroll_top(&mut self, top: f32) {
        self.catalog_scroll_handle.set_offset(point(px(0.0), px(-top.max(0.0))));
    }

    /// Sets the vertical scroll offset of the catalog details view.
    pub fn set_catalog_details_scroll_top(&mut self, top: f32) {
        self.catalog_details_scroll_handle.set_offset(point(px(0.0), px(-top.max(0.0))));
    }

    /// Automatically scrolls the catalog list to ensure the selected plugin is in view.
    pub fn scroll_catalog_to_selected(&mut self, selected_idx: usize) {
        let item_top = selected_idx as f32 * CATALOG_ROW_H;
        let item_bottom = item_top + CATALOG_ROW_H;
        let viewport_h = f32::from(self.catalog_scroll_handle.bounds().size.height);
        if viewport_h <= 0.0 {
            return;
        }
        let current_scroll = -f32::from(self.catalog_scroll_handle.offset().y);

        if item_top < current_scroll {
            self.set_catalog_scroll_top(item_top);
        } else if item_bottom > current_scroll + viewport_h {
            self.set_catalog_scroll_top(item_bottom - viewport_h);
        }
    }

    /// Selects a plugin in the catalog and immediately starts background loading of its README.md if not cached.
    pub fn select_catalog_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        self.plugin_catalog.selected_plugin_id = Some(plugin_id.to_string());
        self.catalog_details_scroll_handle.set_offset(point(px(0.0), px(0.0)));
        self.ensure_catalog_plugin_readme_loaded(plugin_id, cx);
        cx.notify();
    }

    /// Checks if README.md for the given plugin is available locally or in cache; if not, fetches it asynchronously from GitHub.
    pub fn ensure_catalog_plugin_readme_loaded(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        if self.plugin_catalog.readme_cache.contains_key(plugin_id) {
            return;
        }

        // Check local disk first if vault exists
        if let Some(vault) = &self.state.vault {
            let p_dir = vault.root.join(".vault").join("plugins").join(plugin_id);
            for name in &["README.md", "readme.md"] {
                let file = p_dir.join(name);
                if file.is_file() {
                    if let Ok(content) = std::fs::read_to_string(&file) {
                        self.plugin_catalog.readme_cache.insert(plugin_id.to_string(), content);
                        return;
                    }
                }
            }
        }

        // Find repository in catalog entries
        let Some(entry) = self.plugin_catalog.entries.iter().find(|e| e.id == plugin_id) else {
            return;
        };

        let repo = entry.repo.clone();
        let p_id = plugin_id.to_string();
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let bg_executor = cx.background_executor().clone();

        self.plugin_catalog.is_loading_readme = true;

        cx.foreground_executor()
            .spawn(async move {
                let res = bg_executor
                    .spawn(async move { crate::plugins::registry::fetch_readme_for_plugin(&repo) })
                    .await;

                let _ = entity.update(&mut async_cx, move |view, cx| {
                    view.plugin_catalog.is_loading_readme = false;
                    match res {
                        Ok(content) => {
                            view.plugin_catalog.readme_cache.insert(p_id, content);
                        }
                        Err(_) => {
                            view.plugin_catalog.readme_cache.insert(
                                p_id,
                                "# Описание отсутствует\n\nНе удалось загрузить README.md из репозитория GitHub.".to_string(),
                            );
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
    }

    /// Handles keyboard events while the plugin catalog is focused.
    fn handle_catalog_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        if key == "escape" {
            self.close_plugin_catalog(cx);
            return;
        }

        let filtered = self.plugin_catalog.filtered_entries();

        // Arrow Up navigation
        if key == "up" {
            if !filtered.is_empty() {
                let current_idx = filtered
                    .iter()
                    .position(|e| Some(&e.id) == self.plugin_catalog.selected_plugin_id.as_ref())
                    .unwrap_or(0);
                let new_idx = current_idx.saturating_sub(1);
                let new_id = filtered[new_idx].id.clone();
                self.plugin_catalog.selected_plugin_id = Some(new_id.clone());
                self.catalog_details_scroll_handle.set_offset(point(px(0.0), px(0.0)));
                self.scroll_catalog_to_selected(new_idx);
                self.ensure_catalog_plugin_readme_loaded(&new_id, cx);
                cx.notify();
            }
            return;
        }

        // Arrow Down navigation
        if key == "down" {
            if !filtered.is_empty() {
                let current_idx = filtered
                    .iter()
                    .position(|e| Some(&e.id) == self.plugin_catalog.selected_plugin_id.as_ref())
                    .unwrap_or(0);
                let new_idx = (current_idx + 1).min(filtered.len().saturating_sub(1));
                let new_id = filtered[new_idx].id.clone();
                self.plugin_catalog.selected_plugin_id = Some(new_id.clone());
                self.catalog_details_scroll_handle.set_offset(point(px(0.0), px(0.0)));
                self.scroll_catalog_to_selected(new_idx);
                self.ensure_catalog_plugin_readme_loaded(&new_id, cx);
                cx.notify();
            }
            return;
        }

        if key == "backspace" {
            self.plugin_catalog.search_query.pop();
            let filtered = self.plugin_catalog.filtered_entries();
            if !filtered.iter().any(|e| Some(&e.id) == self.plugin_catalog.selected_plugin_id.as_ref()) {
                self.plugin_catalog.selected_plugin_id = filtered.first().map(|e| e.id.clone());
            }
            self.set_catalog_scroll_top(0.0);
            cx.notify();
            return;
        }

        if let Some(ch) = &event.keystroke.key_char {
            let filtered_chars: String = ch
                .chars()
                .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                .collect();
            if !filtered_chars.is_empty() {
                self.plugin_catalog.search_query.push_str(&filtered_chars);
                let filtered = self.plugin_catalog.filtered_entries();
                if !filtered.iter().any(|e| Some(&e.id) == self.plugin_catalog.selected_plugin_id.as_ref()) {
                    self.plugin_catalog.selected_plugin_id = filtered.first().map(|e| e.id.clone());
                }
                self.set_catalog_scroll_top(0.0);
                cx.notify();
            }
        } else if key == "space" {
            self.plugin_catalog.search_query.push(' ');
            let filtered = self.plugin_catalog.filtered_entries();
            if !filtered.iter().any(|e| Some(&e.id) == self.plugin_catalog.selected_plugin_id.as_ref()) {
                self.plugin_catalog.selected_plugin_id = filtered.first().map(|e| e.id.clone());
            }
            self.set_catalog_scroll_top(0.0);
            cx.notify();
        } else if key.chars().count() == 1
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
        {
            self.plugin_catalog.search_query.push_str(key);
            let filtered = self.plugin_catalog.filtered_entries();
            if !filtered.iter().any(|e| Some(&e.id) == self.plugin_catalog.selected_plugin_id.as_ref()) {
                self.plugin_catalog.selected_plugin_id = filtered.first().map(|e| e.id.clone());
            }
            self.set_catalog_scroll_top(0.0);
            cx.notify();
        }
    }

    /// Real asynchronous installation of a community plugin from GitHub Releases.
    /// Downloads manifest.json, main.js, styles.css and README.md into `<vault>/.vault/plugins/<id>/`
    /// without freezing the UI, and automatically activates the plugin.
    pub fn install_plugin_from_catalog(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self
            .plugin_catalog
            .entries
            .iter()
            .find(|e| e.id == plugin_id)
            .cloned()
        else {
            return;
        };
        let Some(vault) = &self.state.vault else {
            self.state.status_text = "Cannot install plugin: No vault opened".to_string();
            cx.notify();
            return;
        };

        if self.plugin_catalog.is_downloading {
            return;
        }

        self.plugin_catalog.is_downloading = true;
        self.plugin_catalog.downloading_plugin_id = Some(entry.id.clone());
        self.plugin_catalog.download_step = Some("Подключение к GitHub Releases...".to_string());
        cx.notify();

        let plugin_dir = vault.root.join(".vault").join("plugins").join(&entry.id);
        let repo = entry.repo.clone();
        let plugin_name = entry.name.clone();
        let entry_id = entry.id.clone();
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let bg_executor = cx.background_executor().clone();

        cx.foreground_executor()
            .spawn(async move {
                let p_dir = plugin_dir.clone();
                let res = bg_executor
                    .spawn(async move {
                        crate::plugins::registry::download_plugin_release(&repo, &p_dir, |_step| {})
                    })
                    .await;

                let _ = entity.update(&mut async_cx, move |view, cx| {
                    view.plugin_catalog.is_downloading = false;
                    view.plugin_catalog.downloading_plugin_id = None;
                    view.plugin_catalog.download_step = None;

                    match res {
                        Ok(()) => {
                            if let Some(vault) = &view.state.vault {
                                let _ = vault.enable_community_plugin(&entry_id);
                                if !view.config.community_plugins.safe_mode {
                                    view.load_installed_plugins_for_vault();
                                }
                                view.state.status_text =
                                    format!("Плагин '{}' успешно установлен и активирован", plugin_name);
                            }
                        }
                        Err(e) => {
                            view.state.status_text =
                                format!("Ошибка установки плагина '{}': {}", plugin_name, e);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
    }

    /// Uninstalls an installed plugin by removing its directory and disabling it.
    pub fn uninstall_plugin_from_catalog(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let Some(vault) = &self.state.vault else {
            return;
        };

        let _ = vault.disable_community_plugin(plugin_id);

        // Unload runtime if active
        if let Some(pos) = self.plugin_runtimes.iter().position(|r| r.manifest.id == plugin_id) {
            let handle = self.plugin_runtimes.remove(pos);
            let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
            self.state.plugin_host.unregister_plugin(plugin_id);
            self.state.commands.unregister_all_of(plugin_id);
            self.state.ribbon.unregister_all_of(plugin_id);
            self.state.status_bar.unregister_all_of(plugin_id);
            self.state.code_block_processors.unregister_all_of(plugin_id);
            self.code_block_views.retain(|_, (pid, _)| pid != plugin_id);
        }

        let plugin_dir = vault.root.join(".vault").join("plugins").join(plugin_id);
        let _ = std::fs::remove_dir_all(&plugin_dir);

        self.state.status_text = format!("Uninstalled plugin '{}'", plugin_id);
        cx.notify();
    }

    /// Renders the Community Plugins Catalog modal window.
    pub fn render_plugin_catalog_modal(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let entries = self.plugin_catalog.filtered_entries();
        let selected_id = self.plugin_catalog.selected_plugin_id.clone();
        let selected_entry = entries
            .iter()
            .find(|e| Some(&e.id) == selected_id.as_ref())
            .or_else(|| entries.first());

        div()
            .id("plugin-catalog-root")
            .key_context("plugin_catalog")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .cursor(gpui::CursorStyle::Arrow)
            .track_focus(&self.catalog_focus_handle)
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.handle_catalog_key(event, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.catalog_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_catalog_scrollbar_drag(y, cx);
                    cx.stop_propagation();
                } else if this.catalog_details_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_catalog_details_scrollbar_drag(y, cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                let mut changed = false;
                if this.catalog_scrollbar_drag.active {
                    this.catalog_scrollbar_drag.end();
                    changed = true;
                }
                if this.catalog_details_scrollbar_drag.active {
                    this.catalog_details_scrollbar_drag.end();
                    changed = true;
                }
                if changed {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            // Backdrop
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(rgba(0x000000aa))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_plugin_catalog(cx);
                    })),
            )
            // Centered Catalog Dialog Window
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("plugin-catalog-dialog")
                            .flex()
                            .flex_col()
                            .w(px(920.0))
                            .h(px(640.0))
                            .max_w(px(960.0))
                            .max_h(px(720.0))
                            .rounded_xl()
                            .overflow_hidden()
                            .bg(theme.bg_sidebar)
                            .border_1()
                            .border_color(theme.border)
                            .shadow_lg()
                            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                                if this.catalog_scrollbar_drag.active {
                                    let y: f32 = event.position.y.into();
                                    this.handle_catalog_scrollbar_drag(y, cx);
                                    cx.stop_propagation();
                                } else if this.catalog_details_scrollbar_drag.active {
                                    let y: f32 = event.position.y.into();
                                    this.handle_catalog_details_scrollbar_drag(y, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                let mut changed = false;
                                if this.catalog_scrollbar_drag.active {
                                    this.catalog_scrollbar_drag.end();
                                    changed = true;
                                }
                                if this.catalog_details_scrollbar_drag.active {
                                    this.catalog_details_scrollbar_drag.end();
                                    changed = true;
                                }
                                if changed {
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_scroll_wheel(|_, _, cx| {
                                cx.stop_propagation();
                            })
                            // Catalog Header with Search
                            .child(self.render_catalog_header(cx))
                            // Catalog Body: List (left) + Details (right)
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_1()
                                    .min_h_0()
                                    .w_full()
                                    .child(self.render_catalog_list(&entries, cx))
                                    .child(self.render_catalog_details(selected_entry, cx)),
                            ),
                    ),
            )
    }

    /// Renders the title bar, search input, count, and sort toggles of the Catalog modal.
    fn render_catalog_header(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let is_ru = self.config.ui.language.starts_with("ru");
        let query = self.plugin_catalog.search_query.clone();
        let total_count = self.plugin_catalog.entries.len();
        let filtered_count = self.plugin_catalog.filtered_entries().len();
        let is_loading = self.plugin_catalog.is_loading;

        div()
            .flex()
            .flex_col()
            .px_5()
            .py_3()
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
                            .child(icons::icon(icons::ICON_GLOBE, 18.0, theme.accent))
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
                                    .px_2()
                                    .py_0p5()
                                    .rounded_full()
                                    .bg(theme.bg_hover)
                                    .text_color(theme.text_secondary)
                                    .child(format!(
                                        "{} {}",
                                        total_count,
                                        if is_ru { "плагинов" } else { "plugins" }
                                    )),
                            )
                            .when(is_loading, |parent| {
                                parent.child(
                                    div()
                                        .text_xs()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_full()
                                        .bg(theme.accent)
                                        .text_color(theme.text_on_accent)
                                        .child(if is_ru {
                                            "Загрузка с GitHub..."
                                        } else {
                                            "Fetching from GitHub..."
                                        }),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            // Refresh registry button
                            .child(
                                div()
                                    .id("btn-refresh-plugin-catalog")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(theme.bg_hover)
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.border))
                                    .child(icons::icon(icons::ICON_REFRESH, 13.0, theme.text_secondary))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_primary)
                                            .child(if is_ru { "Обновить" } else { "Refresh" }),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_remote_plugin_catalog(cx);
                                    })),
                            )
                            // Close dialog button
                            .child(
                                div()
                                    .id("btn-close-plugin-catalog")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .p_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_CLOSE, 16.0, theme.text_muted))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_plugin_catalog(cx);
                                    })),
                            ),
                    ),
            )
            // Search Bar & Sort options
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    // Search input box
                    .child(
                        div()
                            .id("catalog-search-input-box")
                            .flex()
                            .flex_1()
                            .flex_row()
                            .items_center()
                            .bg(theme.bg_editor)
                            .border_1()
                            .border_color(theme.border)
                            .rounded_md()
                            .px_3()
                            .py_1p5()
                            .gap_2()
                            .cursor_text()
                            .on_click(cx.listener(|this, _, window, _cx| {
                                window.focus(&this.catalog_focus_handle);
                            }))
                            .child(icons::icon(icons::ICON_SEARCH, 14.0, theme.text_muted))
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .text_color(if query.is_empty() {
                                        theme.text_muted
                                    } else {
                                        theme.text_primary
                                    })
                                    .child(if query.is_empty() {
                                        if is_ru {
                                            "Поиск по названию, автору или описанию...".to_string()
                                        } else {
                                            "Search community plugins...".to_string()
                                        }
                                    } else {
                                        query.clone()
                                    }),
                            )
                            .when(!query.is_empty(), |parent| {
                                parent.child(
                                    div()
                                        .id("btn-clear-catalog-search")
                                        .cursor_pointer()
                                        .hover(|h| h.text_color(theme.text_primary))
                                        .child(icons::icon(icons::ICON_CLOSE, 12.0, theme.text_muted))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.plugin_catalog.search_query.clear();
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    // Showing count
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(if is_ru {
                                format!("Найдено: {}", filtered_count)
                            } else {
                                format!("Found: {}", filtered_count)
                            }),
                    )
                    // Sort order pills
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .id("sort-catalog-popular")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.plugin_catalog.sort_order == CatalogSortOrder::MostPopular {
                                        theme.accent
                                    } else {
                                        theme.bg_hover
                                    })
                                    .text_color(if self.plugin_catalog.sort_order == CatalogSortOrder::MostPopular {
                                        theme.text_on_accent
                                    } else {
                                        theme.text_secondary
                                    })
                                    .text_xs()
                                    .cursor_pointer()
                                    .child(if is_ru { "Популярные" } else { "Popular" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.plugin_catalog.sort_order = CatalogSortOrder::MostPopular;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("sort-catalog-alpha")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.plugin_catalog.sort_order == CatalogSortOrder::Alphabetical {
                                        theme.accent
                                    } else {
                                        theme.bg_hover
                                    })
                                    .text_color(if self.plugin_catalog.sort_order == CatalogSortOrder::Alphabetical {
                                        theme.text_on_accent
                                    } else {
                                        theme.text_secondary
                                    })
                                    .text_xs()
                                    .cursor_pointer()
                                    .child(if is_ru { "А-Я" } else { "A-Z" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.plugin_catalog.sort_order = CatalogSortOrder::Alphabetical;
                                        cx.notify();
                                     })),
                            )
                            .child(
                                div()
                                    .id("sort-catalog-updated")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.plugin_catalog.sort_order == CatalogSortOrder::RecentlyUpdated {
                                        theme.accent
                                    } else {
                                        theme.bg_hover
                                    })
                                    .text_color(if self.plugin_catalog.sort_order == CatalogSortOrder::RecentlyUpdated {
                                        theme.text_on_accent
                                    } else {
                                        theme.text_secondary
                                    })
                                    .text_xs()
                                    .cursor_pointer()
                                    .child(if is_ru { "Обновленные" } else { "Updated" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.plugin_catalog.sort_order = CatalogSortOrder::RecentlyUpdated;
                                        cx.notify();
                                     })),
                            ),
                    ),
            )
    }

    /// Renders the left virtualized scrollable list of plugin entries in the catalog.
    fn render_catalog_list(&self, entries: &[CatalogPluginEntry], cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let selected_id = self.plugin_catalog.selected_plugin_id.as_deref().unwrap_or("");
        let is_ru = self.config.ui.language.starts_with("ru");

        let total_items = entries.len();
        let total_height = total_items as f32 * CATALOG_ROW_H;

        // Viewport bounds & current scroll offset
        let viewport_h = f32::from(self.catalog_scroll_handle.bounds().size.height).max(100.0);
        let max_offset = f32::from(self.catalog_scroll_handle.max_offset().height);
        let current_scroll_y = (-f32::from(self.catalog_scroll_handle.offset().y)).clamp(0.0, max_offset.max(0.0));

        // Calculate virtual window slice
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

        for (rel_idx, entry) in entries[visible_start..visible_end].iter().enumerate() {
            let actual_idx = visible_start + rel_idx;
            let is_selected = entry.id == selected_id;
            let is_installed = self
                .state
                .vault
                .as_ref()
                .map(|v| v.root.join(".vault").join("plugins").join(&entry.id).exists())
                .unwrap_or(false);

            let entry_id = entry.id.clone();

            let dl_formatted = format_download_count(entry.downloads);
            let author_display = format!("{}: {}", if is_ru { "Автор" } else { "Author" }, entry.author);

            let item_el = div()
                .id(ElementId::from(SharedString::from(format!("cat-item-{}", entry.id))))
                .flex()
                .flex_col()
                .gap_1()
                .h(px(100.0))
                .mb(px(6.0))
                .px_3()
                .py_2()
                .rounded_lg()
                .border_1()
                .border_color(theme.border_subtle)
                .bg(if is_selected {
                    theme.bg_hover
                } else if theme.is_dark {
                    theme.bg_card
                } else {
                    gpui::rgba(0xffffffff)
                })
                .hover(|h| h.bg(theme.bg_hover))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.select_catalog_plugin(&entry_id, cx);
                    this.scroll_catalog_to_selected(actual_idx);
                }))
                // Top row: Plug icon (larger, on the left) + Title + Downloads (with download icon) & Installed badge (on the right)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        // Left: Plug icon + Plugin Name
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
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(theme.text_primary)
                                        .child(entry.name.clone()),
                                ),
                        )
                        // Right: Installed badge + Download count with download icon
                        .child(
                            div()
                                .flex_shrink_0()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .when(is_installed, |parent| {
                                    parent.child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_md()
                                            .bg(rgba(0x7c4dffcc))
                                            .text_color(gpui::white())
                                            .child(if is_ru { "УСТАНОВЛЕН" } else { "INSTALLED" }),
                                    )
                                })
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
                                ),
                        ),
                )
                // Snippet / description (like file search snippet)
                .child(
                    div()
                        .text_xs()
                        .line_height(px(18.0))
                        .text_color(theme.text_secondary)
                        .line_clamp(2)
                        .child(entry.description.clone()),
                )
                // Author row
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .truncate()
                        .child(author_display),
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
            .id("catalog-items-scroll")
            .track_scroll(&self.catalog_scroll_handle)
            .flex()
            .flex_col()
            .size_full()
            .overflow_scroll()
            .p_2()
            .pr_3() // leave breathing room for overlay scrollbar
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let line_h = 36.0;
                let dy = match event.delta {
                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                    gpui::ScrollDelta::Lines(l) => l.y * line_h,
                };
                if dy != 0.0 {
                    let cur = f32::from(this.catalog_scroll_handle.offset().y);
                    let max = f32::from(this.catalog_scroll_handle.max_offset().height);
                    let next = (cur + dy).clamp(-max, 0.0);
                    if next != cur {
                        this.catalog_scroll_handle.set_offset(point(px(0.0), px(next)));
                        cx.notify();
                    }
                }
                cx.stop_propagation();
            }))
            .children(item_elements);

        let scroll_style = catalog_scrollbar_style();
        let mut viewport = div()
            .id("catalog-list-viewport")
            .relative()
            .flex()
            .flex_col()
            .w(px(340.0))
            .min_w(px(340.0))
            .h_full()
            .border_r_1()
            .border_color(theme.border_subtle)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let line_h = 36.0;
                let dy = match event.delta {
                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                    gpui::ScrollDelta::Lines(l) => l.y * line_h,
                };
                if dy != 0.0 {
                    let cur = f32::from(this.catalog_scroll_handle.offset().y);
                    let max = f32::from(this.catalog_scroll_handle.max_offset().height);
                    let next = (cur + dy).clamp(-max, 0.0);
                    if next != cur {
                        this.catalog_scroll_handle.set_offset(point(px(0.0), px(next)));
                        cx.notify();
                    }
                }
                cx.stop_propagation();
            }))
            .child(scroll_container);

        if let Some(geometry) = ScrollbarGeometry::from_handle(&self.catalog_scroll_handle, &scroll_style) {
            let dragging = self.catalog_scrollbar_drag.active;
            viewport = viewport.child(overlay_scrollbar(
                "catalog-scrollbar-track",
                &geometry,
                &scroll_style,
                theme,
                dragging,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let pointer_y: f32 = event.position.y.into();
                    this.set_catalog_scroll_top(geometry.offset_for_track_click(pointer_y));
                    cx.notify();
                }),
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.catalog_scrollbar_drag.begin(event.position.y.into(), geometry.scroll_y);
                    cx.notify();
                }),
            ));
        }

        viewport
    }

    /// Renders the right detail preview pane of the currently selected plugin.
    fn render_catalog_details(
        &self,
        entry_opt: Option<&CatalogPluginEntry>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        let is_ru = self.config.ui.language.starts_with("ru");

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
            .state
            .vault
            .as_ref()
            .map(|v| v.root.join(".vault").join("plugins").join(&entry.id));

        let is_installed = plugin_dir.as_ref().map(|d| d.exists()).unwrap_or(false);

        let installed_version = if is_installed {
            plugin_dir.as_ref().and_then(|d| {
                let manifest_path = d.join("manifest.json");
                std::fs::read_to_string(manifest_path).ok().and_then(|c| {
                    serde_json::from_str::<serde_json::Value>(&c).ok().and_then(|j| {
                        j.get("version").and_then(|v| v.as_str()).map(|s| s.to_string())
                    })
                })
            })
        } else {
            None
        };

        let is_enabled = self
            .state
            .vault
            .as_ref()
            .map(|v| v.is_community_plugin_enabled(&entry.id))
            .unwrap_or(false);

        let is_this_downloading = self.plugin_catalog.is_downloading
            && self.plugin_catalog.downloading_plugin_id.as_deref() == Some(&entry.id);

        let plugin_id = entry.id.clone();
        let plugin_id_uninstall = entry.id.clone();
        let plugin_id_toggle = entry.id.clone();
        let author_name = entry.author.clone();
        let author_url = format!("https://github.com/{}", entry.author);
        let repo_url = format!("https://github.com/{}", entry.repo);
        let author_click = author_url.clone();
        let repo_click = repo_url.clone();

        let version_line = if let Some(inst_ver) = &installed_version {
            format!(
                "{}: {} ({}: {})",
                if is_ru { "Версия" } else { "Version" },
                entry.version,
                if is_ru { "установлена" } else { "installed" },
                inst_ver
            )
        } else {
            format!("{}: {}", if is_ru { "Версия" } else { "Version" }, entry.version)
        };

        let author_line_prefix = if is_ru { "Автор: " } else { "Author: " };
        let repo_line_prefix = if is_ru { "Репозиторий: " } else { "Repository: " };
        let updated_line_prefix = if is_ru { "Последнее обновление: " } else { "Last updated: " };
        let updated_text = format_updated_date(entry.updated, is_ru);

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
                        .px_3p5()
                        .py_1p5()
                        .rounded_md()
                        .bg(theme.accent)
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.accent_hover))
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_on_accent)
                        .child(if is_ru { "Установить" } else { "Install" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.install_plugin_from_catalog(&plugin_id, cx);
                        })),
                )
            })
            .when(!is_installed && is_this_downloading, |parent| {
                parent.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .bg(theme.bg_hover)
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_muted)
                        .child(icons::icon(icons::ICON_REFRESH, 12.0, theme.accent))
                        .child(if is_ru { "Скачивание..." } else { "Downloading..." }),
                )
            })
            .when(is_installed, |parent| {
                parent
                    .child(
                        div()
                            .id("btn-toggle-catalog-plugin")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .bg(if is_enabled { theme.bg_hover } else { theme.accent })
                            .cursor_pointer()
                            .hover(|h| h.bg(if is_enabled { theme.border } else { theme.accent_hover }))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(if is_enabled { theme.text_primary } else { theme.text_on_accent })
                            .child(if is_enabled {
                                if is_ru { "Отключить" } else { "Disable" }
                            } else {
                                if is_ru { "Включить" } else { "Enable" }
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(vault) = &this.state.vault {
                                    if is_enabled {
                                        let _ = vault.disable_community_plugin(&plugin_id_toggle);
                                        if let Some(pos) = this.plugin_runtimes.iter().position(|r| r.manifest.id == plugin_id_toggle) {
                                            let handle = this.plugin_runtimes.remove(pos);
                                            let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
                                            this.state.plugin_host.unregister_plugin(&plugin_id_toggle);
                                            this.state.commands.unregister_all_of(&plugin_id_toggle);
                                            this.state.ribbon.unregister_all_of(&plugin_id_toggle);
                                            this.state.status_bar.unregister_all_of(&plugin_id_toggle);
                                            this.state.code_block_processors.unregister_all_of(&plugin_id_toggle);
                                            this.code_block_views.retain(|_, (pid, _)| pid != &plugin_id_toggle);
                                        }
                                    } else {
                                        let _ = vault.enable_community_plugin(&plugin_id_toggle);
                                        this.load_installed_plugins_for_vault();
                                    }
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("btn-uninstall-catalog-plugin")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .bg(theme.bg_hover)
                            .cursor_pointer()
                            .hover(|h| h.bg(rgba(0xea433522)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgba(0xea4335ff))
                            .child(if is_ru { "Удалить" } else { "Uninstall" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.uninstall_plugin_from_catalog(&plugin_id_uninstall, cx);
                            })),
                    )
                    .when_some(plugin_dir.clone(), |p, d| {
                        let d_str = d.to_string_lossy().to_string();
                        p.child(
                            div()
                                .id("btn-catalog-open-folder")
                                .px_2p5()
                                .py_1p5()
                                .rounded_md()
                                .bg(theme.bg_hover)
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.border))
                                .child(icons::icon(icons::ICON_FOLDER_OPEN, 13.0, theme.text_muted))
                                .on_click(cx.listener(move |_this, _, _, _cx| {
                                    let _ = std::process::Command::new("explorer").arg(&d_str).spawn();
                                })),
                        )
                    })
            });

        // Top info card
        let info_card = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_2()
            .p_5()
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle)
            // Title row
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2p5()
                    .child(icons::icon(icons::ICON_PLUG, 22.0, theme.accent))
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child(entry.name.clone()),
                    )
                    .when(is_installed, |p| {
                        p.child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .px_2()
                                .py_0p5()
                                .rounded_md()
                                .bg(rgba(0x7c4dffcc))
                                .text_color(gpui::white())
                                .child(if is_ru { "УСТАНОВЛЕН" } else { "INSTALLED" }),
                        )
                    }),
            )
            // Download row
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(icons::icon(icons::ICON_DOWNLOAD, 13.0, theme.text_muted))
                    .child(format_download_count(entry.downloads)),
            )
            // Version line
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .child(version_line),
            )
            // Author line
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .child(div().text_color(theme.text_secondary).child(author_line_prefix))
                    .child(
                        div()
                            .id("catalog-author-link")
                            .text_color(rgba(0x7c4dffff))
                            .cursor_pointer()
                            .hover(|h| h.underline())
                            .child(author_name)
                            .on_click(cx.listener(move |_this, _, _, cx| {
                                cx.open_url(&author_click);
                            })),
                    ),
            )
            // Repo line
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .child(div().text_color(theme.text_secondary).child(repo_line_prefix))
                    .child(
                        div()
                            .id("catalog-repo-link")
                            .text_color(rgba(0x7c4dffff))
                            .cursor_pointer()
                            .hover(|h| h.underline())
                            .child(repo_url.clone())
                            .on_click(cx.listener(move |_this, _, _, cx| {
                                cx.open_url(&repo_click);
                            })),
                    ),
            )
            // Last updated line
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .child(div().text_color(theme.text_secondary).child(updated_line_prefix))
                    .child(
                        div()
                            .text_color(rgba(0x7c4dffff))
                            .child(updated_text),
                    ),
            )
            // Description
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .mt_1()
                    .text_sm()
                    .line_height(gpui::relative(1.4))
                    .text_color(theme.text_primary)
                    .whitespace_normal()
                    .child(entry.description.clone()),
            );

        // README Section
        let readme_text = self.plugin_catalog.readme_cache.get(&entry.id).cloned().or_else(|| {
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

        let readme_section = if self.plugin_catalog.is_loading_readme {
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child("README.MD"),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .p_8()
                        .rounded_lg()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .child(icons::icon(icons::ICON_REFRESH, 16.0, theme.accent))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(if is_ru {
                                    "Загрузка README.md из GitHub..."
                                } else {
                                    "Loading README.md from GitHub..."
                                }),
                        ),
                )
        } else if let Some(text) = readme_text {
            let rendered_markdown_lines = self.render_markdown_content(&text, cx);
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child("README.MD"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_w_0()
                        .overflow_x_hidden()
                        .p_5()
                        .rounded_lg()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .children(rendered_markdown_lines),
                )
        } else {
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child("README.MD"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .w_full()
                        .min_w_0()
                        .p_8()
                        .rounded_lg()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(if is_ru {
                                    "README.md не найден"
                                } else {
                                    "README.md not available"
                                }),
                        ),
                )
        };

        let scroll_container = div()
            .id("catalog-details-scroll")
            .track_scroll(&self.catalog_details_scroll_handle)
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .overflow_y_scroll()
            .overflow_x_hidden()
            .bg(theme.bg_editor)
            .p_6()
            .pr_8() // leave breathing room for overlay scrollbar
            .gap_4()
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let line_h = 36.0;
                let dy = match event.delta {
                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                    gpui::ScrollDelta::Lines(l) => l.y * line_h,
                };
                if dy != 0.0 {
                    let cur = f32::from(this.catalog_details_scroll_handle.offset().y);
                    let max = f32::from(this.catalog_details_scroll_handle.max_offset().height);
                    let next = (cur + dy).clamp(-max, 0.0);
                    if next != cur {
                        this.catalog_details_scroll_handle.set_offset(point(px(0.0), px(next)));
                        cx.notify();
                    }
                }
                cx.stop_propagation();
            }))
            .child(top_actions_bar)
            .when(is_this_downloading, |parent| {
                parent.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .p_2p5()
                        .rounded_md()
                        .bg(rgba(0x4285f422))
                        .border_1()
                        .border_color(rgba(0x4285f455))
                        .child(icons::icon(icons::ICON_REFRESH, 14.0, theme.accent))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.accent)
                                .child(self.plugin_catalog.download_step.clone().unwrap_or_else(|| {
                                    if is_ru {
                                        "Загрузка файлов из GitHub Releases...".to_string()
                                    } else {
                                        "Downloading files from GitHub Releases...".to_string()
                                    }
                                })),
                        ),
                )
            })
            .child(info_card)
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
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let line_h = 36.0;
                let dy = match event.delta {
                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                    gpui::ScrollDelta::Lines(l) => l.y * line_h,
                };
                if dy != 0.0 {
                    let cur = f32::from(this.catalog_details_scroll_handle.offset().y);
                    let max = f32::from(this.catalog_details_scroll_handle.max_offset().height);
                    let next = (cur + dy).clamp(-max, 0.0);
                    if next != cur {
                        this.catalog_details_scroll_handle.set_offset(point(px(0.0), px(next)));
                        cx.notify();
                    }
                }
                cx.stop_propagation();
            }))
            .child(scroll_container);

        if let Some(geometry) = ScrollbarGeometry::from_handle(&self.catalog_details_scroll_handle, &scroll_style) {
            let dragging = self.catalog_details_scrollbar_drag.active;
            viewport = viewport.child(overlay_scrollbar(
                "catalog-details-scrollbar-track",
                &geometry,
                &scroll_style,
                theme,
                dragging,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let pointer_y: f32 = event.position.y.into();
                    this.set_catalog_details_scroll_top(geometry.offset_for_track_click(pointer_y));
                    cx.notify();
                }),
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.catalog_details_scrollbar_drag.begin(event.position.y.into(), geometry.scroll_y);
                    cx.notify();
                }),
            ));
        }

        viewport.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_default_entries() {
        let state = PluginCatalogState::default();
        assert!(!state.entries.is_empty());
        assert!(state.entries.iter().any(|e| e.id == "dataview"));
        assert!(state.entries.iter().any(|e| e.id == "calendar"));
    }

    #[test]
    fn test_catalog_search_filtering() {
        let mut state = PluginCatalogState::default();
        state.entries = PluginCatalogState::default_catalog();

        state.search_query = "data".to_string();
        let filtered = state.filtered_entries();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "dataview");

        state.search_query = "liamcain".to_string();
        let author_match = state.filtered_entries();
        assert_eq!(author_match.len(), 1);
        assert_eq!(author_match[0].id, "calendar");

        state.search_query = "non-existent-plugin-query".to_string();
        assert!(state.filtered_entries().is_empty());
    }

    #[test]
    fn test_catalog_sorting_orders() {
        let mut state = PluginCatalogState::default();
        state.sort_order = CatalogSortOrder::Alphabetical;
        let alpha = state.filtered_entries();
        for i in 0..alpha.len() - 1 {
            assert!(alpha[i].name.to_lowercase() <= alpha[i + 1].name.to_lowercase());
        }

        state.sort_order = CatalogSortOrder::MostPopular;
        let popular = state.filtered_entries();
        for i in 0..popular.len() - 1 {
            assert!(popular[i].downloads >= popular[i + 1].downloads);
        }

        state.sort_order = CatalogSortOrder::RecentlyUpdated;
        let updated = state.filtered_entries();
        for i in 0..updated.len() - 1 {
            assert!(updated[i].updated >= updated[i + 1].updated);
        }
    }
}
