use aquilum_core::app_core::{Core, CoreEvent, EventSink};
use aquilum_core::files::models::FileItem;
use aquilum_core::files::workspace::read_directory_impl;
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum FileNode {
    Folder {
        name: String,
        path: PathBuf,
        children: Vec<FileNode>,
    },
    File {
        name: String,
        path: PathBuf,
    },
}

impl FileNode {
    pub fn name(&self) -> &str {
        match self {
            Self::Folder { name, .. } => name,
            Self::File { name, .. } => name,
        }
    }
}

pub struct OpenedNote {
    pub path: PathBuf,
    pub title: String,
    pub content: String,
    #[allow(dead_code)]
    pub version: u64,
    pub cursor_offset: usize,
    pub selection_anchor: Option<usize>,
    pub is_dirty: bool,
    pub undo_stack: Vec<(String, usize)>,
    pub redo_stack: Vec<(String, usize)>,
}

impl OpenedNote {
    pub fn new(path: PathBuf, title: String, content: String, version: u64) -> Self {
        Self {
            path,
            title,
            content,
            version,
            cursor_offset: 0,
            selection_anchor: None,
            is_dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn snapshot_undo(&mut self) {
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push((self.content.clone(), self.cursor_offset));
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some((prev_content, prev_cursor)) = self.undo_stack.pop() {
            self.redo_stack.push((self.content.clone(), self.cursor_offset));
            self.content = prev_content;
            self.cursor_offset = prev_cursor.min(self.content.len());
            self.selection_anchor = None;
            self.is_dirty = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some((next_content, next_cursor)) = self.redo_stack.pop() {
            self.undo_stack.push((self.content.clone(), self.cursor_offset));
            self.content = next_content;
            self.cursor_offset = next_cursor.min(self.content.len());
            self.selection_anchor = None;
            self.is_dirty = true;
        }
    }

    pub fn selection_range(&self) -> Option<Range<usize>> {
        if let Some(anchor) = self.selection_anchor {
            if anchor != self.cursor_offset {
                let start = anchor.min(self.cursor_offset).min(self.content.len());
                let end = anchor.max(self.cursor_offset).min(self.content.len());
                return Some(start..end);
            }
        }
        None
    }

    pub fn selected_text(&self) -> Option<String> {
        let range = self.selection_range()?;
        Some(self.content[range].to_string())
    }

    pub fn delete_selection(&mut self) -> bool {
        if let Some(range) = self.selection_range() {
            self.snapshot_undo();
            self.content.replace_range(range.clone(), "");
            self.cursor_offset = range.start;
            self.selection_anchor = None;
            self.is_dirty = true;
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor_offset = self.content.len();
    }

    /// Возвращает (line_index, col_index) 0-indexed
    pub fn line_col(&self) -> (usize, usize) {
        let offset = self.cursor_offset.min(self.content.len());
        let prefix = &self.content[..offset];
        let line_idx = prefix.matches('\n').count();
        let last_newline = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let col_idx = prefix[last_newline..].chars().count();
        (line_idx, col_idx)
    }

    pub fn offset_for_line_col(&self, target_line: usize, target_col: usize) -> usize {
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                let char_count = line.chars().count();
                let actual_col = target_col.min(char_count);
                let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
                return offset + byte_col;
            }
            offset += line.len() + 1;
        }
        self.content.len()
    }

    pub fn insert_text(&mut self, text: &str) {
        self.snapshot_undo();
        let _ = self.delete_selection();
        let offset = self.cursor_offset.min(self.content.len());
        self.content.insert_str(offset, text);
        self.cursor_offset = offset + text.len();
        self.selection_anchor = None;
        self.is_dirty = true;
    }

    pub fn insert_newline_auto_indent(&mut self) {
        self.snapshot_undo();
        let _ = self.delete_selection();

        let offset = self.cursor_offset.min(self.content.len());
        let prefix = &self.content[..offset];
        let last_line = prefix.split('\n').last().unwrap_or("");
        let indent_len = last_line.chars().take_while(|c| c.is_whitespace()).count();
        let indent: String = last_line.chars().take(indent_len).collect();

        let insert_str = format!("\n{}", indent);
        self.content.insert_str(offset, &insert_str);
        self.cursor_offset = offset + insert_str.len();
        self.selection_anchor = None;
        self.is_dirty = true;
    }

    pub fn delete_backwards(&mut self) {
        if self.delete_selection() {
            return;
        }
        if self.cursor_offset == 0 || self.content.is_empty() {
            return;
        }
        self.snapshot_undo();
        let offset = self.cursor_offset.min(self.content.len());
        let mut prev = offset - 1;
        while prev > 0 && !self.content.is_char_boundary(prev) {
            prev -= 1;
        }
        self.content.remove(prev);
        self.cursor_offset = prev;
        self.selection_anchor = None;
        self.is_dirty = true;
    }

    pub fn delete_forwards(&mut self) {
        if self.delete_selection() {
            return;
        }
        let offset = self.cursor_offset.min(self.content.len());
        if offset >= self.content.len() {
            return;
        }
        self.snapshot_undo();
        self.content.remove(offset);
        self.selection_anchor = None;
        self.is_dirty = true;
    }

    pub fn move_cursor_left(&mut self, shift: bool) {
        if !shift && self.selection_anchor.is_some() {
            if let Some(range) = self.selection_range() {
                self.cursor_offset = range.start;
                self.selection_anchor = None;
                return;
            }
        }
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        if self.cursor_offset == 0 {
            return;
        }
        let mut prev = self.cursor_offset - 1;
        while prev > 0 && !self.content.is_char_boundary(prev) {
            prev -= 1;
        }
        self.cursor_offset = prev;
        if !shift {
            self.selection_anchor = None;
        }
    }

    pub fn move_cursor_right(&mut self, shift: bool) {
        if !shift && self.selection_anchor.is_some() {
            if let Some(range) = self.selection_range() {
                self.cursor_offset = range.end;
                self.selection_anchor = None;
                return;
            }
        }
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        if self.cursor_offset >= self.content.len() {
            return;
        }
        let mut next = self.cursor_offset + 1;
        while next <= self.content.len() && !self.content.is_char_boundary(next) {
            next += 1;
        }
        if next <= self.content.len() {
            self.cursor_offset = next;
        }
        if !shift {
            self.selection_anchor = None;
        }
    }

    pub fn move_cursor_up(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, col_idx) = self.line_col();
        if line_idx == 0 {
            self.cursor_offset = 0;
            if !shift {
                self.selection_anchor = None;
            }
            return;
        }

        let lines: Vec<&str> = self.content.split('\n').collect();
        let target_line = line_idx - 1;
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                let char_count = line.chars().count();
                let actual_col = col_idx.min(char_count);
                let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
                self.cursor_offset = offset + byte_col;
                if !shift {
                    self.selection_anchor = None;
                }
                return;
            }
            offset += line.len() + 1;
        }
    }

    pub fn move_cursor_down(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, col_idx) = self.line_col();
        let lines: Vec<&str> = self.content.split('\n').collect();
        if line_idx + 1 >= lines.len() {
            self.cursor_offset = self.content.len();
            if !shift {
                self.selection_anchor = None;
            }
            return;
        }

        let target_line = line_idx + 1;
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                let char_count = line.chars().count();
                let actual_col = col_idx.min(char_count);
                let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
                self.cursor_offset = offset + byte_col;
                if !shift {
                    self.selection_anchor = None;
                }
                return;
            }
            offset += line.len() + 1;
        }
    }

    pub fn set_cursor_to_line_start(&mut self, target_line: usize) {
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                self.cursor_offset = offset;
                self.selection_anchor = None;
                return;
            }
            offset += line.len() + 1;
        }
    }

    pub fn set_cursor_to_line_end(&mut self, target_line: usize) {
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                self.cursor_offset = offset + line.len();
                self.selection_anchor = None;
                return;
            }
            offset += line.len() + 1;
        }
    }

    #[allow(dead_code)]
    pub fn set_cursor_to_line_col(&mut self, target_line: usize, target_col: usize) {
        self.cursor_offset = self.offset_for_line_col(target_line, target_col);
        self.selection_anchor = None;
    }

    pub fn home(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, _) = self.line_col();
        self.set_cursor_to_line_start(line_idx);
        if !shift {
            self.selection_anchor = None;
        }
    }

    pub fn end(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, _) = self.line_col();
        self.set_cursor_to_line_end(line_idx);
        if !shift {
            self.selection_anchor = None;
        }
    }
}

pub struct AppState {
    pub core: Arc<Core>,
    pub workspace_path: Option<PathBuf>,
    pub workspace_name: String,
    pub file_tree: Vec<FileNode>,
    pub expanded_folders: HashSet<PathBuf>,
    pub search_query: String,
    pub files: Vec<FileItem>,
    pub active_note: Option<OpenedNote>,
    pub open_tabs: Vec<PathBuf>,
    pub nav_history: Vec<PathBuf>,
    pub nav_history_index: usize,
    pub status_text: String,
}

struct CoreEventSink;

impl EventSink for CoreEventSink {
    fn emit(&self, event: CoreEvent) {
        println!("[core event] {}", event.name());
    }
}

const DEFAULT_WELCOME_NOTE: &str = "# Welcome to Aquilum (GPUI Live Preview)\n\nYour fast, distraction-free markdown knowledge base with **true Obsidian Live Preview**!\n\n## Inline Live Preview Demo\n- Click any line or block to edit raw markdown inline\n- Move cursor away (or click another line) to see rendered styling\n- Regular text and code blocks are fully selectable across the entire page with `Ctrl+A`\n\n### Interactive Features\n- [x] Zero Electron / Chromium overhead (~40 MB RAM, 120 FPS)\n- [x] Full document continuous text selection\n- [x] Dedicated Obsidian-style code blocks with syntax styling\n- [ ] Auto-saving and instant search\n\n```rust\nfn main() {\n    println!(\"Hello from Aquilum GPUI Live Preview!\");\n}\n```\n\n> Regular text and code blocks are cleanly organized with seamless Live Preview transitions.\n";

impl AppState {
    pub fn new() -> Self {
        let app_data_dir = dirs::data_dir()
            .map(|d| d.join("com.dmitriy.aquilum-app"))
            .unwrap_or_else(|| PathBuf::from("./data"));

        let _ = std::fs::create_dir_all(&app_data_dir);
        let sink: Arc<dyn EventSink> = Arc::new(CoreEventSink);
        let core = Core::open(&app_data_dir, sink);

        let mut state = Self {
            core,
            workspace_path: None,
            workspace_name: "No Workspace".to_owned(),
            file_tree: Vec::new(),
            expanded_folders: HashSet::new(),
            search_query: String::new(),
            files: Vec::new(),
            active_note: None,
            open_tabs: Vec::new(),
            nav_history: Vec::new(),
            nav_history_index: 0,
            status_text: "GPUI Ready • 0 WebView Processes".to_owned(),
        };

        if let Ok(workspaces) = state.core.ui_state.list_workspaces() {
            if let Some(first) = workspaces.into_iter().next() {
                let path = PathBuf::from(&first.path);
                if path.exists() {
                    state.open_workspace(path);
                }
            }
        }

        if state.active_note.is_none() {
            let welcome_path = PathBuf::from("Welcome.md");
            state.open_tabs.push(welcome_path.clone());
            state.nav_history.push(welcome_path.clone());
            state.nav_history_index = 0;
            state.active_note = Some(OpenedNote::new(
                welcome_path,
                "Welcome to Aquilum".to_owned(),
                DEFAULT_WELCOME_NOTE.to_owned(),
                0,
            ));
        }

        state
    }

    pub fn open_workspace(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Workspace".to_owned());

        self.workspace_name = name;
        self.workspace_path = Some(path.clone());
        self.expanded_folders.insert(path.clone());
        self.reload_file_tree();
        self.status_text = format!("Workspace: {}", self.workspace_name);

        if self.active_note.is_none() {
            if let Some(first_file) = Self::first_file_in_tree(&self.file_tree) {
                self.open_note(first_file);
            }
        }
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
            return;
        };
        self.file_tree = Self::build_tree(ws);
        self.reload_files();
        self.status_text = format!("Vault: {}", self.workspace_name);
    }

    pub fn toggle_folder(&mut self, path: PathBuf) {
        if self.expanded_folders.contains(&path) {
            self.expanded_folders.remove(&path);
        } else {
            self.expanded_folders.insert(path);
        }
    }

    pub fn create_new_note(&mut self) -> Option<PathBuf> {
        let ws = self.workspace_path.clone()?;
        let mut i = 1;
        let mut new_path;
        loop {
            let name = format!("Untitled {}.md", i);
            new_path = ws.join(&name);
            if !new_path.exists() {
                break;
            }
            i += 1;
        }
        let default_text = format!(
            "# {}\n\n",
            new_path.file_stem().unwrap().to_string_lossy()
        );
        let _ = std::fs::write(&new_path, &default_text);
        self.reload_file_tree();
        self.open_note(new_path.clone());
        if let Some(ref mut note) = self.active_note {
            note.cursor_offset = note.content.len();
        }
        Some(new_path)
    }

    pub fn close_tab(&mut self, path: &Path) {
        if let Some(pos) = self.open_tabs.iter().position(|p| p == path) {
            self.open_tabs.remove(pos);
            if self.active_note.as_ref().map(|n| &n.path == path).unwrap_or(false) {
                let next_path = if pos < self.open_tabs.len() {
                    Some(self.open_tabs[pos].clone())
                } else if !self.open_tabs.is_empty() {
                    Some(self.open_tabs[self.open_tabs.len() - 1].clone())
                } else {
                    None
                };
                if let Some(target) = next_path {
                    self.open_note(target);
                } else {
                    self.active_note = None;
                }
            }
        } else if self.active_note.as_ref().map(|n| &n.path == path).unwrap_or(false) {
            self.active_note = None;
        }
    }

    pub fn reload_files(&mut self) {
        if let Some(ref ws) = self.workspace_path {
            match read_directory_impl(ws) {
                Ok(items) => self.files = items,
                Err(err) => eprintln!("Failed to read directory {}: {:?}", ws.display(), err),
            }
        }
    }

    pub fn push_nav_history(&mut self, path: PathBuf) {
        if let Some(current) = self.nav_history.get(self.nav_history_index) {
            if current == &path {
                return;
            }
        }
        if !self.nav_history.is_empty() && self.nav_history_index + 1 < self.nav_history.len() {
            self.nav_history.truncate(self.nav_history_index + 1);
        }
        self.nav_history.push(path);
        if self.nav_history.len() > 100 {
            self.nav_history.remove(0);
        }
        self.nav_history_index = self.nav_history.len().saturating_sub(1);
    }

    pub fn can_navigate_back(&self) -> bool {
        self.nav_history_index > 0 && !self.nav_history.is_empty()
    }

    pub fn can_navigate_forward(&self) -> bool {
        !self.nav_history.is_empty() && self.nav_history_index + 1 < self.nav_history.len()
    }

    pub fn navigate_back(&mut self) -> Option<PathBuf> {
        if self.can_navigate_back() {
            self.nav_history_index -= 1;
            let target = self.nav_history[self.nav_history_index].clone();
            self.open_note_internal(target.clone());
            Some(target)
        } else {
            None
        }
    }

    pub fn navigate_forward(&mut self) -> Option<PathBuf> {
        if self.can_navigate_forward() {
            self.nav_history_index += 1;
            let target = self.nav_history[self.nav_history_index].clone();
            self.open_note_internal(target.clone());
            Some(target)
        } else {
            None
        }
    }

    pub fn note_breadcrumbs(&self) -> Vec<String> {
        let Some(ref note) = self.active_note else {
            return Vec::new();
        };
        if let Some(ref ws) = self.workspace_path {
            if let Ok(rel) = note.path.strip_prefix(ws) {
                let mut parts = Vec::new();
                for comp in rel.components() {
                    let comp_str = comp.as_os_str().to_string_lossy().to_string();
                    if !comp_str.is_empty() {
                        parts.push(comp_str);
                    }
                }
                if let Some(last) = parts.last_mut() {
                    if let Some(stripped) = last.strip_suffix(".md") {
                        *last = stripped.to_string();
                    } else if let Some(stripped) = last.strip_suffix(".markdown") {
                        *last = stripped.to_string();
                    }
                }
                if !parts.is_empty() {
                    return parts;
                }
            }
        }
        vec![note.title.clone()]
    }

    pub fn open_note(&mut self, path: PathBuf) {
        self.push_nav_history(path.clone());
        self.open_note_internal(path);
    }

    pub fn open_note_internal(&mut self, path: PathBuf) {
        if !self.open_tabs.iter().any(|p| p == &path) {
            self.open_tabs.push(path.clone());
        }
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned());

        match self.core.documents.open(&self.core, &path, 0) {
            Ok(opened) => {
                self.active_note = Some(OpenedNote::new(
                    path.clone(),
                    title,
                    opened.text,
                    opened.version,
                ));
                self.status_text = format!("Opened: {}", path.file_name().unwrap_or_default().to_string_lossy());
            }
            Err(_) => {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    self.active_note = Some(OpenedNote::new(
                        path.clone(),
                        title,
                        content,
                        0,
                    ));
                    self.status_text = format!("Read: {}", path.file_name().unwrap_or_default().to_string_lossy());
                }
            }
        }
    }

    pub fn save_active_note(&mut self) {
        if let Some(ref mut note) = self.active_note {
            if let Err(err) = std::fs::write(&note.path, &note.content) {
                self.status_text = format!("Save error: {}", err);
            } else {
                note.is_dirty = false;
                self.status_text = format!("Saved: {}", note.title);
            }
        }
    }
}
