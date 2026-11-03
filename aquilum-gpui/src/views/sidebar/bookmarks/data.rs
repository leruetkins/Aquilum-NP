use std::path::{Path, PathBuf};
use gpui::FocusHandle;
use serde::{Deserialize, Serialize};
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
