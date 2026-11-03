use super::*;

/// Filter and sorting criteria for the catalog browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSortOrder {
    MostPopular,
    Alphabetical,
    RecentlyUpdated,
}

/// Compatibility filter over the embedded corpus registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogCompatFilter {
    All,
    Compatible,
    Partial,
    Blocked,
}

impl CatalogCompatFilter {
    /// Dropdown label for the current value.
    pub fn label(self, is_ru: bool) -> String {
        match self {
            Self::All => (if is_ru { "Все" } else { "All" }).to_string(),
            Self::Compatible => (if is_ru { "Совместимые" } else { "Compatible" }).to_string(),
            Self::Partial => (if is_ru { "Частично" } else { "Partial" }).to_string(),
            Self::Blocked => (if is_ru { "Блокированные" } else { "Blocked" }).to_string(),
        }
    }

    /// Marker dot color for the option row / button.
    pub fn dot_color(self, theme: &crate::theme::Theme) -> gpui::Rgba {
        match self {
            Self::All => theme.text_muted,
            Self::Compatible => rgba(0x3fb950ff),
            Self::Partial => rgba(0xd29922ff),
            Self::Blocked => rgba(0xf85149ff),
        }
    }

    /// Theme-independent variant of [`Self::dot_color`] for contexts without a
    /// `Theme` at hand (the shared popup builds its rows from `TabEntry`).
    pub fn dot_color_static(self) -> gpui::Rgba {
        match self {
            Self::All => rgba(0x8b949eff),
            Self::Compatible => rgba(0x3fb950ff),
            Self::Partial => rgba(0xd29922ff),
            Self::Blocked => rgba(0xf85149ff),
        }
    }

    pub const ALL: [Self; 4] = [Self::All, Self::Compatible, Self::Partial, Self::Blocked];
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
    /// Compatibility filter over the embedded corpus registry.
    pub compat_filter: CatalogCompatFilter,
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
            compat_filter: CatalogCompatFilter::All,
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
        // Compatibility filter: the corpus registry decides; plugins outside the
        // corpus pass only with the "All" filter (they are not verified either way).
        let matches_compat = |id: &str| match self.compat_filter {
            CatalogCompatFilter::All => true,
            CatalogCompatFilter::Compatible => {
                matches!(
                    crate::plugins::compat::catalog_compat(id).map(|r| r.status),
                    Some(crate::plugins::compat::CompatibilityStatus::Full)
                )
            }
            CatalogCompatFilter::Partial => {
                matches!(
                    crate::plugins::compat::catalog_compat(id).map(|r| r.status),
                    Some(crate::plugins::compat::CompatibilityStatus::Partial(_))
                )
            }
            CatalogCompatFilter::Blocked => {
                matches!(
                    crate::plugins::compat::catalog_compat(id).map(|r| r.status),
                    Some(crate::plugins::compat::CompatibilityStatus::Incompatible)
                )
            }
        };

        let raw = self.search_query.trim();
        if raw.is_empty() {
            let mut list: Vec<CatalogPluginEntry> = self
                .entries
                .iter()
                .filter(|e| matches_compat(&e.id))
                .cloned()
                .collect();
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
            if !matches_compat(&entry.id) {
                continue;
            }
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
    pub(super) fn sort_catalog_entries(&self, list: &mut [CatalogPluginEntry]) {
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

/// Options of the compatibility dropdown in `TabEntry` form; the current
/// filter's row is pre-highlighted via `active_ix`. Shared by the standalone
/// catalog window and the in-settings panel (`open_*_compat_popup`).
pub(crate) fn compat_popup_entries(
    filter: CatalogCompatFilter,
) -> (Vec<crate::views::tab_list::TabEntry>, usize) {
    let entries: Vec<crate::views::tab_list::TabEntry> = CatalogCompatFilter::ALL
        .iter()
        .enumerate()
        .map(|(ix, option)| crate::views::tab_list::TabEntry {
            id: ix,
            title: SharedString::from(option.label(true)),
            subtitle: None,
            icon: icons::ICON_CHEVRON_RIGHT,
            font_family: None,
            dot_color: Some(option.dot_color_static()),
            trailing_icon: None,
        })
        .collect();
    let active_ix = CatalogCompatFilter::ALL
        .iter()
        .position(|o| *o == filter)
        .unwrap_or(0);
    (entries, active_ix)
}
