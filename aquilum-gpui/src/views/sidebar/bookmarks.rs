//! Bookmarks (Favorites) sidebar component.
//!
//! Stores bookmarked files, folders, and groups in `.vault/bookmarks.json`.
//! Supports mouse drag-and-drop between groups
//! and the root level, quick-filtering, folding/unfolding groups, revealing folders in the file tree,
//! right-click context menu (Image 1), and inline rename editing with purple border (Image 2).

use std::path::{Path, PathBuf};

use gpui::{
    div, prelude::*, px, rgb, rgba, AnyElement, Context, FocusHandle, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, SharedString, Window,
};
use serde::{Deserialize, Serialize};

use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::text_input::TextInput;

/// Single item in the bookmarks hierarchy.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum BookmarkItem {
    #[serde(rename = "file")]
    File {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subpath: Option<String>,
    },
    #[serde(rename = "folder")]
    Folder {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    #[serde(rename = "group")]
    Group {
        title: String,
        #[serde(default)]
        items: Vec<BookmarkItem>,
    },
    #[serde(other)]
    Other,
}

/// Root data container matching Obsidian's `.obsidian/bookmarks.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookmarksData {
    #[serde(default)]
    pub items: Vec<BookmarkItem>,
}

impl BookmarksData {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Normalizes path string by converting backslashes to forward slashes.
    pub fn normalize_path(p: &str) -> String {
        p.replace('\\', "/").trim_matches('/').to_string()
    }

    /// Checks if a file path is in the bookmarks (at root or inside any group).
    pub fn contains_file(&self, path_str: &str) -> bool {
        let norm = Self::normalize_path(path_str);
        Self::items_contain_file(&self.items, &norm)
    }

    fn items_contain_file(items: &[BookmarkItem], norm_path: &str) -> bool {
        for item in items {
            match item {
                BookmarkItem::File { path, .. } => {
                    if Self::normalize_path(path) == norm_path {
                        return true;
                    }
                }
                BookmarkItem::Group { items, .. } => {
                    if Self::items_contain_file(items, norm_path) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Checks if a folder path is in the bookmarks.
    pub fn contains_folder(&self, path_str: &str) -> bool {
        let norm = Self::normalize_path(path_str);
        for item in &self.items {
            if let BookmarkItem::Folder { path, .. } = item {
                if Self::normalize_path(path) == norm {
                    return true;
                }
            }
        }
        false
    }

    /// Adds a file bookmark. If `group_title` is provided, adds into that group;
    /// otherwise adds at the root level. Avoids duplicates.
    pub fn add_file(&mut self, path: String, title: Option<String>, group_title: Option<&str>) {
        let norm = Self::normalize_path(&path);
        if self.contains_file(&norm) {
            return;
        }

        let new_item = BookmarkItem::File {
            path,
            title,
            subpath: None,
        };

        if let Some(gt) = group_title {
            for item in &mut self.items {
                if let BookmarkItem::Group { title, items } = item {
                    if title == gt {
                        items.push(new_item);
                        return;
                    }
                }
            }
        }

        self.items.push(new_item);
    }

    /// Adds a folder bookmark at the root level.
    pub fn add_folder(&mut self, path: String, title: Option<String>) {
        let norm = Self::normalize_path(&path);
        if self.contains_folder(&norm) {
            return;
        }

        self.items.push(BookmarkItem::Folder { path, title });
    }

    /// Removes a file from all groups or root. Returns true if removed.
    pub fn remove_file(&mut self, path_str: &str) -> bool {
        let norm = Self::normalize_path(path_str);
        Self::items_remove_file(&mut self.items, &norm)
    }

    fn items_remove_file(items: &mut Vec<BookmarkItem>, norm: &str) -> bool {
        let mut changed = false;
        let mut i = 0;
        while i < items.len() {
            match &mut items[i] {
                BookmarkItem::File { path, .. } => {
                    if BookmarksData::normalize_path(path) == norm {
                        items.remove(i);
                        changed = true;
                        continue;
                    }
                }
                BookmarkItem::Group { items: sub_items, .. } => {
                    if Self::items_remove_file(sub_items, norm) {
                        changed = true;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        changed
    }

    /// Removes a folder from bookmarks. Returns true if removed.
    pub fn remove_folder(&mut self, path_str: &str) -> bool {
        let norm = Self::normalize_path(path_str);
        let before_len = self.items.len();
        self.items.retain(|item| match item {
            BookmarkItem::Folder { path, .. } => BookmarksData::normalize_path(path) != norm,
            _ => true,
        });
        self.items.len() != before_len
    }

    /// Creates a new group with a unique name if necessary.
    pub fn create_group(&mut self, base_title: &str) -> String {
        let mut title = base_title.to_string();
        let mut counter = 1;
        while self.items.iter().any(|item| match item {
            BookmarkItem::Group { title: t, .. } => t == &title,
            _ => false,
        }) {
            title = format!("{} ({})", base_title, counter);
            counter += 1;
        }

        self.items.push(BookmarkItem::Group {
            title: title.clone(),
            items: Vec::new(),
        });
        title
    }

    /// Deletes the group at the given root index.
    pub fn delete_group(&mut self, group_idx: usize) {
        if group_idx < self.items.len() && matches!(self.items[group_idx], BookmarkItem::Group { .. }) {
            self.items.remove(group_idx);
        }
    }

    /// Renames the group at the given root index.
    pub fn rename_group(&mut self, group_idx: usize, new_title: String) {
        if group_idx < self.items.len() {
            if let BookmarkItem::Group { ref mut title, .. } = self.items[group_idx] {
                *title = new_title;
            }
        }
    }

    /// Renames a file bookmark's title/alias.
    pub fn rename_file_bookmark(&mut self, group_idx: Option<usize>, item_idx: usize, new_title: String) {
        match group_idx {
            None => {
                if let Some(BookmarkItem::File { ref mut title, .. }) = self.items.get_mut(item_idx) {
                    *title = Some(new_title);
                }
            }
            Some(g) => {
                if let Some(BookmarkItem::Group { ref mut items, .. }) = self.items.get_mut(g) {
                    if let Some(BookmarkItem::File { ref mut title, .. }) = items.get_mut(item_idx) {
                        *title = Some(new_title);
                    }
                }
            }
        }
    }

    /// Renames a folder bookmark's title/alias.
    pub fn rename_folder_bookmark(&mut self, item_idx: usize, new_title: String) {
        if let Some(BookmarkItem::Folder { ref mut title, .. }) = self.items.get_mut(item_idx) {
            *title = Some(new_title);
        }
    }
}

/// Identifies the drag source of a favorite note.
#[derive(Clone, Debug, PartialEq)]
pub struct BookmarkDragSource {
    pub group_index: Option<usize>,
    pub item_index: usize,
    pub path: String,
    pub display_name: String,
}

/// Identifies the drop target in the bookmarks list.
#[derive(Clone, Debug, PartialEq)]
pub enum BookmarkDropTarget {
    IntoGroup(usize),
    GroupItem(usize, usize),
    RootItem(usize),
    RootEnd,
}

/// Complete drag-and-drop state.
#[derive(Clone, Debug)]
pub struct BookmarkDragState {
    pub source: BookmarkDragSource,
    pub start_y: f32,
    pub is_dragging: bool,
    pub target: Option<BookmarkDropTarget>,
}

/// Target element for the right-click context menu (Image 1).
#[derive(Clone, Debug, PartialEq)]
pub enum BookmarkContextTarget {
    File {
        path: String,
        title: Option<String>,
        group_index: Option<usize>,
        item_index: usize,
    },
    Folder {
        path: String,
        title: Option<String>,
        item_index: usize,
    },
    Group {
        group_index: usize,
        title: String,
    },
    EmptyArea,
}

/// Active context menu state for bookmarks sidebar.
#[derive(Clone, Debug)]
pub struct BookmarkContextMenuState {
    pub target: BookmarkContextTarget,
    pub x: f32,
    pub y: f32,
}

/// Target element for inline renaming (Image 2).
#[derive(Clone, Debug, PartialEq)]
pub enum BookmarkRenameTarget {
    Group(usize),
    File {
        group_index: Option<usize>,
        item_index: usize,
    },
    Folder(usize),
}

/// Active inline rename state with TextInput and selection.
pub struct BookmarkRenameState {
    pub target: BookmarkRenameTarget,
    pub input: TextInput,
    pub original_title: String,
}

impl BookmarkRenameState {
    pub fn new(target: BookmarkRenameTarget, initial_title: String, focus_handle: FocusHandle) -> Self {
        let mut input = TextInput::new(focus_handle);
        input.set_text(initial_title.clone());
        input.select_all();
        Self {
            target,
            input,
            original_title: initial_title,
        }
    }
}

/// Helper converting filesystem Path to bookmark relative/absolute string.
pub fn path_to_bookmark_str(workspace: Option<&Path>, path: &Path) -> String {
    if let Some(ws) = workspace {
        if let Ok(rel) = path.strip_prefix(ws) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    path.to_string_lossy().replace('\\', "/")
}

/// Helper converting bookmark string to actual PathBuf.
pub fn bookmark_str_to_path(workspace: Option<&Path>, bookmark_path: &str) -> PathBuf {
    let p = PathBuf::from(bookmark_path);
    if p.is_absolute() {
        p
    } else if let Some(ws) = workspace {
        ws.join(bookmark_path)
    } else {
        p
    }
}

/// Recursively collects all note and document files within a folder path.
/// Skips hidden directories (`.obsidian`, `.git`), build folders (`node_modules`, `target`),
/// and sorts files alphabetically. Limits collection to at most 50 files to prevent performance issues.
pub fn collect_files_in_folder(folder: &Path) -> Vec<PathBuf> {
    if !folder.is_dir() {
        return Vec::new();
    }

    let mut notes = Vec::new();
    let mut other_files = Vec::new();
    let mut dirs_to_visit = vec![folder.to_path_buf()];

    while let Some(current_dir) = dirs_to_visit.pop() {
        let entries = match std::fs::read_dir(&current_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut sub_dirs = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
                continue;
            }

            if path.is_dir() {
                sub_dirs.push(path);
            } else if path.is_file() {
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if matches!(ext.as_str(), "md" | "markdown" | "txt" | "canvas" | "pdf") {
                    notes.push(path);
                } else {
                    other_files.push(path);
                }
            }
        }

        sub_dirs.sort();
        dirs_to_visit.extend(sub_dirs.into_iter().rev());

        if notes.len() >= 50 {
            notes.truncate(50);
            break;
        }
    }

    notes.sort();
    if !notes.is_empty() {
        return notes;
    }

    other_files.sort();
    other_files.truncate(50);
    other_files
}

/// Collects all file paths from a bookmark group (including nested groups/folders).
pub fn collect_files_in_bookmark_group(workspace: Option<&Path>, items: &[BookmarkItem]) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for item in items {
        match item {
            BookmarkItem::File { path, .. } => {
                let abs = bookmark_str_to_path(workspace, path);
                if abs.is_file() && !files.contains(&abs) {
                    files.push(abs);
                }
            }
            BookmarkItem::Folder { path, .. } => {
                let folder_abs = bookmark_str_to_path(workspace, path);
                let folder_files = collect_files_in_folder(&folder_abs);
                for f in folder_files {
                    if !files.contains(&f) {
                        files.push(f);
                    }
                }
            }
            BookmarkItem::Group { items: sub_items, .. } => {
                let sub_files = collect_files_in_bookmark_group(workspace, sub_items);
                for f in sub_files {
                    if !files.contains(&f) {
                        files.push(f);
                    }
                }
            }
            BookmarkItem::Other => {}
        }

        if files.len() >= 50 {
            files.truncate(50);
            break;
        }
    }

    files
}

const BOOKMARK_ROW_H: f32 = 29.0;

impl AppView {
    /// Opens multiple files either in the active leaf (as tabs) or creates a split leaf to the right.
    pub fn open_files_batch(
        &mut self,
        files: &[PathBuf],
        split_right: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if files.is_empty() {
            self.state.status_text = self.t("sidebar.no_files_in_folder");
            cx.notify();
            return;
        }

        self.tab_context_menu.leaf_id = None;
        let first_file = files[0].clone();

        if split_right {
            self.split_tab_right(&first_file, window, cx);
            for f in &files[1..] {
                self.select_note(f.clone(), false, window, cx);
            }
            self.select_note(first_file, true, window, cx);
        } else {
            for f in files {
                self.select_note(f.clone(), false, window, cx);
            }
            self.select_note(first_file, true, window, cx);
        }

        let count_str = files.len().to_string();
        self.state.status_text = self.t_fmt("sidebar.opened_files_count", &[&count_str]);
        cx.notify();
    }
    /// Checks whether any bookmark group is currently expanded.
    pub fn is_any_bookmark_group_expanded(&self) -> bool {
        self.state.bookmarks.items.iter().any(|item| {
            if let BookmarkItem::Group { title, .. } = item {
                self.expanded_bookmark_groups.contains(title)
            } else {
                false
            }
        })
    }

    /// Toggles expand / collapse for all bookmark groups.
    pub fn toggle_expand_all_bookmark_groups(&mut self, cx: &mut Context<Self>) {
        if self.is_any_bookmark_group_expanded() {
            self.expanded_bookmark_groups.clear();
        } else {
            for item in &self.state.bookmarks.items {
                if let BookmarkItem::Group { title, .. } = item {
                    self.expanded_bookmark_groups.insert(title.clone());
                }
            }
        }
        cx.notify();
    }

    /// Creates a new bookmark group and immediately enters inline edit mode (Image 2).
    pub fn create_bookmark_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.bookmark_rename.is_some() {
            self.commit_bookmark_rename(cx);
        }

        let untitled = self.t("sidebar.untitled_group");
        let default_title = if untitled.is_empty() || untitled == "sidebar.untitled_group" {
            "Группа без названия".to_string()
        } else {
            untitled
        };

        let new_group_title = self.state.bookmarks.create_group(&default_title);
        let group_idx = self.state.bookmarks.items.len().saturating_sub(1);
        self.expanded_bookmark_groups.insert(new_group_title.clone());
        self.save_bookmarks();

        self.start_bookmark_rename(
            BookmarkRenameTarget::Group(group_idx),
            &new_group_title,
            window,
            cx,
        );
    }

    /// Starts inline rename mode with pre-selected text matching Image 2.
    pub fn start_bookmark_rename(
        &mut self,
        target: BookmarkRenameTarget,
        initial_title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.bookmark_rename.is_some() {
            self.commit_bookmark_rename(cx);
        }
        let mut rename = BookmarkRenameState::new(
            target,
            initial_title.to_string(),
            self.bookmark_rename_focus_handle.clone(),
        );
        rename
            .input
            .enable_blink(|this: &mut AppView| &mut this.bookmark_rename_blink, cx);
        self.bookmark_rename = Some(rename);
        window.focus(&self.bookmark_rename_focus_handle);
        cx.notify();
    }

    /// Commits the active inline rename.
    pub fn commit_bookmark_rename(&mut self, cx: &mut Context<Self>) {
        let Some(rename) = self.bookmark_rename.take() else {
            return;
        };
        let new_name = rename.input.text.trim().to_string();
        if new_name.is_empty() {
            cx.notify();
            return;
        }

        match rename.target {
            BookmarkRenameTarget::Group(group_idx) => {
                if group_idx < self.state.bookmarks.items.len() {
                    let old_title = rename.original_title;
                    if self.expanded_bookmark_groups.remove(&old_title) {
                        self.expanded_bookmark_groups.insert(new_name.clone());
                    }
                    self.state.bookmarks.rename_group(group_idx, new_name);
                    self.save_bookmarks();
                }
            }
            BookmarkRenameTarget::File {
                group_index,
                item_index,
            } => {
                self.state
                    .bookmarks
                    .rename_file_bookmark(group_index, item_index, new_name);
                self.save_bookmarks();
            }
            BookmarkRenameTarget::Folder(item_index) => {
                self.state.bookmarks.rename_folder_bookmark(item_index, new_name);
                self.save_bookmarks();
            }
        }
        cx.notify();
    }

    /// Cancels active inline rename without modifying bookmarks.
    pub fn cancel_bookmark_rename(&mut self, cx: &mut Context<Self>) {
        self.bookmark_rename = None;
        cx.notify();
    }

    /// Opens the bookmarks right-click context menu (Image 1).
    pub fn open_bookmark_context_menu(
        &mut self,
        target: BookmarkContextTarget,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        if self.bookmark_rename.is_some() {
            self.commit_bookmark_rename(cx);
        }
        self.bookmark_context_menu = Some(BookmarkContextMenuState { target, x, y });
        cx.notify();
    }

    /// Closes the bookmarks right-click context menu.
    pub fn close_bookmark_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.bookmark_context_menu.is_some() {
            self.bookmark_context_menu = None;
            cx.notify();
        }
    }

    /// Opens the sidebar in Bookmarks mode.
    pub fn open_sidebar_bookmarks(&mut self, cx: &mut Context<Self>) {
        self.sidebar_mode = crate::views::sidebar::SidebarMode::Bookmarks;
        self.sidebar_collapsed = false;
        self.state.session.sidebar_mode = crate::views::sidebar::SidebarMode::Bookmarks;
        self.state.save_session_state();
        cx.notify();
    }

    /// Checks if a file path is in the bookmarks.
    pub fn is_file_bookmarked(&self, path: &Path) -> bool {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        self.state.bookmarks.contains_file(&str_val)
    }

    /// Checks if a folder path is in the bookmarks.
    pub fn is_folder_bookmarked(&self, path: &Path) -> bool {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        self.state.bookmarks.contains_folder(&str_val)
    }

    /// Toggles bookmark status for a file.
    pub fn toggle_file_bookmark(&mut self, path: &Path, cx: &mut Context<Self>) {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if self.state.bookmarks.contains_file(&str_val) {
            self.state.bookmarks.remove_file(&str_val);
            self.state.status_text = self.t_fmt("sidebar.file_unbookmarked", &[&name]);
        } else {
            self.state.bookmarks.add_file(str_val, None, None);
            self.state.status_text = self.t_fmt("sidebar.file_bookmarked", &[&name]);
        }
        self.save_bookmarks();
        cx.notify();
    }

    /// Adds a file to bookmarks.
    pub fn add_file_to_bookmarks(
        &mut self,
        path: &Path,
        group_title: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.state.bookmarks.add_file(str_val, None, group_title);
        self.state.status_text = self.t_fmt("sidebar.file_bookmarked", &[&name]);
        self.save_bookmarks();
        cx.notify();
    }

    /// Removes a file from bookmarks.
    pub fn remove_file_from_bookmarks(&mut self, path: &Path, cx: &mut Context<Self>) {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.state.bookmarks.remove_file(&str_val);
        self.state.status_text = self.t_fmt("sidebar.file_unbookmarked", &[&name]);
        self.save_bookmarks();
        cx.notify();
    }

    /// Adds a folder to bookmarks.
    pub fn add_folder_to_bookmarks(&mut self, path: &Path, cx: &mut Context<Self>) {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.state.bookmarks.add_folder(str_val, None);
        self.state.status_text = self.t_fmt("sidebar.folder_bookmarked", &[&name]);
        self.save_bookmarks();
        cx.notify();
    }

    /// Removes a folder from bookmarks.
    pub fn remove_folder_from_bookmarks(&mut self, path: &Path, cx: &mut Context<Self>) {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.state.bookmarks.remove_folder(&str_val);
        self.state.status_text = self.t_fmt("sidebar.folder_unbookmarked", &[&name]);
        self.save_bookmarks();
        cx.notify();
    }

    /// Saves bookmarks to `.vault/bookmarks.json` (if workspace is open) and session state.
    pub fn save_bookmarks(&mut self) {
        if let Some(ref ws) = self.state.workspace_path {
            let vault_dir = ws.join(".vault");
            let _ = std::fs::create_dir_all(&vault_dir);
            let path = vault_dir.join("bookmarks.json");
            if let Ok(json) = serde_json::to_string_pretty(&self.state.bookmarks) {
                let _ = std::fs::write(path, json);
            }
        }
        self.state.session.bookmarks = self.state.bookmarks.clone();
        self.state.save_session_state();
    }

    /// Loads bookmarks from `.vault/bookmarks.json` (or `.obsidian/bookmarks.json` as fallback) or session state.
    pub fn load_bookmarks(&mut self) {
        if let Some(ref ws) = self.state.workspace_path {
            let vault_bm = ws.join(".vault").join("bookmarks.json");
            let obs_bm = ws.join(".obsidian").join("bookmarks.json");
            let bm_path = if vault_bm.exists() {
                Some(vault_bm)
            } else if obs_bm.exists() {
                Some(obs_bm)
            } else {
                None
            };
            if let Some(path) = bm_path {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(data) = serde_json::from_str::<BookmarksData>(&content) {
                        self.state.bookmarks = data;
                        return;
                    }
                }
            }
        }
        if !self.state.session.bookmarks.is_empty() {
            self.state.bookmarks = self.state.session.bookmarks.clone();
        }
    }

    /// Reveals a folder in the file explorer sidebar.
    pub fn reveal_folder_in_sidebar(&mut self, folder_path: PathBuf, cx: &mut Context<Self>) {
        self.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
        self.sidebar_collapsed = false;
        self.selected_sidebar_folder = Some(folder_path.clone());
        self.state.expanded_folders.insert(folder_path.clone());
        self.state.reveal_path_in_tree(&folder_path);
        self.need_scroll_sidebar_to_active.set(true);
        cx.notify();
    }

    /// Executes moving a dragged favorite note into/out of a group or reordering.
    fn execute_bookmark_move(
        &mut self,
        source: BookmarkDragSource,
        target: BookmarkDropTarget,
        cx: &mut Context<Self>,
    ) {
        // 1. Extract item from source
        let item = match source.group_index {
            None => {
                if source.item_index < self.state.bookmarks.items.len() {
                    self.state.bookmarks.items.remove(source.item_index)
                } else {
                    return;
                }
            }
            Some(g_idx) => {
                if let Some(BookmarkItem::Group { ref mut items, .. }) =
                    self.state.bookmarks.items.get_mut(g_idx)
                {
                    if source.item_index < items.len() {
                        items.remove(source.item_index)
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
        };

        // 2. Insert item into target
        match target {
            BookmarkDropTarget::IntoGroup(target_g_idx) => {
                let actual_g_idx = if source.group_index.is_none() && source.item_index < target_g_idx {
                    target_g_idx.saturating_sub(1)
                } else {
                    target_g_idx
                };
                if let Some(BookmarkItem::Group { ref mut items, .. }) =
                    self.state.bookmarks.items.get_mut(actual_g_idx)
                {
                    items.push(item);
                }
            }
            BookmarkDropTarget::GroupItem(target_g_idx, target_item_idx) => {
                let actual_g_idx = if source.group_index.is_none() && source.item_index < target_g_idx {
                    target_g_idx.saturating_sub(1)
                } else {
                    target_g_idx
                };
                if let Some(BookmarkItem::Group { ref mut items, .. }) =
                    self.state.bookmarks.items.get_mut(actual_g_idx)
                {
                    let actual_item_idx = if source.group_index == Some(actual_g_idx)
                        && source.item_index < target_item_idx
                    {
                        target_item_idx.saturating_sub(1)
                    } else {
                        target_item_idx
                    };
                    items.insert(actual_item_idx.min(items.len()), item);
                }
            }
            BookmarkDropTarget::RootItem(target_idx) => {
                let actual_idx = if source.group_index.is_none() && source.item_index < target_idx {
                    target_idx.saturating_sub(1)
                } else {
                    target_idx
                };
                self.state
                    .bookmarks
                    .items
                    .insert(actual_idx.min(self.state.bookmarks.items.len()), item);
            }
            BookmarkDropTarget::RootEnd => {
                self.state.bookmarks.items.push(item);
            }
        }

        self.save_bookmarks();
        cx.notify();
    }

    /// Renders the inline rename input box with purple border matching Image 2.
    pub fn render_bookmark_rename_input(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(ref rename) = self.bookmark_rename else {
            return div().into_any_element();
        };

        let pixel_scale = window.scale_factor();
        let input_div = rename
            .input
            .render("", theme, window, pixel_scale, 12.0, "Segoe UI");

        div()
            .id("bookmark-inline-rename-box")
            .track_focus(&self.bookmark_rename_focus_handle)
            .flex()
            .flex_row()
            .items_center()
            .flex_1()
            .min_w_0()
            .h(px(24.0))
            .px_2()
            .rounded_md()
            .border_2()
            .border_color(rgb(0x8b5cf6)) // Rounded purple border matching Image 2
            .bg(theme.bg_card)
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.bookmark_rename_focus_handle);
                    if let Some(ref mut r) = this.bookmark_rename {
                        r.input.handle_mouse_down(event, window, 12.0, "Segoe UI");
                        r.input
                            .pause_and_show(|t: &mut AppView| &mut t.bookmark_rename_blink, cx);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if let Some(ref mut r) = this.bookmark_rename {
                    if r.input.is_mouse_selecting {
                        r.input.handle_mouse_move(event, window, 12.0, "Segoe UI");
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if let Some(ref mut r) = this.bookmark_rename {
                        r.input.handle_mouse_up();
                        cx.notify();
                    }
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                if key == "enter" {
                    this.commit_bookmark_rename(cx);
                    window.focus(&this.focus_handle);
                    return;
                }
                if key == "escape" {
                    this.cancel_bookmark_rename(cx);
                    window.focus(&this.focus_handle);
                    return;
                }
                if let Some(ref mut r) = this.bookmark_rename {
                    if r.input.handle_key_down(event, cx) {
                        r.input
                            .pause_and_show(|t: &mut AppView| &mut t.bookmark_rename_blink, cx);
                    }
                }
            }))
            .child(input_div)
            .into_any_element()
    }

    /// Renders the Obsidian-style Bookmarks (Favorites) sidebar tree view.
    pub fn render_sidebar_bookmarks(
        &self,
        active_path: Option<&PathBuf>,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let query = self.state.search_query.trim().to_lowercase();
        let is_filtering = !query.is_empty();

        let mut scroll_container = div()
            .id("sidebar-bookmarks-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py_1()
            .px_1()
            .gap_0p5()
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let x: f32 = event.position.x.into();
                let y: f32 = event.position.y.into();
                this.open_bookmark_context_menu(BookmarkContextTarget::EmptyArea, x, y, cx);
            }));

        if self.state.bookmarks.is_empty() {
            let empty_text = self.t("sidebar.no_bookmarks");
            let display_text = if empty_text.is_empty() || empty_text == "sidebar.no_bookmarks" {
                "Нет закладок"
            } else {
                &empty_text
            };

            return div()
                .id("sidebar-bookmarks-empty")
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .flex_1()
                .p_4()
                .gap_2()
                .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.open_bookmark_context_menu(BookmarkContextTarget::EmptyArea, x, y, cx);
                }))
                .child(icons::icon(icons::ICON_BOOKMARK, 24.0, theme.text_muted))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(display_text.to_string()),
                )
                .into_any_element();
        }

        let is_dragging_active = self.bookmark_drag.as_ref().map(|d| d.is_dragging).unwrap_or(false);

        for (item_idx, item) in self.state.bookmarks.items.iter().enumerate() {
            match item {
                BookmarkItem::File { path, title, .. } => {
                    let display_name = title.clone().unwrap_or_else(|| {
                        let filename = Path::new(path)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone());
                        filename.strip_suffix(".md").unwrap_or(&filename).to_string()
                    });

                    if is_filtering && !display_name.to_lowercase().contains(&query) {
                        continue;
                    }

                    scroll_container = scroll_container.child(self.render_bookmark_file_row(
                        path,
                        &display_name,
                        title.clone(),
                        None,
                        item_idx,
                        active_path,
                        is_dragging_active,
                        theme,
                        window,
                        cx,
                    ));
                }
                BookmarkItem::Folder { path, title } => {
                    let display_name = title.clone().unwrap_or_else(|| {
                        Path::new(path)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone())
                    });

                    if is_filtering && !display_name.to_lowercase().contains(&query) {
                        continue;
                    }

                    scroll_container = scroll_container.child(self.render_bookmark_folder_row(
                        path,
                        &display_name,
                        title.clone(),
                        item_idx,
                        theme,
                        window,
                        cx,
                    ));
                }
                BookmarkItem::Group { title, items } => {
                    let group_matches = is_filtering && title.to_lowercase().contains(&query);
                    let matching_children: Vec<(usize, &BookmarkItem)> = items
                        .iter()
                        .enumerate()
                        .filter(|(_, child)| match child {
                            BookmarkItem::File { path, title, .. } => {
                                let name = title.clone().unwrap_or_else(|| {
                                    let filename = Path::new(path)
                                        .file_name()
                                        .map(|s| s.to_string_lossy().to_string())
                                        .unwrap_or_else(|| path.clone());
                                    filename.strip_suffix(".md").unwrap_or(&filename).to_string()
                                });
                                !is_filtering || group_matches || name.to_lowercase().contains(&query)
                            }
                            _ => !is_filtering || group_matches,
                        })
                        .collect();

                    if is_filtering && !group_matches && matching_children.is_empty() {
                        continue;
                    }

                    let is_expanded = (is_filtering && !matching_children.is_empty())
                        || self.expanded_bookmark_groups.contains(title);

                    scroll_container = scroll_container.child(self.render_bookmark_group_row(
                        title,
                        item_idx,
                        is_expanded,
                        &matching_children,
                        active_path,
                        is_dragging_active,
                        theme,
                        window,
                        cx,
                    ));
                }
                _ => {}
            }
        }

        // Empty area at bottom acting as drop target to move notes out of groups to root level
        if is_dragging_active {
            let is_hovered = matches!(
                self.bookmark_drag.as_ref().and_then(|d| d.target.as_ref()),
                Some(BookmarkDropTarget::RootEnd)
            );

            scroll_container = scroll_container.child(
                div()
                    .id("bookmark-root-drop-zone")
                    .h(px(40.0))
                    .w_full()
                    .rounded_md()
                    .border_1()
                    .when(is_hovered, |d| {
                        d.border_color(theme.accent).bg(theme.bg_hover)
                    })
                    .when(!is_hovered, |d| {
                        d.border_color(rgba(0x00000000))
                    })
                    .on_mouse_move(cx.listener(|this, _, _, cx| {
                        if let Some(ref mut drag) = this.bookmark_drag {
                            if drag.is_dragging {
                                drag.target = Some(BookmarkDropTarget::RootEnd);
                                cx.notify();
                            }
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        if let Some(drag) = this.bookmark_drag.take() {
                            if drag.is_dragging {
                                this.execute_bookmark_move(drag.source, BookmarkDropTarget::RootEnd, cx);
                            }
                        }
                    })),
            );
        }

        scroll_container.into_any_element()
    }

    /// Renders a single favorite note row.
    #[allow(clippy::too_many_arguments)]
    fn render_bookmark_file_row(
        &self,
        path: &str,
        display_name: &str,
        custom_title: Option<String>,
        group_index: Option<usize>,
        item_index: usize,
        active_path: Option<&PathBuf>,
        _is_dragging_active: bool,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_menu_open = self.bookmark_context_menu.is_some();
        let abs_path = bookmark_str_to_path(self.state.workspace_path.as_deref(), path);
        let is_selected = active_path.map(|p| p == &abs_path).unwrap_or(false);

        let is_being_dragged = self.bookmark_drag.as_ref().map(|d| {
            d.source.group_index == group_index && d.source.item_index == item_index
        }).unwrap_or(false);

        let is_drop_target = match self.bookmark_drag.as_ref().and_then(|d| d.target.as_ref()) {
            Some(BookmarkDropTarget::RootItem(idx)) => group_index.is_none() && *idx == item_index,
            Some(BookmarkDropTarget::GroupItem(g_idx, idx)) => {
                group_index == Some(*g_idx) && *idx == item_index
            }
            _ => false,
        };

        let is_editing = self
            .bookmark_rename
            .as_ref()
            .map(|r| {
                r.target
                    == BookmarkRenameTarget::File {
                        group_index,
                        item_index,
                    }
            })
            .unwrap_or(false);

        let file_svg = icons::icon(
            icons::ICON_FILE_TEXT,
            13.0,
            if is_selected { theme.accent } else { theme.text_muted },
        );

        let path_owned = path.to_string();
        let name_owned = display_name.to_string();
        let title_owned = custom_title;
        let abs_path_click = abs_path.clone();

        let mut row = div()
            .id(SharedString::from(format!("bm-file-{}-{}", group_index.unwrap_or(999), item_index)))
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .h(px(BOOKMARK_ROW_H))
            .flex_shrink_0()
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .when(is_selected, |s| {
                s.bg(theme.bg_selected)
                    .text_color(theme.accent)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
            })
            .when(!is_selected && !is_drop_target && !is_menu_open, |s| {
                s.hover(|h| h.bg(theme.bg_hover)).text_color(theme.text_primary)
            })
            .when(is_being_dragged, |s| {
                s.opacity(0.5)
            })
            .when(is_drop_target, |s| {
                s.border_t_2().border_color(theme.accent).bg(theme.bg_hover)
            })
            .child(file_svg);

        if is_editing {
            row = row.child(self.render_bookmark_rename_input(theme, window, cx));
        } else {
            row = row
                .child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(if is_selected { theme.accent } else { theme.text_primary })
                        .child(display_name.to_string()),
                )
                // Left mouse down initiates drag tracking
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if this.bookmark_context_menu.is_some() {
                        this.close_bookmark_context_menu(cx);
                        return;
                    }
                    if this.bookmark_rename.is_some() {
                        this.commit_bookmark_rename(cx);
                    }
                    this.bookmark_drag = Some(BookmarkDragState {
                        source: BookmarkDragSource {
                            group_index,
                            item_index,
                            path: path_owned.clone(),
                            display_name: name_owned.clone(),
                        },
                        start_y: event.position.y.into(),
                        is_dragging: false,
                        target: None,
                    });
                    cx.notify();
                }))
                // Mouse move updates drag threshold and hover drop target
                .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                    if this.bookmark_context_menu.is_some() {
                        return;
                    }
                    if let Some(ref mut drag) = this.bookmark_drag {
                        let cur_y: f32 = event.position.y.into();
                        if (cur_y - drag.start_y).abs() > 4.0 {
                            drag.is_dragging = true;
                        }
                        if drag.is_dragging {
                            drag.target = Some(if let Some(g) = group_index {
                                BookmarkDropTarget::GroupItem(g, item_index)
                            } else {
                                BookmarkDropTarget::RootItem(item_index)
                            });
                            cx.notify();
                        }
                    }
                }))
                // Mouse up completes drag or opens note if was plain click
                .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                    if this.bookmark_context_menu.is_some() {
                        return;
                    }
                    if let Some(drag) = this.bookmark_drag.take() {
                        if drag.is_dragging {
                            if let Some(target) = drag.target {
                                this.execute_bookmark_move(drag.source, target, cx);
                            }
                        } else {
                            this.select_note(abs_path_click.clone(), false, window, cx);
                        }
                    }
                }))
                // Right click opens Obsidian-style context menu for this note
                .on_mouse_down(MouseButton::Right, cx.listener({
                    let path = path.to_string();
                    let title = title_owned;
                    move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.open_bookmark_context_menu(
                            BookmarkContextTarget::File {
                                path: path.clone(),
                                title: title.clone(),
                                group_index,
                                item_index,
                            },
                            x,
                            y,
                            cx,
                        );
                    }
                }));
        }

        row.into_any_element()
    }

    /// Renders a single favorite folder row.
    #[allow(clippy::too_many_arguments)]
    fn render_bookmark_folder_row(
        &self,
        path: &str,
        display_name: &str,
        custom_title: Option<String>,
        item_index: usize,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let abs_path = bookmark_str_to_path(self.state.workspace_path.as_deref(), path);
        let folder_svg = icons::icon(icons::ICON_FOLDER, 14.0, theme.accent);
        let abs_path_click = abs_path.clone();

        let is_menu_open = self.bookmark_context_menu.is_some();
        let is_editing = self
            .bookmark_rename
            .as_ref()
            .map(|r| r.target == BookmarkRenameTarget::Folder(item_index))
            .unwrap_or(false);

        let mut row = div()
            .id(SharedString::from(format!("bm-fld-{}", path)))
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .h(px(BOOKMARK_ROW_H))
            .flex_shrink_0()
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .when(!is_menu_open, |s| s.hover(|h| h.bg(theme.bg_hover)))
            .child(folder_svg);

        if is_editing {
            row = row.child(self.render_bookmark_rename_input(theme, window, cx));
        } else {
            let path_owned = path.to_string();
            let title_owned = custom_title;
            row = row
                .child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(theme.text_primary)
                        .child(display_name.to_string()),
                )
                // Clicking folder reveals it in the file explorer sidebar
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.bookmark_context_menu.is_some() {
                        this.close_bookmark_context_menu(cx);
                        return;
                    }
                    if this.bookmark_rename.is_some() {
                        this.commit_bookmark_rename(cx);
                    }
                    this.reveal_folder_in_sidebar(abs_path_click.clone(), cx);
                }))
                // Right click opens folder context menu
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.open_bookmark_context_menu(
                        BookmarkContextTarget::Folder {
                            path: path_owned.clone(),
                            title: title_owned.clone(),
                            item_index,
                        },
                        x,
                        y,
                        cx,
                    );
                }));
        }

        row.into_any_element()
    }

    /// Renders a bookmark group row and its children.
    #[allow(clippy::too_many_arguments)]
    fn render_bookmark_group_row(
        &self,
        title: &str,
        group_idx: usize,
        is_expanded: bool,
        matching_children: &[(usize, &BookmarkItem)],
        active_path: Option<&PathBuf>,
        is_dragging_active: bool,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_menu_open = self.bookmark_context_menu.is_some();
        let is_drop_target_group = matches!(
            self.bookmark_drag.as_ref().and_then(|d| d.target.as_ref()),
            Some(BookmarkDropTarget::IntoGroup(idx)) if idx == &group_idx
        );

        let chevron = if is_expanded {
            icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted)
        } else {
            icons::icon(icons::ICON_CHEVRON_RIGHT, 12.0, theme.text_muted)
        };

        let title_owned = title.to_string();
        let is_editing = self
            .bookmark_rename
            .as_ref()
            .map(|r| r.target == BookmarkRenameTarget::Group(group_idx))
            .unwrap_or(false);

        let mut header = div()
            .id(SharedString::from(format!("bm-group-hdr-{}", group_idx)))
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .h(px(BOOKMARK_ROW_H))
            .flex_shrink_0()
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .when(is_drop_target_group, |s| {
                s.border_1().border_color(theme.accent).bg(theme.bg_hover)
            })
            .when(!is_drop_target_group && !is_menu_open, |s| {
                s.hover(|h| h.bg(theme.bg_hover))
            })
            .child(
                div()
                    .id(SharedString::from(format!("bm-group-chevron-{}", group_idx)))
                    .cursor_pointer()
                    .p_0p5()
                    .child(chevron)
                    .on_click(cx.listener({
                        let title = title_owned.clone();
                        move |this, _, _, cx| {
                            if this.expanded_bookmark_groups.contains(&title) {
                                this.expanded_bookmark_groups.remove(&title);
                            } else {
                                this.expanded_bookmark_groups.insert(title.clone());
                            }
                            cx.notify();
                        }
                    })),
            );

        if is_editing {
            header = header.child(self.render_bookmark_rename_input(theme, window, cx));
        } else {
            header = header
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::NORMAL)
                        .truncate()
                        .text_color(theme.text_primary)
                        .child(title.to_string()),
                )
                .on_click(cx.listener({
                    let title = title_owned.clone();
                    move |this, _, _, cx| {
                        if this.bookmark_context_menu.is_some() {
                            this.close_bookmark_context_menu(cx);
                            return;
                        }
                        if this.bookmark_rename.is_some() {
                            this.commit_bookmark_rename(cx);
                        }
                        if this.expanded_bookmark_groups.contains(&title) {
                            this.expanded_bookmark_groups.remove(&title);
                        } else {
                            this.expanded_bookmark_groups.insert(title.clone());
                        }
                        cx.notify();
                    }
                }))
                .on_mouse_move(cx.listener(move |this, _, _, cx| {
                    if this.bookmark_context_menu.is_some() {
                        return;
                    }
                    if let Some(ref mut drag) = this.bookmark_drag {
                        if drag.is_dragging {
                            drag.target = Some(BookmarkDropTarget::IntoGroup(group_idx));
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                    if this.bookmark_context_menu.is_some() {
                        return;
                    }
                    if let Some(drag) = this.bookmark_drag.take() {
                        if drag.is_dragging {
                            this.execute_bookmark_move(
                                drag.source,
                                BookmarkDropTarget::IntoGroup(group_idx),
                                cx,
                            );
                        }
                    }
                }))
                .on_mouse_down(MouseButton::Right, cx.listener({
                    let title = title_owned.clone();
                    move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.open_bookmark_context_menu(
                            BookmarkContextTarget::Group {
                                group_index: group_idx,
                                title: title.clone(),
                            },
                            x,
                            y,
                            cx,
                        );
                    }
                }));
        }

        let mut group_elem = div()
            .id(SharedString::from(format!("bm-group-{}", group_idx)))
            .flex()
            .flex_col()
            .child(header);

        // If expanded, render children with tree guide line
        if is_expanded {
            let mut children_container = div()
                .id(SharedString::from(format!("bm-group-children-{}", group_idx)))
                .flex()
                .flex_col()
                .pl(px(14.0))
                .border_l_1()
                .border_color(theme.border_subtle)
                .ml(px(8.0))
                .my_0p5();

            for (child_idx, child_item) in matching_children {
                if let BookmarkItem::File { path, title, .. } = child_item {
                    let display_name = title.clone().unwrap_or_else(|| {
                        let filename = Path::new(path)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone());
                        filename.strip_suffix(".md").unwrap_or(&filename).to_string()
                    });

                    children_container = children_container.child(self.render_bookmark_file_row(
                        path,
                        &display_name,
                        title.clone(),
                        Some(group_idx),
                        *child_idx,
                        active_path,
                        is_dragging_active,
                        theme,
                        window,
                        cx,
                    ));
                }
            }

            group_elem = group_elem.child(children_container);
        }

        group_elem.into_any_element()
    }

    /// Renders the floating context menu for bookmarks matching Image 1.
    pub fn render_bookmark_context_menu(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(ref menu_state) = self.bookmark_context_menu else {
            return div().into_any_element();
        };

        let bounds = window.bounds();
        let win_w = f32::from(bounds.size.width);
        let win_h = f32::from(bounds.size.height);

        let menu_w = 260.0;
        let menu_h = 245.0;
        let clamped_x = menu_state.x.clamp(8.0, (win_w - menu_w - 8.0).max(8.0));
        let clamped_y = menu_state.y.clamp(8.0, (win_h - menu_h - 8.0).max(8.0));

        let target = menu_state.target.clone();

        let render_item = |id: &'static str,
                           icon_svg: &'static str,
                           label: String,
                           is_destructive: bool,
                           action: Box<dyn Fn(&mut AppView, &mut Window, &mut Context<AppView>)>| {
            let icon_color = if is_destructive {
                theme.text_secondary
            } else {
                theme.text_secondary
            };
            let text_color = if is_destructive {
                theme.text_primary
            } else {
                theme.text_primary
            };

            div()
                .id(SharedString::from(id))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .hover(|h| h.bg(theme.bg_hover))
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Right, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_move(|_, _, cx| {
                    cx.stop_propagation();
                })
                .child(icons::icon(icon_svg, 13.0, icon_color))
                .child(
                    div()
                        .text_xs()
                        .text_color(text_color)
                        .child(label),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    action(this, window, cx);
                }))
        };

        let render_divider = || {
            div()
                .h(px(1.0))
                .my_0p5()
                .mx_1()
                .bg(theme.border_subtle)
        };

        let target_open_tab = target.clone();
        let target_open_right = target.clone();
        let target_open_window = target.clone();
        let target_rename = target.clone();
        let target_bookmark = target.clone();
        let target_delete = target.clone();

        div()
            .id("bookmark-context-menu-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_move(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .id("bookmark-context-backdrop")
                    .absolute()
                    .inset_0()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_bookmark_context_menu(cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_bookmark_context_menu(cx);
                    }))
                    .on_mouse_move(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    }),
            )
            .child(
                div()
                    .id("bookmark-context-panel")
                    .absolute()
                    .left(px(clamped_x))
                    .top(px(clamped_y))
                    .w(px(menu_w))
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_lg()
                    .shadow_xl()
                    .p_1()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_mouse_move(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    // 1. Открыть в новой вкладке
                    .child(render_item(
                        "bm-item-open-new-tab",
                        icons::ICON_NOTE_CREATE,
                        self.t("sidebar.open_in_new_tab"),
                        false,
                        Box::new(move |this, window, cx| {
                            this.close_bookmark_context_menu(cx);
                            match &target_open_tab {
                                BookmarkContextTarget::File { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    this.select_note(abs_path, true, window, cx);
                                }
                                BookmarkContextTarget::Folder { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    let files = collect_files_in_folder(&abs_path);
                                    this.open_files_batch(&files, false, window, cx);
                                }
                                BookmarkContextTarget::Group { group_index, title } => {
                                    let files = if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.get(*group_index) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.iter().find(|i| matches!(i, BookmarkItem::Group { title: t, .. } if t == title)) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else {
                                        Vec::new()
                                    };
                                    this.open_files_batch(&files, false, window, cx);
                                }
                                BookmarkContextTarget::EmptyArea => {
                                    if let Some((_, doc)) = this.state.active_view_and_doc() {
                                        let p = doc.path.clone();
                                        this.select_note(p, true, window, cx);
                                    }
                                }
                            }
                        }),
                    ))
                    // 2. Открыть в области справа
                    .child(render_item(
                        "bm-item-open-right",
                        icons::ICON_SPLIT,
                        self.t("sidebar.open_in_right"),
                        false,
                        Box::new(move |this, window, cx| {
                            this.close_bookmark_context_menu(cx);
                            match &target_open_right {
                                BookmarkContextTarget::File { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    this.split_tab_right(&abs_path, window, cx);
                                }
                                BookmarkContextTarget::Folder { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    let files = collect_files_in_folder(&abs_path);
                                    this.open_files_batch(&files, true, window, cx);
                                }
                                BookmarkContextTarget::Group { group_index, title } => {
                                    let files = if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.get(*group_index) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.iter().find(|i| matches!(i, BookmarkItem::Group { title: t, .. } if t == title)) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else {
                                        Vec::new()
                                    };
                                    this.open_files_batch(&files, true, window, cx);
                                }
                                BookmarkContextTarget::EmptyArea => {
                                    if let Some((_, doc)) = this.state.active_view_and_doc() {
                                        let p = doc.path.clone();
                                        this.split_tab_right(&p, window, cx);
                                    }
                                }
                            }
                        }),
                    ))
                    // 3. Открыть в новом окне
                    .child(render_item(
                        "bm-item-open-new-window",
                        icons::ICON_APP_WINDOW,
                        self.t("sidebar.open_in_new_window"),
                        false,
                        Box::new(move |this, window, cx| {
                            this.close_bookmark_context_menu(cx);
                            match &target_open_window {
                                BookmarkContextTarget::File { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    this.select_note(abs_path, true, window, cx);
                                }
                                BookmarkContextTarget::Folder { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    let files = collect_files_in_folder(&abs_path);
                                    this.open_files_batch(&files, false, window, cx);
                                }
                                BookmarkContextTarget::Group { group_index, title } => {
                                    let files = if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.get(*group_index) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.iter().find(|i| matches!(i, BookmarkItem::Group { title: t, .. } if t == title)) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else {
                                        Vec::new()
                                    };
                                    this.open_files_batch(&files, false, window, cx);
                                }
                                BookmarkContextTarget::EmptyArea => {
                                    if let Some((_, doc)) = this.state.active_view_and_doc() {
                                        let p = doc.path.clone();
                                        this.select_note(p, true, window, cx);
                                    }
                                }
                            }
                        }),
                    ))
                    // Divider
                    .child(render_divider())
                    // 4. Переименовать
                    .child(render_item(
                        "bm-item-rename",
                        icons::ICON_PENCIL,
                        self.t("sidebar.rename"),
                        false,
                        Box::new(move |this, window, cx| {
                            this.close_bookmark_context_menu(cx);
                            match &target_rename {
                                BookmarkContextTarget::Group { group_index, title } => {
                                    this.start_bookmark_rename(
                                        BookmarkRenameTarget::Group(*group_index),
                                        title,
                                        window,
                                        cx,
                                    );
                                }
                                BookmarkContextTarget::File {
                                    path,
                                    title,
                                    group_index,
                                    item_index,
                                } => {
                                    let cur_name = title.clone().unwrap_or_else(|| {
                                        let filename = Path::new(path)
                                            .file_name()
                                            .map(|s| s.to_string_lossy().to_string())
                                            .unwrap_or_else(|| path.clone());
                                        filename.strip_suffix(".md").unwrap_or(&filename).to_string()
                                    });
                                    this.start_bookmark_rename(
                                        BookmarkRenameTarget::File {
                                            group_index: *group_index,
                                            item_index: *item_index,
                                        },
                                        &cur_name,
                                        window,
                                        cx,
                                    );
                                }
                                BookmarkContextTarget::Folder {
                                    path,
                                    title,
                                    item_index,
                                } => {
                                    let cur_name = title.clone().unwrap_or_else(|| {
                                        Path::new(path)
                                            .file_name()
                                            .map(|s| s.to_string_lossy().to_string())
                                            .unwrap_or_else(|| path.clone())
                                    });
                                    this.start_bookmark_rename(
                                        BookmarkRenameTarget::Folder(*item_index),
                                        &cur_name,
                                        window,
                                        cx,
                                    );
                                }
                                BookmarkContextTarget::EmptyArea => {}
                            }
                        }),
                    ))
                    // 5. Добавить в закладки текущую вкладку...
                    .child(render_item(
                        "bm-item-bookmark-current",
                        icons::ICON_BOOKMARK,
                        self.t("sidebar.bookmark_current_tab"),
                        false,
                        Box::new(move |this, _, cx| {
                            this.close_bookmark_context_menu(cx);
                            let target_group = if let BookmarkContextTarget::Group { ref title, .. } = target_bookmark {
                                Some(title.clone())
                            } else {
                                None
                            };
                            if let Some((_, doc)) = this.state.active_view_and_doc() {
                                let p = doc.path.clone();
                                this.add_file_to_bookmarks(&p, target_group.as_deref(), cx);
                            }
                        }),
                    ))
                    // 6. Новая группа закладок
                    .child(render_item(
                        "bm-item-new-group",
                        icons::ICON_FOLDER_PLUS,
                        self.t("sidebar.new_bookmark_group"),
                        false,
                        Box::new(move |this, window, cx| {
                            this.close_bookmark_context_menu(cx);
                            this.create_bookmark_group(window, cx);
                        }),
                    ))
                    // Divider
                    .child(render_divider())
                    // 7. Удалить
                    .child(render_item(
                        "bm-item-delete",
                        icons::ICON_TRASH_2,
                        self.t("sidebar.delete"),
                        true,
                        Box::new(move |this, _, cx| {
                            this.close_bookmark_context_menu(cx);
                            match &target_delete {
                                BookmarkContextTarget::File { path, .. } => {
                                    this.state.bookmarks.remove_file(path);
                                    this.save_bookmarks();
                                    cx.notify();
                                }
                                BookmarkContextTarget::Folder { path, .. } => {
                                    this.state.bookmarks.remove_folder(path);
                                    this.save_bookmarks();
                                    cx.notify();
                                }
                                BookmarkContextTarget::Group { group_index, .. } => {
                                    this.state.bookmarks.delete_group(*group_index);
                                    this.save_bookmarks();
                                    cx.notify();
                                }
                                BookmarkContextTarget::EmptyArea => {}
                            }
                        }),
                    )),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_obsidian_bookmarks_json_compatibility() {
        let json = r#"{
            "items": [
                { "type": "file", "path": "Дневник.md" },
                { "type": "file", "path": "Минифакью.md" },
                { "type": "folder", "path": "АСТРАЛИНУКС" },
                { "type": "folder", "path": "Продукты" },
                {
                    "type": "group",
                    "title": "Группа без названия",
                    "items": [
                        { "type": "file", "path": "Подключение к стенду.md" }
                    ]
                }
            ]
        }"#;

        let parsed: BookmarksData = serde_json::from_str(json).expect("Must parse Obsidian bookmarks json");
        assert_eq!(parsed.items.len(), 5);
        assert!(parsed.contains_file("Дневник.md"));
        assert!(parsed.contains_file("Минифакью.md"));
        assert!(parsed.contains_file("Подключение к стенду.md"));
        assert!(parsed.contains_folder("АСТРАЛИНУКС"));
        assert!(parsed.contains_folder("Продукты"));
        assert!(!parsed.contains_file("Несуществующая заметка.md"));

        // Round-trip serialize
        let serialized = serde_json::to_string_pretty(&parsed).expect("Must serialize");
        let round_trip: BookmarksData = serde_json::from_str(&serialized).expect("Must re-parse");
        assert_eq!(parsed, round_trip);
    }

    #[test]
    fn test_bookmarks_add_and_remove() {
        let mut data = BookmarksData::default();
        assert!(data.is_empty());

        data.add_file("NoteA.md".into(), None, None);
        assert!(data.contains_file("NoteA.md"));
        assert_eq!(data.items.len(), 1);

        // Avoid duplicate add
        data.add_file("NoteA.md".into(), None, None);
        assert_eq!(data.items.len(), 1);

        data.add_folder("FolderA".into(), None);
        assert!(data.contains_folder("FolderA"));
        assert_eq!(data.items.len(), 2);

        // Group creation
        let group_title = data.create_group("Группа без названия");
        assert_eq!(group_title, "Группа без названия");
        assert_eq!(data.items.len(), 3);

        // Adding into group
        data.add_file("NoteInGroup.md".into(), None, Some("Группа без названия"));
        assert!(data.contains_file("NoteInGroup.md"));

        // Remove file from root
        let removed = data.remove_file("NoteA.md");
        assert!(removed);
        assert!(!data.contains_file("NoteA.md"));

        // Remove file from group
        let removed_group_item = data.remove_file("NoteInGroup.md");
        assert!(removed_group_item);
        assert!(!data.contains_file("NoteInGroup.md"));

        // Remove folder
        let removed_folder = data.remove_folder("FolderA");
        assert!(removed_folder);
        assert!(!data.contains_folder("FolderA"));
    }

    #[test]
    fn test_path_normalization() {
        let mut data = BookmarksData::default();
        data.add_file("sub\\path\\to\\Note.md".into(), None, None);

        assert!(data.contains_file("sub/path/to/Note.md"));
        assert!(data.contains_file("sub\\path\\to\\Note.md"));

        assert!(data.remove_file("sub/path/to/Note.md"));
        assert!(!data.contains_file("sub\\path\\to\\Note.md"));
    }

    #[test]
    fn test_drag_and_drop_reorder_and_move() {
        let mut data = BookmarksData::default();
        data.add_file("Note1.md".into(), None, None);
        data.add_file("Note2.md".into(), None, None);
        data.create_group("GroupA");
        data.create_group("GroupB");

        // Move Note1 from root (index 0) into GroupA (group index 2 in items)
        let item = data.items.remove(0);
        if let BookmarkItem::Group { ref mut items, .. } = data.items[1] {
            items.push(item);
        }
        assert_eq!(data.items.len(), 3);
        if let BookmarkItem::Group { ref items, .. } = data.items[1] {
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0],
                BookmarkItem::File {
                    path: "Note1.md".into(),
                    title: None,
                    subpath: None
                }
            );
        }

        // Move Note1 from GroupA to GroupB
        let item = if let BookmarkItem::Group { ref mut items, .. } = data.items[1] {
            items.remove(0)
        } else {
            panic!("Expected GroupA");
        };
        if let BookmarkItem::Group { ref mut items, .. } = data.items[2] {
            items.push(item);
        }
        if let BookmarkItem::Group { ref items, .. } = data.items[2] {
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0],
                BookmarkItem::File {
                    path: "Note1.md".into(),
                    title: None,
                    subpath: None
                }
            );
        }

        // Move Note1 from GroupB back to Root
        let item = if let BookmarkItem::Group { ref mut items, .. } = data.items[2] {
            items.remove(0)
        } else {
            panic!("Expected GroupB");
        };
        data.items.push(item);
        assert_eq!(data.items.len(), 4);
        assert_eq!(
            data.items.last().unwrap(),
            &BookmarkItem::File {
                path: "Note1.md".into(),
                title: None,
                subpath: None
            }
        );
    }

    #[test]
    fn test_rename_group_and_items() {
        let mut data = BookmarksData::default();
        data.create_group("OriginalGroup");
        data.add_file("Note1.md".into(), None, Some("OriginalGroup"));
        data.add_folder("Folder1".into(), None);

        // Rename group
        data.rename_group(0, "RenamedGroup".into());
        if let BookmarkItem::Group { ref title, .. } = data.items[0] {
            assert_eq!(title, "RenamedGroup");
        } else {
            panic!("Expected group");
        }

        // Rename file in group
        data.rename_file_bookmark(Some(0), 0, "CustomNoteTitle".into());
        if let BookmarkItem::Group { ref items, .. } = data.items[0] {
            if let BookmarkItem::File { ref title, .. } = items[0] {
                assert_eq!(title.as_deref(), Some("CustomNoteTitle"));
            } else {
                panic!("Expected file");
            }
        }

        // Rename folder
        data.rename_folder_bookmark(1, "CustomFolderTitle".into());
        if let BookmarkItem::Folder { ref title, .. } = data.items[1] {
            assert_eq!(title.as_deref(), Some("CustomFolderTitle"));
        } else {
            panic!("Expected folder");
        }
    }

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("vibecodium_bm_test_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_collect_files_in_folder_and_bookmark_group() {
        let temp = TempDir::new();
        let root = temp.path();

        let sub_folder = root.join("SubFolder");
        std::fs::create_dir_all(&sub_folder).unwrap();

        let file1 = root.join("Note1.md");
        let file2 = sub_folder.join("Note2.md");
        let ignored_file = root.join(".hidden.md");
        let git_dir = root.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let git_file = git_dir.join("git_note.md");

        std::fs::write(&file1, "Hello 1").unwrap();
        std::fs::write(&file2, "Hello 2").unwrap();
        std::fs::write(&ignored_file, "Hidden").unwrap();
        std::fs::write(&git_file, "Git").unwrap();

        // Test collect_files_in_folder
        let collected = collect_files_in_folder(root);
        assert_eq!(collected.len(), 2);
        assert!(collected.contains(&file1));
        assert!(collected.contains(&file2));
        assert!(!collected.contains(&ignored_file));
        assert!(!collected.contains(&git_file));

        // Test collect_files_in_bookmark_group
        let group_items = vec![
            BookmarkItem::File {
                path: "Note1.md".to_string(),
                title: None,
                subpath: None,
            },
            BookmarkItem::Folder {
                path: "SubFolder".to_string(),
                title: None,
            },
        ];

        let group_files = collect_files_in_bookmark_group(Some(root), &group_items);
        assert_eq!(group_files.len(), 2);
        assert!(group_files.contains(&file1));
        assert!(group_files.contains(&file2));
    }

    #[test]
    fn test_vault_bookmarks_json_persistence() {
        let temp = TempDir::new();
        let root = temp.path();

        let vault_dir = root.join(".vault");
        std::fs::create_dir_all(&vault_dir).unwrap();
        let bm_path = vault_dir.join("bookmarks.json");

        let mut data = BookmarksData::default();
        data.add_file("TestNote.md".to_string(), Some("My Note".to_string()), None);
        data.add_folder("Work".to_string(), None);

        let json = serde_json::to_string_pretty(&data).unwrap();
        std::fs::write(&bm_path, json).unwrap();

        assert!(bm_path.exists());
        let read_back = std::fs::read_to_string(&bm_path).unwrap();
        let loaded: BookmarksData = serde_json::from_str(&read_back).unwrap();

        assert_eq!(data, loaded);
        assert!(loaded.contains_file("TestNote.md"));
        assert!(loaded.contains_folder("Work"));
    }
}
