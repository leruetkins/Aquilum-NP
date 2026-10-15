use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    div, point, prelude::*, px, rgb, rgba, AnyElement, App, Context, Div, ElementId, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    ParentElement, Render, ScrollHandle, ScrollWheelEvent, SharedString,
    StatefulInteractiveElement, Styled, WeakEntity, Window, WindowControlArea, WindowOptions,
};

use crate::blink_manager::BlinkTarget;
use crate::icons;
use crate::markdown::InlineSpan;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::goto_search::{highlight_query_terms, highlight_terms, word_range_at};
use crate::views::scrollbar::{
    overlay_scrollbar, ScrollbarDrag, ScrollbarGeometry, ScrollbarStyle,
};
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
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub is_mouse_selecting: bool,
    /// Window x of the input text box; measured via canvas for click mapping.
    pub box_origin_x: Rc<Cell<f32>>,
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
            cursor_pos: 0,
            selection_anchor: None,
            is_mouse_selecting: false,
            box_origin_x: Rc::new(Cell::new(0.0)),
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
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.selection_anchor {
            if anchor != self.cursor_pos {
                return Some((anchor.min(self.cursor_pos), anchor.max(self.cursor_pos)));
            }
        }
        None
    }
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

    /// Returns filtered and sorted list of catalog entries according to current search query,
    /// matching full queries and terms identically to file search (`goto_search.rs`).
    pub fn filtered_entries(&self) -> Vec<CatalogPluginEntry> {
        let raw = self.search_query.trim();
        if raw.is_empty() {
            let mut list = self.entries.clone();
            self.sort_catalog_entries(&mut list);
            return list;
        }

        let q = raw.to_lowercase();
        let terms: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            q.split_whitespace()
                .filter(|t| !t.is_empty())
                .filter(|t| seen.insert(t.to_string()))
                .map(|t| t.to_string())
                .collect()
        };

        // Rank and score each entry:
        // Rank 0: Name/ID hit (whole query or all terms match name/id)
        // Rank 1: Description/Author hit (all terms found in description or author or name)
        let mut ranked: Vec<(u8, usize, CatalogPluginEntry)> = Vec::new();

        for entry in &self.entries {
            let name_lower = entry.name.to_lowercase();
            let id_lower = entry.id.to_lowercase();
            let author_lower = entry.author.to_lowercase();
            let desc_lower = entry.description.to_lowercase();

            // Name/ID match: either query is inside name/id, or all terms are inside name/id
            let name_id_match = name_lower.contains(&q)
                || id_lower.contains(&q)
                || (!terms.is_empty() && terms.iter().all(|t| name_lower.contains(t) || id_lower.contains(t)));

            // General match: either name/id matches, or query is in author/desc,
            // or all individual terms are found across the plugin's fields (like file search)
            let all_fields_match = name_id_match
                || author_lower.contains(&q)
                || desc_lower.contains(&q)
                || (!terms.is_empty()
                    && terms.iter().all(|t| {
                        name_lower.contains(t)
                            || id_lower.contains(t)
                            || author_lower.contains(t)
                            || desc_lower.contains(t)
                    }));

            if !all_fields_match {
                continue;
            }

            let rank = if name_id_match { 0u8 } else { 1u8 };

            // Calculate match count / relevance score
            let mut score = 0usize;
            if name_lower.contains(&q) {
                score += 20;
            }
            if id_lower.contains(&q) {
                score += 15;
            }
            if desc_lower.contains(&q) {
                score += 5;
            }
            if author_lower.contains(&q) {
                score += 5;
            }
            for t in &terms {
                if name_lower.contains(t) {
                    score += 5;
                }
                if id_lower.contains(t) {
                    score += 4;
                }
                if desc_lower.contains(t) {
                    score += 2;
                }
                if author_lower.contains(t) {
                    score += 2;
                }
            }

            ranked.push((rank, score, entry.clone()));
        }

        // Sort by rank first (name hits outrank content hits), then by selected sort criteria
        match self.sort_order {
            CatalogSortOrder::MostPopular => {
                ranked.sort_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then_with(|| b.2.downloads.cmp(&a.2.downloads))
                        .then_with(|| b.1.cmp(&a.1))
                });
            }
            CatalogSortOrder::Alphabetical => {
                ranked.sort_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then_with(|| a.2.name.to_lowercase().cmp(&b.2.name.to_lowercase()))
                });
            }
            CatalogSortOrder::RecentlyUpdated => {
                ranked.sort_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then_with(|| b.2.updated.cmp(&a.2.updated))
                        .then_with(|| b.2.version.cmp(&a.2.version))
                });
            }
        }

        ranked.into_iter().map(|(_, _, entry)| entry).collect()
    }

    /// Helper to sort an entry list in-place according to current sort order.
    fn sort_catalog_entries(&self, list: &mut [CatalogPluginEntry]) {
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
    }
}

/// Row height in pixels for each entry in the community plugins list (expanded to fit full descriptions).
pub const CATALOG_ROW_H: f32 = 168.0;
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

pub struct CatalogWindowManager {
    pub window: Option<gpui::WindowHandle<PluginCatalogView>>,
}

impl Default for CatalogWindowManager {
    fn default() -> Self {
        Self { window: None }
    }
}

impl CatalogWindowManager {
    pub fn open_or_focus(
        &mut self,
        app_view: WeakEntity<AppView>,
        vault_name: String,
        vault_root: Option<PathBuf>,
        theme: Theme,
        language: String,
        app_data_dir: &Path,
        cx: &mut App,
    ) {
        // 1. If window is already open, activate and focus it
        if let Some(existing) = &self.window {
            if existing
                .update(cx, |_view, window, _cx| {
                    window.activate_window();
                })
                .is_ok()
            {
                return;
            }
        }

        // 2. Validate saved window geometry
        let state_path = app_data_dir.join("catalog-window.json");
        let saved_state = crate::state::WindowState::load_file(&state_path);
        let bounds = saved_state.to_validated_bounds(cx, 1120.0, 760.0);

        let is_ru = language.starts_with("ru");
        let window_title = if is_ru {
            format!("Сторонние плагины — {} — Aquilum", vault_name)
        } else {
            format!("Community Plugins — {} — Aquilum", vault_name)
        };

        let options = WindowOptions {
            window_bounds: Some(bounds),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(window_title.into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_min_size: Some(gpui::size(px(640.0), px(480.0))),
            ..Default::default()
        };

        let save_path = state_path.clone();
        let handle_res = cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| {
                let v = PluginCatalogView::new(app_view, vault_name, vault_root, theme, is_ru, cx);
                window.focus(&v.focus_handle);
                v
            });

            window.on_window_should_close(cx, move |window, _cx| {
                let is_maximized = window.is_maximized();
                let bounds = window.bounds();
                let mut state = crate::state::WindowState::load_file(&save_path);
                state.maximized = is_maximized;
                if !is_maximized {
                    state.width = f32::from(bounds.size.width);
                    state.height = f32::from(bounds.size.height);
                    state.x = Some(f32::from(bounds.origin.x));
                    state.y = Some(f32::from(bounds.origin.y));
                }
                state.save_file(&save_path);
                true
            });

            view
        });

        match handle_res {
            Ok(handle) => {
                self.window = Some(handle);
            }
            Err(e) => {
                eprintln!("[CatalogWindowManager] Failed to open catalog window: {:?}", e);
            }
        }
    }

    pub fn close_if_open(&mut self, cx: &mut App) {
        if let Some(handle) = self.window.take() {
            let _ = handle.update(cx, |_view, window, _cx| {
                window.remove_window();
            });
        }
    }
}

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
                        })
                        .unwrap_or(false);
                    if !alive {
                        break;
                    }
                }
            })
            .detach();
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

    fn insert_search_text(&mut self, text: &str) {
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

    fn on_search_query_changed(&mut self) {
        let filtered = self.state.filtered_entries();
        if !filtered.iter().any(|e| Some(&e.id) == self.state.selected_plugin_id.as_ref()) {
            self.state.selected_plugin_id = filtered.first().map(|e| e.id.clone());
        }
        self.scroll_handle.set_offset(point(px(0.0), px(0.0)));
    }

    fn scroll_to_selected(&mut self, index: usize) {
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
                        // Unload runtime
                        if let Some(pos) = view.plugin_runtimes.iter().position(|r| r.manifest.id == p_id) {
                            let handle = view.plugin_runtimes.remove(pos);
                            let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
                            view.state.plugin_host.unregister_plugin(&p_id);
                            view.state.commands.unregister_all_of(&p_id);
                            view.state.ribbon.unregister_all_of(&p_id);
                            view.state.status_bar.unregister_all_of(&p_id);
                            view.state.code_block_processors.unregister_all_of(&p_id);
                            view.code_block_views.retain(|_, (pid, _)| pid != &p_id);
                        }
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
                    if let Some(pos) = view.plugin_runtimes.iter().position(|r| r.manifest.id == p_id) {
                        let handle = view.plugin_runtimes.remove(pos);
                        let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
                        view.state.plugin_host.unregister_plugin(&p_id);
                        view.state.commands.unregister_all_of(&p_id);
                        view.state.ribbon.unregister_all_of(&p_id);
                        view.state.status_bar.unregister_all_of(&p_id);
                        view.state.code_block_processors.unregister_all_of(&p_id);
                        view.code_block_views.retain(|_, (pid, _)| pid != &p_id);
                    }
                    cx.notify();
                });
            }).detach();
        }
        cx.notify();
    }

    fn render_titlebar(&self, window: &Window, cx: &Context<Self>) -> Div {
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
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, event: &MouseDownEvent, window, _| {
                        if event.click_count == 2 {
                            window.zoom_window();
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
                            .on_click(cx.listener(|_, _, window, _| {
                                window.remove_window();
                            }))
                    })
            )
    }

    fn render_header(&self, window: &Window, cx: &Context<Self>) -> Div {
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
                    ),
            )
    }

    fn render_list(&self, entries: &[CatalogPluginEntry], cx: &Context<Self>) -> impl IntoElement {
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

    fn render_details(
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
                    ),
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

    fn render_markdown_content(&self, text: &str, cx: &Context<Self>) -> Vec<Div> {
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

    fn render_markdown_inline_span(
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

    fn render_footer_status(&self, _cx: &Context<Self>) -> Div {
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

impl AppView {
    fn render_markdown_content(&self, text: &str, cx: &Context<Self>) -> Vec<Div> {
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

    fn render_markdown_inline_span(
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
                    .id(ElementId::Name(format!("app-cat-md-link-{}", _idx).into()))
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

    /// Opens the Community Plugins catalog in a separate OS window (Multi-Window GPUI).
    pub fn open_plugin_catalog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let vault_name = self.state.vault.as_ref().map(|v| v.name.clone()).unwrap_or_else(|| "Vault".to_string());
        let vault_root = self.state.vault.as_ref().map(|v| v.root.clone());
        let theme = self.theme.clone();
        let language = self.config.ui.language.clone();
        let app_data_dir = crate::state::app_data_dir();
        let weak_self = cx.weak_entity();

        cx.defer(move |cx| {
            let handle = weak_self.clone();
            let _ = weak_self.update(cx, |this, cx| {
                this.catalog_window_manager.open_or_focus(
                    handle,
                    vault_name,
                    vault_root,
                    theme,
                    language,
                    &app_data_dir,
                    cx,
                );
            });
        });
    }

    /// Closes the Community Plugins catalog modal.
    pub fn close_plugin_catalog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.plugin_catalog.is_open {
            return;
        }
        self.plugin_catalog.is_open = false;
        self.plugin_catalog.selection_anchor = None;
        self.plugin_catalog.is_mouse_selecting = false;
        self.catalog_blink.disable(cx);
        window.focus(&self.focus_handle);
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

    /// Inserts text into the Plugin Catalog search query at the current cursor / replacing selection.
    fn insert_catalog_search_text(&mut self, text: &str) {
        let mut chars: Vec<char> = self.plugin_catalog.search_query.chars().collect();
        let pos = if let Some((s, e)) = self.plugin_catalog.selection_range() {
            chars.drain(s..e);
            self.plugin_catalog.selection_anchor = None;
            s
        } else {
            self.plugin_catalog.cursor_pos.min(chars.len())
        };
        for (i, c) in text.chars().enumerate() {
            chars.insert(pos + i, c);
        }
        self.plugin_catalog.cursor_pos = pos + text.chars().count();
        self.plugin_catalog.search_query = chars.into_iter().collect();
    }

    /// Called whenever the search query changes to keep selection in sync.
    fn on_catalog_search_query_changed(&mut self) {
        let filtered = self.plugin_catalog.filtered_entries();
        if !filtered.iter().any(|e| Some(&e.id) == self.plugin_catalog.selected_plugin_id.as_ref()) {
            self.plugin_catalog.selected_plugin_id = filtered.first().map(|e| e.id.clone());
        }
        self.set_catalog_scroll_top(0.0);
    }

    /// Maps a click x (window coords) to a query char index.
    pub fn catalog_search_click_pos(&self, click_x: f32, window: &Window) -> usize {
        let query = &self.plugin_catalog.search_query;
        if query.is_empty() {
            return 0;
        }
        let box_x = self.plugin_catalog.box_origin_x.get();
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

    /// Follows the pointer while catalog search input selection is being dragged.
    pub fn handle_catalog_search_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.plugin_catalog.is_open || !self.plugin_catalog.is_mouse_selecting {
            return;
        }
        let x: f32 = event.position.x.into();
        let new_pos = self.catalog_search_click_pos(x, window);
        if self.plugin_catalog.cursor_pos != new_pos {
            self.plugin_catalog.cursor_pos = new_pos;
            self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
            cx.notify();
        }
    }

    /// Ends an input drag selection in plugin catalog.
    pub fn handle_catalog_search_mouse_up(&mut self, cx: &mut Context<Self>) {
        if self.plugin_catalog.is_mouse_selecting {
            self.plugin_catalog.is_mouse_selecting = false;
            if self.plugin_catalog.selection_anchor == Some(self.plugin_catalog.cursor_pos) {
                self.plugin_catalog.selection_anchor = None;
            }
            cx.notify();
        }
    }

    /// Handles keyboard events while the plugin catalog is focused.
    fn handle_catalog_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        if key == "escape" {
            self.close_plugin_catalog(window, cx);
            return;
        }

        let ctrl = event.keystroke.modifiers.control;
        let alt = event.keystroke.modifiers.alt;
        let shift = event.keystroke.modifiers.shift;

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

        // Home / End
        if key == "home" || key == "end" {
            let total = self.plugin_catalog.search_query.chars().count();
            let pos = if key == "home" { 0 } else { total };
            if shift {
                if self.plugin_catalog.selection_anchor.is_none() {
                    self.plugin_catalog.selection_anchor = Some(self.plugin_catalog.cursor_pos);
                }
            } else {
                self.plugin_catalog.selection_anchor = None;
            }
            self.plugin_catalog.cursor_pos = pos;
            self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
            cx.notify();
            return;
        }

        // Left / Right cursor movement
        if key == "left" || key == "right" {
            let total = self.plugin_catalog.search_query.chars().count();
            if shift && self.plugin_catalog.selection_anchor.is_none() {
                self.plugin_catalog.selection_anchor = Some(self.plugin_catalog.cursor_pos);
            }
            if key == "left" {
                if shift {
                    self.plugin_catalog.cursor_pos = self.plugin_catalog.cursor_pos.saturating_sub(1);
                } else if let Some((start, _)) = self.plugin_catalog.selection_range() {
                    self.plugin_catalog.cursor_pos = start;
                    self.plugin_catalog.selection_anchor = None;
                } else {
                    self.plugin_catalog.cursor_pos = self.plugin_catalog.cursor_pos.saturating_sub(1);
                }
            } else if shift {
                self.plugin_catalog.cursor_pos = (self.plugin_catalog.cursor_pos + 1).min(total);
            } else if let Some((_, end)) = self.plugin_catalog.selection_range() {
                self.plugin_catalog.cursor_pos = end;
                self.plugin_catalog.selection_anchor = None;
            } else {
                self.plugin_catalog.cursor_pos = (self.plugin_catalog.cursor_pos + 1).min(total);
            }
            self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
            cx.notify();
            return;
        }

        // Ctrl+A select all
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.plugin_catalog.selection_anchor = Some(0);
            self.plugin_catalog.cursor_pos = self.plugin_catalog.search_query.chars().count();
            self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
            cx.notify();
            return;
        }

        // Ctrl+C copy
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s, e)) = self.plugin_catalog.selection_range() {
                let text: String = self.plugin_catalog.search_query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
            return;
        }

        // Ctrl+X cut
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s, e)) = self.plugin_catalog.selection_range() {
                let text: String = self.plugin_catalog.search_query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                let mut chars: Vec<char> = self.plugin_catalog.search_query.chars().collect();
                chars.drain(s..e);
                self.plugin_catalog.search_query = chars.into_iter().collect();
                self.plugin_catalog.cursor_pos = s;
                self.plugin_catalog.selection_anchor = None;
                self.on_catalog_search_query_changed();
                self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
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
                        self.insert_catalog_search_text(&filtered);
                        self.on_catalog_search_query_changed();
                        self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
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
            let mut chars: Vec<char> = self.plugin_catalog.search_query.chars().collect();
            if let Some((s, e)) = self.plugin_catalog.selection_range() {
                chars.drain(s..e);
                self.plugin_catalog.cursor_pos = s;
                self.plugin_catalog.selection_anchor = None;
            } else if key == "backspace" {
                let pos = self.plugin_catalog.cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.plugin_catalog.cursor_pos = pos - 1;
                } else {
                    return;
                }
            } else {
                let pos = self.plugin_catalog.cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                } else {
                    return;
                }
            }
            self.plugin_catalog.search_query = chars.into_iter().collect();
            self.on_catalog_search_query_changed();
            self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
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
                self.insert_catalog_search_text(&filtered);
                self.on_catalog_search_query_changed();
                self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
                cx.notify();
            }
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
    pub fn render_plugin_catalog_modal(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
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
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.catalog_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_catalog_scrollbar_drag(y, cx);
                    cx.stop_propagation();
                } else if this.catalog_details_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_catalog_details_scrollbar_drag(y, cx);
                    cx.stop_propagation();
                } else if this.plugin_catalog.is_mouse_selecting {
                    this.handle_catalog_search_mouse_move(event, window, cx);
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
                if this.plugin_catalog.is_mouse_selecting {
                    this.handle_catalog_search_mouse_up(cx);
                    changed = true;
                }
                if changed {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.plugin_catalog.is_mouse_selecting {
                    this.handle_catalog_search_mouse_up(cx);
                }
                if this.catalog_scrollbar_drag.active {
                    this.catalog_scrollbar_drag.end();
                }
                if this.catalog_details_scrollbar_drag.active {
                    this.catalog_details_scrollbar_drag.end();
                }
                cx.notify();
            }))
            // Backdrop
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(rgba(0x000000aa))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_plugin_catalog(window, cx);
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
                            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                                if this.catalog_scrollbar_drag.active {
                                    let y: f32 = event.position.y.into();
                                    this.handle_catalog_scrollbar_drag(y, cx);
                                    cx.stop_propagation();
                                } else if this.catalog_details_scrollbar_drag.active {
                                    let y: f32 = event.position.y.into();
                                    this.handle_catalog_details_scrollbar_drag(y, cx);
                                    cx.stop_propagation();
                                } else if this.plugin_catalog.is_mouse_selecting {
                                    this.handle_catalog_search_mouse_move(event, window, cx);
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
                                if this.plugin_catalog.is_mouse_selecting {
                                    this.handle_catalog_search_mouse_up(cx);
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
                            .child(self.render_catalog_header(window, cx))
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
    fn render_catalog_header(&self, window: &Window, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let is_ru = self.config.ui.language.starts_with("ru");
        let query = self.plugin_catalog.search_query.clone();
        let total_count = self.plugin_catalog.entries.len();
        let filtered_count = self.plugin_catalog.filtered_entries().len();
        let is_loading = self.plugin_catalog.is_loading;
        let pixel_scale = window.scale_factor();

        // Input caret geometry
        let query_chars: Vec<char> = query.chars().collect();
        let cursor_pos = self.plugin_catalog.cursor_pos.min(query_chars.len());
        let selection_range = self.plugin_catalog.selection_range();
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
        let cursor_x = if query.is_empty() {
            0.0
        } else {
            let byte_index: usize = query_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
            crate::snap::snap_x(f32::from(shaped.x_for_index(byte_index)), pixel_scale)
        };
        let (sel_x1, sel_w) = if let Some((s, e)) = selection_range {
            if !query.is_empty() {
                let b1: usize = query_chars[..s.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                let b2: usize = query_chars[..e.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                crate::snap::snapped_span(
                    f32::from(shaped.x_for_index(b1)),
                    f32::from(shaped.x_for_index(b2)),
                    pixel_scale,
                )
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };
        let is_input_focused = self.catalog_focus_handle.is_focused(window);
        let caret_visible = is_input_focused && self.catalog_blink.visible();

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
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_plugin_catalog(window, cx);
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
                            .cursor(gpui::CursorStyle::IBeam)
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                window.focus(&this.catalog_focus_handle);
                                this.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
                            }))
                            .child(icons::icon(icons::ICON_SEARCH, 14.0, theme.text_muted))
                            .child(
                                div()
                                    .id("catalog-search-input")
                                    .relative()
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(22.0))
                                    .track_focus(&self.catalog_focus_handle)
                                    .cursor(gpui::CursorStyle::IBeam)
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        window.focus(&this.catalog_focus_handle);
                                        this.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
                                        let click_x: f32 = event.position.x.into();
                                        let pos = this.catalog_search_click_pos(click_x, window);
                                        let total_chars = this.plugin_catalog.search_query.chars().count();
                                        if event.click_count >= 3 {
                                            this.plugin_catalog.selection_anchor = Some(0);
                                            this.plugin_catalog.cursor_pos = total_chars;
                                            this.plugin_catalog.is_mouse_selecting = false;
                                        } else if event.click_count == 2 {
                                            let (s, e) = word_range_at(&this.plugin_catalog.search_query, pos);
                                            this.plugin_catalog.selection_anchor = Some(s);
                                            this.plugin_catalog.cursor_pos = e;
                                            this.plugin_catalog.is_mouse_selecting = false;
                                        } else if event.modifiers.shift {
                                            if this.plugin_catalog.selection_anchor.is_none() {
                                                this.plugin_catalog.selection_anchor = Some(this.plugin_catalog.cursor_pos);
                                            }
                                            this.plugin_catalog.cursor_pos = pos;
                                            this.plugin_catalog.is_mouse_selecting = true;
                                        } else {
                                            this.plugin_catalog.cursor_pos = pos;
                                            this.plugin_catalog.selection_anchor = Some(pos);
                                            this.plugin_catalog.is_mouse_selecting = true;
                                        }
                                        cx.notify();
                                    }))
                                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        this.handle_catalog_key(event, window, cx);
                                    }))
                                    .child(
                                        gpui::canvas(
                                            {
                                                let cell = self.plugin_catalog.box_origin_x.clone();
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
                                        .id("btn-clear-catalog-search")
                                        .cursor_pointer()
                                        .hover(|h| h.text_color(theme.text_primary))
                                        .child(icons::icon(icons::ICON_CLOSE, 12.0, theme.text_muted))
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.plugin_catalog.search_query.clear();
                                            this.plugin_catalog.cursor_pos = 0;
                                            this.plugin_catalog.selection_anchor = None;
                                            this.on_catalog_search_query_changed();
                                            window.focus(&this.catalog_focus_handle);
                                            this.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
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
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.plugin_catalog.sort_order = CatalogSortOrder::MostPopular;
                                        window.focus(&this.catalog_focus_handle);
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
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.plugin_catalog.sort_order = CatalogSortOrder::Alphabetical;
                                        window.focus(&this.catalog_focus_handle);
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
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.plugin_catalog.sort_order = CatalogSortOrder::RecentlyUpdated;
                                        window.focus(&this.catalog_focus_handle);
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

        let query = self.plugin_catalog.search_query.clone();
        let terms = highlight_query_terms(&query);

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
                .gap_2()
                .h(px(160.0))
                .mb(px(8.0))
                .px_3p5()
                .py_3()
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
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_catalog_plugin(&entry_id, cx);
                    this.scroll_catalog_to_selected(actual_idx);
                    window.focus(&this.catalog_focus_handle);
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
                                        .child(highlight_terms(&entry.name, &terms, theme.text_primary, is_selected, theme)),
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
                // Snippet / description (full description, no 2-line clamp)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_xs()
                        .line_height(px(17.0))
                        .whitespace_normal()
                        .child(highlight_terms(&entry.description, &terms, theme.text_secondary, is_selected, theme)),
                )
                // Author row
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .text_xs()
                        .truncate()
                        .child(highlight_terms(&author_display, &terms, theme.text_muted, is_selected, theme)),
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
        let query = self.plugin_catalog.search_query.clone();
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
                            .child(highlight_terms(&entry.name, &terms, theme.text_primary, false, theme)),
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
                    .child(highlight_terms(&entry.description, &terms, theme.text_primary, false, theme)),
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

    #[test]
    fn test_catalog_selection_range() {
        let mut state = PluginCatalogState::default();
        state.search_query = "hello world".to_string();
        state.cursor_pos = 0;
        state.selection_anchor = None;
        assert_eq!(state.selection_range(), None);

        // Same pos as anchor -> no selection
        state.selection_anchor = Some(0);
        assert_eq!(state.selection_range(), None);

        // Forward selection
        state.cursor_pos = 5;
        state.selection_anchor = Some(0);
        assert_eq!(state.selection_range(), Some((0, 5)));

        // Backward selection
        state.cursor_pos = 2;
        state.selection_anchor = Some(8);
        assert_eq!(state.selection_range(), Some((2, 8)));
    }
}
