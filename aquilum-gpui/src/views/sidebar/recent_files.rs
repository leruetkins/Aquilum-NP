//! Recent Files (Recent Documents) sidebar component.
//!
//! Matches Obsidian's `recent-files-obsidian` plugin data format (`data.json`)
//! stored in `<vault>/.vault/plugins/recent-files-obsidian/data.json` (or `.vault/recent-files.json`).
//! Provides quick filtering, cycling sort order (newest/oldest/alphabetical),
//! clear all, single file deletion ("×"), last opened date/time display,
//! and full path display underneath each note.

use std::path::{Path, PathBuf};
use gpui::{
    div, point, prelude::*, px, Context, ElementId, IntoElement, MouseDownEvent, Window,
};
use serde::{Deserialize, Serialize};

use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::scrollbar::{
    overlay_scrollbar, ScrollbarGeometry,
};
use crate::views::sidebar::tree::{
    sidebar_scrollbar_style, SIDEBAR_ROW_SCROLLBAR_GAP,
};

/// A single recent file item matching `recent-files-obsidian` plugin data schema.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentFileItem {
    pub basename: String,
    pub path: String,
    /// Timestamp of when this note was last opened (seconds since UNIX epoch).
    /// Kept optional for backward compatibility with existing `data.json` from Obsidian.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opened_at: Option<u64>,
}

fn default_update_on() -> String {
    "file-open".to_string()
}

fn default_max_length() -> usize {
    50
}

/// Root data container matching `.vault/plugins/recent-files-obsidian/data.json`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentFilesData {
    #[serde(rename = "recentFiles", default)]
    pub recent_files: Vec<RecentFileItem>,
    #[serde(rename = "omittedPaths", default)]
    pub omitted_paths: Vec<String>,
    #[serde(rename = "omittedTags", default)]
    pub omitted_tags: Vec<String>,
    #[serde(rename = "updateOn", default = "default_update_on")]
    pub update_on: String,
    #[serde(rename = "omitBookmarks", default)]
    pub omit_bookmarks: bool,
    #[serde(rename = "maxLength", default = "default_max_length")]
    pub max_length: usize,
}

impl Default for RecentFilesData {
    fn default() -> Self {
        Self {
            recent_files: Vec::new(),
            omitted_paths: Vec::new(),
            omitted_tags: Vec::new(),
            update_on: default_update_on(),
            omit_bookmarks: false,
            max_length: default_max_length(),
        }
    }
}

/// Sort modes for the recent files sidebar list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RecentFilesSortOrder {
    #[default]
    RecentDesc, // Most recent first (default)
    RecentAsc,  // Oldest first
    AlphabeticalAsc, // A to Z
    AlphabeticalDesc, // Z to A
}

impl RecentFilesData {
    pub fn is_empty(&self) -> bool {
        self.recent_files.is_empty()
    }

    /// Normalizes path string by converting backslashes to forward slashes.
    pub fn normalize_path(p: &str) -> String {
        p.replace('\\', "/").trim_matches('/').to_string()
    }

    /// Checks if a path is omitted according to `omitted_paths` patterns.
    pub fn is_path_omitted(&self, norm_path: &str) -> bool {
        for pattern in &self.omitted_paths {
            let pat = pattern.trim();
            if pat.is_empty() {
                continue;
            }
            if norm_path.starts_with(pat) || norm_path.contains(pat) {
                return true;
            }
        }
        false
    }

    /// Adds or moves a file to the top of recent files.
    pub fn add_file(&mut self, path_str: &str, basename: &str, opened_at: u64) {
        let norm = Self::normalize_path(path_str);
        if self.is_path_omitted(&norm) {
            return;
        }

        // Remove any existing entry matching this path
        self.recent_files.retain(|item| Self::normalize_path(&item.path) != norm);

        // Prepend as most recent
        self.recent_files.insert(
            0,
            RecentFileItem {
                basename: basename.to_string(),
                path: norm,
                opened_at: Some(opened_at),
            },
        );

        // Limit to max_length
        let limit = if self.max_length == 0 { 50 } else { self.max_length };
        if self.recent_files.len() > limit {
            self.recent_files.truncate(limit);
        }
    }

    /// Removes a file from the recent list. Returns true if found and removed.
    pub fn remove_file(&mut self, path_str: &str) -> bool {
        let norm = Self::normalize_path(path_str);
        let before_len = self.recent_files.len();
        self.recent_files.retain(|item| Self::normalize_path(&item.path) != norm);
        self.recent_files.len() != before_len
    }

    /// Removes any entry whose path matches `target` or starts with `target` (for deleted folders).
    pub fn remove_path_or_prefix(&mut self, target: &Path, ws: Option<&Path>) -> bool {
        let norm_target = Self::normalize_path(&target.to_string_lossy());
        let before_len = self.recent_files.len();
        self.recent_files.retain(|item| {
            let item_p = Path::new(&item.path);
            let abs_item = if item_p.is_absolute() {
                item_p.to_path_buf()
            } else if let Some(ws_path) = ws {
                ws_path.join(item_p)
            } else {
                item_p.to_path_buf()
            };
            let norm_abs = Self::normalize_path(&abs_item.to_string_lossy());
            let norm_item = Self::normalize_path(&item.path);

            let is_match = abs_item == target
                || norm_abs == norm_target
                || norm_item == norm_target
                || abs_item.starts_with(target)
                || norm_abs.starts_with(&norm_target);

            !is_match
        });
        self.recent_files.len() != before_len
    }

    /// Updates any entry whose path matches `old_path` or starts with `old_path` (for renamed folders).
    pub fn rename_path_or_prefix(&mut self, old_path: &Path, new_path: &Path, ws: Option<&Path>) -> bool {
        let norm_old = Self::normalize_path(&old_path.to_string_lossy());
        let norm_new = Self::normalize_path(&new_path.to_string_lossy());
        let mut any_renamed = false;

        for item in &mut self.recent_files {
            let item_p = Path::new(&item.path);
            let abs_item = if item_p.is_absolute() {
                item_p.to_path_buf()
            } else if let Some(ws_path) = ws {
                ws_path.join(item_p)
            } else {
                item_p.to_path_buf()
            };
            let norm_abs = Self::normalize_path(&abs_item.to_string_lossy());
            let norm_item = Self::normalize_path(&item.path);

            if abs_item == old_path || norm_abs == norm_old || norm_item == norm_old {
                // Exact file match
                let new_rel = if let Some(ws_path) = ws {
                    new_path.strip_prefix(ws_path).unwrap_or(new_path)
                } else {
                    new_path
                };
                item.path = Self::normalize_path(&new_rel.to_string_lossy());
                item.basename = new_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| item.basename.clone());
                any_renamed = true;
            } else if let Ok(suffix) = abs_item.strip_prefix(old_path) {
                // Folder rename prefix match
                let updated_abs = new_path.join(suffix);
                let new_rel = if let Some(ws_path) = ws {
                    updated_abs.strip_prefix(ws_path).unwrap_or(&updated_abs)
                } else {
                    &updated_abs
                };
                item.path = Self::normalize_path(&new_rel.to_string_lossy());
                any_renamed = true;
            } else if norm_abs.starts_with(&norm_old) {
                // Normalized prefix match
                let suffix = &norm_abs[norm_old.len()..];
                let suffix_clean = suffix.trim_start_matches('/');
                let updated_abs = format!("{}/{}", norm_new, suffix_clean);
                item.path = if let Some(ws_path) = ws {
                    let norm_ws = Self::normalize_path(&ws_path.to_string_lossy());
                    if updated_abs.starts_with(&norm_ws) {
                        updated_abs[norm_ws.len()..].trim_start_matches('/').to_string()
                    } else {
                        updated_abs
                    }
                } else {
                    updated_abs
                };
                any_renamed = true;
            }
        }

        any_renamed
    }

    /// Prunes any entries whose files no longer exist on disk.
    pub fn prune_nonexistent(&mut self, ws: Option<&Path>) -> bool {
        let before_len = self.recent_files.len();
        self.recent_files.retain(|item| {
            let p = Path::new(&item.path);
            let abs_path = if p.is_absolute() {
                p.to_path_buf()
            } else if let Some(ws_path) = ws {
                ws_path.join(p)
            } else {
                p.to_path_buf()
            };
            abs_path.exists()
        });
        self.recent_files.len() != before_len
    }

    /// Renames a file entry if present.
    pub fn rename_file(&mut self, old_path: &str, new_path: &str, new_basename: &str) {
        let old_norm = Self::normalize_path(old_path);
        let new_norm = Self::normalize_path(new_path);
        for item in &mut self.recent_files {
            if Self::normalize_path(&item.path) == old_norm {
                item.path = new_norm.clone();
                item.basename = new_basename.to_string();
            }
        }
    }

    /// Filters and sorts recent files according to current query and sort order.
    pub fn get_filtered_and_sorted(
        &self,
        sort_order: RecentFilesSortOrder,
        query: &str,
        workspace: Option<&Path>,
    ) -> Vec<(RecentFileItem, u64)> {
        let q = query.trim().to_lowercase();
        let is_filtering = !q.is_empty();

        let mut items_with_time: Vec<(RecentFileItem, u64)> = self
            .recent_files
            .iter()
            .filter(|item| {
                if let Some(ws) = workspace {
                    let p = Path::new(&item.path);
                    let abs = if p.is_absolute() {
                        p.to_path_buf()
                    } else {
                        ws.join(p)
                    };
                    if !abs.exists() {
                        return false;
                    }
                }
                if !is_filtering {
                    return true;
                }
                item.basename.to_lowercase().contains(&q)
                    || item.path.to_lowercase().contains(&q)
            })
            .map(|item| {
                let time = item.opened_at.unwrap_or_else(|| {
                    // Fallback to filesystem mtime if opened_at was not stored yet
                    if let Some(ws) = workspace {
                        let abs = ws.join(&item.path);
                        std::fs::metadata(&abs)
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0)
                    } else {
                        0
                    }
                });
                (item.clone(), time)
            })
            .collect();

        match sort_order {
            RecentFilesSortOrder::RecentDesc => {
                // If items already in recent order in data.json, keep their index order if times match
                items_with_time.sort_by(|a, b| b.1.cmp(&a.1));
            }
            RecentFilesSortOrder::RecentAsc => {
                items_with_time.sort_by(|a, b| a.1.cmp(&b.1));
            }
            RecentFilesSortOrder::AlphabeticalAsc => {
                items_with_time.sort_by(|a, b| a.0.basename.to_lowercase().cmp(&b.0.basename.to_lowercase()));
            }
            RecentFilesSortOrder::AlphabeticalDesc => {
                items_with_time.sort_by(|a, b| b.0.basename.to_lowercase().cmp(&a.0.basename.to_lowercase()));
            }
        }

        items_with_time
    }
}

/// Converts seconds since UNIX epoch to Civil date & time (year, month, day, hour, minute).
pub fn civil_from_secs(secs: i64) -> (i64, i64, i64, i64, i64) {
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let (h, m) = (time_of_day / 3600, (time_of_day % 3600) / 60);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mth <= 2 { y + 1 } else { y };
    (y, mth, d, h, m)
}

/// Returns local timezone offset in seconds.
#[cfg(target_os = "windows")]
pub fn local_offset_secs() -> i64 {
    #[repr(C)]
    struct SYSTEMTIME {
        w_year: u16,
        w_month: u16,
        w_day_of_week: u16,
        w_day: u16,
        w_hour: u16,
        w_minute: u16,
        w_second: u16,
        w_milliseconds: u16,
    }
    #[repr(C)]
    struct TIME_ZONE_INFORMATION {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: SYSTEMTIME,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: SYSTEMTIME,
        daylight_bias: i32,
    }
    extern "system" {
        fn GetTimeZoneInformation(lp_time_zone_information: *mut TIME_ZONE_INFORMATION) -> u32;
    }
    unsafe {
        let mut tz = std::mem::zeroed();
        let ret = GetTimeZoneInformation(&mut tz);
        if ret != 0xFFFFFFFF {
            let bias = if ret == 2 {
                tz.bias + tz.daylight_bias
            } else {
                tz.bias + tz.standard_bias
            };
            return -(bias as i64) * 60;
        }
    }
    0
}

#[cfg(not(target_os = "windows"))]
pub fn local_offset_secs() -> i64 {
    0
}

/// Formats a timestamp into user-friendly date and time string.
pub fn format_recent_timestamp(timestamp_secs: u64, yesterday_label: &str) -> String {
    if timestamp_secs == 0 {
        return String::new();
    }

    let offset = local_offset_secs();
    let local_ts = (timestamp_secs as i64) + offset;
    let now_local = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64)
        + offset;

    let (now_y, _now_mth, _now_d, _, _) = civil_from_secs(now_local);
    let (item_y, item_mth, item_d, item_h, item_min) = civil_from_secs(local_ts);

    let now_days = now_local.div_euclid(86_400);
    let item_days = local_ts.div_euclid(86_400);

    if now_days == item_days {
        // Same calendar day: show HH:MM
        format!("{item_h:02}:{item_min:02}")
    } else if now_days == item_days + 1 {
        // Yesterday
        format!("{} {item_h:02}:{item_min:02}", yesterday_label)
    } else if now_y == item_y {
        // Same year: DD.MM HH:MM
        format!("{item_d:02}.{item_mth:02} {item_h:02}:{item_min:02}")
    } else {
        // Older year: YYYY-MM-DD
        format!("{item_y:04}-{item_mth:02}-{item_d:02}")
    }
}

impl AppView {
    /// Saves recent files to `.vault/plugins/recent-files-obsidian/data.json` and session state.
    pub fn save_recent_files(&mut self) {
        if let Some(ref ws) = self.state.workspace_path {
            let plugin_dir = ws.join(".vault").join("plugins").join("recent-files-obsidian");
            let _ = std::fs::create_dir_all(&plugin_dir);
            let path = plugin_dir.join("data.json");
            if let Ok(json) = serde_json::to_string_pretty(&self.state.recent_files) {
                let _ = std::fs::write(&path, json);
            }
        }
        self.state.session.recent_files = self.state.recent_files.clone();
        self.state.save_session_state();
    }

    /// Loads recent files from `.vault/plugins/recent-files-obsidian/data.json` or fallback.
    pub fn load_recent_files(&mut self) {
        if let Some(ref ws) = self.state.workspace_path {
            let plugin_path = ws.join(".vault").join("plugins").join("recent-files-obsidian").join("data.json");
            let vault_path = ws.join(".vault").join("recent-files.json");
            let target = if plugin_path.exists() {
                Some(plugin_path)
            } else if vault_path.exists() {
                Some(vault_path)
            } else {
                None
            };
            if let Some(p) = target {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    if let Ok(data) = serde_json::from_str::<RecentFilesData>(&content) {
                        self.state.recent_files = data;
                        return;
                    }
                }
            }
        }
        if !self.state.session.recent_files.is_empty() {
            self.state.recent_files = self.state.session.recent_files.clone();
        }
        let ws = self.state.workspace_path.as_deref();
        self.state.recent_files.prune_nonexistent(ws);
    }

    /// Opens the Recent Files sidebar panel.
    pub fn open_sidebar_recent_files(&mut self, cx: &mut Context<Self>) {
        let ws = self.state.workspace_path.as_deref();
        if self.state.recent_files.prune_nonexistent(ws) {
            self.state.session.recent_files = self.state.recent_files.clone();
            self.save_recent_files();
        }
        self.sidebar_mode = crate::views::sidebar::SidebarMode::RecentFiles;
        self.sidebar_collapsed = false;
        self.state.session.sidebar_mode = crate::views::sidebar::SidebarMode::RecentFiles;
        self.state.save_session_state();
        cx.notify();
    }

    /// Toggles recent files sort order between newest first (RecentDesc) and oldest first (RecentAsc).
    pub fn toggle_recent_files_sort(&mut self, cx: &mut Context<Self>) {
        self.recent_files_sort_order = match self.recent_files_sort_order {
            RecentFilesSortOrder::RecentDesc => RecentFilesSortOrder::RecentAsc,
            RecentFilesSortOrder::RecentAsc => RecentFilesSortOrder::RecentDesc,
            RecentFilesSortOrder::AlphabeticalAsc => RecentFilesSortOrder::RecentDesc,
            RecentFilesSortOrder::AlphabeticalDesc => RecentFilesSortOrder::RecentAsc,
        };
        self.state.session.recent_files_sort_order = self.recent_files_sort_order;
        self.state.save_session_state();

        let status_key = match self.recent_files_sort_order {
            RecentFilesSortOrder::RecentDesc => "sidebar.sort_recent_desc",
            RecentFilesSortOrder::RecentAsc => "sidebar.sort_recent_asc",
            RecentFilesSortOrder::AlphabeticalAsc => "sidebar.sort_alpha_asc",
            RecentFilesSortOrder::AlphabeticalDesc => "sidebar.sort_alpha_desc",
        };
        self.state.status_text = self.t(status_key);
        cx.notify();
    }

    /// Clears the entire list of recent files.
    pub fn clear_all_recent_files(&mut self, cx: &mut Context<Self>) {
        self.state.recent_files.recent_files.clear();
        self.save_recent_files();
        self.state.status_text = self.t("sidebar.recent_files_cleared");
        cx.notify();
    }

    /// Removes a single recent file entry by path.
    pub fn remove_recent_file(&mut self, path: &str, cx: &mut Context<Self>) {
        self.state.recent_files.remove_file(path);
        self.save_recent_files();
        cx.notify();
    }

    /// Scrolls the recent files list to `scroll_y` pixels from the top.
    pub(crate) fn set_recent_files_scroll_top(&self, scroll_y: f32) {
        self.recent_files_scroll_handle
            .set_offset(point(px(0.0), px(-scroll_y)));
    }

    /// Follows the pointer while the recent files overlay scrollbar thumb is being dragged.
    pub(crate) fn handle_recent_files_scrollbar_drag(
        &mut self,
        pointer_y: f32,
        cx: &mut Context<Self>,
    ) {
        if !self.recent_files_scrollbar_drag.active {
            return;
        }
        let style = sidebar_scrollbar_style();
        let Some(geometry) =
            ScrollbarGeometry::from_handle(&self.recent_files_scroll_handle, &style)
        else {
            return;
        };
        if geometry.apply_drag(
            &self.recent_files_scroll_handle,
            &self.recent_files_scrollbar_drag,
            pointer_y,
        ) {
            cx.notify();
        }
    }

    /// Renders the Recent Files sidebar panel matching the plugin layout.
    pub fn render_sidebar_recent_files(
        &self,
        active_path: Option<&PathBuf>,
        theme: &Theme,
        _window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let query = self.state.search_query.trim().to_lowercase();
        let ws = self.state.workspace_path.as_deref();
        let items = self.state.recent_files.get_filtered_and_sorted(
            self.recent_files_sort_order,
            &query,
            ws,
        );

        if items.is_empty() {
            let msg = if query.is_empty() {
                self.t("sidebar.no_recent_files")
            } else {
                self.t("sidebar.no_matches")
            };
            return div()
                .id("sidebar-recent-files-viewport")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .pt_8()
                        .gap_2()
                        .child(icons::icon(icons::ICON_HISTORY, 24.0, theme.text_muted))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(msg),
                        ),
                );
        }

        let yesterday_str = self.t("sidebar.yesterday");
        let style = sidebar_scrollbar_style();
        // With no scrollbar the cards may span the full content width, flush with
        // the quick filter above; the gap exists only to clear the thumb.
        let has_scrollbar =
            ScrollbarGeometry::from_handle(&self.recent_files_scroll_handle, &style).is_some();
        let mut rows = Vec::with_capacity(items.len());

        for (idx, (item, time_sec)) in items.into_iter().enumerate() {
            let abs_path = if Path::new(&item.path).is_absolute() {
                PathBuf::from(&item.path)
            } else if let Some(ws_path) = ws {
                ws_path.join(&item.path)
            } else {
                PathBuf::from(&item.path)
            };

            let is_active = active_path.map_or(false, |ap| ap == &abs_path);
            let date_label = format_recent_timestamp(time_sec, &yesterday_str);
            let path_to_remove = item.path.clone();
            let path_to_open = abs_path.clone();

            let row = div()
                .id(ElementId::Name(format!("recent-file-{}", idx).into()))
                .flex()
                .flex_col()
                .gap(px(1.0))
                .px_2()
                .py_1p5()
                // The active card needs a wider gutter than the shared 4 px one.
                // The row must not be `w_full()`: the column stretches it, so this
                // margin actually narrows it, while `w_full()` would make the
                // margin overflow and get clipped, leaving the card on the bar.
                .mr(px(if has_scrollbar {
                    SIDEBAR_ROW_SCROLLBAR_GAP + 4.0
                } else {
                    0.0
                }))
                .rounded_md()
                .cursor_pointer()
                .when(is_active, |d| {
                    d.bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                })
                .hover(|h| h.bg(theme.bg_hover))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if path_to_open.exists() {
                        this.select_note(path_to_open.clone(), true, window, cx);
                    } else {
                        let name = path_to_open.file_name().unwrap_or_default().to_string_lossy();
                        this.state.status_text = format!("File not found: {}", name);
                        cx.notify();
                    }
                }))
                // Line 1: icon + title on the left, date + remove button on the right
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .w_full()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .flex_1()
                                .overflow_hidden()
                                .child(icons::icon(
                                    icons::ICON_FILE_TEXT,
                                    13.0,
                                    if is_active { theme.accent } else { theme.text_muted },
                                ))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(if is_active { theme.accent } else { theme.text_primary })
                                        // `flex_1` + `min_w_0` give the text a definite,
                                        // shrinkable width, so it ellipsizes instead of
                                        // overflowing and being clipped mid-glyph.
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(item.basename.clone()),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .flex_shrink_0()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .child(date_label),
                                )
                                .child(
                                    div()
                                        .id(ElementId::Name(format!("btn-remove-recent-{}", idx).into()))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(18.0))
                                        .h(px(18.0))
                                        .rounded_md()
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .hover(|h| h.bg(theme.bg_hover).text_color(gpui::rgb(0xe05252)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.remove_recent_file(&path_to_remove, cx);
                                        }))
                                        .child("×"),
                                ),
                        ),
                )
                // Line 2: Full path underneath in smaller muted font
                .child(
                    div()
                        .pl(px(18.0))
                        .w_full()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .truncate()
                        .child(item.path.clone()),
                );

            rows.push(row);
        }

        let scroll_container = div()
            .id("sidebar-recent-files-scroll")
            .track_scroll(&self.recent_files_scroll_handle)
            .flex()
            .flex_col()
            .flex_1()
            .overflow_scroll()
            .gap(px(2.0))
            .children(rows);

        let mut viewport = div()
            .id("sidebar-recent-files-viewport")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(scroll_container);

        if let Some(geometry) = ScrollbarGeometry::from_handle(&self.recent_files_scroll_handle, &style) {
            let dragging = self.recent_files_scrollbar_drag.active;
            viewport = viewport.child(overlay_scrollbar(
                "sidebar-recent-files-scrollbar-track",
                &geometry,
                &style,
                theme,
                dragging,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if this.is_resizing_any() {
                        return;
                    }
                    cx.stop_propagation();
                    let pointer_y: f32 = event.position.y.into();
                    this.set_recent_files_scroll_top(geometry.offset_for_track_click(pointer_y));
                    cx.notify();
                }),
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if this.is_resizing_any() {
                        return;
                    }
                    cx.stop_propagation();
                    this.recent_files_scrollbar_drag
                        .begin(event.position.y.into(), geometry.scroll_y);
                    cx.notify();
                }),
            ));
        }

        viewport
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_recent_files_obsidian_data_json() {
        let json = r#"{
            "recentFiles": [
                {
                    "basename": "дальше",
                    "path": "дальше.md"
                },
                {
                    "basename": "ИИ",
                    "path": "Входящие/ИИ.md"
                }
            ],
            "omittedPaths": ["^daily/"],
            "omittedTags": [],
            "updateOn": "file-open",
            "omitBookmarks": false,
            "maxLength": 50
        }"#;

        let data: RecentFilesData = serde_json::from_str(json).expect("Must parse recent files json");
        assert_eq!(data.recent_files.len(), 2);
        assert_eq!(data.recent_files[0].basename, "дальше");
        assert_eq!(data.recent_files[0].path, "дальше.md");
        assert_eq!(data.recent_files[1].basename, "ИИ");
        assert_eq!(data.recent_files[1].path, "Входящие/ИИ.md");
        assert_eq!(data.max_length, 50);
        assert_eq!(data.update_on, "file-open");
    }

    #[test]
    fn test_add_and_remove_recent_files() {
        let mut data = RecentFilesData::default();
        data.max_length = 3;

        data.add_file("Note1.md", "Note1", 100);
        data.add_file("Note2.md", "Note2", 200);
        assert_eq!(data.recent_files.len(), 2);
        assert_eq!(data.recent_files[0].basename, "Note2");

        // Bump existing Note1
        data.add_file("Note1.md", "Note1", 300);
        assert_eq!(data.recent_files.len(), 2);
        assert_eq!(data.recent_files[0].basename, "Note1");
        assert_eq!(data.recent_files[0].opened_at, Some(300));

        // Add more than max_length
        data.add_file("Note3.md", "Note3", 400);
        data.add_file("Note4.md", "Note4", 500);
        assert_eq!(data.recent_files.len(), 3);
        assert_eq!(data.recent_files[0].basename, "Note4");

        // Remove
        assert!(data.remove_file("Note3.md"));
        assert_eq!(data.recent_files.len(), 2);
    }

    #[test]
    fn test_sorting_and_filtering() {
        let mut data = RecentFilesData::default();
        data.add_file("b_file.md", "Beta", 100);
        data.add_file("a_file.md", "Alpha", 200);
        data.add_file("c_file.md", "Gamma", 50);

        // RecentDesc
        let sorted = data.get_filtered_and_sorted(RecentFilesSortOrder::RecentDesc, "", None);
        assert_eq!(sorted[0].0.basename, "Alpha");
        assert_eq!(sorted[1].0.basename, "Beta");
        assert_eq!(sorted[2].0.basename, "Gamma");

        // AlphabeticalAsc
        let sorted_alpha = data.get_filtered_and_sorted(RecentFilesSortOrder::AlphabeticalAsc, "", None);
        assert_eq!(sorted_alpha[0].0.basename, "Alpha");
        assert_eq!(sorted_alpha[1].0.basename, "Beta");
        assert_eq!(sorted_alpha[2].0.basename, "Gamma");

        // Filtering
        let filtered = data.get_filtered_and_sorted(RecentFilesSortOrder::RecentDesc, "alp", None);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0.basename, "Alpha");
    }

    #[test]
    fn test_civil_from_secs_format() {
        // 2026-10-30 00:00:00 UTC = 1793318400
        let (y, m, d, _, _) = civil_from_secs(1793318400);
        assert_eq!(y, 2026);
        assert_eq!(m, 10);
        assert_eq!(d, 30);
    }

    #[test]
    fn test_remove_and_rename_path_or_prefix() {
        let mut data = RecentFilesData::default();
        data.add_file("Folder/Sub/Doc1.md", "Doc1", 100);
        data.add_file("Folder/Doc2.md", "Doc2", 200);
        data.add_file("Other.md", "Other", 300);

        let ws = Path::new("/vault");
        // Rename folder
        assert!(data.rename_path_or_prefix(Path::new("/vault/Folder"), Path::new("/vault/NewFolder"), Some(ws)));
        assert_eq!(data.recent_files[1].path, "NewFolder/Doc2.md");
        assert_eq!(data.recent_files[2].path, "NewFolder/Sub/Doc1.md");

        // Delete single file
        assert!(data.remove_path_or_prefix(Path::new("/vault/Other.md"), Some(ws)));
        assert_eq!(data.recent_files.len(), 2);

        // Delete folder
        assert!(data.remove_path_or_prefix(Path::new("/vault/NewFolder"), Some(ws)));
        assert_eq!(data.recent_files.len(), 0);
    }
}
